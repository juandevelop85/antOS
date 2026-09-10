//! USTAR / tarfs ramdisk and block device filesystem parser.
//!
//! Parses standard POSIX USTAR archives from memory or block devices,
//! indexing files and providing zero-copy or sector-buffered file reads.

use super::vfs::{DirEntry, FileInfo, FsError};
#[cfg(target_arch = "x86_64")]
use crate::drivers::virtio_blk;
use alloc::string::{String, ToString};
#[cfg(target_arch = "x86_64")]
use alloc::vec;
use alloc::vec::Vec;

/// Storage backing for the tar filesystem.
#[derive(Debug, Clone, Copy)]
pub enum TarBacking {
    /// In-memory static slice (e.g. initramfs loaded by bootloader).
    Memory(&'static [u8]),
    /// VirtIO block device (sectors read on-demand).
    BlockDevice,
}

/// Metadata and location of an archived file or directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TarEntry {
    pub path: String,
    pub size: usize,
    pub is_dir: bool,
    pub sector_or_offset: usize,
}

/// In-memory USTAR Tar Filesystem driver.
pub struct TarFs {
    backing: TarBacking,
    entries: Vec<TarEntry>,
}

impl TarFs {
    /// Mounts and indexes a USTAR archive from an in-memory byte slice.
    pub fn from_memory(data: &'static [u8]) -> Result<Self, FsError> {
        let mut entries = Vec::new();
        let mut offset = 0;

        while offset + 512 <= data.len() {
            let header_bytes = &data[offset..offset + 512];
            // Check for end-of-archive (two consecutive 512-byte zero blocks)
            if header_bytes.iter().all(|&b| b == 0) {
                break;
            }

            if let Some((path, size, is_dir)) = parse_header(header_bytes) {
                let data_offset = offset + 512;
                entries.push(TarEntry {
                    path,
                    size,
                    is_dir,
                    sector_or_offset: data_offset,
                });

                // Checked (T31.3): `size` comes straight from the archive's
                // header field, so a corrupted or hostile value must not be
                // able to wrap `offset` around and desynchronize the parser
                // from the real layout of the file.
                let block_bytes = size
                    .div_ceil(512)
                    .checked_mul(512)
                    .ok_or(FsError::IoError)?;
                offset = data_offset
                    .checked_add(block_bytes)
                    .ok_or(FsError::IoError)?;
            } else {
                offset += 512;
            }
        }

        Ok(TarFs {
            backing: TarBacking::Memory(data),
            entries,
        })
    }

    /// Mounts and indexes a USTAR archive from the VirtIO block device.
    pub fn from_block_device() -> Result<Self, FsError> {
        #[cfg(target_arch = "x86_64")]
        {
            if !virtio_blk::is_available() {
                return Err(FsError::IoError);
            }

            let mut entries = Vec::new();
            let mut sector = 0u64;
            let mut header_buf = [0u8; 512];

            let max_sectors = virtio_blk::capacity_sectors();
            while sector < max_sectors {
                if virtio_blk::read_blocks(sector, &mut header_buf).is_err() {
                    break;
                }

                // Check for end of tar archive
                if header_buf.iter().all(|&b| b == 0) {
                    break;
                }

                if let Some((path, size, is_dir)) = parse_header(&header_buf) {
                    let data_sector = (sector + 1) as usize;
                    entries.push(TarEntry {
                        path,
                        size,
                        is_dir,
                        sector_or_offset: data_sector,
                    });

                    // Checked (T31.3): see the identical reasoning in
                    // `from_memory` above — `size` is attacker-controlled.
                    let blocks = size.div_ceil(512) as u64;
                    sector = sector
                        .checked_add(1)
                        .and_then(|s| s.checked_add(blocks))
                        .ok_or(FsError::IoError)?;
                } else {
                    sector += 1;
                }
            }

            Ok(TarFs {
                backing: TarBacking::BlockDevice,
                entries,
            })
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            Err(FsError::IoError)
        }
    }

    /// Finds a metadata entry for the given normalized path.
    pub fn find_entry(&self, path: &str) -> Option<&TarEntry> {
        let norm = normalize_path(path);
        self.entries.iter().find(|e| e.path == norm)
    }

    /// Reads all bytes belonging to a file entry.
    pub fn read_file(&self, entry: &TarEntry) -> Result<Vec<u8>, FsError> {
        if entry.is_dir {
            return Err(FsError::NotAFile);
        }

        match self.backing {
            TarBacking::Memory(slice) => {
                // Checked (T31.3): `entry.size` is derived from the archive
                // header, so it must not be able to wrap this addition and
                // slip a truncated or out-of-bounds slice past the check.
                let start = entry.sector_or_offset;
                match start.checked_add(entry.size) {
                    Some(end) if end <= slice.len() => Ok(slice[start..end].to_vec()),
                    _ => Err(FsError::IoError),
                }
            }
            TarBacking::BlockDevice => {
                #[cfg(target_arch = "x86_64")]
                {
                    let sector_count = entry.size.div_ceil(512);
                    let byte_len = sector_count.checked_mul(512).ok_or(FsError::IoError)?;
                    let mut raw_buf = vec![0u8; byte_len];
                    virtio_blk::read_blocks(entry.sector_or_offset as u64, &mut raw_buf)
                        .map_err(|_| FsError::IoError)?;
                    raw_buf.truncate(entry.size);
                    Ok(raw_buf)
                }
                #[cfg(not(target_arch = "x86_64"))]
                {
                    Err(FsError::IoError)
                }
            }
        }
    }

    /// Queries file information.
    pub fn stat(&self, path: &str) -> Result<FileInfo, FsError> {
        let entry = self.find_entry(path).ok_or(FsError::NotFound)?;
        Ok(FileInfo {
            path: entry.path.clone(),
            size: entry.size,
            is_dir: entry.is_dir,
        })
    }

    /// Lists entries inside the specified directory path.
    pub fn list_dir(&self, dir_path: &str) -> Result<Vec<DirEntry>, FsError> {
        let norm_dir = normalize_path(dir_path);
        let prefix = if norm_dir == "/" {
            String::from("/")
        } else {
            alloc::format!("{}/", norm_dir)
        };

        let mut results = Vec::new();
        for e in &self.entries {
            if e.path.starts_with(&prefix) && e.path != norm_dir {
                let remainder = &e.path[prefix.len()..];
                if !remainder.contains('/')
                    || (e.is_dir && remainder.chars().filter(|&c| c == '/').count() == 0)
                {
                    let name = remainder.trim_matches('/').to_string();
                    if !name.is_empty() && !results.iter().any(|r: &DirEntry| r.name == name) {
                        results.push(DirEntry {
                            name,
                            is_dir: e.is_dir,
                            size: e.size,
                        });
                    }
                }
            }
        }

        Ok(results)
    }

    /// Returns the total number of indexed entries.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Returns references to all indexed entries.
    pub fn all_entries(&self) -> &[TarEntry] {
        &self.entries
    }
}

/// Normalizes a path string so that it is absolute, without redundant
/// slashes, `.` components, or `..` components (T31.3).
///
/// A `..` component pops the last resolved segment, same as any ordinary
/// path resolver; a `..` with nothing left to pop is **dropped**, never
/// resolved above the root. Without this, an archive entry literally named
/// `../../etc/passwd` was kept exactly as written and only ever excluded by
/// coincidence — because every lookup so far has required an exact string
/// match against a normalized query path, never a directory walk or an
/// extraction to disk. The moment either of those appears, an unresolved
/// `..` becomes a real path-traversal primitive; this closes that off now,
/// before it's needed to.
pub fn normalize_path(path: &str) -> String {
    let mut components: Vec<&str> = Vec::new();
    for part in path.trim().split('/') {
        match part {
            "" | "." => continue,
            ".." => {
                components.pop();
            }
            other => components.push(other),
        }
    }

    if components.is_empty() {
        String::from("/")
    } else {
        alloc::format!("/{}", components.join("/"))
    }
}

fn parse_header(header: &[u8]) -> Option<(String, usize, bool)> {
    if header.len() < 512 {
        return None;
    }

    // Name is in bytes 0..100
    let name_end = header[0..100].iter().position(|&b| b == 0).unwrap_or(100);
    if name_end == 0 {
        return None;
    }
    let name_str = core::str::from_utf8(&header[0..name_end]).ok()?;

    // Size is in bytes 124..136 (octal string)
    let size_bytes = &header[124..136];
    let size = parse_octal(size_bytes);

    // Typeflag is at byte 156
    let typeflag = header[156];
    let is_dir = typeflag == b'5' || name_str.ends_with('/');

    // Prefix is in bytes 345..500
    let prefix_end = header[345..500].iter().position(|&b| b == 0).unwrap_or(155);
    let full_path = if prefix_end > 0 {
        if let Ok(prefix_str) = core::str::from_utf8(&header[345..345 + prefix_end]) {
            alloc::format!("{}/{}", prefix_str, name_str)
        } else {
            name_str.to_string()
        }
    } else {
        name_str.to_string()
    };

    Some((normalize_path(&full_path), size, is_dir))
}

/// Parses a NUL/space-terminated octal byte string from a USTAR header field.
///
/// Saturates instead of wrapping on overflow (T31.3): an overlong or
/// malicious octal string can no longer produce a small, wrapped-around
/// value that would then pass a downstream bounds check it should have
/// failed. Every caller already treats an implausibly large size as
/// "reject this entry" via the checked arithmetic in `read_file` and the
/// indexing loops above, so saturating here is enough — there is no need to
/// thread a `Result` through a header field parser.
fn parse_octal(bytes: &[u8]) -> usize {
    let mut val = 0usize;
    for &b in bytes {
        if (b'0'..=b'7').contains(&b) {
            val = val.saturating_mul(8).saturating_add((b - b'0') as usize);
        } else if b == 0 || b == b' ' {
            break;
        }
    }
    val
}

// -------------------------------------------------------------------- tests
//
// NOTE (T31.3 / T31.16): see the identical note in `kernel/src/elf.rs` — the
// kernel workspace's `cargo test` does not yet compile (tracked in T31.16).
// This exact logic was additionally verified by hand: copied verbatim into a
// standalone host binary and run with `rustc -O`, confirming `parse_octal`
// saturates instead of wrapping and `normalize_path` never resolves `..`
// above the root. These tests will run for real as soon as T31.16 lands.
#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_parse_octal_saturates_on_overflow_instead_of_wrapping() {
        // Far more digits than fit in a u64/usize.
        let overflow = b"77777777777777777777777";
        assert_eq!(parse_octal(overflow), usize::MAX);
    }

    #[test_case]
    fn test_parse_octal_normal_values_still_correct() {
        assert_eq!(parse_octal(b"0000644\0"), 0o644);
        assert_eq!(parse_octal(b"0000000\0"), 0);
        assert_eq!(parse_octal(b"0001000\0"), 0o1000);
    }

    #[test_case]
    fn test_normalize_path_never_escapes_the_root() {
        assert_eq!(normalize_path("../../etc/passwd"), "/etc/passwd");
        assert_eq!(
            normalize_path("../../../../../../etc/shadow"),
            "/etc/shadow"
        );
        assert_eq!(normalize_path(".."), "/");
        assert_eq!(normalize_path("a/../.."), "/");
    }

    #[test_case]
    fn test_normalize_path_resolves_dot_and_dotdot_components() {
        assert_eq!(normalize_path("a/./b/../c"), "/a/c");
        assert_eq!(normalize_path("./foo/bar"), "/foo/bar");
        assert_eq!(normalize_path("/foo/bar/"), "/foo/bar");
    }

    #[test_case]
    fn test_normalize_path_base_cases_unchanged() {
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path(""), "/");
        assert_eq!(normalize_path("foo"), "/foo");
    }

    #[test_case]
    fn test_read_file_memory_backing_rejects_offset_size_overflow() {
        let data: &'static [u8] = &[0u8; 16];
        let fs = TarFs {
            backing: TarBacking::Memory(data),
            entries: alloc::vec::Vec::new(),
        };
        let entry = TarEntry {
            path: String::from("/evil"),
            size: usize::MAX,
            is_dir: false,
            sector_or_offset: 8,
        };
        assert!(
            fs.read_file(&entry).is_err(),
            "start + size overflow must be rejected, not wrap into a valid slice"
        );
    }
}
