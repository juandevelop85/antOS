//! Compila el programa de espacio de usuario y le dice al kernel dónde está.
//!
//! El binario se incrusta con `include_bytes!`, igual que el bootloader
//! incrustó el nuestro. En un sistema con disco esto lo haría un cargador
//! leyendo del sistema de ficheros; aquí, mientras no exista uno, va dentro.

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
        // Las variables que cargo le pasa a un script de compilación
        // describen ESTA compilación. Heredarlas contaminaría la del
        // programa de usuario, que se compila para otra cosa.
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTC")
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CARGO_BUILD_TARGET")
        .env_remove("CARGO_BUILD_RUSTFLAGS")
        .status()
        .expect("no pude lanzar cargo para el programa de usuario");

    assert!(status.success(), "falló la compilación del programa de usuario");

    let binary = target_dir.join("x86_64-unknown-none/debug/hola");
    println!("cargo:rustc-env=USER_BINARY={}", binary.display());
    println!("cargo:rerun-if-changed={}", user_dir.join("src/main.rs").display());
    println!("cargo:rerun-if-changed={}", user_dir.join("Cargo.toml").display());
}
