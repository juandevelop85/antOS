//! Motor de Instalación y Despliegue de Sistema Base (T15.2 / T30.5 / T36.1).
//!
//! ## Estado de implementación
//!
//! - **Simulación (`dry_run = true`)**: es lo que hace todo el fichero hoy.
//!   Planifica el particionado, genera `/etc/fstab`, la configuración
//!   declarativa de antOS Linux (`/etc/nixos/{flake.nix,flake.lock,
//!   configuration.nix,hardware-configuration.nix}` + copia del árbol
//!   fuente de antOS) y un informe con cada paso. Todo se escribe en un
//!   directorio de *staging* dentro del workspace; el disco no se toca.
//!   Cada `InstallStep` sale con `executed = false` y el informe con
//!   `simulated = true`.
//! - **Instalación real (`dry_run = false`)**: **no implementada** (T36.2).
//!   [`DeployEngine::deploy_system`] comprueba las precondiciones (Linux,
//!   `root`, herramientas en `PATH`, destino montado) y, si todas se
//!   cumplen, aborta con un error explícito antes de escribir nada. Hasta
//!   T36.1 este camino escribía ficheros en un directorio sin montar, con
//!   UUIDs inventados, y declaraba «`nixos-install` ejecutado» sin haber
//!   lanzado ningún comando.
//!
//! Lo que sí es real en ambos modos es la **configuración generada**: el
//! `flake.nix` evalúa contra el árbol de antOS (la CI lo comprueba con
//! `nix eval`), y es exactamente lo que T36.2 pasará a `nixos-install`.

use super::disk::DiskManager;
use antos_protocol::{InstallConfig, InstallReport, InstallStep};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Herramientas que una instalación real necesita en `PATH` (T36.1). Se
/// buscan recorriendo `PATH` a mano, sin `which` ni `sh -c` (T31.4).
const REAL_INSTALL_TOOLS: &[&str] = &[
    "parted",
    "mkfs.vfat",
    "mkfs.ext4",
    "blkid",
    "mount",
    "umount",
    "nixos-generate-config",
    "nixos-install",
];

/// Lo que se copia del árbol de antOS a `/etc/nixos/antos` (T36.1): el
/// `fileset` que `system/nixos/package.nix` y `barra.nix` declaran, más el
/// propio `flake.{nix,lock}` y lo que `desktop.nix` lee de `system/`
/// (`system/desktop/assets`, `rc.xml`, …). Se copia `system/` entero
/// porque es más fácil de mantener honesto que una lista que se desactualiza
/// cada vez que un `include_str!` nuevo entra en `antosd`; lo que se
/// excluye son los directorios de compilación.
const ANTOS_SOURCE_ENTRIES: &[&str] = &[
    "flake.nix",
    "flake.lock",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "recipes",
    "builder/Cargo.toml",
    "builder/src",
    "system",
];

/// Directorios que nunca se copian del árbol fuente.
const ANTOS_SOURCE_SKIP_DIRS: &[&str] = &["target", ".git", "node_modules"];

pub struct DeployEngine;

impl DeployEngine {
    /// Valida que el dispositivo destino cumpla los requisitos de instalación.
    pub fn prepare_target(config: &InstallConfig) -> Result<PathBuf> {
        let dev = match DiskManager::inspect_disk(&config.target_device)? {
            Some(d) => d,
            None => bail!(
                "Dispositivo destino «{}» no encontrado",
                config.target_device
            ),
        };

        if dev.is_read_only {
            bail!(
                "El dispositivo «{}» es de solo lectura y no puede recibir una instalación",
                config.target_device
            );
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

    /// Genera las entradas declarativas de `/etc/fstab` basadas en UUIDs
    /// persistentes. La ESP va en `/boot`, que es donde NixOS
    /// (`boot.loader.efi.efiSysMountPoint`) y `systemd-boot` la esperan;
    /// `/boot/efi` era la disposición del kernel bare-metal.
    pub fn generate_fstab(root_uuid: &str, efi_uuid: &str, swap_uuid: Option<&str>) -> String {
        let mut lines = Vec::new();
        lines.push("# /etc/fstab: antOS base filesystem mount configuration".to_string());
        lines.push("# <file system>                           <mount point>   <type>  <options>                  <dump> <pass>".to_string());
        lines.push(format!(
            "UUID={:<36} /               ext4    defaults,noatime,discard   0      1",
            root_uuid
        ));
        lines.push(format!(
            "UUID={:<36} /boot           vfat    umask=0077,shortname=winnt 0      2",
            efi_uuid
        ));
        if let Some(sw) = swap_uuid {
            lines.push(format!(
                "UUID={:<36} none            swap    sw                         0      0",
                sw
            ));
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
        fs::write(etc_dir.join("os-release"), os_release).context("Escribiendo /etc/os-release")?;

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

    /// Localiza el árbol fuente de antOS con el que se genera `/etc/nixos`
    /// (T36.1), en este orden: `ANTOS_SOURCE` (desarrollo), el workspace si
    /// es el propio repositorio (tiene `flake.nix` y `system/nixos`) o su
    /// padre (la disposición de desarrollo: `<repo>/workspace`), y
    /// `/etc/antos/source` (lo que la ISO lleva, T36.3). `None` si no hay
    /// ninguno: el `flake.nix` generado lo dice en un comentario y la
    /// instalación real no puede continuar sin él.
    pub fn locate_antos_source(workspace: &Path) -> Option<PathBuf> {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(v) = std::env::var("ANTOS_SOURCE") {
            if !v.is_empty() {
                candidates.push(PathBuf::from(v));
            }
        }
        candidates.push(workspace.to_path_buf());
        if let Some(parent) = workspace.parent() {
            candidates.push(parent.to_path_buf());
        }
        candidates.push(PathBuf::from("/etc/antos/source"));
        candidates
            .into_iter()
            .find(|c| c.join("flake.nix").is_file() && c.join("system/nixos").is_dir())
    }

    /// Lee la revisión de `nixpkgs` fijada en el `flake.lock` del árbol
    /// fuente, para que el sistema instalado use exactamente el nixpkgs con
    /// el que se construyó la ISO. Devuelve `(rev, nodo JSON)`.
    fn locked_nixpkgs(source: &Path) -> Option<(String, serde_json::Value)> {
        let raw = fs::read_to_string(source.join("flake.lock")).ok()?;
        let lock: serde_json::Value = serde_json::from_str(&raw).ok()?;
        let node = lock.get("nodes")?.get("nixpkgs")?.clone();
        let locked = node.get("locked")?;
        if locked.get("type")?.as_str()? != "github" {
            return None;
        }
        let rev = locked.get("rev")?.as_str()?.to_string();
        Some((rev, node))
    }

    /// Copia recursiva de un directorio saltando los de compilación.
    fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
        fs::create_dir_all(dst).with_context(|| format!("Creando {}", dst.display()))?;
        for entry in fs::read_dir(src).with_context(|| format!("Leyendo {}", src.display()))? {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            let from = entry.path();
            let to = dst.join(&name);
            let kind = entry.file_type()?;
            if kind.is_dir() {
                if ANTOS_SOURCE_SKIP_DIRS.contains(&name_str.as_ref()) {
                    continue;
                }
                Self::copy_tree(&from, &to)?;
            } else if kind.is_file() {
                fs::copy(&from, &to)
                    .with_context(|| format!("Copiando {} → {}", from.display(), to.display()))?;
            }
            // Enlaces simbólicos (p. ej. `result` de nix) se ignoran: no
            // forman parte del árbol que los paquetes Nix declaran.
        }
        Ok(())
    }

    /// Copia a `dst` el subconjunto del árbol de antOS que el `flake.nix`
    /// generado necesita para evaluar y construir sin red
    /// ([`ANTOS_SOURCE_ENTRIES`]). Falla si `src` no parece el árbol de
    /// antOS.
    pub fn copy_antos_source(src: &Path, dst: &Path) -> Result<()> {
        if !src.join("flake.nix").is_file() || !src.join("system/nixos").is_dir() {
            bail!(
                "«{}» no es el árbol fuente de antOS (falta flake.nix o system/nixos)",
                src.display()
            );
        }
        if dst.exists() {
            fs::remove_dir_all(dst)
                .with_context(|| format!("Limpiando copia anterior en {}", dst.display()))?;
        }
        fs::create_dir_all(dst)?;
        for rel in ANTOS_SOURCE_ENTRIES {
            let from = src.join(rel);
            let to = dst.join(rel);
            if !from.exists() {
                // `rust-toolchain.toml` o `builder/` pueden faltar en un árbol
                // recortado; lo imprescindible se validó arriba.
                continue;
            }
            if from.is_dir() {
                Self::copy_tree(&from, &to)?;
            } else {
                if let Some(parent) = to.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&from, &to)
                    .with_context(|| format!("Copiando {} → {}", from.display(), to.display()))?;
            }
        }
        Ok(())
    }

    /// Genera la configuración declarativa de **antOS Linux** (NixOS) en
    /// `<etc_nixos_dir>`: `flake.nix`, `flake.lock` (si hay fuente),
    /// `configuration.nix`, un `hardware-configuration.nix` de relleno (el
    /// real lo escribe `nixos-generate-config --root <target>` en la
    /// instalación de verdad, T36.2) y, si `antos_source` apunta al árbol de
    /// antOS, una copia de ese árbol en `<etc_nixos_dir>/antos`.
    ///
    /// El `flake.nix` consume el flake de antOS por sus salidas públicas
    /// (`nixosModules.{default,desktop,llm}`, `overlays.default`), nunca por
    /// rutas internas del árbol: así un renombrado dentro de `system/nixos/`
    /// no rompe las máquinas instaladas. El `configuration.nix` activa
    /// `services.antos.desktop` con el `autologinUser`, hostname, timezone y
    /// keymap elegidos en el asistente, y `systemd-boot` en la ESP —que en
    /// modo dual-boot detecta y encadena los demás SO sin tocar sus
    /// entradas—.
    pub fn generate_nixos_config(
        config: &InstallConfig,
        etc_nixos_dir: &Path,
        antos_source: Option<&Path>,
    ) -> Result<()> {
        fs::create_dir_all(etc_nixos_dir).context("Creando /etc/nixos en target")?;

        // ── Fuente de antOS y nixpkgs fijado ─────────────────────────────
        let (nixpkgs_url, nixpkgs_note, nixpkgs_node) = match antos_source
            .and_then(Self::locked_nixpkgs)
        {
            Some((rev, node)) => (
                format!("github:NixOS/nixpkgs/{rev}"),
                "# nixpkgs fijado a la misma revisión con la que se construyó esta imagen\n    # (flake.lock del árbol de antOS): sin red, sin sorpresas.".to_string(),
                Some(node),
            ),
            None => (
                "github:NixOS/nixpkgs/nixos-unstable".to_string(),
                "# ATENCIÓN: no había flake.lock de antOS al generar esto; nixpkgs sin fijar.".to_string(),
                None,
            ),
        };

        let source_note = match antos_source {
            Some(src) => format!(
                "# El árbol de antOS se copió a /etc/nixos/antos desde {} (sin red).",
                src.display()
            ),
            None => "# ATENCIÓN: no se encontró el árbol fuente de antOS (ANTOS_SOURCE,\n  # el workspace o /etc/antos/source); /etc/nixos/antos está VACÍO y este\n  # flake no evaluará hasta que se copie ahí.".to_string(),
        };

        let flake = format!(
            r#"{{
  description = "antOS Linux — máquina instalada (generado por `antos install`, T36.1)";

  inputs = {{
    {nixpkgs_note}
    nixpkgs.url = "{nixpkgs_url}";
    {source_note}
    antos.url = "path:/etc/nixos/antos";
    antos.inputs.nixpkgs.follows = "nixpkgs";
  }};

  outputs = {{ self, nixpkgs, antos }}: {{
    nixosConfigurations."{hostname}" = nixpkgs.lib.nixosSystem {{
      system = "{system}";
      modules = [
        antos.nixosModules.default
        antos.nixosModules.desktop
        antos.nixosModules.llm
        {{ nixpkgs.overlays = [ antos.overlays.default ]; }}
        ./configuration.nix
      ];
    }};
  }};
}}
"#,
            hostname = config.hostname,
            system = config.system,
        );
        fs::write(etc_nixos_dir.join("flake.nix"), flake)
            .context("Escribiendo /etc/nixos/flake.nix")?;

        // `flake.lock` parcial: solo el nodo `nixpkgs`, copiado tal cual del
        // árbol de antOS (narHash incluido). `nix` añade el nodo `antos`
        // (entrada `path:`, local) la primera vez que evalúa; lo que no
        // vuelve a resolver es nixpkgs.
        if let Some(mut node) = nixpkgs_node {
            // `original` tiene que coincidir con lo que pide el `flake.nix`
            // generado (`github:NixOS/nixpkgs/<rev>`); si se dejara el
            // `ref = nixos-unstable` del árbol, nix vería una entrada
            // distinta y volvería a resolverla por red.
            if let Some(rev) = node
                .get("locked")
                .and_then(|l| l.get("rev"))
                .and_then(|r| r.as_str())
                .map(str::to_owned)
            {
                node["original"] = serde_json::json!({
                    "owner": "NixOS",
                    "repo": "nixpkgs",
                    "rev": rev,
                    "type": "github"
                });
            }
            let lock = serde_json::json!({
                "nodes": {
                    "nixpkgs": node,
                    "root": { "inputs": { "nixpkgs": "nixpkgs" } }
                },
                "root": "root",
                "version": 7
            });
            fs::write(
                etc_nixos_dir.join("flake.lock"),
                serde_json::to_string_pretty(&lock)?,
            )
            .context("Escribiendo /etc/nixos/flake.lock")?;
        }

        // ── configuration.nix ────────────────────────────────────────────
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
            r#"# antOS Linux · configuración de la máquina (generada por `antos install`, T30.5/T36.1).
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

        // ── hardware-configuration.nix de relleno ────────────────────────
        // En la instalación real lo sobrescribe `nixos-generate-config
        // --root <target>` (T36.2). Declara la raíz y la ESP por las
        // etiquetas que T36.2 pone al formatear (`antos-root`, `ANTOS_ESP`),
        // para que el flake evalúe completo sin ficheros extra.
        let hw_stub = format!(
            r#"# PLACEHOLDER — lo reemplaza `nixos-generate-config --root <target>`.
{{ lib, modulesPath, ... }}:
{{
  imports = [ (modulesPath + "/installer/scan/not-detected.nix") ];
  boot.initrd.availableKernelModules = [ "nvme" "ahci" "xhci_pci" "usbhid" "sd_mod" "virtio_pci" "virtio_blk" ];
  boot.loader.grub.enable = lib.mkDefault false;
  fileSystems."/" = {{
    device = "/dev/disk/by-label/antos-root";
    fsType = "ext4";
  }};
  fileSystems."/boot" = {{
    device = "/dev/disk/by-label/ANTOS_ESP";
    fsType = "vfat";
    options = [ "fmask=0077" "dmask=0077" ];
  }};
  nixpkgs.hostPlatform = lib.mkDefault "{system}";
}}
"#,
            system = config.system
        );
        fs::write(etc_nixos_dir.join("hardware-configuration.nix"), hw_stub)
            .context("Escribiendo /etc/nixos/hardware-configuration.nix")?;

        // ── Copia del árbol de antOS ─────────────────────────────────────
        if let Some(src) = antos_source {
            Self::copy_antos_source(src, &etc_nixos_dir.join("antos"))?;
        }

        Ok(())
    }

    /// Busca un ejecutable recorriendo `PATH`, sin `which` ni intérprete.
    fn tool_in_path(name: &str) -> bool {
        let Some(path) = std::env::var_os("PATH") else {
            return false;
        };
        std::env::split_paths(&path).any(|dir| dir.join(name).is_file())
    }

    /// `true` si `path` aparece como punto de montaje en `/proc/mounts`.
    fn is_mountpoint(path: &str) -> bool {
        let Ok(mounts) = fs::read_to_string("/proc/mounts") else {
            return false;
        };
        mounts
            .lines()
            .filter_map(|l| l.split_whitespace().nth(1))
            .any(|mp| mp == path)
    }

    /// Precondiciones de una instalación real (T36.1): Linux, `root`, las
    /// herramientas de [`REAL_INSTALL_TOOLS`] en `PATH` y el destino
    /// montado. Devuelve la lista completa de lo que falta, no solo el
    /// primer fallo, para que el usuario lo arregle de una vez.
    pub fn real_install_preconditions(config: &InstallConfig) -> Vec<String> {
        let mut missing = Vec::new();
        if !cfg!(target_os = "linux") {
            missing
                .push("la instalación a disco solo se ejecuta desde Linux (la ISO en vivo)".into());
        }
        // `euid` por `/proc/self/status` (no hay `libc::geteuid` que
        // justificar aquí; en macOS `/proc` no existe y ya falló arriba).
        let is_root = fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("Uid:"))
                    .and_then(|l| l.split_whitespace().nth(2).map(|euid| euid == "0"))
            })
            .unwrap_or(false);
        if !is_root {
            missing.push("hace falta ejecutar como root (sudo antos install --apply)".into());
        }
        let tools: Vec<&str> = REAL_INSTALL_TOOLS
            .iter()
            .copied()
            .filter(|t| !Self::tool_in_path(t))
            .collect();
        if !tools.is_empty() {
            missing.push(format!("faltan en PATH: {}", tools.join(", ")));
        }
        if !Self::is_mountpoint(&config.target_mount) {
            missing.push(format!(
                "«{}» no es un punto de montaje (la raíz del destino tiene que estar montada ahí)",
                config.target_mount
            ));
        }
        missing
    }

    /// Ejecuta o simula el despliegue completo del sistema base.
    ///
    /// Con `dry_run = false` **no instala** (ver la cabecera del módulo):
    /// verifica las precondiciones y aborta antes de tocar el disco. Con
    /// `dry_run = true` escribe todo en `<workspace>/target/installer-staging`
    /// y devuelve un informe `simulated = true` con cada paso
    /// `executed = false`.
    pub fn deploy_system(config: &InstallConfig, workspace: &Path) -> Result<InstallReport> {
        let _mount_target = Self::prepare_target(config)?;

        if !config.dry_run {
            let missing = Self::real_install_preconditions(config);
            if !missing.is_empty() {
                bail!(
                    "La instalación real no puede continuar:\n  - {}",
                    missing.join("\n  - ")
                );
            }
            // Todo está en su sitio y aun así no se hace nada: el
            // particionado, el formateo y `nixos-install` reales son T36.2.
            // Antes de T36.1 este camino escribía en un directorio sin
            // montar y anunciaba el éxito.
            bail!(
                "La instalación a disco todavía no está implementada (T36.2). \
                 Nada se ha escrito en «{}». Usa `antos install` sin `--apply` \
                 para ver la simulación completa.",
                config.target_device
            );
        }

        // ── Simulación ───────────────────────────────────────────────────
        // Ningún paso de aquí abajo toca el disco: `executed` es `false` en
        // todos. T36.2 irá poniéndolo a `true` paso a paso, a medida que
        // cada uno ejecute su comando de verdad.
        let executed = false;
        let mut steps = Vec::new();

        // 1. Planificación de particiones
        let plan = DiskManager::plan_partitioning(&config.target_device, config.clean_install)?;
        steps.push(InstallStep {
            name: "partitioning".into(),
            description: if config.clean_install {
                format!(
                    "Simulación: tabla GPT limpia en {} (512 MiB ESP + raíz)",
                    config.target_device
                )
            } else {
                format!(
                    "Simulación: se preserva la ESP existente y se asigna la raíz en el hueco libre de {}",
                    config.target_device
                )
            },
            completed: true,
            executed,
        });

        // 2. Formateo (UUIDs de ejemplo: los reales los da `blkid` en T36.2)
        let efi_uuid = "C12A-7328";
        let root_uuid = "3a8d8e62-f72b-4e1b-9721-a1e4c7d81234";
        let swap_uuid = if plan.swap_partition_bytes > 0 {
            Some("b2c3d4e5-6789-0123-4567-89abcdef0123")
        } else {
            None
        };
        steps.push(InstallStep {
            name: "filesystem_format".into(),
            description: "Simulación: ESP (mkfs.vfat -F32 -n ANTOS_ESP), raíz (mkfs.ext4 -L antos-root); UUIDs de ejemplo".into(),
            completed: true,
            executed,
        });

        // 3. Montaje
        let (efi_part, root_part) = Self::partition_names(&config.target_device);
        steps.push(InstallStep {
            name: "mount_hierarchy".into(),
            description: format!(
                "Simulación: raíz en {mnt} y ESP en {mnt}/boot",
                mnt = config.target_mount
            ),
            completed: true,
            executed,
        });

        // 4. Staging del sistema base (siempre dentro del workspace)
        let staging_dir = workspace.join("target/installer-staging");
        if staging_dir.exists() {
            fs::remove_dir_all(&staging_dir).context("Limpiando el staging anterior")?;
        }
        fs::create_dir_all(&staging_dir)?;
        for d in &[
            "bin",
            "etc",
            "usr/local/bin",
            "etc/antos/capabilities",
            "boot",
            "var/log/antos",
        ] {
            let _ = fs::create_dir_all(staging_dir.join(d));
        }
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
            description: format!(
                "Simulación: capacidades declarativas y utilidades base en {}",
                staging_dir.display()
            ),
            completed: true,
            executed,
        });

        // 5. fstab, hostname, timezone
        let fstab_content = Self::generate_fstab(root_uuid, efi_uuid, swap_uuid);
        fs::write(staging_dir.join("etc/fstab"), &fstab_content)?;
        Self::generate_system_config(config, &staging_dir)?;
        steps.push(InstallStep {
            name: "system_configuration".into(),
            description: format!(
                "Simulación: fstab (UUIDs de ejemplo), hostname ({}), usuario ({})",
                config.hostname, config.username
            ),
            completed: true,
            executed,
        });

        // 6. Configuración declarativa de antOS Linux (NixOS) en /etc/nixos.
        // Esto sí es lo real: es el flake que T36.2 pasará a `nixos-install`.
        let antos_source = Self::locate_antos_source(workspace);
        let etc_nixos = staging_dir.join("etc/nixos");
        Self::generate_nixos_config(config, &etc_nixos, antos_source.as_deref())?;
        steps.push(InstallStep {
            name: "nixos_configuration".into(),
            description: format!(
                "flake.nix + configuration.nix en {} (services.antos.desktop, autologin «{}», keymap «{}», sistema «{}», systemd-boot{}; fuente de antOS: {})",
                etc_nixos.display(),
                config.username,
                config.keymap,
                config.system,
                if config.clean_install { "" } else { ", dual-boot" },
                antos_source
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "NO ENCONTRADA".into()),
            ),
            completed: true,
            executed,
        });

        // 7. nixos-generate-config + nixos-install: solo descripción (T36.2).
        let host = &config.hostname;
        let mnt = &config.target_mount;
        steps.push(InstallStep {
            name: "nixos_install".into(),
            description: format!(
                "Simulación: `nixos-generate-config --root {mnt}` + `nixos-install --root {mnt} --flake {mnt}/etc/nixos#{host} --no-root-passwd` (no implementado: T36.2)"
            ),
            completed: true,
            executed,
        });

        let mode = if config.clean_install {
            "clean"
        } else {
            "dual-boot"
        };
        let simulated = steps.iter().any(|s| !s.executed);
        let summary = if simulated {
            format!(
                "Simulación de instalación de antOS en «{}» completada (Modo: {}); el disco no se ha tocado",
                config.target_device, mode
            )
        } else {
            format!(
                "Instalación de antOS completada en «{}» (Modo: {})",
                config.target_device, mode
            )
        };

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
            simulated,
        })
    }

    /// Nombres de la ESP y la raíz para un dispositivo: `p1`/`p2` si el
    /// nombre acaba en dígito (`nvme0n1`, `mmcblk0`), `1`/`2` si no (`sda`).
    pub fn partition_names(device: &str) -> (String, String) {
        let sep = if device
            .chars()
            .last()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            "p"
        } else {
            ""
        };
        (format!("{device}{sep}1"), format!("{device}{sep}2"))
    }
}

#[cfg(test)]
mod tests {
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
            dry_run: true,
        };
        let etc_nixos = temp.join("etc/nixos");
        DeployEngine::generate_nixos_config(&cfg, &etc_nixos, None).expect("generate nixos config");

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
        assert!(flake.contains(r#"system = "aarch64-linux";"#));
        assert!(flake.contains("antos.nixosModules.desktop"));
        // `desktop.nix` fija `services.antos.llm.enable`: sin el módulo `llm`
        // la evaluación falla con «option does not exist».
        assert!(flake.contains("antos.nixosModules.llm"));
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
        assert!(!flake.contains("callPackage"));
        assert!(!flake.contains("system/nixos/"));

        let root_flake = fs::read_to_string(repo_root().join("flake.nix")).unwrap();
        assert!(root_flake.contains("overlays.default = overlayAntos;"));
        for module in ["default", "desktop", "llm"] {
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
            serde_json::from_str(&fs::read_to_string(etc_nixos.join("flake.lock")).unwrap())
                .unwrap();
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
            system: "x86_64-linux".into(),
            dry_run: true,
        };

        let report = DeployEngine::deploy_system(&cfg, &temp).expect("deploy");
        assert!(report.success);
        assert_eq!(report.mode, "dual-boot");
        assert_eq!(report.steps.len(), 7);
        assert!(report.steps.iter().any(|s| s.name == "nixos_configuration"));
        assert!(report.steps.iter().any(|s| s.name == "nixos_install"));
        assert!(report.steps.iter().all(|s| s.completed));
        // Honestidad (T36.1): una simulación no ejecuta nada y lo dice.
        assert!(report.simulated);
        assert!(report.steps.iter().all(|s| !s.executed));
        assert!(report.summary.contains("Simulación"));
        assert!(report.summary.contains("dual-boot"));
        assert!(!report.summary.contains("éxito"));
        assert!(report
            .steps
            .iter()
            .all(|s| !s.description.contains("ejecutado")));
        assert_eq!(report.efi_partition, "/dev/nvme0n1p1");
        assert_eq!(report.root_partition, "/dev/nvme0n1p2");
        // Todo el staging cae dentro del workspace, nunca en target_mount.
        assert!(temp
            .join("target/installer-staging/etc/nixos/flake.nix")
            .exists());
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
            system: "x86_64-linux".into(),
            dry_run: true,
        };

        let report = DeployEngine::deploy_system(&cfg, &temp).expect("deploy clean");
        assert!(report.success);
        assert!(report.simulated);
        assert_eq!(report.mode, "clean");
        assert!(report.summary.contains("clean"));
        assert_eq!(report.efi_partition, "/dev/sda1");
        assert_eq!(report.root_partition, "/dev/sda2");
        let _ = fs::remove_dir_all(&temp);
    }

    /// `--apply` en cualquier máquina que no sea una ISO en vivo como root
    /// aborta ANTES de escribir nada, con la lista de lo que falta. Y aunque
    /// todo estuviera, la instalación real es T36.2: nunca se llega a un
    /// informe con `dry_run = false`.
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
            msg.contains("no puede continuar") || msg.contains("no está implementada"),
            "mensaje inesperado: {msg}"
        );
        // Nada escrito: ni staging, ni /etc en el destino.
        assert!(!temp.join("target").exists());
        assert!(!temp.join("mnt").exists());
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_real_install_preconditions_list_everything_missing() {
        let cfg = InstallConfig {
            target_mount: "/definitivamente/no/montado".into(),
            dry_run: false,
            ..InstallConfig::default()
        };
        let missing = DeployEngine::real_install_preconditions(&cfg);
        assert!(missing
            .iter()
            .any(|m| m.contains("no es un punto de montaje")));
        if !cfg!(target_os = "linux") {
            assert!(missing
                .iter()
                .any(|m| m.contains("solo se ejecuta desde Linux")));
            // En macOS no hay `nixos-install` ni `/proc`.
            assert!(missing.iter().any(|m| m.contains("faltan en PATH")));
            assert!(missing.iter().any(|m| m.contains("root")));
        }
    }
}
