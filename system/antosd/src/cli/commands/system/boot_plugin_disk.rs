//! `antos boot`, `antos plugin` y `antos disk`.
#![allow(unused_imports, dead_code)]

extern crate antos_protocol;

use crate::capability::{Catalog, Tier};
use crate::cli::args::Opts;
use crate::ctx::Ctx;
use crate::grants::Grants;
use crate::journal::{Outcome, Record};
use crate::planner::{
    claude::ClaudePlanner, local::LocalPlanner, ollama::OllamaPlanner,
    openai_compat::OpenAiCompatPlanner, Planner,
};
use crate::terminal::{ellipsis, paint, tier_color, BLUE, BOLD, CYAN, DIM, GREEN, RED, YELLOW};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub fn cmd_boot(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    let engine = crate::boot::BootEngine::global();

    match sub {
        "build" | "compile" => {
            println!(
                "\n{} Compilando kernel no_std y empaquetando imagen BIOS/UEFI...",
                paint("antOS Boot ·", BOLD)
            );
            let img = engine.build(&ctx.workspace)?;
            println!(
                "  {} {}\n",
                paint("✓ Imagen generada:", GREEN),
                img.display()
            );
        }
        "test" | "check" => {
            println!(
                "\n{} Ejecutando prueba automatizada de arranque en QEMU (headless)...",
                paint("antOS Boot ·", BOLD)
            );
            let res = engine.test_boot(&ctx.workspace)?;
            println!("{}\n", res);
        }
        "qemu" | "run" => {
            println!("\n{} Iniciando QEMU...", paint("antOS Boot ·", BOLD));
            let img = engine.build(&ctx.workspace)?;
            let run_script = ctx.workspace.join("run.sh");
            if run_script.exists() {
                let status = std::process::Command::new("bash")
                    .arg(&run_script)
                    .status()?;
                if !status.success() {
                    bail!("QEMU finalizó con código {:?}", status.code());
                }
            } else {
                let status = std::process::Command::new("qemu-system-x86_64")
                    .args(["-m", "256M", "-serial", "stdio", "-drive"])
                    .arg(format!("format=raw,file={}", img.display()))
                    .status()?;
                if !status.success() {
                    bail!("QEMU finalizó con código {:?}", status.code());
                }
            }
        }
        "iso" => {
            let mut arch = "x86_64";
            let mut iter = args.iter().skip(1);
            while let Some(a) = iter.next() {
                if a == "--arch" {
                    if let Some(val) = iter.next() {
                        arch = val.as_str();
                    }
                } else if a == "aarch64" || a == "arm64" {
                    arch = "aarch64";
                }
            }
            println!(
                "\n{} Construyendo imagen Live ISO autoarrancable ({arch})...",
                paint("antOS Boot ·", BOLD)
            );
            let iso = engine.build_iso_arch(&ctx.workspace, arch)?;
            println!(
                "  {} {}\n",
                paint("✓ Live ISO generada:", GREEN),
                iso.display()
            );
        }
        "release" | "dist" => {
            println!(
                "\n{} Ejecutando pipeline oficial de empaquetado release...",
                paint("antOS Release ·", BOLD)
            );
            let res = engine.build_release(&ctx.workspace)?;
            println!("{}\n", res);
        }
        _ => {
            let st = engine.status(&ctx.workspace);
            println!(
                "\n{} Estado del Pipeline de Arranque Bare Metal:",
                paint("antOS Boot ·", BOLD)
            );
            println!("  • Arquitectura:         {}", paint(&st.target_arch, CYAN));
            println!(
                "  • Binario Kernel ELF:   {} ({})",
                if st.kernel_elf_exists {
                    paint("Presente", GREEN)
                } else {
                    paint("No compilado", YELLOW)
                },
                if st.kernel_elf_exists {
                    format!("{} KiB", st.kernel_elf_size_bytes / 1024)
                } else {
                    "0 B".into()
                }
            );
            println!(
                "  • Imagen BIOS/MBR:      {} ({})",
                if st.bios_image_exists {
                    paint("Presente", GREEN)
                } else {
                    paint("No generada", YELLOW)
                },
                if st.bios_image_exists {
                    format!(
                        "{:.1} MB",
                        st.bios_image_size_bytes as f64 / (1024.0 * 1024.0)
                    )
                } else {
                    "0 B".into()
                }
            );
            println!(
                "  • Emulador QEMU:        {}",
                if st.qemu_installed {
                    paint("Disponible (qemu-system-x86_64)", GREEN)
                } else {
                    paint("No instalado", RED)
                }
            );
            println!("\n  Uso:");
            println!(
                "    antos boot build      Compila el kernel no_std y crea la imagen de disco"
            );
            println!("    antos boot test       Prueba automatizada de arranque en QEMU headless");
            println!("    antos boot qemu       Lanza la máquina virtual interactiva en QEMU");
            println!("    antos boot iso        Genera la imagen Live ISO autoarrancable");
            println!("    antos boot release    Ejecuta el pipeline de empaquetado y checksums\n");
        }
    }
    Ok(())
}

// ------------------------------------------------------------------- plugins

pub fn cmd_plugin(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    let plugins_dir = crate::wasm::PluginManager::get_plugins_dir(&ctx.workspace);

    match sub {
        "list" | "ls" => {
            let list = crate::wasm::PluginManager::list_plugins(&plugins_dir);
            println!(
                "\n{} Plugins WebAssembly (WASM) Registrados:",
                paint("antOS ·", BOLD)
            );
            if list.is_empty() {
                println!(
                    "  (no hay plugins instalados en {})\n",
                    plugins_dir.display()
                );
            } else {
                for p in list {
                    let kb = p.wasm_size_bytes.div_ceil(1024);
                    println!(
                        "  • {} v{} ({} KiB) - {}",
                        paint(&p.name, GREEN),
                        paint(&p.version, CYAN),
                        kb,
                        p.description
                    );
                    println!("    Acciones disponibles: {}", p.capabilities.join(", "));
                }
                println!();
            }
        }
        "install" => {
            let path_str = args.get(1).map(String::as_str).unwrap_or(".");
            let src = ctx.workspace.join(path_str);
            println!(
                "\n{} Instalando plugin desde {}...",
                paint("antOS ·", BOLD),
                src.display()
            );
            let summary = crate::wasm::PluginManager::install_plugin(&plugins_dir, &src)?;
            println!(
                "  {} {} v{} (acciones: {})\n",
                paint("✓ Plugin instalado:", GREEN),
                summary.name,
                summary.version,
                summary.capabilities.join(", ")
            );
        }
        "run" => {
            let name = args.get(1).map(String::as_str).unwrap_or("");
            if name.is_empty() {
                bail!("Uso: antos plugin run <nombre> [accion] [clave=valor...]");
            }
            let action = args.get(2).map(String::as_str).unwrap_or("run");
            let mut params = std::collections::BTreeMap::new();
            for arg in args.iter().skip(3) {
                if let Some((k, v)) = arg.split_once('=') {
                    params.insert(k.to_string(), v.to_string());
                }
            }

            println!(
                "\n{} Ejecutando plugin [{}:{}] en sandbox aislado WASM...",
                paint("antOS ·", BOLD),
                name,
                action
            );
            let res = crate::wasm::PluginManager::run_plugin(&plugins_dir, name, action, &params);
            if res.success {
                println!("  {} {}", paint("✓ Resultado:", GREEN), res.output);
                println!("    Ciclos de instrucción:  {}", res.fuel_consumed);
                println!(
                    "    Memoria lineal:         {} KiB (cuota máx: 64 MB)\n",
                    res.memory_allocated_bytes / 1024
                );
            } else {
                let err = res.error.unwrap_or_else(|| "Error desconocido".into());
                println!("  {} {}\n", paint("✗ Error:", RED), err);
                bail!("Fallo durante ejecución en sandbox WASM");
            }
        }
        _ => {
            println!(
                "\n{} Gestor de Plugins WebAssembly (WASM):",
                paint("antOS Plugins ·", BOLD)
            );
            println!("  Uso:");
            println!("    antos plugin list                   Enumera plugins instalados");
            println!("    antos plugin install <directorio>   Instala un plugin con plugin.toml");
            println!(
                "    antos plugin run <nombre> [accion]  Ejecuta una acción en sandbox WASM\n"
            );
        }
    }
    Ok(())
}

// -------------------------------------------------------- screenshot & visual qa

pub fn cmd_disk(_ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("list");

    match sub {
        "list" | "ls" => {
            println!(
                "\n{} Unidades de Almacenamiento Detectadas:",
                paint("antOS Almacenamiento ·", BOLD)
            );
            let disks = crate::installer::DiskManager::list_disks()?;
            if disks.is_empty() {
                println!("  (no se detectaron unidades de bloque en el sistema)\n");
                return Ok(());
            }

            for d in &disks {
                let gb = d.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                println!(
                    "  • {} ({:.1} GB, Bus: {}, Tabla: {})",
                    paint(&d.path, CYAN),
                    gb,
                    d.bus_type,
                    d.partition_table
                );
                println!("    Modelo:      {}", d.model);
                println!(
                    "    Sectores:    {} bytes / sector (RO: {})",
                    d.sector_size, d.is_read_only
                );
                if d.partitions.is_empty() {
                    println!("    Particiones: (disco sin particiones)");
                } else {
                    println!("    Particiones: {} detectadas", d.partitions.len());
                    for p in &d.partitions {
                        let p_mb = p.size_bytes / (1024 * 1024);
                        let efi_badge = if p.is_efi {
                            paint(" [EFI ESP]", GREEN)
                        } else {
                            "".into()
                        };
                        let fs = p.fs_type.as_deref().unwrap_or("desconocido");
                        let mount = p.mountpoint.as_deref().unwrap_or("no montada");
                        println!(
                            "      - {} ({:.0} MB, {}) → {}{}",
                            paint(&p.name, BOLD),
                            p_mb,
                            fs,
                            mount,
                            efi_badge
                        );
                    }
                }
                println!();
            }
        }
        "inspect" | "info" => {
            let target = args.get(1).map(String::as_str).unwrap_or("");
            if target.is_empty() {
                bail!("Uso: antos disk inspect <dispositivo>");
            }

            println!(
                "\n{} Inspeccionando Dispositivo {}:",
                paint("antOS Almacenamiento ·", BOLD),
                paint(target, CYAN)
            );
            let disk = crate::installer::DiskManager::inspect_disk(target)?;
            match disk {
                Some(d) => {
                    let gb = d.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                    println!("  • Ruta física:       {}", paint(&d.path, BOLD));
                    println!("  • Modelo / Vendor:   {}", d.model);
                    println!(
                        "  • Tamaño total:      {:.2} GB ({} bytes)",
                        gb, d.size_bytes
                    );
                    println!("  • Tamaño de sector:  {} bytes (LBA)", d.sector_size);
                    println!("  • Tipo de Bus:       {}", d.bus_type);
                    println!("  • Tabla:             {}", d.partition_table);
                    println!("  • Solo Lectura:      {}\n", d.is_read_only);

                    println!("  {}:", paint("Mapa de Particiones", BOLD));
                    if d.partitions.is_empty() {
                        println!("    (sin particiones registradas)");
                    } else {
                        for p in &d.partitions {
                            let efi_str = if p.is_efi {
                                paint(" [SISTEMA EFI]", GREEN)
                            } else {
                                "".into()
                            };
                            println!(
                                "    #{}: {} | {:.1} MB | FS: {} | UUID: {}{}",
                                p.number,
                                paint(&p.name, CYAN),
                                p.size_bytes as f64 / (1024.0 * 1024.0),
                                p.fs_type.as_deref().unwrap_or("none"),
                                p.uuid.as_deref().unwrap_or("N/A"),
                                efi_str
                            );
                        }
                    }
                    println!();
                }
                None => {
                    bail!("No se encontró el dispositivo «{}»", target);
                }
            }
        }
        "partition" | "part" => {
            let target = args.get(1).map(String::as_str).unwrap_or("");
            if target.is_empty() {
                bail!("Uso: antos disk partition <dispositivo> [--clean | --dual-boot] [--apply]");
            }

            let clean = args.iter().any(|a| a == "--clean");
            let apply = args.iter().any(|a| a == "--apply");
            let dry_run = !apply;

            println!(
                "\n{} Calculando esquema de particionado para {}:",
                paint("antOS Particionador ·", BOLD),
                paint(target, CYAN)
            );

            let plan = crate::installer::DiskManager::plan_partitioning(target, clean)?;
            let report = crate::installer::DiskManager::apply_partitioning(target, &plan, dry_run)?;
            println!("{}\n", report);
            if dry_run {
                println!("  {} Para aplicar estos cambios en el disco use «--apply» (acción destructiva).\n", paint("Nota:", YELLOW));
            }
        }
        _ => {
            println!(
                "\n{} Gestor de Discos y Particiones GPT:",
                paint("antOS Almacenamiento ·", BOLD)
            );
            println!("  Uso:");
            println!("    antos disk list                           Enumera discos físicos y particiones");
            println!("    antos disk inspect <dispositivo>          Muestra el mapa de particiones y metadatos");
            println!("    antos disk partition <dispositivo> [modo] Calcula o aplica tabla GPT (--clean / --dual-boot)\n");
        }
    }
    Ok(())
}

// ---------------------------------------------------- installer & deploy (T15.2 / T24.4)
