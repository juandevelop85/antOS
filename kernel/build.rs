//! Compiles the userspace binary (antos-init) and bundles an initrd.tar ramdisk.

use std::path::Path;
use std::process::Command;

fn add_tar_file(out: &mut Vec<u8>, path: &str, content: &[u8]) {
    let mut header = [0u8; 512];

    let path_bytes = path.as_bytes();
    let name_len = path_bytes.len().min(100);
    header[..name_len].copy_from_slice(&path_bytes[..name_len]);

    header[100..108].copy_from_slice(b"0000755\0"); // mode
    header[108..116].copy_from_slice(b"0000000\0"); // uid
    header[116..124].copy_from_slice(b"0000000\0"); // gid

    let size_octal = format!("{:011o}\0", content.len());
    header[124..136].copy_from_slice(size_octal.as_bytes());

    header[136..148].copy_from_slice(b"00000000000\0"); // mtime
    header[156] = b'0'; // typeflag: regular file

    header[257..263].copy_from_slice(b"ustar\0"); // magic
    header[263..265].copy_from_slice(b"00"); // version

    // Checksum calculation with spaces
    header[148..156].copy_from_slice(b"        ");
    let sum: u32 = header.iter().map(|&b| b as u32).sum();
    let chksum_str = format!("{:06o}\0 ", sum);
    header[148..156].copy_from_slice(chksum_str.as_bytes());

    out.extend_from_slice(&header);
    out.extend_from_slice(content);

    let rem = content.len() % 512;
    if rem != 0 {
        out.resize(out.len() + (512 - rem), 0);
    }
}

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let user_dir = Path::new(&manifest).join("..").join("user");
    let target_dir = Path::new(&out_dir).join("user-target");

    let status = Command::new(std::env::var("CARGO").unwrap())
        .current_dir(&user_dir)
        .args(["build", "--target", "x86_64-unknown-none", "--target-dir"])
        .arg(&target_dir)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTC")
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CARGO_BUILD_TARGET")
        .env_remove("CARGO_BUILD_RUSTFLAGS")
        .status()
        .expect("could not launch cargo for the userspace binary");

    assert!(status.success(), "userspace binary compilation failed");

    let binary = target_dir.join("x86_64-unknown-none/debug/antos-init");
    let binary_bytes = std::fs::read(&binary).expect("could not read compiled user binary");

    // Construct USTAR archive containing /bin/init, /bin/worker, and configuration files
    let mut tar = Vec::new();
    add_tar_file(&mut tar, "bin/init", &binary_bytes);
    add_tar_file(&mut tar, "bin/worker", &binary_bytes);

    let conf = b"# antOS System Configuration\nhostname=antos-node0\nversion=0.1.0-alpha\nscheduler=round-robin\nquantum_ms=20\nvfs=tarfs\nroot_device=virtio-blk\n";
    add_tar_file(&mut tar, "etc/antos.conf", conf);

    let readme = b"Welcome to antOS - Native AI & Multi-Agent Operating System\nVFS initialized with USTAR tarfs backing.\n";
    add_tar_file(&mut tar, "README.txt", readme);

    // End-of-archive marker (two 512-byte zero blocks)
    tar.resize(tar.len() + 1024, 0);

    let initrd_path = Path::new(&out_dir).join("initrd.tar");
    std::fs::write(&initrd_path, &tar).expect("could not write initrd.tar");

    // Also write copy to debug target directory if it exists
    let debug_dir = Path::new(&manifest).join("target/x86_64-unknown-none/debug");
    if debug_dir.exists() {
        let _ = std::fs::write(debug_dir.join("initrd.tar"), &tar);
    }

    println!("cargo:rustc-env=USER_BINARY={}", binary.display());
    println!("cargo:rustc-env=INITRD_TAR={}", initrd_path.display());
    println!("cargo:rerun-if-changed={}", user_dir.join("src/main.rs").display());
    println!("cargo:rerun-if-changed={}", user_dir.join("Cargo.toml").display());
}
