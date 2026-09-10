//! In-memory Live Ramdisk (initramfs) driver for antOS (T24.2).
//!
//! Provides parsing, validation, and mounting of live ramdisks delivered
//! by the bootloader (Limine module or BIOS bootloader) in RAM.

use super::tarfs::TarFs;
use super::vfs::{mount_root, FsError};

/// Represents an in-memory Live Ramdisk archive.
pub struct Ramdisk {
    data: &'static [u8],
    entry_count: usize,
    total_size: usize,
}

impl Ramdisk {
    /// Initializes and verifies a Ramdisk instance from an in-memory byte slice.
    pub fn new(data: &'static [u8]) -> Result<Self, FsError> {
        if !Self::is_valid_archive(data) {
            return Err(FsError::InvalidPath);
        }
        let total_size = data.len();
        // Count entries using TarFs indexer
        let temp_fs = TarFs::from_memory(data)?;
        let entry_count = temp_fs.entry_count();

        Ok(Self {
            data,
            entry_count,
            total_size,
        })
    }

    /// Checks whether the memory slice begins with a recognized archive magic
    /// (USTAR tar archive "ustar" or CPIO archive "070701" / "070702").
    pub fn is_valid_archive(data: &[u8]) -> bool {
        if data.len() < 512 {
            return false;
        }
        // Check for USTAR at offset 257
        let is_ustar = &data[257..262] == b"ustar";
        // Check for CPIO magic at offset 0
        let is_cpio = &data[0..6] == b"070701" || &data[0..6] == b"070702";

        is_ustar || is_cpio
    }

    /// Mounts this ramdisk as the system VFS root filesystem (`/`).
    pub fn mount_as_root(&self) -> Result<(), FsError> {
        let fs = TarFs::from_memory(self.data)?;
        mount_root(fs);
        Ok(())
    }

    /// Returns the total size of the ramdisk in bytes.
    pub fn size(&self) -> usize {
        self.total_size
    }

    /// Returns the number of indexed files and directories in the ramdisk.
    pub fn entry_count(&self) -> usize {
        self.entry_count
    }

    /// Returns true if the ramdisk contains an executable init binary (/bin/init or /sbin/init).
    pub fn has_init(&self) -> bool {
        if let Ok(fs) = TarFs::from_memory(self.data) {
            fs.find_entry("/bin/init").is_some() || fs.find_entry("/sbin/init").is_some()
        } else {
            false
        }
    }
}

/// Global helper to initialize and mount the live ramdisk into VFS.
pub fn init_live_rootfs(slice: &'static [u8]) -> Result<Ramdisk, FsError> {
    let ramdisk = Ramdisk::new(slice)?;
    ramdisk.mount_as_root()?;
    Ok(ramdisk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_ramdisk_validation_and_ustar() {
        let mut mock_tar = [0u8; 1024];
        assert!(!Ramdisk::is_valid_archive(&mock_tar));

        // Inject USTAR magic at offset 257
        mock_tar[257..262].copy_from_slice(b"ustar");
        assert!(Ramdisk::is_valid_archive(&mock_tar));

        // Inject CPIO magic
        let mut mock_cpio = [0u8; 512];
        mock_cpio[0..6].copy_from_slice(b"070701");
        assert!(Ramdisk::is_valid_archive(&mock_cpio));
    }
}
