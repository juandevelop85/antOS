//! Tests de la tubería de instalación (`deploy/mod.rs`), separados por
//! tamaño (T31.15): movimiento de código, no cambio de comportamiento.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn test_generate_fstab_with_and_without_swap() {
    let fstab_swap =
        DeployEngine::generate_fstab("uuid-root-123", "uuid-efi-456", Some("uuid-swap-789"));
    assert!(fstab_swap.contains("UUID=uuid-root-123"));
    assert!(fstab_swap.contains("UUID=uuid-efi-456"));
    assert!(fstab_swap.contains("UUID=uuid-swap-789"));
    // La ESP va en /boot (NixOS / systemd-boot), no en /boot/efi.
    assert!(fstab_swap.contains(" /boot "));
    assert!(!fstab_swap.contains("/boot/efi"));

    let fstab_noswap = DeployEngine::generate_fstab("uuid-root-123", "uuid-efi-456", None);
    assert!(fstab_noswap.contains("UUID=uuid-root-123"));
    assert!(!fstab_noswap.contains("swap"));
}

#[test]
fn test_partition_names_follow_kernel_naming() {
    assert_eq!(
        DeployEngine::partition_names("/dev/nvme0n1"),
        ("/dev/nvme0n1p1".to_string(), "/dev/nvme0n1p2".to_string())
    );
    assert_eq!(
        DeployEngine::partition_names("/dev/sda"),
        ("/dev/sda1".to_string(), "/dev/sda2".to_string())
    );
    assert_eq!(
        DeployEngine::partition_names("/dev/mmcblk0"),
        ("/dev/mmcblk0p1".to_string(), "/dev/mmcblk0p2".to_string())
    );
}

fn make_temp_test_dir(tag: &str) -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("antos-deploy-{tag}-{now}"));
    let _ = fs::create_dir_all(&path);
    path
}

/// Raíz del repositorio (el crate vive en `system/antosd`).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
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
        system: "x86_64-linux".into(),
        password_hash: None,
        locale: "en_US.UTF-8".into(),
        encrypt: false,
        dry_run: true,
    };

    DeployEngine::generate_system_config(&cfg, &temp).expect("generate config");
    assert_eq!(
        fs::read_to_string(temp.join("etc/hostname"))
            .unwrap()
            .trim(),
        "test-node"
    );
    assert_eq!(
        fs::read_to_string(temp.join("etc/timezone"))
            .unwrap()
            .trim(),
        "Europe/Madrid"
    );
    assert!(fs::read_to_string(temp.join("etc/os-release"))
        .unwrap()
        .contains("antOS"));
    assert!(
        fs::read_to_string(temp.join("etc/systemd/system/antosd.service"))
            .unwrap()
            .contains("ExecStart=/usr/local/bin/antosd")
    );
    assert!(fs::read_to_string(temp.join("etc/environment"))
        .unwrap()
        .contains("EDITOR=nvim"));
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
        system: "aarch64-linux".into(),
        password_hash: None,
        locale: "en_US.UTF-8".into(),
        encrypt: false,
        dry_run: true,
    };
    let etc_nixos = temp.join("etc/nixos");
    DeployEngine::generate_nixos_config(&cfg, &etc_nixos, None).expect("generate nixos config");

    let conf = fs::read_to_string(etc_nixos.join("configuration.nix")).unwrap();
    assert!(conf.contains("services.antos.desktop.enable = true;"));
    assert!(conf.contains(r#"services.antos.desktop.autologinUser = "juan";"#));
    assert!(conf.contains(r#"networking.hostName = "antos-laptop";"#));
    // Teclado, locale y zona horaria van por `services.antos.machine` (T36.4),
    // que también pone systemd-boot, red, audio, etc.
    assert!(conf.contains("services.antos.machine = {"));
    assert!(conf.contains(r#"keyboardLayout = "es";"#));
    assert!(conf.contains(r#"locale = "en_US.UTF-8";"#));
    assert!(conf.contains(r#"timeZone = "Europe/Madrid";"#));
    assert!(!conf.contains("time.timeZone ="));
    assert!(!conf.contains("console.keyMap ="));
    assert!(!conf.contains("boot.loader.systemd-boot.enable"));
    assert!(!conf.contains("networking.networkmanager.enable"));
    assert!(conf.contains(r#"users.users."juan""#));
    assert!(conf.contains(r#"system.stateVersion = "25.05";"#));
    // Disco completo: sin la nota de dual-boot.
    assert!(!conf.contains("Dual-boot: la ESP es compartida"));

    let flake = fs::read_to_string(etc_nixos.join("flake.nix")).unwrap();
    assert!(flake.contains(r#"nixosConfigurations."antos-laptop""#));
    assert!(flake.contains(r#"system = "aarch64-linux";"#));
    assert!(flake.contains("antos.nixosModules.desktop"));
    // `desktop.nix` fija `services.antos.llm.enable`: sin el módulo `llm`
    // la evaluación falla con «option does not exist».
    assert!(flake.contains("antos.nixosModules.llm"));
    assert!(flake.contains("antos.nixosModules.machine"));
    assert!(flake.contains(r#"antos.url = "path:/etc/nixos/antos";"#));
    // Sin fuente: se avisa en el propio flake y no hay lock que fije nada.
    assert!(flake.contains("nixos-unstable"));
    assert!(flake.contains("ATENCIÓN"));
    assert!(!etc_nixos.join("flake.lock").exists());
    assert!(!etc_nixos.join("antos").exists());

    let hw = fs::read_to_string(etc_nixos.join("hardware-configuration.nix")).unwrap();
    assert!(hw.contains(r#"fileSystems."/""#));
    assert!(hw.contains("by-label/antos-root"));
    assert!(hw.contains("by-label/ANTOS_ESP"));
    assert!(hw.contains(r#"nixpkgs.hostPlatform = lib.mkDefault "aarch64-linux";"#));
    let _ = fs::remove_dir_all(&temp);
}

/// El flake generado no puede referenciar rutas internas del árbol de
/// antOS (`paquete.nix` se renombró en T31.12 y el instalador siguió
/// apuntando al nombre viejo): consume `overlays.default`, y ese output
/// tiene que existir en el `flake.nix` real. Si alguien lo quita de la
/// raíz, este test lo dice antes que una máquina instalada.
#[test]
fn test_generated_flake_uses_only_public_outputs_of_the_antos_flake() {
    let temp = make_temp_test_dir("nixos-outputs");
    let cfg = InstallConfig {
        hostname: "antos-outputs".into(),
        ..InstallConfig::default()
    };
    let etc_nixos = temp.join("etc/nixos");
    DeployEngine::generate_nixos_config(&cfg, &etc_nixos, None).expect("generate");
    let flake = fs::read_to_string(etc_nixos.join("flake.nix")).unwrap();

    assert!(flake.contains("antos.overlays.default"));
    // nixpkgs y antos en el registro: sus fuentes viajan en la closure
    // y `nixos-rebuild` funciona sin red (T36.2).
    assert!(flake.contains("nix.registry.nixpkgs.flake = nixpkgs;"));
    assert!(flake.contains("nix.registry.antos.flake = antos;"));
    assert!(!flake.contains("callPackage"));
    assert!(!flake.contains("system/nixos/"));

    let root_flake = fs::read_to_string(repo_root().join("flake.nix")).unwrap();
    assert!(root_flake.contains("overlays.default = overlayAntos;"));
    // `machine` (T36.4): el configuration.nix generado fija
    // `services.antos.machine`; sin el módulo no evalúa — lo descubrió la
    // evaluación real del flake generado en la podman-machine (2026-09-22).
    for module in ["default", "desktop", "llm", "machine"] {
        assert!(
            root_flake.contains(&format!("nixosModules.{module} =")),
            "el flake raíz no exporta nixosModules.{module}"
        );
        assert!(flake.contains(&format!("antos.nixosModules.{module}")));
    }
    let _ = fs::remove_dir_all(&temp);
}

/// Con el árbol de antOS como fuente: nixpkgs queda fijado a la revisión
/// del `flake.lock` real, el `flake.lock` generado lleva ese nodo, y la
/// copia en `antos/` trae lo que el flake necesita sin `target/`.
#[test]
fn test_generate_nixos_config_with_source_pins_nixpkgs_and_copies_tree() {
    let temp = make_temp_test_dir("nixos-source");
    let root = repo_root();
    let cfg = InstallConfig {
        hostname: "antos-src".into(),
        ..InstallConfig::default()
    };
    let etc_nixos = temp.join("etc/nixos");
    DeployEngine::generate_nixos_config(&cfg, &etc_nixos, Some(&root)).expect("generate");

    let (rev, _) = DeployEngine::locked_nixpkgs(&root).expect("flake.lock real con nixpkgs");
    let flake = fs::read_to_string(etc_nixos.join("flake.nix")).unwrap();
    assert!(flake.contains(&format!("github:NixOS/nixpkgs/{rev}")));
    assert!(!flake.contains("nixos-unstable"));
    assert!(!flake.contains("ATENCIÓN"));

    let lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(etc_nixos.join("flake.lock")).unwrap()).unwrap();
    assert_eq!(lock["version"], 7);
    assert_eq!(lock["nodes"]["nixpkgs"]["locked"]["rev"], rev);
    // `original` coincide con la URL del flake generado (rev fijo), no
    // con el `ref` del árbol: así nix no vuelve a resolver nixpkgs.
    assert_eq!(lock["nodes"]["nixpkgs"]["original"]["rev"], rev);
    assert!(lock["nodes"]["nixpkgs"]["original"].get("ref").is_none());
    assert_eq!(lock["nodes"]["root"]["inputs"]["nixpkgs"], "nixpkgs");

    let copy = etc_nixos.join("antos");
    for rel in [
        "flake.nix",
        "flake.lock",
        "Cargo.toml",
        "Cargo.lock",
        "system/nixos/package.nix",
        "system/nixos/barra.nix",
        "system/nixos/desktop.nix",
        "system/nixos/llm.nix",
        "system/desktop/rc.xml",
        "system/capabilities",
        "system/stacks",
        "system/llm",
        "recipes",
    ] {
        assert!(copy.join(rel).exists(), "falta {rel} en la copia");
    }
    assert!(!copy.join("target").exists());
    assert!(!copy.join("system/antosd/target").exists());
    assert!(!copy.join(".git").exists());
    assert!(!copy.join("docs").exists());
    let _ = fs::remove_dir_all(&temp);
}

#[test]
fn test_copy_antos_source_rejects_a_directory_that_is_not_antos() {
    let temp = make_temp_test_dir("not-antos");
    let err = DeployEngine::copy_antos_source(&temp, &temp.join("out")).unwrap_err();
    assert!(err.to_string().contains("no es el árbol fuente de antOS"));
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
    DeployEngine::generate_nixos_config(&cfg, &etc_nixos, None).expect("generate nixos config");
    let conf = fs::read_to_string(etc_nixos.join("configuration.nix")).unwrap();
    // Dual-boot: systemd-boot en la ESP compartida, sin tocar los demás SO.
    assert!(conf.contains("Dual-boot: la ESP es compartida"));
    assert!(conf.contains("configurationLimit = 10;"));
    assert!(conf.contains("boot.loader.timeout = 5;"));
    let _ = fs::remove_dir_all(&temp);
}

fn simulated(disk_path: &str) -> SimulatedRunner {
    let disk = DiskManager::inspect_disk(disk_path)
        .expect("inspect")
        .expect("disco sintético");
    SimulatedRunner::for_disk(&disk)
}

fn run_simulated(cfg: &InstallConfig, workspace: &Path) -> (InstallReport, SimulatedRunner) {
    let mut runner = simulated(&cfg.target_device);
    let mut lines = Vec::new();
    let report = DeployEngine::install(cfg, workspace, &mut runner, &mut |l| {
        lines.push(l.to_string())
    })
    .expect("install simulada");
    assert!(
        lines.iter().any(|l| l.contains("nixos-install")),
        "nixos-install debe emitir salida por on_line"
    );
    (report, runner)
}

/// Simulación en dual-boot sobre el NVMe sintético (ESP + NTFS + hueco):
/// la secuencia exacta de comandos que la instalación real lanzaría.
#[test]
fn test_install_dual_boot_command_sequence() {
    let temp = make_temp_test_dir("dual");
    let cfg = InstallConfig {
        target_device: "/dev/nvme0n1".into(),
        clean_install: false,
        target_mount: "/mnt/target".into(),
        hostname: "antos-dual".into(),
        username: "developer".into(),
        timezone: "UTC".into(),
        keymap: "us".into(),
        system: "x86_64-linux".into(),
        password_hash: None,
        locale: "en_US.UTF-8".into(),
        encrypt: false,
        dry_run: true,
    };
    let (report, runner) = run_simulated(&cfg, &temp);
    let staging = temp.join("target/installer-staging");
    let root = staging.display().to_string();

    assert!(report.success);
    assert!(report.simulated);
    assert_eq!(report.mode, "dual-boot");
    assert!(report.steps.iter().all(|s| s.completed && !s.executed));
    let names: Vec<&str> = report.steps.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "partitioning",
            "filesystem_format",
            "mount_hierarchy",
            "nixos_configuration",
            "nixos_install",
            "unmount"
        ]
    );
    // La ESP ajena (#1) se preserva y la raíz cae en el hueco libre (#3).
    assert_eq!(report.efi_partition, "/dev/nvme0n1p1");
    assert_eq!(report.root_partition, "/dev/nvme0n1p3");

    let c = &runner.commands;
    assert_eq!(c[0], "parted -s -m /dev/nvme0n1 unit MiB print free");
    assert!(c[1].starts_with("parted -s /dev/nvme0n1 mkpart antos-root ext4 "));
    assert_eq!(c[2], "partprobe /dev/nvme0n1");
    assert_eq!(c[3], "udevadm settle");
    assert_eq!(c[4], "parted -s -m /dev/nvme0n1 unit MiB print free");
    // Nunca mkfs.vfat sobre una ESP ajena.
    assert!(!c.iter().any(|x| x.starts_with("mkfs.vfat")));
    assert_eq!(c[5], "mkfs.ext4 -F -L antos-root /dev/nvme0n1p3");
    assert_eq!(c[6], "blkid -s UUID -o value /dev/nvme0n1p1");
    assert_eq!(c[7], "blkid -s UUID -o value /dev/nvme0n1p3");
    assert_eq!(c[8], format!("mount /dev/nvme0n1p3 {root}"));
    assert_eq!(c[9], format!("mount /dev/nvme0n1p1 {root}/boot"));
    assert_eq!(c[10], format!("nixos-generate-config --root {root}"));
    assert_eq!(
        c[11],
        format!(
            "nixos-install --root {root} --flake {root}/etc/nixos#antos-dual --no-root-passwd --no-channel-copy --override-input antos path:{root}/etc/nixos/antos --no-write-lock-file --option substitute true"
        )
    );
    assert_eq!(c[12], "sync");
    assert_eq!(c[13], format!("umount -R {root}"));
    assert_eq!(c.len(), 14);

    // UUIDs del `blkid` (simulado), no constantes inventadas.
    assert!(report.fstab_entries.iter().any(|l| l.contains("SIM1-ESP0")));
    assert!(!report.fstab_entries.iter().any(|l| l.contains("C12A-7328")));
    // El staging tiene la configuración; el hardware stub porque
    // `nixos-generate-config` simulado no escribe.
    assert!(staging.join("etc/nixos/flake.nix").exists());
    assert!(staging
        .join("etc/nixos/hardware-configuration.nix")
        .exists());
    assert!(!Path::new("/mnt/target").join("etc/nixos").exists());
    let _ = fs::remove_dir_all(&temp);
}

/// Disco completo sobre el SATA sintético: tabla nueva, ESP formateada.
#[test]
fn test_install_clean_command_sequence() {
    let temp = make_temp_test_dir("clean");
    let cfg = InstallConfig {
        target_device: "/dev/sda".into(),
        clean_install: true,
        hostname: "antos-primary".into(),
        ..InstallConfig::default()
    };
    let (report, runner) = run_simulated(&cfg, &temp);
    assert!(report.simulated);
    assert_eq!(report.mode, "clean");
    assert_eq!(report.efi_partition, "/dev/sda1");
    assert_eq!(report.root_partition, "/dev/sda2");
    let c = &runner.commands;
    assert_eq!(
        c[0],
        "parted -s /dev/sda mklabel gpt mkpart ESP fat32 1MiB 513MiB set 1 esp on mkpart antos-root ext4 513MiB 100%"
    );
    assert_eq!(c[1], "partprobe /dev/sda");
    assert_eq!(c[2], "udevadm settle");
    assert_eq!(c[3], "mkfs.vfat -F32 -n ANTOS_ESP /dev/sda1");
    assert_eq!(c[4], "mkfs.ext4 -F -L antos-root /dev/sda2");
    assert!(report.summary.contains("Simulación"));
    assert!(report.summary.contains("clean"));
    let _ = fs::remove_dir_all(&temp);
}

/// Dual-boot sobre un disco sin ESP: se rechaza antes de tocar nada.
#[test]
fn test_install_dual_boot_without_esp_is_refused() {
    let temp = make_temp_test_dir("noesp");
    let cfg = InstallConfig {
        target_device: "/dev/sda".into(),
        clean_install: false,
        ..InstallConfig::default()
    };
    let mut runner = simulated("/dev/sda");
    let err = DeployEngine::install(&cfg, &temp, &mut runner, &mut |_| {}).unwrap_err();
    assert!(err.to_string().contains("no tiene partición ESP"));
    assert_eq!(runner.commands.len(), 1, "solo el print free");
    let _ = fs::remove_dir_all(&temp);
}

/// Dual-boot sin hueco suficiente: dice cuánto falta y no particiona.
#[test]
fn test_install_dual_boot_without_enough_free_space_says_how_much() {
    let temp = make_temp_test_dir("nospace");
    let mut disk = DiskManager::inspect_disk("/dev/nvme0n1")
        .unwrap()
        .expect("nvme sintético");
    // Rellenar el disco casi entero con la NTFS: queda ~8 GiB libres.
    let used: u64 = disk.partitions.iter().map(|p| p.size_bytes).sum();
    let ntfs = disk.partitions.iter_mut().find(|p| !p.is_efi).unwrap();
    ntfs.size_bytes += disk.size_bytes - used - 8 * 1024 * 1024 * 1024;
    let mut runner = SimulatedRunner::for_disk(&disk);
    let cfg = InstallConfig {
        target_device: "/dev/nvme0n1".into(),
        clean_install: false,
        ..InstallConfig::default()
    };
    let err = DeployEngine::install(&cfg, &temp, &mut runner, &mut |_| {}).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("faltan"), "{msg}");
    assert!(msg.contains("no redimensiona"), "{msg}");
    assert!(!runner.commands.iter().any(|c| c.contains("mkpart")));
    let _ = fs::remove_dir_all(&temp);
}

/// Si `nixos-install` falla, el error es el del comando y el destino
/// queda desmontado.
#[test]
fn test_install_failure_after_mount_unmounts_and_propagates() {
    let temp = make_temp_test_dir("fail");
    let cfg = InstallConfig {
        target_device: "/dev/sda".into(),
        clean_install: true,
        ..InstallConfig::default()
    };
    let mut runner = simulated("/dev/sda").fail_on("nixos-install");
    let err = DeployEngine::install(&cfg, &temp, &mut runner, &mut |_| {}).unwrap_err();
    assert!(err.to_string().contains("nixos-install"));
    let root = temp.join("target/installer-staging").display().to_string();
    assert_eq!(
        runner.commands.last().unwrap(),
        &format!("umount -R {root}"),
        "tras el fallo se desmonta"
    );
    assert!(!runner.is_mountpoint(&temp.join("target/installer-staging")));
    let _ = fs::remove_dir_all(&temp);
}

/// Un `mkfs.ext4` que falla aborta antes de montar: sin `umount`.
#[test]
fn test_install_failure_before_mount_does_not_unmount() {
    let temp = make_temp_test_dir("fail-early");
    let cfg = InstallConfig {
        target_device: "/dev/sda".into(),
        clean_install: true,
        ..InstallConfig::default()
    };
    let mut runner = simulated("/dev/sda").fail_on("mkfs.ext4");
    let err = DeployEngine::install(&cfg, &temp, &mut runner, &mut |_| {}).unwrap_err();
    assert!(err.to_string().contains("mkfs.ext4"));
    assert!(!runner.commands.iter().any(|c| c.starts_with("mount ")));
    assert!(!runner.commands.iter().any(|c| c.starts_with("umount")));
    let _ = fs::remove_dir_all(&temp);
}

/// `deploy_system` con `dry_run` sigue siendo la simulación (lo que usan
/// `--config`, `--target` e `install.deploy`).
#[test]
fn test_deploy_system_dry_run_is_the_simulation() {
    let temp = make_temp_test_dir("deploy-dry");
    let cfg = InstallConfig {
        target_device: "/dev/nvme0n1".into(),
        clean_install: false,
        hostname: "antos-dual".into(),
        ..InstallConfig::default()
    };
    let report = DeployEngine::deploy_system(&cfg, &temp).expect("deploy");
    assert!(report.simulated);
    assert_eq!(report.steps.len(), 6);
    assert!(report.steps.iter().all(|s| !s.executed));
    assert!(report.summary.contains("el disco no se ha tocado"));
    assert!(temp
        .join("target/installer-staging/etc/nixos/flake.nix")
        .exists());
    let _ = fs::remove_dir_all(&temp);
}

/// `--apply` fuera de una ISO en vivo como root aborta ANTES de tocar
/// nada, con la lista de lo que falta.
#[test]
fn test_deploy_system_apply_aborts_before_touching_anything() {
    let temp = make_temp_test_dir("apply");
    let cfg = InstallConfig {
        target_device: "/dev/nvme0n1".into(),
        target_mount: temp.join("mnt").display().to_string(),
        dry_run: false,
        ..InstallConfig::default()
    };
    let err = DeployEngine::deploy_system(&cfg, &temp).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("no puede continuar"),
        "mensaje inesperado: {msg}"
    );
    // Nada escrito: ni staging, ni el destino.
    assert!(!temp.join("target").exists());
    assert!(!temp.join("mnt").exists());
    let _ = fs::remove_dir_all(&temp);
}

#[test]
fn test_real_install_preconditions_list_everything_missing() {
    let temp = make_temp_test_dir("precond");
    let cfg = InstallConfig {
        target_mount: "/definitivamente/no/montado".into(),
        dry_run: false,
        ..InstallConfig::default()
    };
    let missing = DeployEngine::real_install_preconditions(&cfg, &temp);
    // Sin árbol de antOS localizable desde un workspace temporal.
    assert!(missing.iter().any(|m| m.contains("árbol fuente de antOS")));
    if !cfg!(target_os = "linux") {
        assert!(missing
            .iter()
            .any(|m| m.contains("solo se ejecuta desde Linux")));
        // En macOS no hay `nixos-install` ni `/proc`.
        assert!(missing.iter().any(|m| m.contains("faltan en PATH")));
        assert!(missing.iter().any(|m| m.contains("root")));
    }
    let _ = fs::remove_dir_all(&temp);
}

/// `password_hash` va al `configuration.nix` como `initialHashedPassword`;
/// sin él, la contraseña de serie. Un hash con caracteres fuera de
/// `mkpasswd` se rechaza (no puede romper la cadena Nix).
#[test]
fn test_password_hash_goes_to_configuration_nix() {
    let temp = make_temp_test_dir("pw");
    let hash = "$y$j9T$abc./DEF$0123456789abcdefghijklmnopqrstuvwxyzABCDEFG";
    let cfg = InstallConfig {
        hostname: "antos-pw".into(),
        username: "juan".into(),
        password_hash: Some(hash.into()),
        ..InstallConfig::default()
    };
    let etc = temp.join("etc/nixos");
    DeployEngine::generate_nixos_config(&cfg, &etc, None).expect("generate");
    let conf = fs::read_to_string(etc.join("configuration.nix")).unwrap();
    assert!(conf.contains(&format!(r#"initialHashedPassword = "{hash}";"#)));
    assert!(!conf.contains("initialPassword"));

    let bad = InstallConfig {
        password_hash: Some("$y$j9T$abc\"; evil = true; # ".into()),
        ..cfg.clone()
    };
    let err = DeployEngine::generate_nixos_config(&bad, &temp.join("bad"), None).unwrap_err();
    assert!(err.to_string().contains("mkpasswd"));

    let stock = InstallConfig {
        password_hash: None,
        ..cfg
    };
    DeployEngine::generate_nixos_config(&stock, &temp.join("stock"), None).expect("generate");
    let conf = fs::read_to_string(temp.join("stock/configuration.nix")).unwrap();
    // Sin hash solo en simulación, y marcado como tal.
    assert!(conf.contains(r#"initialPassword = "antos";"#));
    assert!(conf.contains("SIMULACIÓN sin contraseña"));
    let _ = fs::remove_dir_all(&temp);
}

/// Un `hardware-configuration.nix` ya presente (el de
/// `nixos-generate-config`) no se pisa con el de relleno.
#[test]
fn test_generate_nixos_config_keeps_existing_hardware_configuration() {
    let temp = make_temp_test_dir("hw-keep");
    let etc = temp.join("etc/nixos");
    fs::create_dir_all(&etc).unwrap();
    fs::write(etc.join("hardware-configuration.nix"), "# real\n{ }\n").unwrap();
    let cfg = InstallConfig::default();
    DeployEngine::generate_nixos_config(&cfg, &etc, None).expect("generate");
    assert_eq!(
        fs::read_to_string(etc.join("hardware-configuration.nix")).unwrap(),
        "# real\n{ }\n"
    );
    let _ = fs::remove_dir_all(&temp);
}

#[test]
fn test_partition_device_follows_kernel_naming() {
    assert_eq!(
        DeployEngine::partition_device("/dev/nvme0n1", 3),
        "/dev/nvme0n1p3"
    );
    assert_eq!(DeployEngine::partition_device("/dev/sda", 3), "/dev/sda3");
}

/// `system/nixos/installed.nix` (la máquina de referencia cuya closure viaja
/// en la ISO, T36.3) y el `configuration.nix` que genera el instalador tienen
/// que pedir los mismos paquetes: si divergen, la instalación deja de ser
/// offline. Se fijan las líneas que determinan la closure.
#[test]
fn test_generated_configuration_matches_installed_nix() {
    let temp = make_temp_test_dir("installed-sync");
    let cfg = InstallConfig {
        clean_install: true,
        ..InstallConfig::default()
    };
    let etc = temp.join("etc/nixos");
    DeployEngine::generate_nixos_config(&cfg, &etc, None).expect("generate");
    let generated = fs::read_to_string(etc.join("configuration.nix")).unwrap();
    let hw = fs::read_to_string(etc.join("hardware-configuration.nix")).unwrap();
    let installed = fs::read_to_string(repo_root().join("system/nixos/installed.nix")).unwrap();

    let squash = |s: &str| s.split_whitespace().collect::<String>();
    let (generated, hw, installed) = (squash(&generated), squash(&hw), squash(&installed));
    for line in [
        "services.antos.enable = true;",
        "services.antos.desktop.enable = true;",
        "services.antos.machine = {",
        "enable = true;",
        r#"extraGroups = [ "wheel" "video" "input" "networkmanager" ];"#,
        r#"nix.settings.experimental-features = [ "nix-command" "flakes" ];"#,
        r#"system.stateVersion = "25.05";"#,
    ] {
        let l = squash(line);
        assert!(
            generated.contains(&l),
            "configuration.nix generado sin {line}"
        );
        assert!(installed.contains(&l), "installed.nix sin {line}");
    }
    for line in [
        r#"device = "/dev/disk/by-label/antos-root";"#,
        r#"device = "/dev/disk/by-label/ANTOS_ESP";"#,
        r#"boot.initrd.availableKernelModules = [ "nvme" "ahci" "xhci_pci" "usbhid" "sd_mod" "virtio_pci" "virtio_blk" ];"#,
    ] {
        let l = squash(line);
        assert!(
            hw.contains(&l),
            "hardware-configuration.nix de relleno sin {line}"
        );
        assert!(installed.contains(&l), "installed.nix sin {line}");
    }
    // Nada de lo que `machine.nix` ya aporta se duplica en ninguno de los dos.
    for dup in [
        "networking.networkmanager.enable",
        "boot.loader.systemd-boot.enable",
        "zramSwap.enable",
        "i18n.defaultLocale",
    ] {
        let d = squash(dup);
        assert!(
            !generated.contains(&d),
            "configuration.nix generado duplica {dup}"
        );
        assert!(!installed.contains(&d), "installed.nix duplica {dup}");
    }
    // Y la contraseña de desarrollo no está en la máquina física de referencia.
    assert!(!installed.contains(&squash(r#"initialPassword = "antos";"#)));
    let _ = fs::remove_dir_all(&temp);
}

/// La contraseña del asistente (T36.4) se convierte en hash con `mkpasswd`
/// a través del runner: la simulación graba el comando (sin la contraseña,
/// que solo va por stdin) y el `configuration.nix` lleva el hash. Y sin
/// contraseña la instalación real se rechaza en las precondiciones.
#[test]
fn test_install_with_password_hashes_through_mkpasswd() {
    let temp = make_temp_test_dir("pw-wizard");
    let cfg = InstallConfig {
        target_device: "/dev/sda".into(),
        clean_install: true,
        hostname: "antos-pw".into(),
        ..InstallConfig::default()
    };
    let secret = crate::crypto::SecretValue::new("contraseña-larga".into());
    let mut runner = simulated("/dev/sda");
    let report =
        DeployEngine::install_with_password(&cfg, &temp, &mut runner, &mut |_| {}, Some(&secret))
            .expect("install con contraseña");
    assert!(report.simulated);
    assert_eq!(runner.commands[0], "mkpasswd -m yescrypt --stdin");
    assert!(!runner
        .commands
        .iter()
        .any(|c| c.contains("contraseña-larga")));
    let conf =
        fs::read_to_string(temp.join("target/installer-staging/etc/nixos/configuration.nix"))
            .unwrap();
    assert!(conf.contains(r#"initialHashedPassword = "$y$j9T$SIMULADO"#));
    assert!(!conf.contains("contraseña-larga"));
    assert!(!conf.contains("SIMULACIÓN sin contraseña"));

    let missing = DeployEngine::real_install_preconditions(&cfg, &temp);
    assert!(missing.iter().any(|m| m.contains("sin contraseña")));
    let with_hash = InstallConfig {
        password_hash: Some("$y$j9T$x$y".into()),
        ..cfg.clone()
    };
    let missing = DeployEngine::real_install_preconditions(&with_hash, &temp);
    assert!(!missing.iter().any(|m| m.contains("sin contraseña")));
    let _ = fs::remove_dir_all(&temp);
}

/// Lo que va entre comillas al `configuration.nix` se valida antes de
/// escribir nada (T36.4): teclado, locale, zona horaria, hostname, usuario.
#[test]
fn test_validate_machine_settings_rejects_bad_values() {
    let ok = InstallConfig {
        keymap: "pt-br".into(),
        locale: "pt_BR.UTF-8".into(),
        timezone: "America/Sao_Paulo".into(),
        hostname: "meu-pc-1".into(),
        username: "ana_1".into(),
        ..InstallConfig::default()
    };
    DeployEngine::validate_machine_settings(&ok).expect("valores correctos");
    let bad_cases = [
        (
            "keymap",
            InstallConfig {
                keymap: "es\"; x = 1".into(),
                ..ok.clone()
            },
        ),
        (
            "keymap",
            InstallConfig {
                keymap: "ES".into(),
                ..ok.clone()
            },
        ),
        (
            "locale",
            InstallConfig {
                locale: "es ES".into(),
                ..ok.clone()
            },
        ),
        (
            "timezone",
            InstallConfig {
                timezone: "Europe/Madrid\"".into(),
                ..ok.clone()
            },
        ),
        (
            "hostname",
            InstallConfig {
                hostname: "mi pc".into(),
                ..ok.clone()
            },
        ),
        (
            "username",
            InstallConfig {
                username: "Juan".into(),
                ..ok.clone()
            },
        ),
        (
            "username",
            InstallConfig {
                username: "".into(),
                ..ok.clone()
            },
        ),
    ];
    for (field, bad) in bad_cases {
        assert!(
            DeployEngine::validate_machine_settings(&bad).is_err(),
            "{field} inválido debería fallar"
        );
        let etc = make_temp_test_dir("validate");
        assert!(
            DeployEngine::generate_nixos_config(&bad, &etc, None).is_err(),
            "{field} inválido no debe llegar al configuration.nix"
        );
        let _ = fs::remove_dir_all(&etc);
    }
}

/// `encrypt = true` está declarado pero no ejecutable: la instalación
/// real lo rechaza en las precondiciones en vez de ignorarlo.
#[test]
fn test_encrypt_is_refused_until_implemented() {
    let temp = make_temp_test_dir("encrypt");
    let cfg = InstallConfig {
        encrypt: true,
        password_hash: Some("$y$j9T$x$y".into()),
        ..InstallConfig::default()
    };
    let missing = DeployEngine::real_install_preconditions(&cfg, &temp);
    assert!(missing.iter().any(|m| m.contains("LUKS")));
    let _ = fs::remove_dir_all(&temp);
}

/// El `--override-input nixpkgs` de `nixos-install` (primer smoke real):
/// entrada `path:` a la fuente que la ISO expone, con `rev` y
/// `lastModified` del `flake.lock` del árbol para que el `toplevel`
/// coincida con la closure de la ISO. Sin árbol, solo la ruta.
#[test]
fn test_nixpkgs_override_carries_rev_and_last_modified_from_the_lock() {
    let root = repo_root();
    let (rev, node) = DeployEngine::locked_nixpkgs(&root).expect("flake.lock real");
    let lm = node["locked"]["lastModified"]
        .as_u64()
        .expect("lastModified");
    let url = DeployEngine::nixpkgs_override(Path::new("/nix/store/abc-source"), Some(&root));
    assert_eq!(
        url,
        format!("path:/nix/store/abc-source?rev={rev}&lastModified={lm}")
    );
    assert_eq!(
        DeployEngine::nixpkgs_override(Path::new("/nix/store/abc-source"), None),
        "path:/nix/store/abc-source"
    );
    // Sin `/etc/antos/nixpkgs-source` (esta máquina) no hay override, y la
    // secuencia simulada no lo lleva.
    if std::env::var_os("ANTOS_NIXPKGS_SOURCE").is_none()
        && !Path::new("/etc/antos/nixpkgs-source").exists()
    {
        assert!(DeployEngine::locate_nixpkgs_source().is_none());
    }
}
