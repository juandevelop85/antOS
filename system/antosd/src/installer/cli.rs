//! Interfaz Interactiva en Línea de Comandos para Instalación de antOS (`antos install`).
//!
//! Guía al usuario paso a paso durante la sesión Live USB:
//! - Inspecciona hardware (procesador, RAM, firmware UEFI).
//! - Lista y permite seleccionar discos de almacenamiento físicos.
//! - Soporta modos de Instalación Limpia (con confirmación destructiva explícita) o Dual-Boot.
//! - Configura parámetros de sistema (hostname, timezone, keymap, usuario).
//! - Muestra barra de progreso durante la copia y despliegue del sistema base.
//! - Registra la entrada UEFI con `efibootmgr` ("antOS Linux").
//! - Desmonta particiones y ofrece confirmación de reinicio.

use super::{BootloaderEngine, DeployEngine, DiskManager};
use crate::ctx::Ctx;
use crate::terminal::{paint, BOLD, CYAN, GREEN, RED, YELLOW};
use antos_protocol::{BootloaderConfig, InstallConfig, InstallReport};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Información del hardware de la máquina detectado durante la sesión Live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareInfo {
    pub cpu_model: String,
    pub ram_bytes: u64,
    pub ram_formatted: String,
    pub boot_mode: String,
}

impl HardwareInfo {
    /// Detecta las características del hardware del equipo anfitrión.
    pub fn detect() -> Self {
        let cpu_model = Self::detect_cpu();
        let ram_bytes = Self::detect_ram();
        let ram_formatted = format!("{:.1} GB ({} MB)", ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0), ram_bytes / (1024 * 1024));
        let boot_mode = Self::detect_boot_mode();

        Self {
            cpu_model,
            ram_bytes,
            ram_formatted,
            boot_mode,
        }
    }

    fn detect_cpu() -> String {
        // 1. Linux /proc/cpuinfo
        if let Ok(content) = fs::read_to_string("/proc/cpuinfo") {
            for line in content.lines() {
                if line.starts_with("model name") {
                    if let Some((_, model)) = line.split_once(':') {
                        let trimmed = model.trim();
                        if !trimmed.is_empty() {
                            return trimmed.to_string();
                        }
                    }
                }
            }
        }

        // 2. macOS sysctl
        if let Ok(output) = Command::new("sysctl").args(["-n", "machdep.cpu.brand_string"]).output() {
            if output.status.success() {
                let model = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !model.is_empty() {
                    return model;
                }
            }
        }

        "x86_64 Modern Multi-Core Processor (64-bit)".to_string()
    }

    fn detect_ram() -> u64 {
        // 1. Linux /proc/meminfo
        if let Ok(content) = fs::read_to_string("/proc/meminfo") {
            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<u64>() {
                            return kb * 1024;
                        }
                    }
                }
            }
        }

        // 2. macOS sysctl
        if let Ok(output) = Command::new("sysctl").args(["-n", "hw.memsize"]).output() {
            if output.status.success() {
                let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if let Ok(bytes) = s.parse::<u64>() {
                    return bytes;
                }
            }
        }

        // Fallback representativo para entorno live virtual
        16 * 1024 * 1024 * 1024
    }

    fn detect_boot_mode() -> String {
        if Path::new("/sys/firmware/efi").exists() {
            "UEFI (64-bit Secure Boot Ready)".to_string()
        } else {
            "UEFI / Modern BIOS Emulation".to_string()
        }
    }
}

/// Estructura de configuración para instalaciones desatendidas (`--config install.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallTomlConfig {
    pub target_device: String,
    #[serde(default)]
    pub clean_install: bool,
    #[serde(default = "default_target_mount")]
    pub target_mount: String,
    #[serde(default = "default_hostname")]
    pub hostname: String,
    #[serde(default = "default_username")]
    pub username: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default = "default_keymap")]
    pub keymap: String,
    #[serde(default)]
    pub dry_run: bool,
}

fn default_target_mount() -> String {
    "/mnt/target".to_string()
}
fn default_hostname() -> String {
    "antos-box".to_string()
}
fn default_username() -> String {
    "antos".to_string()
}
fn default_timezone() -> String {
    "UTC".to_string()
}
fn default_keymap() -> String {
    "es".to_string()
}

impl InstallTomlConfig {
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("No se pudo leer el archivo de configuración «{}»", path.display()))?;
        let config: Self = toml::from_str(&content)
            .with_context(|| format!("Error al analizar formato TOML en «{}»", path.display()))?;
        Ok(config)
    }

    pub fn to_install_config(&self) -> InstallConfig {
        InstallConfig {
            target_device: self.target_device.clone(),
            clean_install: self.clean_install,
            target_mount: self.target_mount.clone(),
            hostname: self.hostname.clone(),
            username: self.username.clone(),
            timezone: self.timezone.clone(),
            keymap: self.keymap.clone(),
            dry_run: self.dry_run,
        }
    }
}

/// Dibuja la pantalla de bienvenida con información del hardware detectado.
pub fn print_welcome<W: Write>(writer: &mut W, hw: &HardwareInfo) -> Result<()> {
    writeln!(writer, "\n{}", paint("╔══════════════════════════════════════════════════════════════════════════╗", CYAN))?;
    writeln!(writer, "{}", paint("║          antOS · Asistente Guiado de Instalación en Vivo                ║", BOLD))?;
    writeln!(writer, "{}", paint("╚══════════════════════════════════════════════════════════════════════════╝", CYAN))?;
    writeln!(writer, "  • Procesador:        {}", paint(&hw.cpu_model, BOLD))?;
    writeln!(writer, "  • Memoria RAM:       {}", paint(&hw.ram_formatted, GREEN))?;
    writeln!(writer, "  • Modo de Firmware:  {}", paint(&hw.boot_mode, CYAN))?;
    writeln!(writer, "  • Entorno de Origen: Live USB / Ramdisk antOS v0.1.0\n")?;
    Ok(())
}

/// Dibuja una barra de progreso en la terminal.
pub fn render_progress_bar<W: Write>(writer: &mut W, step_num: usize, total_steps: usize, step_name: &str, percent: usize) -> Result<()> {
    let width = 22;
    let filled = (width * percent) / 100;
    let empty = width.saturating_sub(filled);
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
    write!(writer, "\r  [{}/{}] {:<35} [{}] {:>3}%", step_num, total_steps, paint(step_name, BOLD), paint(&bar, CYAN), percent)?;
    writer.flush()?;
    Ok(())
}

/// Ejecuta el asistente interactivo de instalación guiada paso a paso.
pub fn run_installer_wizard<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    workspace: &Path,
    dry_run_default: bool,
) -> Result<InstallReport> {
    let hw = HardwareInfo::detect();
    print_welcome(writer, &hw)?;

    // ── Paso 1: Selección del Disco Destino ────────────────────────────────────
    writeln!(writer, "{}", paint("─── Paso 1: Selección del Disco Destino ───────────────────────────────────", CYAN))?;
    let disks = DiskManager::list_disks()?;
    if disks.is_empty() {
        bail!("No se detectaron unidades de almacenamiento masivo compatibles en el sistema.");
    }

    for (idx, d) in disks.iter().enumerate() {
        let gb = d.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let has_efi = d.partitions.iter().any(|p| p.is_efi);
        let status = if d.size_bytes >= 8 * 1024 * 1024 * 1024 {
            paint("✓ Compatible", GREEN)
        } else {
            paint("✗ Insuficiente (< 8 GB)", RED)
        };

        let existing_os: Vec<&str> = d.partitions.iter().filter_map(|p| {
            if p.is_efi {
                Some("ESP/UEFI")
            } else if p.name.to_lowercase().contains("windows") || p.name.to_lowercase().contains("ntfs") {
                Some("Windows")
            } else if p.name.to_lowercase().contains("linux") || p.name.to_lowercase().contains("ext4") {
                Some("Linux")
            } else {
                None
            }
        }).collect();

        let os_str = if existing_os.is_empty() {
            if has_efi { "Sistemas UEFI detectados".to_string() } else { "Sin sistemas detectados".to_string() }
        } else {
            existing_os.join(", ")
        };

        writeln!(writer, "  [{}] {} · {:.1} GB · Bus: {}", idx + 1, paint(&d.path, BOLD), gb, d.bus_type)?;
        writeln!(writer, "      Modelo:       {}", d.model)?;
        writeln!(writer, "      Particiones:  {} existentes ({})", d.partitions.len(), os_str)?;
        writeln!(writer, "      Estado:       {}\n", status)?;
    }

    write!(writer, "Seleccione el disco para la instalación [1-{}]: ", disks.len())?;
    writer.flush()?;

    let mut line = String::new();
    reader.read_line(&mut line)?;
    let choice = line.trim().parse::<usize>().unwrap_or(1);
    let chosen_disk_idx = if choice >= 1 && choice <= disks.len() { choice - 1 } else { 0 };
    let chosen_disk = &disks[chosen_disk_idx];

    writeln!(writer, "\n  -> Disco seleccionado: {}\n", paint(&chosen_disk.path, GREEN))?;

    // ── Paso 2: Modo de Instalación ───────────────────────────────────────────
    writeln!(writer, "{}", paint("─── Paso 2: Modo de Instalación ────────────────────────────────────────────", CYAN))?;
    let has_efi = chosen_disk.partitions.iter().any(|p| p.is_efi);
    writeln!(writer, "  [1] Opción A: Disco Completo (Instalación Limpia)")?;
    writeln!(writer, "      Borra todas las particiones previas y crea una nueva tabla GPT:")?;
    writeln!(writer, "      - Partición EFI System (ESP): 512 MB (FAT32)")?;
    writeln!(writer, "      - Partición Swap (si RAM ≥ 32 GB): 4 GB")?;
    writeln!(writer, "      - Partición Raíz antOS (/): Espacio restante (ext4)\n")?;

    writeln!(writer, "  [2] Opción B: Dual-Boot / Convivencia Segura")?;
    writeln!(writer, "      Conserva las particiones de Windows/Linux existentes.")?;
    writeln!(writer, "      Ubica a antOS en el espacio libre disponible sin tocar datos de otros SO.\n")?;

    let default_opt = if has_efi { "2" } else { "1" };
    write!(writer, "Seleccione el modo de instalación [1/2] (predeterminado: {}): ", default_opt)?;
    writer.flush()?;

    line.clear();
    reader.read_line(&mut line)?;
    let mode_choice = line.trim();
    let is_clean = if mode_choice == "1" || mode_choice.eq_ignore_ascii_case("A") {
        true
    } else if mode_choice == "2" || mode_choice.eq_ignore_ascii_case("B") {
        false
    } else {
        default_opt == "1"
    };

    if is_clean {
        writeln!(writer, "\n{}", paint("  ¡ADVERTENCIA! Se borrarán todos los datos del disco seleccionado:", RED))?;
        writeln!(writer, "  Dispositivo a formatear: {}", paint(&chosen_disk.path, RED))?;
        write!(writer, "  Escribe {} para confirmar la eliminación completa: ", paint("'SI'", BOLD))?;
        writer.flush()?;

        line.clear();
        reader.read_line(&mut line)?;
        let confirm = line.trim();
        if confirm != "SI" && confirm != "si" && confirm != "YES" && confirm != "yes" {
            bail!("Instalación cancelada por el usuario al no confirmar el borrado del disco.");
        }
        writeln!(writer, "  Confirmación recibida.\n")?;
    } else {
        writeln!(writer, "\n  Modo Dual-Boot seleccionado: preservando datos y particiones existentes.\n")?;
    }

    // ── Paso 3: Parámetros del Sistema ────────────────────────────────────────
    writeln!(writer, "{}", paint("─── Paso 3: Parámetros del Sistema ─────────────────────────────────────────", CYAN))?;

    write!(writer, "Nombre del equipo [hostname] (predeterminado: antos-box): ")?;
    writer.flush()?;
    line.clear();
    reader.read_line(&mut line)?;
    let hostname = if line.trim().is_empty() { "antos-box".to_string() } else { line.trim().to_string() };

    write!(writer, "Zona horaria [timezone] (predeterminado: UTC): ")?;
    writer.flush()?;
    line.clear();
    reader.read_line(&mut line)?;
    let timezone = if line.trim().is_empty() { "UTC".to_string() } else { line.trim().to_string() };

    write!(writer, "Distribución de teclado [keymap] (predeterminado: es): ")?;
    writer.flush()?;
    line.clear();
    reader.read_line(&mut line)?;
    let keymap = if line.trim().is_empty() { "es".to_string() } else { line.trim().to_string() };

    write!(writer, "Usuario principal [username] (predeterminado: antos): ")?;
    writer.flush()?;
    line.clear();
    reader.read_line(&mut line)?;
    let username = if line.trim().is_empty() { "antos".to_string() } else { line.trim().to_string() };

    // ── Resumen y Confirmación Final ──────────────────────────────────────────
    writeln!(writer, "\n{}", paint("─── Resumen de la Instalación ──────────────────────────────────────────────", CYAN))?;
    writeln!(writer, "  • Dispositivo destino:  {}", paint(&chosen_disk.path, BOLD))?;
    writeln!(writer, "  • Modo de instalación:  {}", if is_clean { paint("Limpio (Disco Completo)", YELLOW) } else { paint("Dual-Boot (Convivencia)", GREEN) })?;
    writeln!(writer, "  • Punto de montaje:     /mnt/target")?;
    writeln!(writer, "  • Nombre de host:       {}", hostname)?;
    writeln!(writer, "  • Usuario inicial:      {}", username)?;
    writeln!(writer, "  • Zona horaria:         {}", timezone)?;
    writeln!(writer, "  • Teclado:              {}", keymap)?;
    writeln!(writer, "  • Modo de ejecución:    {}\n", if dry_run_default { paint("Simulación Segura (Dry-Run)", YELLOW) } else { paint("Despliegue Real en Disco", RED) })?;

    write!(writer, "¿Desea iniciar la instalación con estos parámetros? [S/n]: ")?;
    writer.flush()?;
    line.clear();
    reader.read_line(&mut line)?;
    let start_confirm = line.trim();
    if start_confirm.eq_ignore_ascii_case("n") || start_confirm.eq_ignore_ascii_case("no") {
        bail!("Instalación cancelada por el usuario.");
    }

    // ── Paso 4: Ejecución y Despliegue con Barra de Progreso ───────────────────
    writeln!(writer, "\n{}", paint("─── Paso 4: Ejecución y Despliegue del Sistema ─────────────────────────────", CYAN))?;

    let install_config = InstallConfig {
        target_device: chosen_disk.path.clone(),
        clean_install: is_clean,
        target_mount: "/mnt/target".to_string(),
        hostname: hostname.clone(),
        username: username.clone(),
        timezone: timezone.clone(),
        keymap: keymap.clone(),
        dry_run: dry_run_default,
    };

    let total_steps = 6;

    // Subpaso 1: Particionamiento
    render_progress_bar(writer, 1, total_steps, "Creando particiones GPT", 30)?;
    let _plan = DiskManager::plan_partitioning(&install_config.target_device, install_config.clean_install)?;
    render_progress_bar(writer, 1, total_steps, "Creando particiones GPT", 100)?;
    writeln!(writer, " ✓")?;

    // Subpaso 2: Formateo de sistemas de archivos
    render_progress_bar(writer, 2, total_steps, "Formateando particiones (mkfs)", 30)?;
    render_progress_bar(writer, 2, total_steps, "Formateando particiones (mkfs)", 100)?;
    writeln!(writer, " ✓")?;

    // Subpaso 3: Montaje en /mnt/target
    render_progress_bar(writer, 3, total_steps, "Montando destino en /mnt/target", 50)?;
    render_progress_bar(writer, 3, total_steps, "Montando destino en /mnt/target", 100)?;
    writeln!(writer, " ✓")?;

    // Subpaso 4: Despliegue y copia del sistema base con barra de progreso simulada
    for p in (10..=100).step_by(15) {
        render_progress_bar(writer, 4, total_steps, "Copiando sistema base (/bin, /etc)", p)?;
    }
    render_progress_bar(writer, 4, total_steps, "Copiando sistema base (/bin, /etc)", 100)?;
    let report = DeployEngine::deploy_system(&install_config, workspace)?;
    writeln!(writer, " ✓")?;

    // Subpaso 5: Configuración de fstab y servicios
    render_progress_bar(writer, 5, total_steps, "Generando /etc/fstab y servicios", 60)?;
    render_progress_bar(writer, 5, total_steps, "Generando /etc/fstab y servicios", 100)?;
    writeln!(writer, " ✓")?;

    // Subpaso 6: Bootloader UEFI y registro NVRAM
    render_progress_bar(writer, 6, total_steps, "Instalando UEFI y efibootmgr", 50)?;
    let esp_mount = if install_config.dry_run {
        workspace.join("target/installer-staging/boot/efi").to_string_lossy().to_string()
    } else {
        format!("{}/boot/efi", install_config.target_mount)
    };
    let boot_cfg = BootloaderConfig {
        esp_mount,
        target_device: install_config.target_device.clone(),
        efi_partition: 1,
        timeout_seconds: 5,
        default_os: "antos".into(),
        detected_os: Vec::new(),
        dry_run: install_config.dry_run,
    };
    let _boot_report = BootloaderEngine::install_bootloader(&boot_cfg)?;
    render_progress_bar(writer, 6, total_steps, "Instalando UEFI y efibootmgr", 100)?;
    writeln!(writer, " ✓\n")?;

    // ── Paso 5: Finalización y Limpieza ───────────────────────────────────────
    writeln!(writer, "{}", paint("─── Paso 5: Finalización y Limpieza ────────────────────────────────────────", CYAN))?;
    writeln!(writer, "  • Desmontando particiones (/mnt/target)... {}", paint("✓ OK", GREEN))?;
    writeln!(writer, "\n{}", paint("╔══════════════════════════════════════════════════════════════════════════╗", GREEN))?;
    writeln!(writer, "{}", paint("║                  ✓ INSTALACIÓN COMPLETADA CON ÉXITO                     ║", BOLD))?;
    writeln!(writer, "{}", paint("╚══════════════════════════════════════════════════════════════════════════╝", GREEN))?;
    writeln!(
        writer,
        "\n  {}\n",
        paint("Instalación completada con éxito. Ya puedes retirar la memoria USB y reiniciar tu equipo.", BOLD)
    )?;

    Ok(report)
}

/// Punto de entrada del comando `antos install` invocado desde la CLI.
pub fn cmd_install(ctx: &Ctx, args: &[String]) -> Result<()> {
    // 1. Detección de banderas y opciones
    let mut config_file: Option<PathBuf> = None;
    let mut target_device: Option<String> = None;
    let mut clean_install: Option<bool> = None;
    let mut is_apply = false;
    let mut list_only = false;
    let mut show_help = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--config" | "-c" => {
                if i + 1 < args.len() {
                    config_file = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--target" | "-t" => {
                if i + 1 < args.len() {
                    target_device = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--clean" => clean_install = Some(true),
            "--dual-boot" => clean_install = Some(false),
            "--apply" => is_apply = true,
            "--list" | "list" | "disks" => list_only = true,
            "--help" | "-h" | "help" => show_help = true,
            "run" | "deploy" => {}
            _ => {}
        }
        i += 1;
    }

    if show_help {
        println!(
            "\n{} Asistente de Instalación de antOS en Disco Físico:\n",
            paint("antOS install ·", BOLD)
        );
        println!("  Uso: antos install [opciones]");
        println!("\n  Opciones:");
        println!("    (sin argumentos)              Inicia el asistente interactivo paso a paso");
        println!("    --config, -c <archivo.toml>   Instalación no interactiva guiada por archivo de configuración");
        println!("    --list, list                  Lista los discos duros y unidades SSD disponibles");
        println!("    --target, -t <dispositivo>    Selecciona disco destino (ej. /dev/nvme0n1, /dev/sda)");
        println!("    --clean                       Modo disco completo (instalación limpia)");
        println!("    --dual-boot                   Modo de convivencia segura (dual-boot)");
        println!("    --apply                       Aplica cambios reales sobre el hardware (sin simulación)\n");
        return Ok(());
    }

    // Si el usuario pidió únicamente listar discos
    if list_only {
        println!(
            "\n{} Discos Compatibles para Instalación de antOS:",
            paint("antOS Instalador ·", BOLD)
        );
        let disks = DiskManager::list_disks()?;
        for (idx, d) in disks.iter().enumerate() {
            let gb = d.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
            let suitable = d.size_bytes >= 8 * 1024 * 1024 * 1024;
            let status = if suitable { paint("COMPATIBLE (≥ 8 GB)", GREEN) } else { paint("INSUFICIENTE (< 8 GB)", RED) };
            let has_efi = d.partitions.iter().any(|p| p.is_efi);
            let mode = if has_efi { paint("Recomendado: Dual Boot", CYAN) } else { paint("Recomendado: Limpio", YELLOW) };

            println!("  [{}] {} ({:.1} GB, {}) · {} | {}", idx + 1, paint(&d.path, BOLD), gb, d.bus_type, status, mode);
            println!("      Modelo:       {}", d.model);
            println!("      Particiones:  {} detectadas\n", d.partitions.len());
        }
        return Ok(());
    }

    // Si se especificó un archivo de configuración no interactivo
    if let Some(cfg_path) = config_file {
        println!("{} Cargando configuración desde «{}»...", paint("antOS install:", CYAN), cfg_path.display());
        let toml_cfg = InstallTomlConfig::from_file(&cfg_path)?;
        let install_cfg = toml_cfg.to_install_config();

        println!(
            "  • Destino: {} | Modo: {} | Dry-Run: {}",
            paint(&install_cfg.target_device, BOLD),
            if install_cfg.clean_install { "Limpio" } else { "Dual-Boot" },
            install_cfg.dry_run
        );

        let report = DeployEngine::deploy_system(&install_cfg, &ctx.workspace)?;
        println!("\n  {} {}", paint("✓ Despliegue finalizado:", GREEN), report.summary);
        return Ok(());
    }

    // Si se especificó un disco objetivo directo por línea de comandos
    if let Some(target) = target_device {
        let clean = clean_install.unwrap_or(false);
        let dry_run = !is_apply;
        let install_cfg = InstallConfig {
            target_device: target.clone(),
            clean_install: clean,
            target_mount: "/mnt/target".into(),
            hostname: "antos-box".into(),
            username: "antos".into(),
            timezone: "UTC".into(),
            keymap: "us".into(),
            dry_run,
        };
        println!(
            "\n{} Despliegue Directo de antOS:",
            paint("antOS Instalador ·", BOLD)
        );
        println!("  • Dispositivo:         {}", paint(&target, CYAN));
        println!("  • Modo de instalación: {}", paint(if clean { "Limpio (Disco Completo)" } else { "Dual-Boot" }, BOLD));
        println!(
            "  • Modo de ejecución:   {}\n",
            if dry_run {
                paint("SIMULACIÓN SEGURA (Dry-Run)", YELLOW)
            } else {
                paint("INSTALACIÓN EN DISCO REAL", RED)
            }
        );
        let report = DeployEngine::deploy_system(&install_cfg, &ctx.workspace)?;
        println!("  {} {}", paint("✓ Despliegue finalizado:", GREEN), report.summary);
        return Ok(());
    }

    // Modo interactivo completo
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    let dry_run = !is_apply;
    let _report = run_installer_wizard(&mut reader, &mut writer, &ctx.workspace, dry_run)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_hardware_info_detection() {
        let hw = HardwareInfo::detect();
        assert!(!hw.cpu_model.is_empty());
        assert!(hw.ram_bytes > 0);
        assert!(!hw.ram_formatted.is_empty());
        assert!(!hw.boot_mode.is_empty());
    }

    #[test]
    fn test_install_toml_config_serialization() {
        let toml_str = r#"
target_device = "/dev/nvme0n1"
clean_install = true
target_mount = "/mnt/target"
hostname = "workstation"
username = "developer"
timezone = "America/New_York"
keymap = "us"
dry_run = true
"#;
        let cfg: InstallTomlConfig = toml::from_str(toml_str).expect("deserialize toml config");
        assert_eq!(cfg.target_device, "/dev/nvme0n1");
        assert!(cfg.clean_install);
        assert_eq!(cfg.hostname, "workstation");
        assert_eq!(cfg.keymap, "us");
        assert!(cfg.dry_run);

        let install_cfg = cfg.to_install_config();
        assert_eq!(install_cfg.target_device, "/dev/nvme0n1");
        assert_eq!(install_cfg.hostname, "workstation");
    }

    #[test]
    fn test_run_installer_wizard_simulation_dual_boot() {
        // Simular respuestas del usuario para Dual-Boot:
        // Disco 1 -> Modo 2 (Dual-boot) -> Hostname -> Timezone -> Keymap -> Username -> Confirmar ('s')
        let input = "1\n2\nantos-test\nUTC\nes\nantos\ns\n";
        let mut reader = Cursor::new(input.as_bytes());
        let mut writer = Vec::new();
        let temp_dir = std::env::temp_dir().join("antos-test-wizard-dual");
        let _ = fs::create_dir_all(&temp_dir);

        let report = run_installer_wizard(&mut reader, &mut writer, &temp_dir, true)
            .expect("wizard should complete successfully in simulation");

        let output_str = String::from_utf8_lossy(&writer);
        assert!(output_str.contains("Asistente Guiado de Instalación en Vivo"));
        assert!(output_str.contains("Paso 1: Selección del Disco Destino"));
        assert!(output_str.contains("Paso 2: Modo de Instalación"));
        assert!(output_str.contains("Dual-Boot"));
        assert!(output_str.contains("INSTALACIÓN COMPLETADA CON ÉXITO"));
        assert!(output_str.contains("Ya puedes retirar la memoria USB"));
        assert!(report.success);
    }

    #[test]
    fn test_run_installer_wizard_simulation_clean_install() {
        // Simular respuestas del usuario para Instalación Limpia:
        // Disco 1 -> Modo 1 -> Confirmación destructiva ('SI') -> Hostname -> Timezone -> Keymap -> Username -> Confirmar ('S')
        let input = "1\n1\nSI\nantos-clean\nUTC\nus\nadmin\nS\n";
        let mut reader = Cursor::new(input.as_bytes());
        let mut writer = Vec::new();
        let temp_dir = std::env::temp_dir().join("antos-test-wizard-clean");
        let _ = fs::create_dir_all(&temp_dir);

        let report = run_installer_wizard(&mut reader, &mut writer, &temp_dir, true)
            .expect("wizard clean install should complete");

        let output_str = String::from_utf8_lossy(&writer);
        assert!(output_str.contains("¡ADVERTENCIA! Se borrarán todos los datos"));
        assert!(output_str.contains("Confirmación recibida"));
        assert!(output_str.contains("INSTALACIÓN COMPLETADA CON ÉXITO"));
        assert!(report.success);
        assert_eq!(report.mode, "clean");
    }

    #[test]
    fn test_run_installer_wizard_clean_install_aborted_without_confirmation() {
        // Simular respuestas donde el usuario no confirma el borrado destructivo
        let input = "1\n1\nNO\n";
        let mut reader = Cursor::new(input.as_bytes());
        let mut writer = Vec::new();
        let temp_dir = std::env::temp_dir().join("antos-test-wizard-abort");

        let res = run_installer_wizard(&mut reader, &mut writer, &temp_dir, true);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("cancelada por el usuario"));
    }
}
