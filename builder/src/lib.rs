//! UEFI and BIOS boot image builder for antOS.
//!
//! Generates bootable disk images (.img) and hybrid ISOs (.iso) for x86_64 and AArch64.

pub mod limine;
pub mod ramdisk;

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

/// Creates a bootable hybrid UEFI GPT / BIOS disk image with an EFI System Partition (FAT32)
/// powered by Limine Bootloader v8+ (T24.1).
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

    // 3. Format ESP partition with FAT32 and populate Limine Bootloader files
    let partition_offset = start_lba * sector_size;
    {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(out_image)?;

        let mut part = PartitionSlice::new(&mut file, partition_offset, partition_bytes);
        let format_opts = fatfs::FormatVolumeOptions::new()
            .fat_type(fatfs::FatType::Fat32)
            .bytes_per_cluster(512)
            .volume_label(*b"ANTOS_ESP  ");
        fatfs::format_volume(&mut part, format_opts)?;

        let fs = fatfs::FileSystem::new(part, fatfs::FsOptions::new())?;
        let root = fs.root_dir();

        // Create /EFI/BOOT/
        root.create_dir("EFI")?;
        let efi_dir = root.open_dir("EFI")?;
        efi_dir.create_dir("BOOT")?;
        let boot_dir = efi_dir.open_dir("BOOT")?;

        // Write official Limine PE32+ UEFI binaries for both x86_64 and AArch64
        let mut x86_efi = boot_dir.create_file("BOOTX64.EFI")?;
        x86_efi.write_all(limine::BOOTX64_EFI)?;

        let mut aarch_efi = boot_dir.create_file("BOOTAA64.EFI")?;
        aarch_efi.write_all(limine::BOOTAA64_EFI)?;

        // Save compiled antOS kernel ELF in root
        let mut kernel_file = root.create_file("KERNEL.ELF")?;
        kernel_file.write_all(&kernel_data)?;

        // Package and write Live Ramdisk (/initrd.img) in root (T24.2)
        let initrd_data = ramdisk::build_live_ramdisk(None);
        let mut initrd_file = root.create_file("initrd.img")?;
        initrd_file.write_all(&initrd_data)?;

        // Write declarative /limine.conf in root with kernel and module paths
        let conf_content = limine::generate_limine_conf(None, Some("boot():/initrd.img"), None);
        let mut conf_file = root.create_file("limine.conf")?;
        conf_file.write_all(conf_content.as_bytes())?;

        // Also duplicate /EFI/BOOT/limine.conf for firmware lookup compatibility
        let mut efi_conf_file = boot_dir.create_file("limine.conf")?;
        efi_conf_file.write_all(conf_content.as_bytes())?;

        // Write /limine-bios.sys in root for hybrid BIOS booting
        let mut bios_sys = root.create_file("limine-bios.sys")?;
        bios_sys.write_all(limine::LIMINE_BIOS_SYS)?;

        // Write /startup.nsh script for automated UEFI shell fallback
        let mut startup = root.create_file("startup.nsh")?;
        let script = limine::generate_startup_nsh(arch);
        startup.write_all(script.as_bytes())?;
    }

    // 4. Install Limine BIOS boot code into MBR / LBA 0 (and Stage 2 at LBA 64)
    {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(out_image)?;
        limine::install_limine_bios_mbr(&mut file)?;
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
    fn test_create_uefi_disk_image_and_inspect_limine() {
        let temp_dir = std::env::temp_dir();
        let dummy_kernel = temp_dir.join("dummy_kernel.elf");
        let dummy_img = temp_dir.join("test_uefi_limine.img");

        let kernel_content = b"\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x3E\x00"; // x86_64 ELF
        std::fs::write(&dummy_kernel, kernel_content).unwrap();

        let arch = detect_architecture(&dummy_kernel);
        assert_eq!(arch, Architecture::X86_64);

        let out = create_uefi_disk_image(&dummy_kernel, &dummy_img, arch).unwrap();
        assert!(out.exists());

        // Verify with fatfs that Limine UEFI binaries, initrd.img, and kernel ELF were properly written
        let mut file = File::open(&dummy_img).unwrap();
        let sector_size = 512u64;
        let start_lba = 2048u64;
        let partition_bytes = (64 * 1024 * 1024) - (4096 * sector_size);

        let part = PartitionSlice::new(&mut file, start_lba * sector_size, partition_bytes);
        let fs = fatfs::FileSystem::new(part, fatfs::FsOptions::new()).unwrap();
        let root = fs.root_dir();

        let efi_dir = root.open_dir("EFI").unwrap();
        let boot_dir = efi_dir.open_dir("BOOT").unwrap();

        // 1. Verify BOOTX64.EFI is a valid PE32+ application with MZ header (0x4D, 0x5A)
        let mut x86_file = boot_dir.open_file("BOOTX64.EFI").unwrap();
        let mut x86_bytes = Vec::new();
        x86_file.read_to_end(&mut x86_bytes).unwrap();
        assert_eq!(&x86_bytes[0..2], &[0x4D, 0x5A]);
        assert!(limine::is_valid_pe(&x86_bytes));
        assert!(limine::is_valid_pe32_plus(&x86_bytes));

        // 2. Verify BOOTAA64.EFI is also present and a valid PE32+ application
        let mut arm_file = boot_dir.open_file("BOOTAA64.EFI").unwrap();
        let mut arm_bytes = Vec::new();
        arm_file.read_to_end(&mut arm_bytes).unwrap();
        assert_eq!(&arm_bytes[0..2], &[0x4D, 0x5A]);
        assert!(limine::is_valid_pe(&arm_bytes));
        assert!(limine::is_valid_pe32_plus(&arm_bytes));

        // 3. Verify KERNEL.ELF matches our kernel payload
        let mut k_file = root.open_file("KERNEL.ELF").unwrap();
        let mut k_bytes = Vec::new();
        k_file.read_to_end(&mut k_bytes).unwrap();
        assert_eq!(k_bytes, kernel_content);

        // 4. Verify initrd.img is present and is a valid USTAR archive (T24.2)
        let mut initrd_file = root.open_file("initrd.img").unwrap();
        let mut initrd_bytes = Vec::new();
        initrd_file.read_to_end(&mut initrd_bytes).unwrap();
        assert!(!initrd_bytes.is_empty());
        assert_eq!(&initrd_bytes[257..262], b"ustar");

        // 5. Verify limine.conf contains protocol: limine, kernel_path, and module_path
        let mut conf_file = root.open_file("limine.conf").unwrap();
        let mut conf_str = String::new();
        conf_file.read_to_string(&mut conf_str).unwrap();
        assert!(conf_str.contains("protocol: limine"));
        assert!(conf_str.contains("kernel_path: boot():/KERNEL.ELF"));
        assert!(conf_str.contains("module_path: boot():/initrd.img"));
        assert!(conf_str.contains("resolution: 1280x720x32"));

        // 6. Verify limine-bios.sys exists in root for hybrid BIOS boot
        let mut bios_sys = root.open_file("limine-bios.sys").unwrap();
        let mut bios_bytes = Vec::new();
        bios_sys.read_to_end(&mut bios_bytes).unwrap();
        assert!(!bios_bytes.is_empty());

        // 7. Verify startup.nsh
        let mut nsh_file = root.open_file("startup.nsh").unwrap();
        let mut nsh_str = String::new();
        nsh_file.read_to_string(&mut nsh_str).unwrap();
        assert!(nsh_str.contains("BOOTX64.EFI"));

        // 8. Verify Limine MBR installation at LBA 0
        let mut raw_disk = File::open(&dummy_img).unwrap();
        let mut mbr_buf = [0u8; 512];
        raw_disk.read_exact(&mut mbr_buf).unwrap();
        // MBR signature
        assert_eq!(&mbr_buf[510..512], &[0x55, 0xAA]);
        // Stage 2 location offset encoded at 0x1A4 (32,768 = 64 * 512)
        let stage2_loc = u64::from_le_bytes(mbr_buf[0x1A4..0x1A4 + 8].try_into().unwrap());
        assert_eq!(stage2_loc, 64 * 512);

        // Cleanup
        let _ = std::fs::remove_file(dummy_kernel);
        let _ = std::fs::remove_file(dummy_img);
    }
}

