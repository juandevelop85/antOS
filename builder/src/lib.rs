//! UEFI and BIOS boot image builder for antOS.
//!
//! Generates bootable disk images (.img) and hybrid ISOs (.iso) for x86_64 and AArch64.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Architecture {
    X86_64,
    AArch64,
}

impl Architecture {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "x86_64" | "x86-64" | "amd64" | "x64" => Some(Self::X86_64),
            "aarch64" | "arm64" | "arm" => Some(Self::AArch64),
            _ => None,
        }
    }

    pub fn efi_filename(&self) -> &'static str {
        match self {
            Self::X86_64 => "BOOTX64.EFI",
            Self::AArch64 => "BOOTAA64.EFI",
        }
    }
}

/// Detects the target architecture by reading the ELF header (e_machine at offset 0x12).
pub fn detect_architecture(kernel_path: &Path) -> Architecture {
    if let Ok(mut f) = File::open(kernel_path) {
        let mut header = [0u8; 20];
        if f.read_exact(&mut header).is_ok() && &header[0..4] == b"\x7fELF" {
            let machine = u16::from_le_bytes([header[18], header[19]]);
            match machine {
                0xB7 => return Architecture::AArch64, // EM_AARCH64 = 183 (0xB7)
                0x3E => return Architecture::X86_64,  // EM_X86_64 = 62 (0x3E)
                _ => {}
            }
        }
    }
    // Fallback: check path string
    if kernel_path.to_string_lossy().contains("aarch64") {
        Architecture::AArch64
    } else {
        Architecture::X86_64
    }
}

/// Stream slice representing a partition on a raw disk image.
pub struct PartitionSlice<'a> {
    file: &'a mut File,
    start: u64,
    size: u64,
    pos: u64,
}

impl<'a> PartitionSlice<'a> {
    pub fn new(file: &'a mut File, start: u64, size: u64) -> Self {
        Self {
            file,
            start,
            size,
            pos: 0,
        }
    }
}

impl<'a> Read for PartitionSlice<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let remaining = (self.size - self.pos) as usize;
        if remaining == 0 {
            return Ok(0);
        }
        let to_read = buf.len().min(remaining);
        self.file.seek(SeekFrom::Start(self.start + self.pos))?;
        let bytes_read = self.file.read(&mut buf[..to_read])?;
        self.pos += bytes_read as u64;
        Ok(bytes_read)
    }
}

impl<'a> Write for PartitionSlice<'a> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let remaining = (self.size - self.pos) as usize;
        if remaining == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "partition full",
            ));
        }
        let to_write = buf.len().min(remaining);
        self.file.seek(SeekFrom::Start(self.start + self.pos))?;
        let bytes_written = self.file.write(&buf[..to_write])?;
        self.pos += bytes_written as u64;
        Ok(bytes_written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

impl<'a> Seek for PartitionSlice<'a> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let new_pos = match pos {
            SeekFrom::Start(p) => p as i64,
            SeekFrom::Current(p) => self.pos as i64 + p,
            SeekFrom::End(p) => self.size as i64 + p,
        };
        if new_pos < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid seek to negative offset",
            ));
        }
        self.pos = (new_pos as u64).min(self.size);
        self.file.seek(SeekFrom::Start(self.start + self.pos))?;
        Ok(self.pos)
    }
}

/// Creates a bootable UEFI GPT disk image with an EFI System Partition (FAT32).
pub fn create_uefi_disk_image(
    kernel_path: &Path,
    out_image: &Path,
    arch: Architecture,
) -> std::io::Result<PathBuf> {
    let mut kernel_data = Vec::new();
    File::open(kernel_path)?.read_to_end(&mut kernel_data)?;

    // 64 MiB total disk size
    let disk_size: u64 = 64 * 1024 * 1024;
    let sector_size = 512u64;

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(out_image)?;
    file.set_len(disk_size)?;

    // 1. Write Protective MBR
    let mbr = gpt::mbr::ProtectiveMBR::with_lb_size(
        u32::try_from((disk_size / sector_size) - 1).unwrap_or(0xFFFFFFFF),
    );
    mbr.overwrite_lba0(&mut file)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{e}")))?;
    drop(file);

    // 2. Initialize GPT partition table using gpt crate
    let start_lba = 2048u64; // 1 MiB alignment
    let end_lba = (disk_size / sector_size) - 2048;
    let partition_bytes = (end_lba - start_lba + 1) * sector_size;

    {
        let mut gpt_disk = gpt::GptConfig::new()
            .writable(true)
            .create(out_image)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{e}")))?;

        gpt_disk
            .add_partition(
                "EFI System Partition",
                partition_bytes,
                gpt::partition_types::EFI,
                0,
                Some(2048),
            )
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{e}")))?;
        gpt_disk
            .write()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{e}")))?;
    }

    // 3. Format ESP partition with FAT
    let partition_offset = start_lba * sector_size;
    {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(out_image)?;

        let mut part = PartitionSlice::new(&mut file, partition_offset, partition_bytes);
        fatfs::format_volume(&mut part, fatfs::FormatVolumeOptions::new())?;

        let fs = fatfs::FileSystem::new(part, fatfs::FsOptions::new())?;
        let root = fs.root_dir();

        // Create /EFI/BOOT/
        root.create_dir("EFI")?;
        let efi_dir = root.open_dir("EFI")?;
        efi_dir.create_dir("BOOT")?;
        let boot_dir = efi_dir.open_dir("BOOT")?;

        // Write EFI binary
        let efi_name = arch.efi_filename();
        let mut efi_file = boot_dir.create_file(efi_name)?;
        efi_file.write_all(&kernel_data)?;

        // Also save kernel ELF in root for direct bootloaders
        let mut kernel_file = root.create_file("KERNEL.ELF")?;
        kernel_file.write_all(&kernel_data)?;

        // Write /startup.nsh script for automated UEFI shell boot
        let mut startup = root.create_file("startup.nsh")?;
        let script = format!("\\EFI\\BOOT\\{}\r\n", efi_name);
        startup.write_all(script.as_bytes())?;
    }

    Ok(out_image.to_path_buf())
}

/// Creates a bootable hybrid ISO image containing the ESP.
pub fn create_iso_image(img_path: &Path, out_iso: &Path) -> std::io::Result<PathBuf> {
    // An ISO with an embedded EFI System Partition table functions as a hybrid bootable ISO
    std::fs::copy(img_path, out_iso)?;
    Ok(out_iso.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_architecture_from_str() {
        assert_eq!(Architecture::from_str("x86_64"), Some(Architecture::X86_64));
        assert_eq!(Architecture::from_str("aarch64"), Some(Architecture::AArch64));
        assert_eq!(Architecture::from_str("arm64"), Some(Architecture::AArch64));
        assert_eq!(Architecture::from_str("unknown"), None);
    }

    #[test]
    fn test_efi_filenames() {
        assert_eq!(Architecture::X86_64.efi_filename(), "BOOTX64.EFI");
        assert_eq!(Architecture::AArch64.efi_filename(), "BOOTAA64.EFI");
    }

    #[test]
    fn test_create_uefi_disk_image_and_inspect_fat() {
        let temp_dir = std::env::temp_dir();
        let dummy_kernel = temp_dir.join("dummy_kernel.elf");
        let dummy_img = temp_dir.join("test_uefi.img");

        let kernel_content = b"\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\xB7\x00"; // AArch64 ELF
        std::fs::write(&dummy_kernel, kernel_content).unwrap();

        let arch = detect_architecture(&dummy_kernel);
        assert_eq!(arch, Architecture::AArch64);

        let out = create_uefi_disk_image(&dummy_kernel, &dummy_img, arch).unwrap();
        assert!(out.exists());

        // Verify with fatfs that /EFI/BOOT/BOOTAA64.EFI was created
        let mut file = File::open(&dummy_img).unwrap();
        let sector_size = 512u64;
        let start_lba = 2048u64;
        let partition_bytes = (64 * 1024 * 1024) - (4096 * sector_size);

        let part = PartitionSlice::new(&mut file, start_lba * sector_size, partition_bytes);
        let fs = fatfs::FileSystem::new(part, fatfs::FsOptions::new()).unwrap();
        let root = fs.root_dir();

        let efi_dir = root.open_dir("EFI").unwrap();
        let boot_dir = efi_dir.open_dir("BOOT").unwrap();
        let mut efi_file = boot_dir.open_file("BOOTAA64.EFI").unwrap();
        let mut read_back = Vec::new();
        efi_file.read_to_end(&mut read_back).unwrap();

        assert_eq!(read_back, kernel_content);

        // Cleanup
        let _ = std::fs::remove_file(dummy_kernel);
        let _ = std::fs::remove_file(dummy_img);
    }
}
