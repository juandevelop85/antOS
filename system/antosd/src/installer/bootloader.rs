//! Gestor de Arranque UEFI y Dual Boot Automatizado (T15.3 / T36.1).
//!
//! ## Estado de implementación
//!
//! Dos vías, elegidas por [`BootTarget`] en [`BootloaderConfig::target`]:
//!
//! - **`NixOs` (antOS Linux, por defecto):** este módulo **no instala
//!   ningún gestor**. `systemd-boot` lo escribe en la ESP y lo registra en
//!   la NVRAM `nixos-install` a partir del `configuration.nix` generado
//!   (`boot.loader.systemd-boot.enable`). Aquí solo se sondea la ESP
//!   ([`BootloaderEngine::probe_operating_systems`]) para informar de los
//!   sistemas vecinos que `systemd-boot` encadenará. No se escribe ningún
//!   fichero ni se invoca `efibootmgr`.
//! - **`BareMetal` (kernel `no_std`, Vía B):** se escribe la disposición
//!   completa —`EFI/antOS/antos.efi`, `EFI/BOOT/BOOTX64.EFI`,
//!   `loader/loader.conf`, `loader/entries/*.conf`— y, fuera de `dry_run`,
//!   se registra la entrada con `efibootmgr`. El binario EFI tiene que
//!   venir de fuera ([`BootloaderConfig::efi_binary`]): fuera de `dry_run`
//!   **nunca** se fabrica un stub. Hasta T36.1 este camino escribía
//!   `\x7fELF antOS UEFI Stub Binary` como `antos.efi` y lo registraba en la
//!   NVRAM también en instalaciones reales, dejando una entrada de arranque
//!   que no arranca.
//!
//! Los generadores de texto (`generate_loader_conf`, `generate_*_entry`)
//! son puros y sirven a ambas vías.

use antos_protocol::{BootTarget, BootloaderConfig, BootloaderReport, OsEntry};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub struct BootloaderEngine;

impl BootloaderEngine {
    /// Sondea los sistemas operativos vecinos presentes en el directorio o partición ESP.
    pub fn probe_operating_systems(esp_path: &Path) -> Result<Vec<OsEntry>> {
        let mut entries = Vec::new();

        // 1. Si existe directorio EFI en esp_path, escanear rutas canónicas
        let efi_dir = if esp_path.join("EFI").exists() {
            esp_path.join("EFI")
        } else if esp_path.join("efi").exists() {
            esp_path.join("efi")
        } else {
            esp_path.to_path_buf()
        };

        if efi_dir.exists() {
            // Windows Boot Manager
            let win_path = efi_dir.join("Microsoft/Boot/bootmgfw.efi");
            if win_path.exists() {
                entries.push(OsEntry {
                    name: "Windows Boot Manager".into(),
                    os_type: "windows".into(),
                    efi_path: "\\EFI\\Microsoft\\Boot\\bootmgfw.efi".into(),
                    disk_device: "/dev/nvme0n1".into(),
                    partition_number: 1,
                });
            }

            // Ubuntu / Debian
            let ubuntu_path = efi_dir.join("ubuntu/grubx64.efi");
            if ubuntu_path.exists() {
                entries.push(OsEntry {
                    name: "Ubuntu Linux (GRUB)".into(),
                    os_type: "linux".into(),
                    efi_path: "\\EFI\\ubuntu\\grubx64.efi".into(),
                    disk_device: "/dev/nvme0n1".into(),
                    partition_number: 1,
                });
            }

            // Fedora
            let fedora_path = efi_dir.join("fedora/grubx64.efi");
            if fedora_path.exists() {
                entries.push(OsEntry {
                    name: "Fedora Linux (GRUB)".into(),
                    os_type: "linux".into(),
                    efi_path: "\\EFI\\fedora\\grubx64.efi".into(),
                    disk_device: "/dev/nvme0n1".into(),
                    partition_number: 1,
                });
            }

            // Arch Linux
            let arch_path = efi_dir.join("arch/grubx64.efi");
            if arch_path.exists() {
                entries.push(OsEntry {
                    name: "Arch Linux (GRUB)".into(),
                    os_type: "linux".into(),
                    efi_path: "\\EFI\\arch\\grubx64.efi".into(),
                    disk_device: "/dev/nvme0n1".into(),
                    partition_number: 1,
                });
            }

            // antOS
            let antos_path = efi_dir.join("antOS/antos.efi");
            if antos_path.exists() {
                entries.push(OsEntry {
                    name: "antOS v0.1.0".into(),
                    os_type: "antos".into(),
                    efi_path: "\\EFI\\antOS\\antos.efi".into(),
                    disk_device: "/dev/nvme0n1".into(),
                    partition_number: 1,
                });
            }
        }

        // Una ESP sin nada reconocible devuelve una lista vacía. Hasta T36.1
        // aquí se inventaban un «Windows Boot Manager» y un «antOS» de
        // relleno, y el informe de instalación anunciaba «2 sistemas
        // vecinos» sobre una ESP vacía.
        Ok(entries)
    }

    /// Genera la configuración principal de `systemd-boot` (`loader/loader.conf`).
    pub fn generate_loader_conf(timeout: u32, default_entry: &str) -> String {
        let mut lines = Vec::new();
        lines.push("# /loader/loader.conf: antOS UEFI systemd-boot configuration".into());
        lines.push(format!("default      {}", default_entry));
        lines.push(format!("timeout      {}", timeout));
        lines.push("console-mode max".into());
        lines.push("editor       no".into());
        lines.push("auto-entries 1".into());
        lines.push("auto-firmware 1".into());
        lines.push("".into());
        lines.join("\n")
    }

    /// Genera la entrada de arranque para antOS (`loader/entries/antos.conf`).
    pub fn generate_antos_entry(kernel_path: &str, initrd_path: &str, root_uuid: &str) -> String {
        let mut lines = Vec::new();
        lines.push("title    antOS v0.1.0 (Developer Native AI OS)".into());
        lines.push("version  0.1.0-release".into());
        lines.push(format!("linux    {}", kernel_path));
        lines.push(format!("initrd   {}", initrd_path));
        lines.push(format!(
            "options  root=UUID={} rw quiet loglevel=3 console=tty1 console=ttyS0,115200 antos.mode=native",
            root_uuid
        ));
        lines.push("".into());
        lines.join("\n")
    }

    /// Genera la entrada de arranque para Windows Boot Manager (`loader/entries/windows.conf`).
    pub fn generate_windows_entry(efi_path: &str) -> String {
        let mut lines = Vec::new();
        lines.push("title Windows Boot Manager".into());
        lines.push(format!("efi   {}", efi_path.replace('\\', "/")));
        lines.push("".into());
        lines.join("\n")
    }

    /// Genera la entrada de arranque para otra distribución Linux vecina.
    pub fn generate_linux_entry(title: &str, efi_path: &str) -> String {
        let mut lines = Vec::new();
        lines.push(format!("title {}", title));
        lines.push(format!("efi   {}", efi_path.replace('\\', "/")));
        lines.push("".into());
        lines.join("\n")
    }

    /// Instala el bootloader en la partición ESP y registra la entrada NVRAM UEFI.
    /// Despliega (o simula) el gestor de arranque según `config.target`.
    /// Ver la cabecera del módulo: con `NixOs` solo se sondea la ESP.
    pub fn install_bootloader(config: &BootloaderConfig) -> Result<BootloaderReport> {
        match config.target {
            BootTarget::NixOs => Self::report_for_nixos(config),
            BootTarget::BareMetal => Self::install_bare_metal(config),
        }
    }

    /// antOS Linux: el gestor lo pone `nixos-install`. Se sondea la ESP si
    /// existe y se devuelve lo encontrado; no se escribe nada.
    fn report_for_nixos(config: &BootloaderConfig) -> Result<BootloaderReport> {
        let esp_dir = PathBuf::from(&config.esp_mount);
        let probed = if !config.detected_os.is_empty() {
            config.detected_os.clone()
        } else if esp_dir.is_dir() {
            Self::probe_operating_systems(&esp_dir).unwrap_or_default()
        } else {
            Vec::new()
        };
        let entries_configured: Vec<String> = probed
            .iter()
            .map(|os| format!("{} ({}, {})", os.name, os.os_type, os.efi_path))
            .collect();
        let summary = format!(
            "antOS Linux: systemd-boot lo instala y registra `nixos-install` (boot.loader.systemd-boot); \
             ESP «{}» sondeada, {} sistema(s) vecino(s) que encadenará. Nada escrito.",
            config.esp_mount,
            probed.len()
        );
        Ok(BootloaderReport {
            success: true,
            esp_path: config.esp_mount.clone(),
            efibootmgr_command: "(ninguno: la entrada NVRAM la registra nixos-install)".into(),
            entries_configured,
            loader_conf_content: String::new(),
            summary,
        })
    }

    /// Kernel bare-metal: disposición completa de la ESP y registro NVRAM.
    fn install_bare_metal(config: &BootloaderConfig) -> Result<BootloaderReport> {
        let esp_dir = PathBuf::from(&config.esp_mount);
        let entries_dir = esp_dir.join("loader/entries");
        let efi_antos_dir = esp_dir.join("EFI/antOS");
        let efi_boot_dir = esp_dir.join("EFI/BOOT");

        // Fuera de la simulación hace falta un binario EFI de verdad. Se
        // comprueba antes de crear un solo directorio.
        let efi_binary = match (&config.efi_binary, config.dry_run) {
            (Some(p), _) => {
                let path = PathBuf::from(p);
                if !path.is_file() {
                    bail!("El binario EFI «{}» no existe", path.display());
                }
                Some(path)
            }
            (None, true) => None,
            (None, false) => bail!(
                "Instalación bare-metal sin `efi_binary`: no se fabrica un stub que no arranca. \
                 Indica el binario EFI real (antos.efi) o usa la simulación."
            ),
        };

        fs::create_dir_all(&entries_dir).context("Creando loader/entries")?;
        fs::create_dir_all(&efi_antos_dir).context("Creando EFI/antOS")?;
        fs::create_dir_all(&efi_boot_dir).context("Creando EFI/BOOT")?;

        // 1. Binario cargador antos.efi y BOOTX64.EFI
        let antos_bin = efi_antos_dir.join("antos.efi");
        let fallback_bin = efi_boot_dir.join("BOOTX64.EFI");
        match efi_binary {
            Some(src) => {
                fs::copy(&src, &antos_bin).context("Copiando antos.efi")?;
                fs::copy(&src, &fallback_bin).context("Copiando BOOTX64.EFI")?;
            }
            None => {
                // Solo en simulación: un marcador legible, nunca un binario.
                let stub = b"antOS UEFI stub (SIMULACION, no arranca)";
                let _ = fs::write(&antos_bin, stub);
                let _ = fs::write(&fallback_bin, stub);
            }
        }

        // 2. loader.conf
        let loader_conf = Self::generate_loader_conf(config.timeout_seconds, "antos.conf");
        fs::write(esp_dir.join("loader/loader.conf"), &loader_conf)
            .context("Escribiendo loader.conf")?;

        // 3. loader/entries/antos.conf
        let antos_entry = Self::generate_antos_entry(
            "/EFI/antOS/vmlinuz",
            "/EFI/antOS/initrd.img",
            "3a8d8e62-f72b-4e1b-9721-a1e4c7d81234",
        );
        fs::write(entries_dir.join("antos.conf"), &antos_entry)
            .context("Escribiendo antos.conf")?;

        let mut entries_configured = vec!["antos.conf (antOS v0.1.0)".into()];

        // 4. Entradas detectadas de otros sistemas (Dual Boot)
        let probed = if config.detected_os.is_empty() {
            Self::probe_operating_systems(&esp_dir).unwrap_or_default()
        } else {
            config.detected_os.clone()
        };

        for os in &probed {
            if os.os_type == "windows" {
                let win_conf = Self::generate_windows_entry(&os.efi_path);
                fs::write(entries_dir.join("windows.conf"), &win_conf)
                    .context("Escribiendo windows.conf")?;
                entries_configured.push("windows.conf (Windows Boot Manager)".into());
            } else if os.os_type == "linux" {
                let fname = format!("{}.conf", os.name.to_lowercase().replace(' ', "-"));
                let lin_conf = Self::generate_linux_entry(&os.name, &os.efi_path);
                fs::write(entries_dir.join(&fname), &lin_conf)
                    .with_context(|| format!("Escribiendo {fname}"))?;
                entries_configured.push(format!("{} ({})", fname, os.name));
            }
        }

        // 5. Registro NVRAM con efibootmgr
        let efibootmgr_cmd = format!(
            "efibootmgr -c -d {} -p {} -L \"antOS Linux\" -l \"\\EFI\\antOS\\antos.efi\"",
            config.target_device, config.efi_partition
        );

        if !config.dry_run {
            let status = std::process::Command::new("efibootmgr")
                .args([
                    "-c",
                    "-d",
                    &config.target_device,
                    "-p",
                    &config.efi_partition.to_string(),
                    "-L",
                    "antOS Linux",
                    "-l",
                    "\\EFI\\antOS\\antos.efi",
                ])
                .status()
                .context("Ejecutando efibootmgr")?;
            if !status.success() {
                bail!("efibootmgr terminó con {status}: la entrada NVRAM no se registró");
            }
        }

        let summary = format!(
            "Gestor de arranque UEFI bare-metal {} en «{}» (Entradas: {})",
            if config.dry_run {
                "simulado"
            } else {
                "instalado"
            },
            config.esp_mount,
            entries_configured.len()
        );

        Ok(BootloaderReport {
            success: true,
            esp_path: config.esp_mount.clone(),
            efibootmgr_command: efibootmgr_cmd,
            entries_configured,
            loader_conf_content: loader_conf,
            summary,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn make_test_esp_dir(tag: &str) -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("antos-bootloader-{tag}-{now}"));
        let _ = fs::create_dir_all(&path);
        path
    }

    #[test]
    fn test_generate_loader_conf_and_entries() {
        let conf = BootloaderEngine::generate_loader_conf(10, "antos.conf");
        assert!(conf.contains("default      antos.conf"));
        assert!(conf.contains("timeout      10"));
        assert!(conf.contains("console-mode max"));

        let antos_entry = BootloaderEngine::generate_antos_entry(
            "/EFI/antOS/vmlinuz",
            "/EFI/antOS/initrd",
            "uuid-root-test",
        );
        assert!(antos_entry.contains("title    antOS"));
        assert!(antos_entry.contains("root=UUID=uuid-root-test"));

        let win_entry =
            BootloaderEngine::generate_windows_entry("\\EFI\\Microsoft\\Boot\\bootmgfw.efi");
        assert!(win_entry.contains("title Windows Boot Manager"));
        assert!(win_entry.contains("/EFI/Microsoft/Boot/bootmgfw.efi"));
    }

    #[test]
    fn test_probe_operating_systems_mock_esp() {
        let esp = make_test_esp_dir("probe");
        let win_efi = esp.join("EFI/Microsoft/Boot");
        fs::create_dir_all(&win_efi).unwrap();
        fs::write(win_efi.join("bootmgfw.efi"), b"mock-win-efi").unwrap();

        let probed = BootloaderEngine::probe_operating_systems(&esp).expect("probe");
        assert!(probed.iter().any(|os| os.os_type == "windows"));
        let _ = fs::remove_dir_all(&esp);
    }

    /// Una ESP vacía no tiene vecinos: nada de entradas de relleno (T36.1).
    #[test]
    fn test_probe_operating_systems_empty_esp_reports_nothing() {
        let esp = make_test_esp_dir("empty");
        fs::create_dir_all(esp.join("EFI")).unwrap();
        let probed = BootloaderEngine::probe_operating_systems(&esp).expect("probe");
        assert!(probed.is_empty());
        let _ = fs::remove_dir_all(&esp);
    }

    #[test]
    fn test_install_bootloader_dry_run() {
        let esp = make_test_esp_dir("install");
        let cfg = BootloaderConfig {
            esp_mount: esp.display().to_string(),
            target_device: "/dev/nvme0n1".into(),
            efi_partition: 1,
            default_os: "antos".into(),
            timeout_seconds: 5,
            detected_os: vec![OsEntry {
                name: "Windows Boot Manager".into(),
                os_type: "windows".into(),
                efi_path: "\\EFI\\Microsoft\\Boot\\bootmgfw.efi".into(),
                disk_device: "/dev/nvme0n1".into(),
                partition_number: 1,
            }],
            dry_run: true,
            target: BootTarget::BareMetal,
            efi_binary: None,
        };

        let report = BootloaderEngine::install_bootloader(&cfg).expect("install bootloader");
        assert!(report.success);
        assert!(report.entries_configured.len() >= 2);
        assert!(report
            .efibootmgr_command
            .contains("efibootmgr -c -d /dev/nvme0n1 -p 1 -L \"antOS Linux\""));
        assert!(esp.join("loader/loader.conf").exists());
        assert!(esp.join("loader/entries/antos.conf").exists());
        assert!(esp.join("loader/entries/windows.conf").exists());
        // En simulación el marcador es texto legible, no un falso ELF.
        let stub = fs::read(esp.join("EFI/antOS/antos.efi")).unwrap();
        assert!(stub.starts_with(b"antOS UEFI stub (SIMULACION"));
        assert!(!stub.starts_with(b"\x7fELF"));
        let _ = fs::remove_dir_all(&esp);
    }

    /// antOS Linux (T36.1): el gestor es de `nixos-install`. Aunque no sea
    /// `dry_run`, aquí no se escribe nada en la ESP ni se llama a
    /// `efibootmgr`; solo se sondea.
    #[test]
    fn test_install_bootloader_nixos_target_writes_nothing() {
        let esp = make_test_esp_dir("nixos");
        let win_efi = esp.join("EFI/Microsoft/Boot");
        fs::create_dir_all(&win_efi).unwrap();
        fs::write(win_efi.join("bootmgfw.efi"), b"mock-win-efi").unwrap();
        let before: Vec<PathBuf> = walk(&esp);

        let cfg = BootloaderConfig {
            esp_mount: esp.display().to_string(),
            dry_run: false,
            target: BootTarget::NixOs,
            ..BootloaderConfig::default()
        };
        let report = BootloaderEngine::install_bootloader(&cfg).expect("nixos report");
        assert!(report.success);
        assert!(report.summary.contains("nixos-install"));
        assert!(report.summary.contains("Nada escrito"));
        assert!(report.efibootmgr_command.contains("ninguno"));
        assert!(report.loader_conf_content.is_empty());
        assert!(report
            .entries_configured
            .iter()
            .any(|e| e.contains("windows")));

        assert_eq!(walk(&esp), before, "la ESP cambió en la vía NixOs");
        assert!(!esp.join("EFI/antOS").exists());
        assert!(!esp.join("loader").exists());
        let _ = fs::remove_dir_all(&esp);
    }

    /// Bare-metal fuera de simulación sin binario EFI: se aborta antes de
    /// crear un solo directorio. Nunca más un stub registrado en la NVRAM.
    #[test]
    fn test_install_bootloader_bare_metal_real_requires_efi_binary() {
        let esp = make_test_esp_dir("bare-real");
        let cfg = BootloaderConfig {
            esp_mount: esp.join("esp").display().to_string(),
            dry_run: false,
            target: BootTarget::BareMetal,
            efi_binary: None,
            ..BootloaderConfig::default()
        };
        let err = BootloaderEngine::install_bootloader(&cfg).unwrap_err();
        assert!(err.to_string().contains("efi_binary"));
        assert!(!esp.join("esp").exists());

        let missing = BootloaderConfig {
            efi_binary: Some(esp.join("no-existe.efi").display().to_string()),
            ..cfg
        };
        let err = BootloaderEngine::install_bootloader(&missing).unwrap_err();
        assert!(err.to_string().contains("no existe"));
        assert!(!esp.join("esp").exists());
        let _ = fs::remove_dir_all(&esp);
    }

    /// El `Default` del protocolo es la vía NixOs con la ESP en `/boot`.
    #[test]
    fn test_bootloader_config_default_is_nixos_on_boot() {
        let cfg = BootloaderConfig::default();
        assert_eq!(cfg.target, BootTarget::NixOs);
        assert_eq!(cfg.esp_mount, "/boot");
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if let Ok(rd) = fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    out.extend(walk(&p));
                }
                out.push(p);
            }
        }
        out.sort();
        out
    }
}
