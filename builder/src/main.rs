//! antOS Boot Image Builder CLI.
//!
//! Generates bootable disk images (.img) and hybrid ISOs (.iso) for x86_64 and AArch64.

use builder::{create_iso_image, create_uefi_disk_image, detect_architecture, Architecture};
use std::path::PathBuf;

fn print_usage() {
    eprintln!("antOS Boot Image Builder");
    eprintln!(
        "Uso: builder <ruta-al-kernel.elf> [--arch x86_64|aarch64] [--format all|uefi|bios|iso]"
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    let mut kernel_path: Option<PathBuf> = None;
    let mut explicit_arch: Option<Architecture> = None;
    let mut format = String::from("all");

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--arch" => {
                if i + 1 < args.len() {
                    explicit_arch = Architecture::from_name(&args[i + 1]);
                    i += 1;
                }
            }
            "--format" => {
                if i + 1 < args.len() {
                    format = args[i + 1].clone();
                    i += 1;
                }
            }
            "--help" | "-h" => {
                print_usage();
                return;
            }
            arg if !arg.starts_with("--") && kernel_path.is_none() => {
                kernel_path = Some(PathBuf::from(arg));
            }
            _ => {}
        }
        i += 1;
    }

    let kernel = kernel_path.expect("debe especificarse la ruta al kernel ELF");
    assert!(
        kernel.exists(),
        "no existe el archivo del kernel: {}",
        kernel.display()
    );

    let arch = explicit_arch.unwrap_or_else(|| detect_architecture(&kernel));
    let out_dir = kernel
        .parent()
        .expect("el kernel debe estar dentro de un directorio");

    println!("antOS · Boot Image Builder");
    println!("══════════════════════════");
    println!("  kernel       {}", kernel.display());
    println!("  arquitectura {:?}", arch);

    let initrd_path = out_dir.join("initrd.img");
    let initrd_data = builder::ramdisk::build_live_ramdisk(None);
    std::fs::write(&initrd_path, &initrd_data).expect("no se pudo escribir initrd.img");
    println!(
        "  live ramdisk {} ({} KiB)",
        initrd_path.display(),
        initrd_data.len() / 1024
    );

    match arch {
        Architecture::X86_64 => {
            if format == "all" || format == "bios" {
                let bios_image = out_dir.join("antos-bios.img");
                bootloader::BiosBoot::new(&kernel)
                    .create_disk_image(&bios_image)
                    .expect("no se pudo crear la imagen BIOS x86_64");
                println!("  imagen bios  {}", bios_image.display());
            }

            if format == "all" || format == "uefi" {
                let uefi_image = out_dir.join("antos-uefi-x86_64.img");
                create_uefi_disk_image(&kernel, &uefi_image, Architecture::X86_64)
                    .expect("no se pudo crear la imagen UEFI x86_64");
                println!("  imagen uefi  {}", uefi_image.display());

                if format == "all" || format == "iso" {
                    let iso_image = out_dir.join("antos-x86_64.iso");
                    create_iso_image(&uefi_image, &iso_image)
                        .expect("no se pudo crear la imagen ISO x86_64");
                    println!("  imagen iso   {}", iso_image.display());
                }
            }
        }
        Architecture::AArch64 => {
            let uefi_image = out_dir.join("antos-uefi-aarch64.img");
            create_uefi_disk_image(&kernel, &uefi_image, Architecture::AArch64)
                .expect("no se pudo crear la imagen UEFI AArch64");
            println!("  imagen uefi  {}", uefi_image.display());

            if format == "all" || format == "iso" {
                let iso_image = out_dir.join("antos-aarch64.iso");
                create_iso_image(&uefi_image, &iso_image)
                    .expect("no se pudo crear la imagen ISO AArch64");
                println!("  imagen iso   {}", iso_image.display());
            }

            println!();
            println!("  ejecución en QEMU AArch64 bare-metal:");
            println!("    qemu-system-aarch64 -M virt -cpu cortex-a72 -nographic -kernel {} -serial stdio", kernel.display());
            println!("  ejecución en QEMU AArch64 con UEFI:");
            println!("    qemu-system-aarch64 -M virt -cpu cortex-a72 -nographic -bios QEMU_EFI.fd -drive format=raw,file={} -serial stdio", uefi_image.display());
        }
    }
}
