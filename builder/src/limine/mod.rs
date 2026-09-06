//! Limine Bootloader Integration Module (v8+) for antOS (T24.1).
//!
//! Provides official Limine static binaries (PE32+ EFI applications and BIOS stages),
//! declarative configuration generation, and hybrid BIOS/UEFI disk installation.

use crate::Architecture;
use std::io::{Read, Seek, SeekFrom, Write};

/// Official Limine x86_64 UEFI Bootloader (PE32+ executable).
pub static BOOTX64_EFI: &[u8] = include_bytes!("binaries/BOOTX64.EFI");

/// Official Limine AArch64 UEFI Bootloader (PE32+ executable).
pub static BOOTAA64_EFI: &[u8] = include_bytes!("binaries/BOOTAA64.EFI");

/// Official Limine BIOS Stage 3 loader.
pub static LIMINE_BIOS_SYS: &[u8] = include_bytes!("binaries/limine-bios.sys");

/// Official Limine BIOS MBR and Stage 2 raw binary.
pub static LIMINE_HDD_BIN: &[u8] = include_bytes!("binaries/limine-hdd.bin");

/// Official Limine BIOS CD/ISO El Torito binary.
pub static LIMINE_BIOS_CD_BIN: &[u8] = include_bytes!("binaries/limine-bios-cd.bin");

/// DOS "MZ" executable header signature (0x5A4D little-endian).
pub const PE_MZ_SIGNATURE: [u8; 2] = [0x4D, 0x5A];

/// PE signature "PE\0\0" (0x00004550 little-endian).
pub const PE_SIGNATURE: [u8; 4] = [0x50, 0x45, 0x00, 0x00];

/// PE32+ (64-bit) Optional Header Magic (0x020B).
pub const PE32_PLUS_MAGIC: u16 = 0x020B;

/// Validates whether the given binary data possesses a valid PE/COFF header.
pub fn is_valid_pe(data: &[u8]) -> bool {
    if data.len() < 64 {
        return false;
    }
    if data[0..2] != PE_MZ_SIGNATURE {
        return false;
    }
    let pe_offset = u32::from_le_bytes([data[0x3C], data[0x3D], data[0x3E], data[0x3F]]) as usize;
    if data.len() < pe_offset + 4 {
        return false;
    }
    data[pe_offset..pe_offset + 4] == PE_SIGNATURE
}

/// Validates whether the given binary data is a genuine 64-bit PE32+ EFI application.
pub fn is_valid_pe32_plus(data: &[u8]) -> bool {
    if !is_valid_pe(data) {
        return false;
    }
    let pe_offset = u32::from_le_bytes([data[0x3C], data[0x3D], data[0x3E], data[0x3F]]) as usize;
    // COFF Header is 20 bytes after "PE\0\0", followed by the 2-byte Optional Header Magic.
    if data.len() < pe_offset + 26 {
        return false;
    }
    let magic = u16::from_le_bytes([data[pe_offset + 24], data[pe_offset + 25]]);
    magic == PE32_PLUS_MAGIC
}

/// Returns the embedded PE32+ EFI bootloader binary for the specified architecture.
pub fn get_efi_bootloader(arch: Architecture) -> &'static [u8] {
    match arch {
        Architecture::X86_64 => BOOTX64_EFI,
        Architecture::AArch64 => BOOTAA64_EFI,
    }
}

/// Generates declarative `limine.conf` configuration matching T24.1 & T24.2 specifications.
pub fn generate_limine_conf(
    kernel_path: Option<&str>,
    module_path: Option<&str>,
    resolution: Option<&str>,
) -> String {
    let kpath = kernel_path.unwrap_or("boot():/KERNEL.ELF");
    let mpath = module_path.unwrap_or("boot():/initrd.img");
    let res = resolution.unwrap_or("1280x720x32");
    format!(
        "timeout: 3\n\
         default_entry: 1\n\
         graphics: yes\n\n\
         /antOS (Desarrollo y Orquestación Multi-Agente)\n    \
             protocol: limine\n    \
             kernel_path: {kpath}\n    \
             module_path: {mpath}\n    \
             resolution: {res}\n    \
             comment: Sistema operativo antOS en modo bare-metal nativo\n"
    )
}

/// Generates automated UEFI Shell fallback script (`startup.nsh`).
pub fn generate_startup_nsh(arch: Architecture) -> String {
    format!("\\EFI\\BOOT\\{}\r\n", arch.efi_filename())
}

/// Installs Limine BIOS bootsector and Stage 2 to a disk image, preserving partition tables.
///
/// Stage 2 is placed at LBA 64 (offset 32,768), well within the post-MBR gap before LBA 2048.
pub fn install_limine_bios_mbr<F: Read + Write + Seek>(file: &mut F) -> std::io::Result<()> {
    if LIMINE_HDD_BIN.len() < 512 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Limine HDD binary corrupted",
        ));
    }

    // 1. Read existing MBR partition table (bytes 440..510) and boot signature (510..512)
    file.seek(SeekFrom::Start(440))?;
    let mut orig_partition_table = [0u8; 72];
    file.read_exact(&mut orig_partition_table)?;

    // 2. Prepare bootsector from Limine HDD binary
    let mut bootsector = [0u8; 512];
    bootsector.copy_from_slice(&LIMINE_HDD_BIN[0..512]);

    // Stage 2 will reside at offset 32,768 (LBA 64, 512-byte aligned)
    let stage2_offset: u64 = 64 * 512;
    let stage2_bytes = &LIMINE_HDD_BIN[512..];

    // Encode stage 2 location into bootsector at offset 0x1A4 (little-endian uint64)
    let loc_bytes = stage2_offset.to_le_bytes();
    bootsector[0x1A4..0x1A4 + 8].copy_from_slice(&loc_bytes);

    // Preserve the original MBR partition table and signature
    bootsector[440..512].copy_from_slice(&orig_partition_table);

    // 3. Write bootsector to LBA 0
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&bootsector)?;

    // 4. Write Stage 2 into the gap at LBA 64
    file.seek(SeekFrom::Start(stage2_offset))?;
    file.write_all(stage2_bytes)?;
    file.flush()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_binaries_are_valid_pe32_plus() {
        assert!(is_valid_pe(BOOTX64_EFI));
        assert!(is_valid_pe32_plus(BOOTX64_EFI));
        assert_eq!(&BOOTX64_EFI[0..2], &PE_MZ_SIGNATURE);

        assert!(is_valid_pe(BOOTAA64_EFI));
        assert!(is_valid_pe32_plus(BOOTAA64_EFI));
        assert_eq!(&BOOTAA64_EFI[0..2], &PE_MZ_SIGNATURE);
    }

    #[test]
    fn test_generate_limine_conf() {
        let conf = generate_limine_conf(None, None, None);
        assert!(conf.contains("timeout: 3"));
        assert!(conf.contains("default_entry: 1"));
        assert!(conf.contains("graphics: yes"));
        assert!(conf.contains("protocol: limine"));
        assert!(conf.contains("kernel_path: boot():/KERNEL.ELF"));
        assert!(conf.contains("module_path: boot():/initrd.img"));
        assert!(conf.contains("resolution: 1280x720x32"));
    }

    #[test]
    fn test_generate_startup_nsh() {
        let nsh_x86 = generate_startup_nsh(Architecture::X86_64);
        assert_eq!(nsh_x86, "\\EFI\\BOOT\\BOOTX64.EFI\r\n");

        let nsh_arm = generate_startup_nsh(Architecture::AArch64);
        assert_eq!(nsh_arm, "\\EFI\\BOOT\\BOOTAA64.EFI\r\n");
    }
}
