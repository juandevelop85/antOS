//! Compiles the userspace binary (antos-init) and tells the kernel where it is.
//!
//! The binary is embedded via `include_bytes!`, just as the bootloader embedded
//! our kernel. In a system with a filesystem this would be done by a loader;
//! while none exists, it travels inside the kernel image.

use std::path::Path;
use std::process::Command;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let user_dir = Path::new(&manifest).join("..").join("user");
    let target_dir = Path::new(&out_dir).join("user-target");

    let status = Command::new(std::env::var("CARGO").unwrap())
        .current_dir(&user_dir)
        .args(["build", "--target", "x86_64-unknown-none", "--target-dir"])
        .arg(&target_dir)
        // Environment variables that cargo passes to build scripts describe
        // THIS compilation. Inheriting them would contaminate the user build
        // which targets a different triple.
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
    println!("cargo:rustc-env=USER_BINARY={}", binary.display());
    println!("cargo:rerun-if-changed={}", user_dir.join("src/main.rs").display());
    println!("cargo:rerun-if-changed={}", user_dir.join("Cargo.toml").display());
}
