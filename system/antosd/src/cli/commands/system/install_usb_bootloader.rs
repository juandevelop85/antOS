//! `antos install`, `antos usb` y `antos bootloader`.
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

pub fn cmd_install(ctx: &Ctx, args: &[String]) -> Result<()> {
    crate::installer::cli::cmd_install(ctx, args)
}

// ---------------------------------------------------- live usb & flash (T24.5)

pub fn cmd_usb(ctx: &Ctx, args: &[String]) -> Result<()> {
    crate::installer::usb::cmd_usb(ctx, args)
}

// ---------------------------------------------------- bootloader & uefi (T15.3)

/// Argumentos de `antos bootloader probe` (T31.13).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BootloaderProbeArgs {
    pub esp_path: String,
}

/// Analiza los argumentos de `antos bootloader probe [--esp <ruta>]`, sin
/// efectos secundarios, para poder probar el análisis por separado de la
/// sonda real de sistemas operativos (T31.13).
fn parse_bootloader_probe_args(args: &[String]) -> BootloaderProbeArgs {
    let mut esp_path = "/boot/efi".to_string();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--esp" && i + 1 < args.len() {
            esp_path = args[i + 1].clone();
            i += 1;
        }
        i += 1;
    }
    BootloaderProbeArgs { esp_path }
}

/// Argumentos de `antos bootloader install` (T31.13).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BootloaderInstallArgs {
    pub esp_path: String,
    pub target_device: String,
    pub efi_partition: u32,
    pub timeout_seconds: u32,
    pub apply: bool,
    /// `--bare-metal`: disposición de la ESP del kernel `no_std` (T36.1);
    /// sin él, la vía antOS Linux, que solo sondea.
    pub bare_metal: bool,
    /// `--efi-binary <ruta>`: binario EFI real para la vía bare-metal.
    pub efi_binary: Option<String>,
}

/// Analiza los argumentos de `antos bootloader install`. Los valores
/// numéricos que no se pueden interpretar (`--partition abc`) se ignoran en
/// silencio y conservan su valor por defecto — comportamiento preexistente
/// que esta extracción no cambia, solo hace comprobable (T31.13).
fn parse_bootloader_install_args(args: &[String]) -> BootloaderInstallArgs {
    let mut esp_path = "/boot".to_string();
    let mut target_device = "/dev/nvme0n1".to_string();
    let mut efi_partition = 1u32;
    let mut timeout_seconds = 5u32;
    let mut apply = false;
    let mut bare_metal = false;
    let mut efi_binary: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--esp" => {
                if i + 1 < args.len() {
                    esp_path = args[i + 1].clone();
                    i += 1;
                }
            }
            "--target" | "-t" => {
                if i + 1 < args.len() {
                    target_device = args[i + 1].clone();
                    i += 1;
                }
            }
            "--partition" | "-p" => {
                if i + 1 < args.len() {
                    if let Ok(num) = args[i + 1].parse::<u32>() {
                        efi_partition = num;
                    }
                    i += 1;
                }
            }
            "--timeout" => {
                if i + 1 < args.len() {
                    if let Ok(num) = args[i + 1].parse::<u32>() {
                        timeout_seconds = num;
                    }
                    i += 1;
                }
            }
            "--apply" => apply = true,
            "--bare-metal" => bare_metal = true,
            "--efi-binary" if i + 1 < args.len() => {
                efi_binary = Some(args[i + 1].clone());
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    BootloaderInstallArgs {
        esp_path,
        target_device,
        efi_partition,
        timeout_seconds,
        apply,
        bare_metal,
        efi_binary,
    }
}

pub fn cmd_bootloader(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");

    match sub {
        "probe" | "detect" | "os" => {
            let BootloaderProbeArgs { esp_path } = parse_bootloader_probe_args(args);

            println!(
                "\n{} Sondeando sistemas operativos en «{}»:",
                paint("antOS Bootloader ·", BOLD),
                esp_path
            );
            let entries = crate::installer::BootloaderEngine::probe_operating_systems(
                std::path::Path::new(&esp_path),
            )?;
            if entries.is_empty() {
                println!(
                    "  (no se detectaron sistemas operativos en el directorio especificado)\n"
                );
                return Ok(());
            }

            for (idx, os) in entries.iter().enumerate() {
                let badge = match os.os_type.as_str() {
                    "windows" => paint("[WINDOWS]", CYAN),
                    "linux" => paint("[LINUX]", GREEN),
                    "macos" => paint("[MACOS]", YELLOW),
                    _ => paint("[ANTOS]", BOLD),
                };
                println!("  [{}] {} {}", idx + 1, badge, paint(&os.name, BOLD));
                println!("      Ruta binario EFI:  {}", os.efi_path);
                println!(
                    "      Dispositivo/Part:  {} (Partición #{})\n",
                    os.disk_device, os.partition_number
                );
            }
        }
        "install" | "deploy" => {
            let BootloaderInstallArgs {
                esp_path,
                target_device,
                efi_partition,
                timeout_seconds,
                apply,
                bare_metal,
                efi_binary,
            } = parse_bootloader_install_args(args);
            let dry_run = !apply;
            let target = if bare_metal {
                antos_protocol::BootTarget::BareMetal
            } else {
                antos_protocol::BootTarget::NixOs
            };

            let esp = std::path::PathBuf::from(if dry_run {
                ctx.workspace
                    .join("target/esp-staging")
                    .display()
                    .to_string()
            } else {
                esp_path.clone()
            });

            let config = antos_protocol::BootloaderConfig {
                esp_mount: esp.display().to_string(),
                target_device: target_device.clone(),
                efi_partition,
                default_os: "antos".into(),
                timeout_seconds,
                detected_os: Vec::new(),
                dry_run,
                target,
                efi_binary,
            };

            println!(
                "\n{} Gestor de Arranque UEFI ({}):",
                paint("antOS Bootloader ·", BOLD),
                match target {
                    antos_protocol::BootTarget::NixOs =>
                        "antOS Linux · systemd-boot lo instala nixos-install; aquí solo se sondea la ESP",
                    antos_protocol::BootTarget::BareMetal =>
                        "bare-metal · disposición de la ESP y registro NVRAM",
                }
            );
            println!("  • Directorio ESP:     {}", paint(&config.esp_mount, CYAN));
            println!("  • Dispositivo destino: {}", paint(&target_device, CYAN));
            println!("  • Partición EFI:      #{}", efi_partition);
            println!("  • Timeout menú:       {} segundos", timeout_seconds);
            println!(
                "  • Modo de ejecución:  {}\n",
                if dry_run {
                    paint("SIMULACIÓN SEGURA (Dry-Run)", YELLOW)
                } else {
                    paint("ESCRITURA EN ESP Y NVRAM", RED)
                }
            );

            let report = crate::installer::BootloaderEngine::install_bootloader(&config)?;
            println!(
                "  {}: {}",
                paint("Resultado", BOLD),
                if report.success {
                    paint("EXITOSO", GREEN)
                } else {
                    paint("FALLIDO", RED)
                }
            );
            println!("  {}\n", report.summary);
            println!("  {}:", paint("Entradas de Arranque Generadas", BOLD));
            for e in &report.entries_configured {
                println!("    ✓ {}", e);
            }
            println!("\n  {}:", paint("Comando de Registro NVRAM", BOLD));
            println!("    {}\n", paint(&report.efibootmgr_command, CYAN));

            if dry_run {
                println!(
                    "  {} Para aplicar estos cambios en el firmware UEFI use «--apply».\n",
                    paint("Nota:", YELLOW)
                );
            }
        }
        _ => {
            println!(
                "\n{} Gestor de Arranque UEFI y Dual Boot:",
                paint("antOS Bootloader ·", BOLD)
            );
            println!("  Uso:");
            println!("    antos bootloader probe [--esp <ruta>]        Sondea sistemas operativos instalados");
            println!("    antos bootloader install [--esp <ruta>]      antOS Linux: sondea la ESP (el gestor lo pone nixos-install)");
            println!("    antos bootloader install --bare-metal        Kernel bare-metal: simula la disposición de la ESP");
            println!("    antos bootloader install --bare-metal --efi-binary <antos.efi> --apply");
            println!("                                                 Escribe la ESP y registra la NVRAM con efibootmgr (binario real obligatorio)\n");
        }
    }
    Ok(())
}

// ----------------------------------------------------------------- microvms

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn args_of(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // -- antos bootloader probe ------------------------------------------

    #[test]
    fn test_parse_bootloader_probe_args_reads_the_esp_flag() {
        let args = args_of(&["probe", "--esp", "/mnt/esp"]);
        let parsed = parse_bootloader_probe_args(&args);
        assert_eq!(parsed.esp_path, "/mnt/esp");
    }

    #[test]
    fn test_parse_bootloader_probe_args_defaults_when_no_flag_is_given() {
        let args = args_of(&["probe"]);
        let parsed = parse_bootloader_probe_args(&args);
        assert_eq!(parsed.esp_path, "/boot/efi");
    }

    #[test]
    fn test_parse_bootloader_probe_args_ignores_a_trailing_flag_with_no_value() {
        // Ejercita el límite exacto `i + 1 < args.len()`: «--esp» como último
        // argumento no debe leer fuera de rango ni entrar en pánico.
        let args = args_of(&["probe", "--esp"]);
        let parsed = parse_bootloader_probe_args(&args);
        assert_eq!(parsed.esp_path, "/boot/efi");
    }

    // -- antos bootloader install -----------------------------------------

    #[test]
    fn test_parse_bootloader_install_args_reads_every_flag() {
        let args = args_of(&[
            "install",
            "--esp",
            "/mnt/esp",
            "--target",
            "/dev/sda",
            "--partition",
            "2",
            "--timeout",
            "10",
            "--apply",
            "--bare-metal",
            "--efi-binary",
            "/tmp/antos.efi",
        ]);
        let parsed = parse_bootloader_install_args(&args);
        assert_eq!(
            parsed,
            BootloaderInstallArgs {
                esp_path: "/mnt/esp".into(),
                target_device: "/dev/sda".into(),
                efi_partition: 2,
                timeout_seconds: 10,
                apply: true,
                bare_metal: true,
                efi_binary: Some("/tmp/antos.efi".into()),
            }
        );
    }

    /// Sin banderas: la vía antOS Linux (solo sondeo) con la ESP en `/boot`
    /// (T36.1). La disposición bare-metal ya no es el camino por defecto.
    #[test]
    fn test_parse_bootloader_install_args_defaults_to_nixos_path() {
        let parsed = parse_bootloader_install_args(&args_of(&["install"]));
        assert_eq!(parsed.esp_path, "/boot");
        assert!(!parsed.bare_metal);
        assert!(parsed.efi_binary.is_none());
        assert!(!parsed.apply);
    }

    #[test]
    fn test_parse_bootloader_install_args_keeps_defaults_on_unparseable_numbers() {
        // «--partition abc» no es un u32 válido: el valor por defecto se
        // conserva en silencio, comportamiento preexistente que esta prueba
        // fija (T31.13).
        let args = args_of(&["install", "--partition", "abc", "--timeout", "nope"]);
        let parsed = parse_bootloader_install_args(&args);
        assert_eq!(parsed.efi_partition, 1);
        assert_eq!(parsed.timeout_seconds, 5);
        assert!(!parsed.apply);
    }

    #[test]
    fn test_parse_bootloader_install_args_ignores_a_trailing_flag_with_no_value() {
        let args = args_of(&["install", "--target"]);
        let parsed = parse_bootloader_install_args(&args);
        assert_eq!(parsed.target_device, "/dev/nvme0n1");
    }
}
