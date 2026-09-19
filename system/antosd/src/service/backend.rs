//! Los tres backends de un servicio efímero y la mecánica común: localizar
//! binarios, lanzar un proceso separado de la sesión, sondear el puerto,
//! comprobar un PID y terminarlo.
//!
//! No sabe nada de PostgreSQL ni de Ollama: recibe un `LaunchPlan` de
//! `kinds.rs` y lo ejecuta. Ningún comando pasa por un intérprete: programa y
//! argumentos van como vector (T31.4).

use super::kinds::{Cmd, LaunchPlan, Probe, ServiceKind};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// De dónde ha salido el proceso que atiende el puerto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ServiceBackend {
    /// Ya escuchaba antes de que antOS hiciera nada (Ollama.app, un
    /// `services.ollama` de NixOS, un postgres del sistema). antOS lo
    /// registra y lo usa, pero no lo arrancó y nunca lo mata.
    External,
    /// Binario encontrado en la máquina, lanzado por antOS.
    System,
    /// Sin binario pero con `nix`: `nix shell nixpkgs#<paquete> -c …`.
    Nix,
    /// Registro anterior a T34.1, cuando `service up` solo escribía
    /// metadatos: no hay proceso conocido detrás.
    #[default]
    Unknown,
}

impl ServiceBackend {
    pub fn label(self) -> &'static str {
        match self {
            Self::External => "external",
            Self::System => "system",
            Self::Nix => "nix",
            Self::Unknown => "unknown",
        }
    }
}

/// Cómo se va a ejecutar cada programa de un plan.
#[derive(Debug, Clone)]
pub enum Launcher {
    /// Binarios resueltos a rutas absolutas, por nombre de programa.
    System { bins: Vec<(&'static str, PathBuf)> },
    /// `nix shell nixpkgs#<pkg> -c <programa> <args…>`.
    Nix { nix: PathBuf, package: &'static str },
}

impl Launcher {
    pub fn backend(&self) -> ServiceBackend {
        match self {
            Self::System { .. } => ServiceBackend::System,
            Self::Nix { .. } => ServiceBackend::Nix,
        }
    }

    /// Construye el `Command` de un `Cmd` del plan, sin entorno todavía.
    fn command(&self, cmd: &Cmd) -> Result<Command> {
        match self {
            Self::System { bins } => {
                let path = bins
                    .iter()
                    .find(|(name, _)| *name == cmd.program)
                    .map(|(_, p)| p.clone())
                    .ok_or_else(|| {
                        anyhow::anyhow!("el binario «{}» no se localizó", cmd.program)
                    })?;
                let mut c = Command::new(path);
                c.args(&cmd.args);
                Ok(c)
            }
            Self::Nix { nix, package } => {
                let mut c = Command::new(nix);
                c.arg("--extra-experimental-features")
                    .arg("nix-command flakes")
                    .arg("shell")
                    .arg(format!("nixpkgs#{package}"))
                    .arg("-c")
                    .arg(cmd.program)
                    .args(&cmd.args);
                Ok(c)
            }
        }
    }

    /// Cómo se mostrará el comando principal en `antos services`.
    pub fn describe(&self, cmd: &Cmd) -> String {
        let mut parts: Vec<String> = match self {
            Self::System { bins } => bins
                .iter()
                .find(|(name, _)| *name == cmd.program)
                .map(|(_, p)| vec![p.to_string_lossy().to_string()])
                .unwrap_or_else(|| vec![cmd.program.to_string()]),
            Self::Nix { package, .. } => vec![
                "nix".into(),
                "shell".into(),
                format!("nixpkgs#{package}"),
                "-c".into(),
                cmd.program.into(),
            ],
        };
        parts.extend(cmd.args.iter().cloned());
        parts.join(" ")
    }
}

// ------------------------------------------------------------ localización

/// Busca el binario principal (y los auxiliares) de un servicio en `PATH` y
/// en los directorios habituales; si no, `nix`. `None` si no hay forma de
/// arrancarlo en esta máquina.
pub fn find_launcher(kind: ServiceKind) -> Option<Launcher> {
    let dirs = search_dirs(kind);
    let main = kind
        .binaries()
        .iter()
        .find_map(|name| locate_in(&dirs, name).map(|p| (*name, p)));
    if let Some((main_name, main_path)) = main {
        let mut bins = vec![(main_name, main_path.clone())];
        // Los auxiliares se buscan primero junto al principal (misma versión).
        let sibling_dir = main_path.parent().map(Path::to_path_buf);
        for helper in kind.helper_binaries() {
            let found = sibling_dir
                .as_ref()
                .and_then(|d| executable_at(&d.join(helper)))
                .or_else(|| locate_in(&dirs, helper));
            if let Some(p) = found {
                bins.push((*helper, p));
            }
        }
        // Para el plan hace falta el principal y, si los hay, los
        // auxiliares; sin `initdb` no hay clúster que arrancar.
        let all_helpers = kind
            .helper_binaries()
            .iter()
            .all(|h| bins.iter().any(|(n, _)| n == h));
        if all_helpers {
            return Some(Launcher::System { bins });
        }
    }
    find_nix().map(|nix| Launcher::Nix {
        nix,
        package: kind.nix_package(),
    })
}

pub fn find_nix() -> Option<PathBuf> {
    let mut dirs = path_dirs();
    dirs.push(PathBuf::from("/nix/var/nix/profiles/default/bin"));
    dirs.push(PathBuf::from("/run/current-system/sw/bin"));
    locate_in(&dirs, "nix")
}

pub(crate) fn path_dirs() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default()
}

fn search_dirs(kind: ServiceKind) -> Vec<PathBuf> {
    let mut dirs = path_dirs();
    for pattern in kind.extra_binary_dirs() {
        dirs.extend(expand_single_wildcard(Path::new(pattern)));
    }
    dirs
}

/// Expande un patrón con como mucho un componente `*` (p. ej.
/// `/usr/lib/postgresql/*/bin`). Las coincidencias van de mayor a menor por
/// nombre, para preferir la versión más alta. Sin `*`, devuelve el path.
pub(crate) fn expand_single_wildcard(pattern: &Path) -> Vec<PathBuf> {
    let components: Vec<_> = pattern.components().collect();
    let Some(star) = components
        .iter()
        .position(|c| c.as_os_str().to_string_lossy() == "*")
    else {
        return vec![pattern.to_path_buf()];
    };
    let prefix: PathBuf = components[..star].iter().collect();
    let suffix: PathBuf = components[star + 1..].iter().collect();
    let Ok(read) = fs::read_dir(&prefix) else {
        return Vec::new();
    };
    let mut names: Vec<String> = read
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    names.sort_by(|a, b| b.cmp(a));
    names
        .into_iter()
        .map(|n| prefix.join(n).join(&suffix))
        .collect()
}

pub(crate) fn locate_in(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    dirs.iter().find_map(|d| executable_at(&d.join(name)))
}

fn executable_at(path: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let meta = fs::metadata(path).ok()?;
    if meta.is_file() && meta.permissions().mode() & 0o111 != 0 {
        Some(path.to_path_buf())
    } else {
        None
    }
}

// ------------------------------------------------------------------ sondas

/// Si desde este proceso se puede sondear loopback. Dentro del ejecutor
/// confinado sin `network` declarada (Seatbelt `deny network*`, Landlock
/// ABI 4) ni `bind` ni `connect` funcionan, y una sonda fallida ahí no
/// significa que el servicio esté caído: significa que no se pudo mirar.
pub fn loopback_probing_available() -> bool {
    let Ok(listener) = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) else {
        return false;
    };
    let Ok(addr) = listener.local_addr() else {
        return false;
    };
    TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
}

pub fn probe_once(probe: &Probe, port: u16, timeout: Duration) -> bool {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    match probe {
        Probe::Tcp => TcpStream::connect_timeout(&addr, timeout).is_ok(),
        Probe::Http { path } => {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(timeout))
                .http_status_as_error(false)
                .build()
                .into();
            match agent.get(format!("http://127.0.0.1:{port}{path}")).call() {
                Ok(resp) => resp.status().is_success(),
                Err(_) => false,
            }
        }
    }
}

/// Espera a que la sonda pase o a que el proceso muera o se agote el tiempo.
fn wait_until_healthy(
    probe: &Probe,
    port: u16,
    pid: u32,
    timeout: Duration,
    log_path: &Path,
) -> Result<()> {
    let start = Instant::now();
    loop {
        if probe_once(probe, port, Duration::from_millis(400)) {
            return Ok(());
        }
        if !pid_alive(pid) {
            bail!(
                "el proceso (PID {pid}) terminó antes de escuchar en 127.0.0.1:{port}\n{}",
                log_tail(log_path, 15)
            );
        }
        if start.elapsed() >= timeout {
            let _ = terminate(pid, Duration::from_secs(5));
            bail!(
                "127.0.0.1:{port} no respondió en {}s; proceso terminado\n{}",
                timeout.as_secs(),
                log_tail(log_path, 15)
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Últimas `n` líneas del log, para que un fallo de arranque diga por qué.
pub fn log_tail(log_path: &Path, n: usize) -> String {
    let Ok(content) = fs::read_to_string(log_path) else {
        return String::new();
    };
    let lines: Vec<&str> = content.lines().collect();
    let from = lines.len().saturating_sub(n);
    lines[from..].join("\n")
}

// ---------------------------------------------------------------- systemd

/// ¿Gestiona systemd una unidad con este nombre y está activa? (T34.3: el
/// `services.ollama` de la imagen NixOS.) Solo tiene sentido en Linux con
/// systemd corriendo; en cualquier otro caso, `false` sin ejecutar nada.
pub fn systemd_unit_active(unit: &str) -> bool {
    if !Path::new("/run/systemd/system").is_dir() {
        return false;
    }
    Command::new("systemctl")
        .args(["is-active", "--quiet", unit])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// --------------------------------------------------------------- procesos

pub fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    // SAFETY: `kill` con señal 0 no envía nada; solo comprueba existencia y
    // permiso. No toca memoria ni estado del proceso llamante.
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if rc == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Línea de comando de un proceso vivo, o `None` si no existe. `ps` va con
/// argumentos como vector, sin intérprete; funciona igual en macOS y Linux.
pub fn process_command(pid: u32) -> Option<String> {
    let out = Command::new("ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if line.is_empty() {
        None
    } else {
        Some(line)
    }
}

/// Un PID de un registro puede haber sido reutilizado por otro proceso
/// desde que lo anotamos. Antes de tratarlo como nuestro (y, sobre todo,
/// antes de señalarlo) se comprueba que su línea de comando nombra alguno
/// de los programas esperados. Sin `ps` disponible se responde `false`:
/// mejor un «ya no está» que un `SIGTERM` a un desconocido.
pub fn pid_belongs_to(pid: u32, expected_programs: &[&str]) -> bool {
    if !pid_alive(pid) {
        return false;
    }
    let Some(cmd) = process_command(pid) else {
        return false;
    };
    let first = cmd.split_whitespace().next().unwrap_or("");
    let exe = Path::new(first)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();
    expected_programs.iter().any(|p| exe == *p)
}

fn signal(pid: u32, sig: libc::c_int) {
    // El proceso se lanzó con `setsid`, así que su PID es también su grupo:
    // señalar al grupo alcanza a los hijos que haya creado (postgres lanza
    // varios). Si por lo que sea no es líder, la señal directa lo cubre.
    // SAFETY: enviar una señal a un PID que registramos nosotros; el peor
    // caso (PID reutilizado) es el mismo que tiene cualquier `kill`.
    unsafe {
        libc::kill(-(pid as libc::pid_t), sig);
        libc::kill(pid as libc::pid_t, sig);
    }
}

/// `SIGTERM`, espera hasta `grace`, y `SIGKILL` si sigue vivo. Devuelve si
/// hizo falta el `SIGKILL`.
pub fn terminate(pid: u32, grace: Duration) -> Result<bool> {
    if !pid_alive(pid) {
        return Ok(false);
    }
    signal(pid, libc::SIGTERM);
    let start = Instant::now();
    while start.elapsed() < grace {
        if !pid_alive(pid) {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    signal(pid, libc::SIGKILL);
    std::thread::sleep(Duration::from_millis(200));
    if pid_alive(pid) {
        bail!("el proceso {pid} no murió ni con SIGKILL");
    }
    Ok(true)
}

/// Entorno mínimo del proceso: `PATH`, un `HOME` propio dentro del
/// directorio del servicio, y lo imprescindible para `nix`. Todo lo demás
/// (claves de API, tokens) se queda fuera: un servicio de desarrollo no
/// tiene por qué verlo.
fn minimal_env(cmd: &mut Command, home: &Path, run: &Path) {
    cmd.env_clear();
    for key in ["PATH", "USER", "LOGNAME", "LANG", "LC_ALL", "SSL_CERT_FILE"] {
        if let Some(v) = std::env::var_os(key) {
            cmd.env(key, v);
        }
    }
    for (key, v) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("NIX_") {
            cmd.env(key, v);
        }
    }
    cmd.env("HOME", home);
    cmd.env("TMPDIR", run);
}

/// Lo que hace falta para lanzar un plan.
pub struct SpawnContext<'a> {
    pub launcher: &'a Launcher,
    pub plan: &'a LaunchPlan,
    pub kind: ServiceKind,
    pub port: u16,
    pub home: &'a Path,
    pub run: &'a Path,
    pub log_path: &'a Path,
}

/// Ejecuta `setup` (si hace falta), lanza `main` separado de la sesión con
/// stdio en el log, espera a que la sonda pase y corre `post_start`.
/// Devuelve el PID del proceso principal.
pub fn spawn_and_wait(ctx: &SpawnContext) -> Result<u32> {
    let SpawnContext {
        launcher,
        plan,
        kind,
        port,
        home,
        run,
        log_path,
    } = ctx;
    let log = || -> Result<fs::File> {
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .with_context(|| format!("abriendo el log {}", log_path.display()))
    };

    let setup_done = plan
        .setup_marker
        .as_ref()
        .map(|m| m.exists())
        .unwrap_or(false);
    if !setup_done {
        for step in &plan.setup {
            let mut c = launcher.command(step)?;
            minimal_env(&mut c, home, run);
            c.envs(plan.env.iter().cloned());
            c.stdin(Stdio::null()).stdout(log()?).stderr(log()?);
            let status = c
                .status()
                .with_context(|| format!("ejecutando «{}»", launcher.describe(step)))?;
            if !status.success() {
                bail!(
                    "el paso previo «{}» terminó con {status}\n{}",
                    launcher.describe(step),
                    log_tail(log_path, 15)
                );
            }
        }
    }

    let mut c = launcher.command(&plan.main)?;
    minimal_env(&mut c, home, run);
    c.envs(plan.env.iter().cloned());
    c.stdin(Stdio::null()).stdout(log()?).stderr(log()?);
    // SAFETY: `setsid` solo cambia la sesión del hijo entre `fork` y `exec`;
    // no toca memoria compartida ni cerrojos. Sin esto el servicio moriría
    // con la terminal, con el CLI o con el ejecutor confinado que lo lanzó.
    unsafe {
        c.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = c
        .spawn()
        .with_context(|| format!("lanzando «{}»", launcher.describe(&plan.main)))?;
    let pid = child.id();
    // Alguien tiene que recoger el estado de salida cuando muera; si no,
    // queda como zombi mientras viva el proceso que lo lanzó (el demonio).
    std::thread::spawn(move || {
        let mut child = child;
        let _ = child.wait();
    });

    wait_until_healthy(
        &kind.probe(),
        *port,
        pid,
        Duration::from_secs(kind.startup_timeout_secs()),
        log_path,
    )?;

    for step in &plan.post_start {
        let mut c = launcher.command(step)?;
        minimal_env(&mut c, home, run);
        c.envs(plan.env.iter().cloned());
        c.stdin(Stdio::null()).stdout(log()?).stderr(log()?);
        // `createdb` sobre una base que ya existe falla, y eso está bien: el
        // servicio sigue sano. Queda en el log por si era otra cosa.
        let _ = c.status();
    }
    Ok(pid)
}
