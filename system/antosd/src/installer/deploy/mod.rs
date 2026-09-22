//! Motor de Instalación y Despliegue de Sistema Base (T15.2 / T30.5 / T36.1 / T36.2).
//!
//! ## Estado de implementación
//!
//! La instalación real y la simulación recorren **la misma tubería**
//! ([`DeployEngine::install`]) sobre un [`InstallRunner`]: particionado
//! (`parted`), formateo (`mkfs.vfat` / `mkfs.ext4`), UUIDs (`blkid`),
//! montaje en `target_mount` (raíz) y `target_mount/boot` (ESP),
//! `nixos-generate-config`, la configuración declarativa de antOS Linux
//! (`/etc/nixos/{flake.nix,flake.lock,configuration.nix}` + copia del árbol
//! fuente), `nixos-install --flake` y desmontaje.
//!
//! - **Simulación (`dry_run = true`)**: [`SimulatedRunner`] graba cada
//!   comando tal cual se lanzaría y contesta con salidas sintéticas; los
//!   ficheros se escriben en `<workspace>/target/installer-staging` y el
//!   disco no se toca. Cada `InstallStep` sale con `executed = false` y el
//!   informe con `simulated = true`, y la descripción de cada paso es el
//!   comando literal que la instalación real ejecutaría.
//! - **Instalación real (`dry_run = false`)**: [`SystemRunner`] lanza los
//!   procesos. Antes se comprueban las precondiciones (Linux, `root`,
//!   herramientas en `PATH`, árbol fuente de antOS localizable, destino sin
//!   montar); cualquier fallo posterior aborta con el error real del
//!   comando y, si ya había algo montado, se desmonta (`umount -R`) antes
//!   de devolver el error. Ningún «✓» sin comando ejecutado.
//!
//! Lo que **no** hace: redimensionar particiones ajenas (en dual-boot la
//! raíz va al mayor hueco libre, y si no hay ≥ 20 GiB se dice cuánto
//! falta), cifrar (LUKS), ni elegir otro sistema de ficheros. La
//! contraseña del usuario entra como `password_hash` y va al
//! `configuration.nix` (`initialHashedPassword`), no por `chpasswd`.
//!
//! La verificación end-to-end (ISO → disco → arranque al escritorio) es
//! `system/nixos/install-smoke.sh` en QEMU; los tests de este módulo fijan
//! la secuencia exacta de comandos con el runner simulado.

use super::disk::DiskManager;
use super::runner::{parse_parted_print_free, InstallRunner, SimulatedRunner, SystemRunner};
use antos_protocol::{DiskDevice, InstallConfig, InstallReport, InstallStep};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Tamaño mínimo del hueco libre para la raíz en dual-boot (20 GiB).
pub const MIN_ROOT_MIB: u64 = 20 * 1024;
/// Tamaño de la ESP que crea la instalación limpia.
pub const ESP_MIB: u64 = 512;

/// Herramientas que una instalación real necesita en `PATH` (T36.1). Se
/// buscan recorriendo `PATH` a mano, sin `which` ni `sh -c` (T31.4).
const REAL_INSTALL_TOOLS: &[&str] = &[
    "parted",
    "partprobe",
    "udevadm",
    "mkfs.vfat",
    "mkfs.ext4",
    "blkid",
    "mount",
    "umount",
    "sync",
    "nixos-generate-config",
    "nixos-install",
    "mkpasswd",
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
        Self::inspect_target(config).map(|_| PathBuf::from(&config.target_mount))
    }

    /// Como [`DeployEngine::prepare_target`], devolviendo el disco.
    pub fn inspect_target(config: &InstallConfig) -> Result<DiskDevice> {
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

        Ok(dev)
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

    /// La fuente de nixpkgs que la ISO expone en `/etc/antos/nixpkgs-source`
    /// (T36.3; `ANTOS_NIXPKGS_SOURCE` la sobrescribe en desarrollo).
    pub fn locate_nixpkgs_source() -> Option<PathBuf> {
        let candidate = std::env::var_os("ANTOS_NIXPKGS_SOURCE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/etc/antos/nixpkgs-source"));
        let resolved = fs::canonicalize(&candidate).ok()?;
        resolved.join("lib").is_dir().then_some(resolved)
    }

    /// El `--override-input nixpkgs …` de `nixos-install`: la fuente local
    /// como entrada `path:`, con `rev` y `lastModified` del `flake.lock` del
    /// árbol de antOS si lo hay, para que `system.nixos.versionSuffix` (y
    /// con él el `toplevel`) coincida con la closure que la ISO ya trae.
    pub fn nixpkgs_override(nixpkgs_source: &Path, antos_source: Option<&Path>) -> String {
        let mut url = format!("path:{}", nixpkgs_source.display());
        if let Some((rev, node)) = antos_source.and_then(Self::locked_nixpkgs) {
            let last_modified = node
                .get("locked")
                .and_then(|l| l.get("lastModified"))
                .and_then(|v| v.as_u64());
            url.push_str(&format!("?rev={rev}"));
            if let Some(lm) = last_modified {
                url.push_str(&format!("&lastModified={lm}"));
            }
        }
        url
    }

    /// Copia recursiva de un directorio saltando los de compilación.
    pub fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
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
        antos.nixosModules.machine
        {{ nixpkgs.overlays = [ antos.overlays.default ]; }}
        # Las fuentes de nixpkgs y de antOS entran en la closure del sistema
        # (el registro las referencia por ruta del store): `nixos-install`
        # las copia al disco y `nixos-rebuild` evalúa después SIN red.
        {{ nix.registry.nixpkgs.flake = nixpkgs; nix.registry.antos.flake = antos; }}
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
        // El gestor de arranque (`systemd-boot`), la red, el audio, el
        // teclado, el locale y la zona horaria los pone
        // `services.antos.machine` (T36.4); aquí solo van los valores que
        // el usuario eligió y, en dual-boot, el menú con los demás SO.
        Self::validate_machine_settings(config)?;
        let bootloader = if config.clean_install {
            String::new()
        } else {
            // Dual-boot: systemd-boot en la ESP compartida; detecta y encadena
            // Windows y otros Linux automáticamente, sin reescribir sus entradas.
            "\n  # Dual-boot: la ESP es compartida. systemd-boot encadena los demás\n  \
             # SO automáticamente; NO se tocan sus entradas.\n  \
             boot.loader.systemd-boot.configurationLimit = 10;\n  \
             boot.loader.timeout = 5;\n"
                .to_string()
        };

        let configuration = format!(
            r#"# antOS Linux · configuración de la máquina (generada por `antos install`, T30.5/T36.1/T36.4).
#
# Es antOS Linux (NixOS + escritorio antOS), NO el kernel bare-metal.
# Evoluciónala y aplica con:  sudo nixos-rebuild switch --flake /etc/nixos#{hostname}
{{ ... }}:

{{
  imports = [ ./hardware-configuration.nix ];

  networking.hostName = "{hostname}";

  # El escritorio antOS: Wayland + antos-barra + Neovim/Git + autologin.
  services.antos.enable = true;
  services.antos.desktop.enable = true;
  services.antos.desktop.autologinUser = "{username}";

  # La máquina física: NetworkManager, PipeWire, bluetooth, firmware,
  # teclado (consola + Wayland + Plasma), locale, zona horaria, sudo con
  # contraseña, energía, zram y systemd-boot (system/nixos/machine.nix).
  services.antos.machine = {{
    enable = true;
    keyboardLayout = "{keymap}";
    locale = "{locale}";
    timeZone = "{timezone}";
  }};
{bootloader}
  users.users."{username}" = {{
    isNormalUser = true;
    description = "antOS";
    extraGroups = [ "wheel" "video" "input" "networkmanager" ];
    {password}
  }};

  nix.settings.experimental-features = [ "nix-command" "flakes" ];

  system.stateVersion = "25.05";
}}
"#,
            hostname = config.hostname,
            timezone = config.timezone,
            keymap = config.keymap,
            locale = config.locale,
            username = config.username,
            password = Self::password_line(config)?,
            bootloader = bootloader.trim_end_matches('\n'),
        );
        fs::write(etc_nixos_dir.join("configuration.nix"), configuration)
            .context("Escribiendo /etc/nixos/configuration.nix")?;

        // ── hardware-configuration.nix de relleno ────────────────────────
        // Solo si no existe: en la instalación real lo escribe antes
        // `nixos-generate-config --root <target>` (T36.2) y ese es el que
        // vale. El de relleno declara la raíz y la ESP por las etiquetas que
        // el formateo pone (`antos-root`, `ANTOS_ESP`), para que el flake
        // evalúe completo sin ficheros extra.
        let hw_path = etc_nixos_dir.join("hardware-configuration.nix");
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
        if !hw_path.exists() {
            fs::write(&hw_path, hw_stub)
                .context("Escribiendo /etc/nixos/hardware-configuration.nix")?;
        }

        // ── Copia del árbol de antOS ─────────────────────────────────────
        if let Some(src) = antos_source {
            Self::copy_antos_source(src, &etc_nixos_dir.join("antos"))?;
        }

        Ok(())
    }

    /// Línea de contraseña del `configuration.nix`: `initialHashedPassword`
    /// si hay hash (validado: solo `[A-Za-z0-9./$]`, lo que produce
    /// `mkpasswd`), `initialPassword = "antos"` si no.
    /// Valida lo que va entre comillas al `configuration.nix` (T36.4): son
    /// las mismas expresiones que `machine.nix` exige con `strMatching`,
    /// comprobadas aquí para fallar con un mensaje claro antes de escribir
    /// nada, y para que ninguna comilla o `$` entre en la cadena Nix.
    pub fn validate_machine_settings(config: &InstallConfig) -> Result<()> {
        let layout_ok = config
            .keymap
            .chars()
            .next()
            .map(|c| c.is_ascii_lowercase())
            .unwrap_or(false)
            && config
                .keymap
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '-');
        if !layout_ok {
            bail!(
                "teclado «{}»: solo minúsculas y guiones (us, es, latam, de, fr, gb, pt-br, it, …)",
                config.keymap
            );
        }
        if config.locale.is_empty()
            || !config
                .locale
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '@' | '.' | '-'))
        {
            bail!(
                "locale «{}»: formato esperado como es_ES.UTF-8",
                config.locale
            );
        }
        if config.timezone.is_empty()
            || !config
                .timezone
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '/' | '-'))
        {
            bail!(
                "zona horaria «{}»: formato esperado como Europe/Madrid",
                config.timezone
            );
        }
        let host_ok = !config.hostname.is_empty()
            && config
                .hostname
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-');
        if !host_ok {
            bail!(
                "hostname «{}»: solo letras, dígitos y guiones",
                config.hostname
            );
        }
        let user_ok = config
            .username
            .chars()
            .next()
            .map(|c| c.is_ascii_lowercase() || c == '_')
            .unwrap_or(false)
            && config
                .username
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'));
        if !user_ok {
            bail!(
                "usuario «{}»: minúsculas, dígitos, `_` y `-`, empezando por letra",
                config.username
            );
        }
        Ok(())
    }

    /// Línea de contraseña del `configuration.nix`: `initialHashedPassword`
    /// con el hash del asistente (`mkpasswd -m yescrypt`) o de
    /// `password_hash` del TOML. Sin hash solo se admite en simulación
    /// (`install` lo rechaza en real): queda la contraseña de desarrollo,
    /// marcada como tal.
    fn password_line(config: &InstallConfig) -> Result<String> {
        match config.password_hash.as_deref() {
            None => Ok(
                "# SIMULACIÓN sin contraseña: la instalación real exige una (asistente o password_hash).\n    initialPassword = \"antos\";"
                    .to_string(),
            ),
            Some(hash) => {
                if hash.is_empty()
                    || !hash
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '/' | '$'))
                {
                    bail!(
                        "password_hash no tiene el formato de `mkpasswd` (solo letras, dígitos, `.`, `/` y `$`)"
                    );
                }
                Ok(format!(r#"initialHashedPassword = "{hash}";"#))
            }
        }
    }

    /// Busca un ejecutable recorriendo `PATH`, sin `which` ni intérprete.
    fn tool_in_path(name: &str) -> bool {
        let Some(path) = std::env::var_os("PATH") else {
            return false;
        };
        std::env::split_paths(&path).any(|dir| dir.join(name).is_file())
    }

    /// Precondiciones de una instalación real: Linux, `root`, las
    /// herramientas de [`REAL_INSTALL_TOOLS`] en `PATH`, el árbol fuente de
    /// antOS localizable y el destino **sin** montar todavía (lo monta la
    /// tubería). Devuelve la lista completa de lo que falta.
    pub fn real_install_preconditions(config: &InstallConfig, workspace: &Path) -> Vec<String> {
        let mut missing = Vec::new();
        if !cfg!(target_os = "linux") {
            missing
                .push("la instalación a disco solo se ejecuta desde Linux (la ISO en vivo)".into());
        }
        // `euid` por `/proc/self/status` (en macOS `/proc` no existe y ya
        // falló arriba).
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
        if config.password_hash.is_none() {
            missing.push(
                "sin contraseña para el usuario: el asistente la pide; con --config, `password_hash` (mkpasswd -m yescrypt)"
                    .into(),
            );
        }
        if config.encrypt {
            missing.push(
                "cifrado LUKS (`encrypt = true`): todavía no implementado; instala sin cifrar o espera al ticket de cifrado"
                    .into(),
            );
        }
        if Self::locate_antos_source(workspace).is_none() {
            missing.push(
                "no se encuentra el árbol fuente de antOS (ANTOS_SOURCE, el workspace o /etc/antos/source)"
                    .into(),
            );
        }
        if SystemRunner.is_mountpoint(Path::new(&config.target_mount)) {
            missing.push(format!(
                "ya hay algo montado en «{}»; desmóntalo antes (umount -R)",
                config.target_mount
            ));
        }
        missing
    }

    /// Nombre de la partición `n` de `device` (`p<n>` si el nombre acaba en
    /// dígito: `nvme0n1p1`, `mmcblk0p1`; `<n>` si no: `sda1`).
    pub fn partition_device(device: &str, n: u32) -> String {
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
        format!("{device}{sep}{n}")
    }

    /// Ejecuta o simula el despliegue completo: simulación con
    /// [`SimulatedRunner`] (`dry_run`), real con [`SystemRunner`]. Las
    /// líneas de salida de los comandos largos van a `stdout`.
    pub fn deploy_system(config: &InstallConfig, workspace: &Path) -> Result<InstallReport> {
        let mut on_line = |line: &str| println!("      {line}");
        if config.dry_run {
            let disk = Self::inspect_target(config)?;
            let mut runner = SimulatedRunner::for_disk(&disk);
            Self::install(config, workspace, &mut runner, &mut on_line)
        } else {
            Self::install(config, workspace, &mut SystemRunner, &mut on_line)
        }
    }

    /// La tubería de instalación de antOS Linux, real o simulada según el
    /// `runner` (ver la cabecera del módulo). `on_line` recibe la salida de
    /// `nixos-install` según llega.
    pub fn install(
        config: &InstallConfig,
        workspace: &Path,
        runner: &mut dyn InstallRunner,
        on_line: &mut dyn FnMut(&str),
    ) -> Result<InstallReport> {
        Self::install_with_password(config, workspace, runner, on_line, None)
    }

    /// Como [`DeployEngine::install`], con la contraseña en claro que el
    /// asistente acaba de pedir (T36.4). Se convierte en hash con
    /// `mkpasswd -m yescrypt --stdin` a través del `runner` (la contraseña
    /// solo viaja por la entrada estándar de ese proceso) y entra en la
    /// configuración como `initialHashedPassword`; nunca se escribe en
    /// claro ni forma parte de `InstallConfig`.
    pub fn install_with_password(
        config: &InstallConfig,
        workspace: &Path,
        runner: &mut dyn InstallRunner,
        on_line: &mut dyn FnMut(&str),
        password: Option<&crate::crypto::SecretValue>,
    ) -> Result<InstallReport> {
        // Las precondiciones se comprueban ANTES de tocar la contraseña:
        // un `--apply` fuera de la ISO tiene que fallar por lo que falta
        // (root, herramientas…), no por un `mkpasswd` ausente.
        if runner.is_real() {
            let probe = InstallConfig {
                password_hash: password
                    .map(|_| "pendiente".to_string())
                    .or(config.password_hash.clone()),
                ..config.clone()
            };
            let missing = Self::real_install_preconditions(&probe, workspace);
            if !missing.is_empty() {
                bail!(
                    "La instalación real no puede continuar:\n  - {}",
                    missing.join("\n  - ")
                );
            }
        }
        let hashed;
        let config = match password {
            Some(secret) => {
                let hash = runner
                    .run(
                        "mkpasswd",
                        &args(&["-m", "yescrypt", "--stdin"]),
                        Some(secret.expose()),
                    )
                    .context("mkpasswd (hash de la contraseña)")?;
                let hash = hash.trim().to_string();
                if hash.is_empty() {
                    bail!("mkpasswd no devolvió ningún hash");
                }
                hashed = InstallConfig {
                    password_hash: Some(hash),
                    ..config.clone()
                };
                &hashed
            }
            None => config,
        };
        Self::inspect_target(config)?;
        let real = runner.is_real();

        if real {
            let missing = Self::real_install_preconditions(config, workspace);
            if !missing.is_empty() {
                bail!(
                    "La instalación real no puede continuar:\n  - {}",
                    missing.join("\n  - ")
                );
            }
        }

        // Raíz del destino: el punto de montaje real, o el staging del
        // workspace en simulación (nunca `target_mount` sin montar).
        let root_dir = if real {
            PathBuf::from(&config.target_mount)
        } else {
            let staging = workspace.join("target/installer-staging");
            if staging.exists() {
                fs::remove_dir_all(&staging).context("Limpiando el staging anterior")?;
            }
            staging
        };
        fs::create_dir_all(&root_dir).with_context(|| format!("Creando {}", root_dir.display()))?;

        let mut state = PipelineState {
            steps: Vec::new(),
            mounted: false,
            executed: real,
        };
        let result = Self::run_pipeline(config, workspace, &root_dir, runner, on_line, &mut state);

        match result {
            Ok(report) => Ok(report),
            Err(err) => {
                // Dejar el disco desmontado antes de devolver el error, con
                // lo que haya fallado por delante.
                if state.mounted {
                    let _ = runner.run("umount", &args(&["-R", &root_dir.to_string_lossy()]), None);
                }
                Err(err)
            }
        }
    }

    fn run_pipeline(
        config: &InstallConfig,
        workspace: &Path,
        root_dir: &Path,
        runner: &mut dyn InstallRunner,
        on_line: &mut dyn FnMut(&str),
        state: &mut PipelineState,
    ) -> Result<InstallReport> {
        let dev = config.target_device.as_str();
        let root_str = root_dir.to_string_lossy().to_string();
        let boot_dir = root_dir.join("boot");
        let boot_str = boot_dir.to_string_lossy().to_string();

        // ── 1. Particionado ──────────────────────────────────────────────
        let (esp_dev, root_dev, esp_is_new, partition_desc) = if config.clean_install {
            let cmd = args(&[
                "-s",
                dev,
                "mklabel",
                "gpt",
                "mkpart",
                "ESP",
                "fat32",
                "1MiB",
                &format!("{}MiB", ESP_MIB + 1),
                "set",
                "1",
                "esp",
                "on",
                "mkpart",
                "antos-root",
                "ext4",
                &format!("{}MiB", ESP_MIB + 1),
                "100%",
            ]);
            runner.run("parted", &cmd, None)?;
            (
                Self::partition_device(dev, 1),
                Self::partition_device(dev, 2),
                true,
                format!("parted {}", cmd.join(" ")),
            )
        } else {
            let print = args(&["-s", "-m", dev, "unit", "MiB", "print", "free"]);
            let regions = parse_parted_print_free(&runner.run("parted", &print, None)?);
            let esp = regions.iter().find(|r| r.is_esp()).ok_or_else(|| {
                anyhow::anyhow!(
                    "«{dev}» no tiene partición ESP: el dual-boot reutiliza la existente. \
                     Si el disco está vacío, usa Disco Completo."
                )
            })?;
            let esp_number = esp
                .number
                .ok_or_else(|| anyhow::anyhow!("la ESP de «{dev}» no tiene número de partición"))?;
            let free = regions
                .iter()
                .filter(|r| r.is_free())
                .max_by_key(|r| r.size_mib)
                .cloned();
            let free = match free {
                Some(f) if f.size_mib >= MIN_ROOT_MIB => f,
                Some(f) => bail!(
                    "el mayor hueco libre de «{dev}» mide {} MiB y la raíz de antOS necesita {} MiB: \
                     faltan {} MiB. Libera espacio desde el otro sistema (antOS no redimensiona particiones ajenas).",
                    f.size_mib,
                    MIN_ROOT_MIB,
                    MIN_ROOT_MIB - f.size_mib
                ),
                None => bail!(
                    "«{dev}» no tiene espacio libre sin particionar: la raíz de antOS necesita {} MiB. \
                     Libera espacio desde el otro sistema (antOS no redimensiona particiones ajenas).",
                    MIN_ROOT_MIB
                ),
            };
            let mkpart = args(&[
                "-s",
                dev,
                "mkpart",
                "antos-root",
                "ext4",
                &format!("{}MiB", free.start_mib),
                &format!("{}MiB", free.end_mib),
            ]);
            runner.run("parted", &mkpart, None)?;
            runner.run("partprobe", &args(&[dev]), None)?;
            runner.run("udevadm", &args(&["settle"]), None)?;
            let after = parse_parted_print_free(&runner.run("parted", &print, None)?);
            let root_number = after
                .iter()
                .filter(|r| !r.is_free())
                .find(|r| r.name == "antos-root" || r.start_mib.abs_diff(free.start_mib) <= 2)
                .and_then(|r| r.number)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "parted no muestra la partición antos-root recién creada en «{dev}»"
                    )
                })?;
            (
                Self::partition_device(dev, esp_number),
                Self::partition_device(dev, root_number),
                false,
                format!(
                    "ESP existente #{esp_number} preservada; parted {} (hueco libre de {} MiB)",
                    mkpart.join(" "),
                    free.size_mib
                ),
            )
        };
        if config.clean_install {
            runner.run("partprobe", &args(&[dev]), None)?;
            runner.run("udevadm", &args(&["settle"]), None)?;
        }
        state.push("partitioning", partition_desc);

        // ── 2. Formateo y UUIDs ──────────────────────────────────────────
        let mut format_desc = Vec::new();
        if esp_is_new {
            let cmd = args(&["-F32", "-n", "ANTOS_ESP", &esp_dev]);
            runner.run("mkfs.vfat", &cmd, None)?;
            format_desc.push(format!("mkfs.vfat {}", cmd.join(" ")));
        } else {
            format_desc.push(format!("ESP ajena {esp_dev} sin formatear"));
        }
        let cmd = args(&["-F", "-L", "antos-root", &root_dev]);
        runner.run("mkfs.ext4", &cmd, None)?;
        format_desc.push(format!("mkfs.ext4 {}", cmd.join(" ")));
        let esp_uuid = runner
            .run(
                "blkid",
                &args(&["-s", "UUID", "-o", "value", &esp_dev]),
                None,
            )?
            .trim()
            .to_string();
        let root_uuid = runner
            .run(
                "blkid",
                &args(&["-s", "UUID", "-o", "value", &root_dev]),
                None,
            )?
            .trim()
            .to_string();
        if esp_uuid.is_empty() || root_uuid.is_empty() {
            bail!("blkid no devolvió UUID para {esp_dev} / {root_dev}");
        }
        format_desc.push(format!("UUIDs: ESP {esp_uuid}, raíz {root_uuid}"));
        state.push("filesystem_format", format_desc.join("; "));

        // ── 3. Montaje ───────────────────────────────────────────────────
        runner.run("mount", &args(&[&root_dev, &root_str]), None)?;
        state.mounted = true;
        fs::create_dir_all(&boot_dir).with_context(|| format!("Creando {}", boot_dir.display()))?;
        runner.run("mount", &args(&[&esp_dev, &boot_str]), None)?;
        if !runner.is_mountpoint(root_dir) || !runner.is_mountpoint(&boot_dir) {
            bail!("tras `mount`, {root_str} o {boot_str} no aparecen como puntos de montaje");
        }
        state.push(
            "mount_hierarchy",
            format!("mount {root_dev} {root_str}; mount {esp_dev} {boot_str}"),
        );

        // ── 4. Configuración ─────────────────────────────────────────────
        runner.run("nixos-generate-config", &args(&["--root", &root_str]), None)?;
        let antos_source = Self::locate_antos_source(workspace);
        if runner.is_real() && antos_source.is_none() {
            bail!("no se encuentra el árbol fuente de antOS para copiar a /etc/nixos/antos");
        }
        let etc_nixos = root_dir.join("etc/nixos");
        Self::generate_nixos_config(config, &etc_nixos, antos_source.as_deref())?;
        state.push(
            "nixos_configuration",
            format!(
                "nixos-generate-config --root {root_str}; flake.nix + configuration.nix en {} (services.antos.desktop, autologin «{}», keymap «{}», sistema «{}», systemd-boot{}; fuente de antOS: {})",
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
        );

        // ── 5. nixos-install ─────────────────────────────────────────────
        // `antos.url` del flake es `path:/etc/nixos/antos`, que solo existe
        // así tras el arranque; durante la instalación se apunta a la copia
        // que acaba de dejarse bajo el destino. `--no-write-lock-file` para
        // que el lock no guarde esa ruta temporal: el primer
        // `nixos-rebuild` del sistema instalado añade el nodo `antos` con
        // la ruta definitiva (local, sin red).
        let flake_ref = format!("{}#{}", etc_nixos.display(), config.hostname);
        let override_path = format!("path:{}", etc_nixos.join("antos").display());
        let mut install_args = args(&[
            "--root",
            &root_str,
            "--flake",
            &flake_ref,
            "--no-root-passwd",
            "--no-channel-copy",
            "--override-input",
            "antos",
            &override_path,
            "--no-write-lock-file",
            // Sin red, nix «desactiva funciones dependientes de la red»,
            // entre ellas TODOS los substituters — también el store local
            // del live, que es de donde `nixos-install` copia la closure al
            // destino — y se pone a construir el sistema desde
            // `bootstrap-tools` (6277 derivaciones en el primer smoke real).
            // El ajuste explícito se respeta y deja la sustitución activa.
            "--option",
            "substitute",
            "true",
        ]);
        // Primer smoke real (2026-09-22): `nixos-install` hace `nix build
        // --store <destino>`, y con ello la resolución de `nixpkgs` ocurre
        // contra el store VACÍO del destino: nix no la encuentra ahí por
        // `narHash` y sale a GitHub aunque el live la tenga (sin `--store`,
        // como hace `nixos-rebuild` después, el atajo sí funciona). La ISO
        // expone la fuente en `/etc/antos/nixpkgs-source` y aquí se pasa como
        // entrada `path:` con `rev`/`lastModified` del lock: mismo
        // `toplevel` que la closure de la ISO, nada que construir ni bajar.
        let nixpkgs_override = Self::locate_nixpkgs_source()
            .map(|p| Self::nixpkgs_override(&p, antos_source.as_deref()));
        if let Some(url) = &nixpkgs_override {
            install_args.push("--override-input".into());
            install_args.push("nixpkgs".into());
            install_args.push(url.clone());
        }
        runner.run_streaming("nixos-install", &install_args, on_line)?;
        state.push(
            "nixos_install",
            format!("nixos-install {}", install_args.join(" ")),
        );

        // ── 6. Gestor de arranque: lo puso nixos-install; se sondea la ESP ─
        let neighbours =
            super::BootloaderEngine::probe_operating_systems(&boot_dir).unwrap_or_default();
        let neighbour_names: Vec<String> = neighbours.iter().map(|o| o.name.clone()).collect();

        // ── 7. Cierre ────────────────────────────────────────────────────
        runner.run("sync", &[], None)?;
        runner.run("umount", &args(&["-R", &root_str]), None)?;
        state.mounted = false;
        state.push(
            "unmount",
            format!(
                "sync; umount -R {root_str}; systemd-boot en la ESP con {} sistema(s) vecino(s){}",
                neighbours.len(),
                if neighbour_names.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", neighbour_names.join(", "))
                }
            ),
        );

        let mode = if config.clean_install {
            "clean"
        } else {
            "dual-boot"
        };
        let simulated = state.steps.iter().any(|s| !s.executed);
        let summary = if simulated {
            format!(
                "Simulación de instalación de antOS en «{dev}» completada (Modo: {mode}); el disco no se ha tocado"
            )
        } else {
            format!(
                "antOS Linux instalado en «{dev}» (Modo: {mode}): raíz {root_dev}, ESP {esp_dev}. Retira el medio y reinicia."
            )
        };
        let fstab_entries: Vec<String> = Self::generate_fstab(&root_uuid, &esp_uuid, None)
            .lines()
            .map(String::from)
            .collect();

        Ok(InstallReport {
            target_device: config.target_device.clone(),
            mode: mode.into(),
            success: true,
            steps: std::mem::take(&mut state.steps),
            efi_partition: esp_dev,
            root_partition: root_dev,
            fstab_entries,
            summary,
            simulated,
        })
    }

    /// Nombres de la ESP (1) y la raíz (2) de una instalación limpia.
    pub fn partition_names(device: &str) -> (String, String) {
        (
            Self::partition_device(device, 1),
            Self::partition_device(device, 2),
        )
    }
}

/// Estado que la tubería arrastra entre pasos.
struct PipelineState {
    steps: Vec<InstallStep>,
    mounted: bool,
    executed: bool,
}

impl PipelineState {
    fn push(&mut self, name: &str, description: String) {
        self.steps.push(InstallStep {
            name: name.into(),
            description,
            completed: true,
            executed: self.executed,
        });
    }
}

/// `&[&str]` → `Vec<String>` para los runners.
fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests;
