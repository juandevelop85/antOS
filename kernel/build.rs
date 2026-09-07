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

    // The bundled `antos-init` must match the kernel's own architecture: an
    // x86_64 binary is meaningless to `elf::load_aarch64`'s `e_machine`
    // check, and vice versa. Both `USER_SPACE_VIRT` (AArch64) and the x86_64
    // loader's own convention agree on the same numeric image base
    // (`0x400000`), so the one RUSTFLAGS recipe below works for either
    // target — only the `--target` triple needs to track the kernel's.
    let kernel_target = std::env::var("TARGET").unwrap_or_else(|_| "x86_64-unknown-none".into());
    let user_target = if kernel_target.contains("aarch64") {
        "aarch64-unknown-none"
    } else {
        "x86_64-unknown-none"
    };

    let status = Command::new(std::env::var("CARGO").unwrap())
        .current_dir(&user_dir)
        .args(["build", "--target", user_target, "--target-dir"])
        .arg(&target_dir)
        .env("RUSTFLAGS", "-C relocation-model=static -C link-arg=--image-base=0x400000")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTC")
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CARGO_BUILD_TARGET")
        .env_remove("CARGO_BUILD_RUSTFLAGS")
        .status()
        .expect("could not launch cargo for the userspace binary");

    assert!(status.success(), "userspace binary compilation failed");

    let binary = target_dir.join(user_target).join("debug/antos-init");
    let binary_bytes = std::fs::read(&binary).expect("could not read compiled user binary");

    // Construct USTAR archive containing live userspace hierarchy (T24.2)
    let mut tar = Vec::new();
    add_tar_file(&mut tar, "bin/init", &binary_bytes);
    add_tar_file(&mut tar, "bin/antos", &binary_bytes);
    add_tar_file(&mut tar, "bin/antosd", &binary_bytes);
    add_tar_file(&mut tar, "bin/sh", b"#!/bin/sh\necho 'antOS Live Minimal Shell ready'\n");
    add_tar_file(&mut tar, "bin/parted", b"#!/bin/sh\necho 'antOS parted partition tool'\n");
    add_tar_file(&mut tar, "bin/mkfs.ext4", b"#!/bin/sh\necho 'antOS mkfs filesystem formatter'\n");
    add_tar_file(&mut tar, "bin/worker", &binary_bytes);
    add_tar_file(&mut tar, "sbin/init", &binary_bytes);

    add_tar_file(&mut tar, "etc/hostname", b"antos-live\n");
    add_tar_file(&mut tar, "etc/os-release", b"NAME=\"antOS\"\nID=antos\nPRETTY_NAME=\"antOS Live Developer OS\"\nVERSION=\"0.1.0-alpha\"\n");
    add_tar_file(&mut tar, "etc/fstab", b"rootfs / tmpfs rw 0 0\nproc /proc proc defaults 0 0\nsysfs /sys sysfs defaults 0 0\n");
    let conf = b"# antOS System Configuration\nhostname=antos-live\nversion=0.1.0-alpha\nscheduler=round-robin\nquantum_ms=20\nvfs=tarfs\nroot_device=initrd\n";
    add_tar_file(&mut tar, "etc/antos.conf", conf);

    let readme = b"Welcome to antOS - Native AI & Multi-Agent Operating System\nVFS initialized with Live Ramdisk backing.\n";
    add_tar_file(&mut tar, "README.txt", readme);

    // End-of-archive marker (two 512-byte zero blocks)
    tar.resize(tar.len() + 1024, 0);

    let initrd_path = Path::new(&out_dir).join("initrd.tar");
    std::fs::write(&initrd_path, &tar).expect("could not write initrd.tar");

    // Also write a copy to the kernel's own debug target directory, if it exists
    let debug_dir = Path::new(&manifest).join("target").join(&kernel_target).join("debug");
    if debug_dir.exists() {
        let _ = std::fs::write(debug_dir.join("initrd.tar"), &tar);
    }

    println!("cargo:rustc-env=USER_BINARY={}", binary.display());
    println!("cargo:rustc-env=INITRD_TAR={}", initrd_path.display());

    // Building with `--features limine` (T27.1) links a completely different
    // entry point (`arch::<arch>::limine_boot::_start`, gated on the same
    // feature) that speaks the Limine boot protocol instead of either the
    // `bootloader` crate's own convention (x86_64) or the from-scratch
    // direct-QEMU-boot sequence (AArch64) — and Limine refuses to load an
    // executable whose segments sit in the lower half of the address space
    // ("Lower half PHDRs are not allowed"), so this build also needs a
    // different base address.
    let limine_build = std::env::var("CARGO_FEATURE_LIMINE").is_ok();

    if kernel_target.contains("aarch64") {
        let linker_script_name = if limine_build { "linker_limine.ld" } else { "linker.ld" };
        let linker_script = Path::new(&manifest).join("src").join("arch").join("aarch64").join(linker_script_name);
        println!("cargo:rustc-link-arg=-T{}", linker_script.display());
        println!("cargo:rerun-if-changed={}", linker_script.display());
    } else if limine_build {
        // No custom linker script exists for x86_64 today — the `bootloader`
        // crate's own loader stage places the kernel wherever lld's default
        // (low, non-PIE) layout puts it and reads that address straight from
        // the ELF headers, so nothing has ever needed to control it. Limine
        // does care: `0xffffffff80000000` is the exact boundary its protocol
        // specification names as the start of the higher half.
        println!("cargo:rustc-link-arg=--image-base=0xffffffff80000000");
    }

    println!("cargo:rerun-if-changed={}", user_dir.join("src/main.rs").display());
    println!("cargo:rerun-if-changed={}", user_dir.join("Cargo.toml").display());
    println!("cargo:rerun-if-changed={}", user_dir.join("libantos/src/lib.rs").display());
    println!("cargo:rerun-if-changed={}", user_dir.join("libantos/src/syscall.rs").display());
    println!("cargo:rerun-if-changed={}", user_dir.join("libantos/src/io.rs").display());
    println!("cargo:rerun-if-changed={}", user_dir.join("libantos/src/allocator.rs").display());
    println!("cargo:rerun-if-changed={}", user_dir.join("libantos/src/channel.rs").display());
}
