//! Construye las imágenes de disco arrancables a partir del ELF del kernel.
//!
//! Se ejecuta en el host (macOS), no en el kernel. Por eso usa `std`.

use std::path::PathBuf;

fn main() {
    let kernel = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("uso: builder <ruta-al-elf-del-kernel>"),
    );
    assert!(kernel.exists(), "no existe el kernel: {}", kernel.display());

    let out_dir = kernel
        .parent()
        .expect("el kernel debe estar dentro de un directorio");

    // BIOS: arranque legacy. QEMU lo soporta sin firmware externo, así que
    // es el camino más corto para ver algo en pantalla.
    let bios_image = out_dir.join("syso-bios.img");
    bootloader::BiosBoot::new(&kernel)
        .create_disk_image(&bios_image)
        .expect("no se pudo crear la imagen BIOS");

    println!("{}", bios_image.display());
}
