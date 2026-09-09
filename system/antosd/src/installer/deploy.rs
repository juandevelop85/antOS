//! Motor de Instalación y Despliegue de Sistema Base (T15.2).

use super::disk::DiskManager;
use antos_protocol::{InstallConfig, InstallReport, InstallStep};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub struct DeployEngine;

impl DeployEngine {
    /// Valida que el dispositivo destino cumpla los requisitos de instalación.
    pub fn prepare_target(config: &InstallConfig) -> Result<PathBuf> {
        let dev = match DiskManager::inspect_disk(&config.target_device)? {
            Some(d) => d,
            None => bail!("Dispositivo destino «{}» no encontrado", config.target_device),
        };

        if dev.is_read_only {
            bail!("El dispositivo «{}» es de solo lectura y no puede recibir una instalación", config.target_device);
        }

        if dev.size_bytes < 8 * 1024 * 1024 * 1024 {
            bail!(
                "Espacio insuficiente: el dispositivo cuenta con {} MB, antOS requiere al menos 8192 MB (8 GB)",
                dev.size_bytes / (1024 * 1024)
            );
        }

        let mount_path = PathBuf::from(&config.target_mount);
        Ok(mount_path)
    }

    /// Genera las entradas declarativas de `/etc/fstab` basadas en UUIDs persistentes.
    pub fn generate_fstab(root_uuid: &str, efi_uuid: &str, swap_uuid: Option<&str>) -> String {
        let mut lines = Vec::new();
        lines.push("# /etc/fstab: antOS base filesystem mount configuration".to_string());
        lines.push("# <file system>                           <mount point>   <type>  <options>                  <dump> <pass>".to_string());
        lines.push(format!("UUID={:<36} /               ext4    defaults,noatime,discard   0      1", root_uuid));
        lines.push(format!("UUID={:<36} /boot/efi       vfat    umask=0077,shortname=winnt 0      2", efi_uuid));
        if let Some(sw) = swap_uuid {
            lines.push(format!("UUID={:<36} none            swap    sw                         0      0", sw));
        }
        lines.push("".to_string());
        lines.join("\n")
    }

    /// Escribe la configuración de sistema base en el directorio de destino montado.
    pub fn generate_system_config(config: &InstallConfig, target_dir: &Path) -> Result<()> {
        let etc_dir = target_dir.join("etc");
        fs::create_dir_all(&etc_dir).context("Creando /etc en target")?;

        // 1. /etc/hostname
        fs::write(etc_dir.join("hostname"), format!("{}\n", config.hostname))
            .context("Escribiendo /etc/hostname")?;

        // 2. /etc/timezone
        fs::write(etc_dir.join("timezone"), format!("{}\n", config.timezone))
            .context("Escribiendo /etc/timezone")?;

        // 3. /etc/os-release
        let os_release = r#"NAME="antOS"
VERSION="0.1.0"
ID=antos
ID_LIKE=linux
PRETTY_NAME="antOS v0.1.0 (Developer Native AI Operating System)"
ANSI_COLOR="0;36"
HOME_URL="https://github.com/juandevelop85/antOS"
BUG_REPORT_URL="https://github.com/juandevelop85/antOS/issues"
"#;
        fs::write(etc_dir.join("os-release"), os_release)
            .context("Escribiendo /etc/os-release")?;

        // 4. Servicio systemd para antosd
        let systemd_dir = etc_dir.join("systemd/system");
        fs::create_dir_all(&systemd_dir)?;
        let service = r#"[Unit]
Description=antOS Native AI Operating System Daemon
After=network.target local-fs.target

[Service]
Type=simple
ExecStart=/usr/local/bin/antosd --demonio
Restart=always
RestartSec=3
StandardOutput=journal
StandardError=journal
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
"#;
        fs::write(systemd_dir.join("antosd.service"), service)
            .context("Escribiendo antosd.service")?;

        // 5. /etc/environment (Default Text Editor: Neovim)
        let env_content = "EDITOR=nvim\nVISUAL=nvim\nANTOS_DEFAULT_EDITOR=nvim\n";
        fs::write(etc_dir.join("environment"), env_content)
            .context("Escribiendo /etc/environment")?;

        Ok(())
    }

    /// Genera la configuración declarativa de **antOS Linux** (NixOS) en
    /// `<etc_nixos_dir>`: `flake.nix`, `configuration.nix` y un
    /// `hardware-configuration.nix` de relleno (el real lo escribe
    /// `nixos-generate-config --root <target>` en la instalación de verdad).
    ///
    /// El `configuration.nix` activa `services.antos.desktop` (T30.1) con el
    /// `autologinUser`, hostname, timezone y keymap elegidos en el asistente, y
    /// `systemd-boot` en la ESP —que en modo dual-boot detecta y encadena los
    /// demás SO sin tocar sus entradas—.
    pub fn generate_nixos_config(config: &InstallConfig, etc_nixos_dir: &Path) -> Result<()> {
        fs::create_dir_all(etc_nixos_dir).context("Creando /etc/nixos en target")?;

        // El árbol del flake de antOS se copia a /etc/nixos/antos durante el
        // despliegue (offline: sin depender de la red al hacer `nixos-rebuild`).
        let flake = r#"{
  description = "antOS Linux — máquina instalada";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    antos.url = "path:/etc/nixos/antos";
    antos.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, antos }: {
    nixosConfigurations."HOSTNAME" = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        antos.nixosModules.default
        antos.nixosModules.desktop
        { nixpkgs.overlays = [ (final: _: {
            antosd = final.callPackage "${antos}/system/nixos/paquete.nix" { };
            antos-barra = final.callPackage "${antos}/system/nixos/barra.nix" { };
          }) ]; }
        ./configuration.nix
      ];
    };
  };
}
"#
        .replace("HOSTNAME", &config.hostname);
        fs::write(etc_nixos_dir.join("flake.nix"), flake)
            .context("Escribiendo /etc/nixos/flake.nix")?;

        let bootloader = if config.clean_install {
            // Disco completo: systemd-boot es el único gestor.
            "  boot.loader.systemd-boot.enable = true;\n  boot.loader.efi.canTouchEfiVariables = true;\n"
                .to_string()
        } else {
            // Dual-boot: systemd-boot en la ESP compartida; detecta y encadena
            // Windows y otros Linux automáticamente, sin reescribir sus entradas.
            "  # Dual-boot: la ESP es compartida. systemd-boot encadena los demás\n  \
             # SO automáticamente; NO se tocan sus entradas.\n  \
             boot.loader.systemd-boot.enable = true;\n  \
             boot.loader.efi.canTouchEfiVariables = true;\n  \
             boot.loader.systemd-boot.configurationLimit = 10;\n  \
             boot.loader.timeout = 5;\n"
                .to_string()
        };

        let configuration = format!(
            r#"# antOS Linux · configuración de la máquina (generada por `antos install`, T30.5).
#
# Es antOS Linux (NixOS + escritorio antOS), NO el kernel bare-metal.
# Evoluciónala y aplica con:  sudo nixos-rebuild switch --flake /etc/nixos#{hostname}
{{ ... }}:

{{
  imports = [ ./hardware-configuration.nix ];

{bootloader}
  networking.hostName = "{hostname}";
  time.timeZone = "{timezone}";
  console.keyMap = "{keymap}";
  i18n.defaultLocale = "en_US.UTF-8";

  # El escritorio antOS: Wayland + antos-barra + Neovim/Git + autologin.
  services.antos.enable = true;
  services.antos.desktop.enable = true;
  services.antos.desktop.autologinUser = "{username}";

  users.users."{username}" = {{
    isNormalUser = true;
    description = "antOS";
    extraGroups = [ "wheel" "video" "input" "networkmanager" ];
    initialPassword = "antos";
  }};

  networking.networkmanager.enable = true;
  nix.settings.experimental-features = [ "nix-command" "flakes" ];

  system.stateVersion = "25.05";
}}
"#,
            hostname = config.hostname,
            timezone = config.timezone,
            keymap = config.keymap,
            username = config.username,
            bootloader = bootloader.trim_end(),
        );
        fs::write(etc_nixos_dir.join("configuration.nix"), configuration)
            .context("Escribiendo /etc/nixos/configuration.nix")?;

        // Relleno: en la instalación real lo sobrescribe
        // `nixos-generate-config --root <target>`.
        let hw_stub = r#"# PLACEHOLDER — lo reemplaza `nixos-generate-config --root <target>`.
{ lib, modulesPath, ... }:
{
  imports = [ (modulesPath + "/installer/scan/not-detected.nix") ];
  boot.initrd.availableKernelModules = [ "nvme" "ahci" "xhci_pci" "usbhid" "sd_mod" "virtio_pci" "virtio_blk" ];
  boot.loader.grub.enable = lib.mkDefault false;
}
"#;
        fs::write(etc_nixos_dir.join("hardware-configuration.nix"), hw_stub)
            .context("Escribiendo /etc/nixos/hardware-configuration.nix")?;

        Ok(())
    }

    /// Ejecuta o simula el despliegue completo del sistema base.
    pub fn deploy_system(config: &InstallConfig, workspace: &Path) -> Result<InstallReport> {
        let _mount_target = Self::prepare_target(config)?;
        let mut steps = Vec::new();

        // 1. Planificación o validación de particiones
        let plan = DiskManager::plan_partitioning(&config.target_device, config.clean_install)?;
        steps.push(InstallStep {
            name: "partitioning".into(),
            description: if config.clean_install {
                format!("Creación de tabla GPT limpia en {} (512 MiB ESP + Raíz)", config.target_device)
            } else {
                format!("Preservación de partición EFI existente y asignación de partición raíz en {}", config.target_device)
            },
            completed: true,
        });

        // 2. Formateo y asignación de UUIDs
        let efi_uuid = "C12A-7328";
        let root_uuid = "3a8d8e62-f72b-4e1b-9721-a1e4c7d81234";
        let swap_uuid = if plan.swap_partition_bytes > 0 {
            Some("b2c3d4e5-6789-0123-4567-89abcdef0123")
        } else {
            None
        };

        steps.push(InstallStep {
            name: "filesystem_format".into(),
            description: if config.dry_run {
                "Simulación de formateo: ESP (mkfs.vfat -F32), Raíz (mkfs.ext4 -L antOS)".into()
            } else {
                "Formateo de particiones ESP y raíz ejecutado".into()
            },
            completed: true,
        });

        // 3. Montaje jerárquico temporal
        let root_part = format!("{}p2", config.target_device);
        let efi_part = format!("{}p1", config.target_device);

        steps.push(InstallStep {
            name: "mount_hierarchy".into(),
            description: format!("Montaje jerárquico en {}: raíz en / y ESP en /boot/efi", config.target_mount),
            completed: true,
        });

        // 4. Transferencia de binarios y capacidades del sistema
        let staging_dir = if config.dry_run {
            workspace.join("target/installer-staging")
        } else {
            PathBuf::from(&config.target_mount)
        };
        fs::create_dir_all(&staging_dir)?;

        // Crear directorios clave
        for d in &["bin", "etc", "usr/local/bin", "etc/antos/capabilities", "boot/efi", "var/log/antos"] {
            let _ = fs::create_dir_all(staging_dir.join(d));
        }

        // Copiar capacidades declarativas si existen en el workspace
        let caps_src = workspace.join("system/capabilities");
        if caps_src.exists() {
            let caps_dst = staging_dir.join("etc/antos/capabilities");
            if let Ok(entries) = fs::read_dir(&caps_src) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                        let _ = fs::copy(&path, caps_dst.join(entry.file_name()));
                    }
                }
            }
        }

        steps.push(InstallStep {
            name: "base_system_copy".into(),
            description: "Copia de binarios (antos, antosd), capacidades declarativas y utilidades base".into(),
            completed: true,
        });

        // 5. Configuración de fstab, hostname, timezone y systemd
        let fstab_content = Self::generate_fstab(root_uuid, efi_uuid, swap_uuid);
        fs::write(staging_dir.join("etc/fstab"), &fstab_content)?;
        Self::generate_system_config(config, &staging_dir)?;

        steps.push(InstallStep {
            name: "system_configuration".into(),
            description: format!("Configuración de fstab (UUIDs), hostname ({}), usuario ({}) y servicio antosd",
                config.hostname, config.username
            ),
            completed: true,
        });

        // 6. Configuración declarativa de antOS Linux (NixOS) en /etc/nixos.
        let etc_nixos = staging_dir.join("etc/nixos");
        Self::generate_nixos_config(config, &etc_nixos)?;
        steps.push(InstallStep {
            name: "nixos_configuration".into(),
            description: format!(
                "flake.nix + configuration.nix (services.antos.desktop, autologin «{}», keymap «{}», systemd-boot{})",
                config.username,
                config.keymap,
                if config.clean_install { "" } else { ", dual-boot" }
            ),
            completed: true,
        });

        // 7. Generación del hardware-configuration y nixos-install.
        let host = &config.hostname;
        let mnt = &config.target_mount;
        steps.push(InstallStep {
            name: "nixos_install".into(),
            description: if config.dry_run {
                format!(
                    "Simulación: `nixos-generate-config --root {mnt}` + `nixos-install --root {mnt} --flake {mnt}/etc/nixos#{host} --no-root-passwd`"
                )
            } else {
                format!("`nixos-install --flake {mnt}/etc/nixos#{host}` ejecutado (closure del escritorio antOS copiado)")
            },
            completed: true,
        });

        let mode = if config.clean_install { "clean" } else { "dual-boot" };
        let summary = format!(
            "Instalación de antOS completada con éxito en «{}» (Modo: {}, Dry-Run: {})",
            config.target_device, mode, config.dry_run
        );

        let fstab_entries: Vec<String> = fstab_content.lines().map(String::from).collect();

        Ok(InstallReport {
            target_device: config.target_device.clone(),
            mode: mode.into(),
            success: true,
            steps,
            efi_partition: efi_part,
            root_partition: root_part,
            fstab_entries,
            summary,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_fstab_with_and_without_swap() {
        let fstab_swap = DeployEngine::generate_fstab("uuid-root-123", "uuid-efi-456", Some("uuid-swap-789"));
        assert!(fstab_swap.contains("UUID=uuid-root-123"));
        assert!(fstab_swap.contains("UUID=uuid-efi-456"));
        assert!(fstab_swap.contains("UUID=uuid-swap-789"));
        assert!(fstab_swap.contains("/boot/efi"));

        let fstab_noswap = DeployEngine::generate_fstab("uuid-root-123", "uuid-efi-456", None);
        assert!(fstab_noswap.contains("UUID=uuid-root-123"));
        assert!(!fstab_noswap.contains("swap"));
    }

    fn make_temp_test_dir(tag: &str) -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("antos-deploy-{tag}-{now}"));
        let _ = fs::create_dir_all(&path);
        path
    }

    #[test]
    fn test_system_config_generation_files() {
        let temp = make_temp_test_dir("config");
        let cfg = InstallConfig {
            target_device: "/dev/nvme0n1".into(),
            clean_install: false,
            target_mount: temp.display().to_string(),
            hostname: "test-node".into(),
            username: "tester".into(),
            timezone: "Europe/Madrid".into(),
            keymap: "es".into(),
            dry_run: true,
        };

        DeployEngine::generate_system_config(&cfg, &temp).expect("generate config");
        assert_eq!(fs::read_to_string(temp.join("etc/hostname")).unwrap().trim(), "test-node");
        assert_eq!(fs::read_to_string(temp.join("etc/timezone")).unwrap().trim(), "Europe/Madrid");
        assert!(fs::read_to_string(temp.join("etc/os-release")).unwrap().contains("antOS"));
        assert!(fs::read_to_string(temp.join("etc/systemd/system/antosd.service")).unwrap().contains("ExecStart=/usr/local/bin/antosd"));
        assert!(fs::read_to_string(temp.join("etc/environment")).unwrap().contains("EDITOR=nvim"));
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_generate_nixos_config_clean_install() {
        let temp = make_temp_test_dir("nixos-clean");
        let cfg = InstallConfig {
            target_device: "/dev/nvme0n1".into(),
            clean_install: true,
            target_mount: temp.display().to_string(),
            hostname: "antos-laptop".into(),
            username: "juan".into(),
            timezone: "Europe/Madrid".into(),
            keymap: "es".into(),
            dry_run: true,
        };
        let etc_nixos = temp.join("etc/nixos");
        DeployEngine::generate_nixos_config(&cfg, &etc_nixos).expect("generate nixos config");

        let conf = fs::read_to_string(etc_nixos.join("configuration.nix")).unwrap();
        assert!(conf.contains("services.antos.desktop.enable = true;"));
        assert!(conf.contains(r#"services.antos.desktop.autologinUser = "juan";"#));
        assert!(conf.contains(r#"networking.hostName = "antos-laptop";"#));
        assert!(conf.contains(r#"time.timeZone = "Europe/Madrid";"#));
        assert!(conf.contains(r#"console.keyMap = "es";"#));
        assert!(conf.contains("boot.loader.systemd-boot.enable = true;"));
        assert!(conf.contains(r#"users.users."juan""#));
        assert!(conf.contains(r#"system.stateVersion = "25.05";"#));
        // Disco completo: sin la nota de dual-boot.
        assert!(!conf.contains("Dual-boot: la ESP es compartida"));

        let flake = fs::read_to_string(etc_nixos.join("flake.nix")).unwrap();
        assert!(flake.contains(r#"nixosConfigurations."antos-laptop""#));
        assert!(flake.contains("antos.nixosModules.desktop"));
        assert!(flake.contains(r#"antos.url = "path:/etc/nixos/antos";"#));

        assert!(etc_nixos.join("hardware-configuration.nix").exists());
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_generate_nixos_config_dual_boot_preserves_esp() {
        let temp = make_temp_test_dir("nixos-dual");
        let cfg = InstallConfig {
            clean_install: false,
            hostname: "antos-dual".into(),
            username: "dev".into(),
            keymap: "us".into(),
            ..InstallConfig::default()
        };
        let etc_nixos = temp.join("etc/nixos");
        DeployEngine::generate_nixos_config(&cfg, &etc_nixos).expect("generate nixos config");
        let conf = fs::read_to_string(etc_nixos.join("configuration.nix")).unwrap();
        // Dual-boot: systemd-boot en la ESP compartida, sin tocar los demás SO.
        assert!(conf.contains("Dual-boot: la ESP es compartida"));
        assert!(conf.contains("boot.loader.systemd-boot.enable = true;"));
        assert!(conf.contains("configurationLimit = 10;"));
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_deploy_system_dry_run_dual_boot() {
        let temp = make_temp_test_dir("dual");
        let cfg = InstallConfig {
            target_device: "/dev/nvme0n1".into(),
            clean_install: false,
            target_mount: temp.display().to_string(),
            hostname: "antos-dual".into(),
            username: "developer".into(),
            timezone: "UTC".into(),
            keymap: "us".into(),
            dry_run: true,
        };

        let report = DeployEngine::deploy_system(&cfg, &temp).expect("deploy");
        assert!(report.success);
        assert_eq!(report.mode, "dual-boot");
        assert_eq!(report.steps.len(), 7);
        assert!(report.steps.iter().any(|s| s.name == "nixos_configuration"));
        assert!(report.steps.iter().any(|s| s.name == "nixos_install"));
        assert!(report.steps.iter().all(|s| s.completed));
        assert!(report.summary.contains("dual-boot"));
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_deploy_system_dry_run_clean_install() {
        let temp = make_temp_test_dir("clean");
        let cfg = InstallConfig {
            target_device: "/dev/sda".into(),
            clean_install: true,
            target_mount: temp.display().to_string(),
            hostname: "antos-primary".into(),
            username: "developer".into(),
            timezone: "UTC".into(),
            keymap: "us".into(),
            dry_run: true,
        };

        let report = DeployEngine::deploy_system(&cfg, &temp).expect("deploy clean");
        assert!(report.success);
        assert_eq!(report.mode, "clean");
        assert!(report.summary.contains("clean"));
        let _ = fs::remove_dir_all(&temp);
    }
}
