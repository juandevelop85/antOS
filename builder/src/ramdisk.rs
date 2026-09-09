//! Live Ramdisk (Initramfs) Packager for antOS (T24.2).
//!
//! Generates standalone USTAR archives containing the essential base system,
//! user utilities, declarative configuration, and virtual mount points.

use std::vec::Vec;

/// Default live utility fallback shell script payload if no native ELF is supplied.
pub const FALLBACK_SH_PAYLOAD: &[u8] =
    b"#!/bin/sh\n# antOS Live Minimal Shell\necho 'antOS Live Environment Ready.'\n";

/// Default parted utility script payload.
pub const FALLBACK_PARTED_PAYLOAD: &[u8] =
    b"#!/bin/sh\n# antOS Parted CLI Helper\necho 'antOS Storage Partitioner Tool'\n";

/// Default mkfs utility script payload.
pub const FALLBACK_MKFS_PAYLOAD: &[u8] =
    b"#!/bin/sh\n# antOS mkfs.ext4 CLI Helper\necho 'antOS Filesystem Creation Tool'\n";

/// Appends a single file or directory entry to a USTAR tar archive buffer.
pub fn add_tar_entry(out: &mut Vec<u8>, path: &str, content: &[u8], is_dir: bool, mode: u32) {
    let mut header = [0u8; 512];

    // Normalize path for tar: strip leading slash, ensure trailing slash for directories
    let clean_path = path.trim_start_matches('/');
    let final_path = if is_dir && !clean_path.ends_with('/') {
        format!("{}/", clean_path)
    } else {
        clean_path.to_string()
    };

    let path_bytes = final_path.as_bytes();
    let name_len = path_bytes.len().min(100);
    header[..name_len].copy_from_slice(&path_bytes[..name_len]);

    // Permissions mode (octal, 8 bytes including NUL)
    let mode_str = format!("{:07o}\0", mode);
    header[100..108].copy_from_slice(mode_str.as_bytes());

    // UID & GID
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");

    // Size (octal, 12 bytes including NUL)
    let size = if is_dir { 0 } else { content.len() };
    let size_octal = format!("{:011o}\0", size);
    header[124..136].copy_from_slice(size_octal.as_bytes());

    // Mtime
    header[136..148].copy_from_slice(b"00000000000\0");

    // Typeflag: '5' for directory, '0' for regular file
    header[156] = if is_dir { b'5' } else { b'0' };

    // USTAR magic and version
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");

    // Checksum calculation with spaces
    header[148..156].copy_from_slice(b"        ");
    let sum: u32 = header.iter().map(|&b| b as u32).sum();
    let chksum_str = format!("{:06o}\0 ", sum);
    header[148..156].copy_from_slice(chksum_str.as_bytes());

    out.extend_from_slice(&header);

    if !is_dir && !content.is_empty() {
        out.extend_from_slice(content);
        let rem = content.len() % 512;
        if rem != 0 {
            out.resize(out.len() + (512 - rem), 0);
        }
    }
}

/// Builds a complete Live Ramdisk (`initrd.img`) in USTAR format.
///
/// Contains the standard hierarchy:
/// - `/bin`: `antos`, `antosd`, `sh`, `parted`, `mkfs.ext4`, `init`
/// - `/sbin/init`: boot initialization binary
/// - `/etc`: `hostname`, `os-release`, `fstab`, `antos.conf`
/// - Virtual mount directories: `/dev/`, `/proc/`, `/sys/`, `/mnt/`, `/tmp/`
pub fn build_live_ramdisk(user_binary: Option<&[u8]>) -> Vec<u8> {
    let mut tar = Vec::new();

    // 1. Virtual and System Directories (mode 0755)
    let dirs = [
        "bin", "sbin", "etc", "dev", "proc", "sys", "mnt", "tmp", "var", "usr",
    ];
    for d in &dirs {
        add_tar_entry(&mut tar, d, &[], true, 0o755);
    }

    // 2. Binary Executables in /bin and /sbin (mode 0755)
    let bin_payload = user_binary.unwrap_or(FALLBACK_SH_PAYLOAD);

    add_tar_entry(&mut tar, "bin/init", bin_payload, false, 0o755);
    add_tar_entry(&mut tar, "bin/antos", bin_payload, false, 0o755);
    add_tar_entry(&mut tar, "bin/antosd", bin_payload, false, 0o755);
    add_tar_entry(&mut tar, "bin/sh", FALLBACK_SH_PAYLOAD, false, 0o755);
    add_tar_entry(
        &mut tar,
        "bin/parted",
        FALLBACK_PARTED_PAYLOAD,
        false,
        0o755,
    );
    add_tar_entry(
        &mut tar,
        "bin/mkfs.ext4",
        FALLBACK_MKFS_PAYLOAD,
        false,
        0o755,
    );
    add_tar_entry(&mut tar, "sbin/init", bin_payload, false, 0o755);

    // 3. Configuration Files in /etc (mode 0644)
    let hostname = b"antos-live\n";
    add_tar_entry(&mut tar, "etc/hostname", hostname, false, 0o644);

    let os_release = b"NAME=\"antOS\"\nID=antos\nPRETTY_NAME=\"antOS Live Developer OS\"\nVERSION=\"0.1.0-alpha\"\nHOME_URL=\"https://github.com/juandevelop85/antOS\"\n";
    add_tar_entry(&mut tar, "etc/os-release", os_release, false, 0o644);

    let fstab = b"rootfs / tmpfs rw 0 0\nproc /proc proc defaults 0 0\nsysfs /sys sysfs defaults 0 0\ndevtmpfs /dev devtmpfs defaults 0 0\n";
    add_tar_entry(&mut tar, "etc/fstab", fstab, false, 0o644);

    let antos_conf = b"# antOS Live Environment Configuration\nhostname=antos-live\nmode=live-installer\nvfs=tarfs\nquantum_ms=20\nroot_device=initrd\n";
    add_tar_entry(&mut tar, "etc/antos.conf", antos_conf, false, 0o644);

    let readme = b"antOS Live Installer and Multi-Agent Orchestration Environment.\nRunning in memory from Live Ramdisk (initrd.img). Internal disks are untouched.\n";
    add_tar_entry(&mut tar, "README.txt", readme, false, 0o644);

    // 4. End of Archive Marker (two consecutive 512-byte zero blocks)
    tar.resize(tar.len() + 1024, 0);

    tar
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_live_ramdisk_structure() {
        let dummy_binary = b"\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x3E\x00\x90\x90\x90\x90";
        let ramdisk_data = build_live_ramdisk(Some(dummy_binary));

        assert!(ramdisk_data.len() > 1024);
        assert_eq!(ramdisk_data.len() % 512, 0);

        // Verify USTAR magic in the first header
        assert_eq!(&ramdisk_data[257..262], b"ustar");
        assert_eq!(&ramdisk_data[263..265], b"00");

        // Parse entries and collect file paths
        let mut entries = Vec::new();
        let mut offset = 0;
        while offset + 512 <= ramdisk_data.len() {
            let header = &ramdisk_data[offset..offset + 512];
            if header.iter().all(|&b| b == 0) {
                break;
            }

            let name_end = header[0..100].iter().position(|&b| b == 0).unwrap_or(100);
            let name = std::str::from_utf8(&header[0..name_end]).unwrap();

            let mut size = 0usize;
            for &b in &header[124..136] {
                if b >= b'0' && b <= b'7' {
                    size = size * 8 + (b - b'0') as usize;
                } else if b == 0 || b == b' ' {
                    break;
                }
            }

            entries.push((name.to_string(), size));
            let blocks = (size + 511) / 512;
            offset += 512 + blocks * 512;
        }

        let paths: Vec<String> = entries.iter().map(|(p, _)| p.clone()).collect();

        // Check required directories
        assert!(paths.contains(&"bin/".to_string()));
        assert!(paths.contains(&"sbin/".to_string()));
        assert!(paths.contains(&"etc/".to_string()));
        assert!(paths.contains(&"dev/".to_string()));
        assert!(paths.contains(&"proc/".to_string()));
        assert!(paths.contains(&"sys/".to_string()));
        assert!(paths.contains(&"mnt/".to_string()));
        assert!(paths.contains(&"tmp/".to_string()));

        // Check essential executables
        assert!(paths.contains(&"bin/antos".to_string()));
        assert!(paths.contains(&"bin/antosd".to_string()));
        assert!(paths.contains(&"bin/sh".to_string()));
        assert!(paths.contains(&"bin/parted".to_string()));
        assert!(paths.contains(&"bin/mkfs.ext4".to_string()));
        assert!(paths.contains(&"bin/init".to_string()));
        assert!(paths.contains(&"sbin/init".to_string()));

        // Check declarative configuration files
        assert!(paths.contains(&"etc/hostname".to_string()));
        assert!(paths.contains(&"etc/os-release".to_string()));
        assert!(paths.contains(&"etc/fstab".to_string()));
        assert!(paths.contains(&"etc/antos.conf".to_string()));
    }
}
