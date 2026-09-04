//! USTAR / tarfs ramdisk and block device filesystem parser.
//!
//! Parses standard POSIX USTAR archives from memory or block devices,
//! indexing files and providing zero-copy or sector-buffered file reads.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use crate::drivers::virtio_blk;
use super::vfs::{DirEntry, FileInfo, FsError};

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

                let blocks = (size + 511) / 512;
                offset = data_offset + blocks * 512;
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

                let blocks = ((size + 511) / 512) as u64;
                sector = sector + 1 + blocks;
            } else {
                sector += 1;
            }
        }

        Ok(TarFs {
            backing: TarBacking::BlockDevice,
            entries,
        })
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
                let start = entry.sector_or_offset;
                let end = start + entry.size;
                if end <= slice.len() {
                    Ok(slice[start..end].to_vec())
                } else {
                    Err(FsError::IoError)
                }
            }
            TarBacking::BlockDevice => {
                let sector_count = (entry.size + 511) / 512;
                let mut raw_buf = vec![0u8; sector_count * 512];
                virtio_blk::read_blocks(entry.sector_or_offset as u64, &mut raw_buf)
                    .map_err(|_| FsError::IoError)?;
                raw_buf.truncate(entry.size);
                Ok(raw_buf)
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
                if !remainder.contains('/') || (e.is_dir && remainder.chars().filter(|&c| c == '/').count() == 0) {
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

/// Normalizes a path string so that it is absolute, without redundant slashes or relative components.
pub fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    let without_rel = trimmed.strip_prefix("./").unwrap_or(trimmed);
    let with_leading = if without_rel.starts_with('/') {
        without_rel.to_string()
    } else {
        alloc::format!("/{}", without_rel)
    };
    let without_trailing = if with_leading.len() > 1 {
        with_leading.trim_end_matches('/').to_string()
    } else {
        with_leading
    };
    without_trailing
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

fn parse_octal(bytes: &[u8]) -> usize {
    let mut val = 0usize;
    for &b in bytes {
        if b >= b'0' && b <= b'7' {
            val = val * 8 + (b - b'0') as usize;
        } else if b == 0 || b == b' ' {
            break;
        }
    }
    val
}
