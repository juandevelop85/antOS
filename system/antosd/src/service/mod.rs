//! Servicios locales efímeros (T5.1, reescrito en T34.1).
//!
//! `antos service up <svc>` arranca un proceso **real** que atiende un puerto
//! en `127.0.0.1`, registra su PID, su log y su salud en
//! `$STATE/services/<svc>/service.json`, e inyecta la variable de conexión en
//! el `.env` del workspace **solo cuando la sonda de salud ha pasado**.
//!
//! De dónde sale el proceso lo dice `ServiceBackend`, en este orden:
//!
//! 1. `External` — el puerto ya respondía antes de hacer nada (Ollama.app,
//!    `services.ollama` de NixOS, un postgres del sistema). Se adopta: se
//!    registra sin PID propio y `service down` solo retira el registro.
//! 2. `System` — el binario está en `PATH` o en los directorios habituales
//!    (`kinds.rs`). Se lanza separado de la sesión, con stdio en el log y un
//!    entorno mínimo sin credenciales.
//! 3. `Nix` — no hay binario pero sí `nix`: `nix shell nixpkgs#<pkg> -c …`.
//!
//! Si no aplica ninguno, `service up` falla nombrando los tres caminos y no
//! escribe nada: ni registro ni `.env`.
//!
//! ## Estado de implementación
//!
//! - Con plan de arranque verificado: Ollama, PostgreSQL, Redis. Meilisearch
//!   tiene plan pero no se ha verificado contra un binario real.
//! - Solo adopción (sin plan de arranque): MariaDB y RabbitMQ. `service up`
//!   lo dice si no están ya escuchando.
//! - Los servicios no son unidades del sistema: sobreviven al CLI y al
//!   ejecutor que los lanzó (`setsid`), pero no a un reinicio. Lo persistente
//!   es cosa de la imagen (T34.3).
//! - Cuando `env.service_up` llega por una capacidad, el proceso se lanza
//!   desde dentro del ejecutor confinado y hereda su recinto: Seatbelt/
//!   Landlock con las escrituras declaradas y red en loopback. Bajo Landlock
//!   el backend `Nix` no funciona (`/nix/store` no es legible desde el
//!   recinto, limitación conocida de T33.2); desde el CLI directo sí.
//! - `antos service up` desde el CLI corre sin recinto, como el resto de
//!   subcomandos directos.

pub mod backend;
pub mod kinds;

pub use backend::ServiceBackend;
pub use kinds::{build_connection_url, default_port, ServiceKind};

use anyhow::{bail, Context, Result};
use kinds::{service_dir, LaunchDirs, Probe};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::Duration;

/// Estado y datos de conexión de un servicio registrado.
///
/// Los campos nuevos de T34.1 llevan `default` para poder leer registros
/// anteriores (que quedan como `backend: Unknown`, sin proceso conocido).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub port: u16,
    /// `running` (proceso nuestro vivo y sano), `external` (adoptado y sano),
    /// `unhealthy` (proceso vivo o externo pero la sonda falla) o `stopped`.
    pub status: String,
    pub env_var_key: String,
    pub env_var_value: String,
    pub pid: Option<u32>,
    pub data_dir: String,
    #[serde(default)]
    pub backend: ServiceBackend,
    /// Resultado de la sonda la última vez que se consultó.
    #[serde(default)]
    pub health: Health,
    #[serde(default)]
    pub log_path: Option<String>,
    /// Segundos desde la época Unix.
    #[serde(default)]
    pub started_at: Option<u64>,
    /// Comando principal, para que `antos services` diga qué está corriendo.
    #[serde(default)]
    pub command: Option<String>,
}

/// Resultado de la sonda de salud. `Unknown` es distinto de `Unhealthy`:
/// significa que no se pudo sondear (p. ej. `env.service_status` dentro de
/// un recinto sin red), no que el servicio esté caído.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Health {
    Healthy,
    Unhealthy,
    #[default]
    Unknown,
}

impl Health {
    pub fn label(self) -> &'static str {
        match self {
            Self::Healthy => "sí",
            Self::Unhealthy => "no",
            Self::Unknown => "?",
        }
    }
}

/// Qué hizo `stop_service`, para que el CLI lo cuente sin adivinar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopOutcome {
    /// Proceso nuestro terminado (`forced` si hizo falta `SIGKILL`).
    Terminated { pid: u32, forced: bool },
    /// El proceso ya no existía; solo se actualizó el registro.
    AlreadyGone,
    /// Servicio adoptado: se retira el registro, el proceso sigue.
    ExternalUnregistered,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn parse_kind(service: &str) -> Result<ServiceKind> {
    ServiceKind::parse(service).ok_or_else(|| {
        anyhow::anyhow!(
            "servicio desconocido «{service}»; los conocidos son: {}",
            kinds::known_services_list()
        )
    })
}

fn read_record(dir: &Path) -> Option<ServiceInfo> {
    let content = fs::read_to_string(dir.join("service.json")).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_record(dir: &Path, info: &ServiceInfo) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("creando {}", dir.display()))?;
    let json = serde_json::to_string_pretty(info)?;
    fs::write(dir.join("service.json"), json)
        .with_context(|| format!("escribiendo el registro en {}", dir.display()))
}

/// Recalcula `status` y `health` de un registro contra la realidad: PID
/// vivo y sonda. Un `service.json` que diga `running` con el proceso muerto
/// es lo que pasaba siempre antes de T34.1; ahora no puede ocurrir.
fn refresh(info: &mut ServiceInfo, kind: ServiceKind) {
    let health = if !backend::loopback_probing_available() {
        Health::Unknown
    } else if backend::probe_once(&kind.probe(), info.port, Duration::from_millis(400)) {
        Health::Healthy
    } else {
        Health::Unhealthy
    };
    info.health = health;
    info.status = match info.backend {
        ServiceBackend::External => match health {
            Health::Healthy | Health::Unknown => "external",
            Health::Unhealthy => "unhealthy",
        },
        ServiceBackend::System | ServiceBackend::Nix => {
            // Vivo Y con el comando esperado: un PID reutilizado por otro
            // proceso cuenta como muerto, no como nuestro.
            let alive = info
                .pid
                .map(|p| backend::pid_belongs_to(p, kind.expected_process_names()))
                .unwrap_or(false);
            match (alive, health) {
                (true, Health::Healthy | Health::Unknown) => "running",
                (true, Health::Unhealthy) => "unhealthy",
                (false, _) => {
                    info.pid = None;
                    "stopped"
                }
            }
        }
        // Sin proceso conocido: solo la sonda puede decir algo.
        ServiceBackend::Unknown => match health {
            Health::Healthy => {
                // Algo escucha ahí, pero no es nuestro: el registro viejo no
                // puede presumir de ello. Pasa a externo sin PID.
                info.backend = ServiceBackend::External;
                "external"
            }
            Health::Unhealthy | Health::Unknown => "stopped",
        },
    }
    .to_string();
}

/// Arranca (o adopta) un servicio y registra su conexión en `.env`.
pub fn start_service(
    service: &str,
    port: Option<u16>,
    db_name: Option<&str>,
    state_dir: &Path,
    workspace: &Path,
) -> Result<ServiceInfo> {
    let kind = parse_kind(service)?;
    let port = port.unwrap_or_else(|| kind.default_port());
    let db = db_name.unwrap_or("antos_dev");
    let dir = service_dir(state_dir, kind);
    let (env_key, env_val) = kind.connection(port, db);

    // 1. ¿Ya lo tenemos nosotros, vivo y sano, en ese puerto? Idempotente.
    if let Some(mut existing) = read_record(&dir) {
        let ours = matches!(
            existing.backend,
            ServiceBackend::System | ServiceBackend::Nix
        );
        if ours && existing.port == port {
            refresh(&mut existing, kind);
            if existing.status == "running" {
                inject_env_variable(workspace, &env_key, &env_val)?;
                write_record(&dir, &existing)?;
                return Ok(existing);
            }
        }
    }

    let data_dir = dir.join("data");
    let probe = kind.probe();

    // 2. ¿Ya escucha alguien? Se adopta, no se arranca otro encima.
    if backend::probe_once(&probe, port, Duration::from_millis(400)) {
        let info = ServiceInfo {
            name: kind.canonical_name().into(),
            port,
            status: "external".into(),
            env_var_key: env_key.clone(),
            env_var_value: env_val.clone(),
            pid: None,
            data_dir: data_dir.to_string_lossy().to_string(),
            backend: ServiceBackend::External,
            health: Health::Healthy,
            log_path: None,
            started_at: Some(now_secs()),
            command: None,
        };
        write_record(&dir, &info)?;
        inject_env_variable(workspace, &env_key, &env_val)?;
        return Ok(info);
    }

    // 3. Arrancar de verdad, si sabemos cómo y con qué.
    let launch_dirs = LaunchDirs {
        data: data_dir.clone(),
        run: dir.join("run"),
        home: dir.join("home"),
    };
    let Some(plan) = kind.launch_plan(port, db, &launch_dirs) else {
        bail!(
            "antOS todavía no sabe arrancar «{}»: solo puede adoptarlo si ya está \
             escuchando en 127.0.0.1:{port} (arráncalo tú y repite el comando)",
            kind.canonical_name()
        );
    };
    let Some(launcher) = backend::find_launcher(kind) else {
        bail!(
            "no hay forma de arrancar «{}» en esta máquina:\n  \
             • ningún proceso escucha en 127.0.0.1:{port} (si lo arrancas tú, antOS lo adopta)\n  \
             • no se encontró el binario «{}» en PATH ni en los directorios habituales\n  \
             • no hay `nix` para `nix shell nixpkgs#{}`",
            kind.canonical_name(),
            kind.binaries().join("» ni «"),
            kind.nix_package()
        );
    };

    for d in [&launch_dirs.data, &launch_dirs.run, &launch_dirs.home] {
        fs::create_dir_all(d).with_context(|| format!("creando {}", d.display()))?;
    }
    let log_path = dir.join("log");
    let pid = backend::spawn_and_wait(&backend::SpawnContext {
        launcher: &launcher,
        plan: &plan,
        kind,
        port,
        home: &launch_dirs.home,
        run: &launch_dirs.run,
        log_path: &log_path,
    })?;

    let info = ServiceInfo {
        name: kind.canonical_name().into(),
        port,
        status: "running".into(),
        env_var_key: env_key.clone(),
        env_var_value: env_val.clone(),
        pid: Some(pid),
        data_dir: data_dir.to_string_lossy().to_string(),
        backend: launcher.backend(),
        health: Health::Healthy,
        log_path: Some(log_path.to_string_lossy().to_string()),
        started_at: Some(now_secs()),
        command: Some(launcher.describe(&plan.main)),
    };
    write_record(&dir, &info)?;
    inject_env_variable(workspace, &env_key, &env_val)?;
    Ok(info)
}

/// Detiene un servicio nuestro o retira el registro de uno adoptado. Los
/// datos (`data/`, y con Ollama los modelos) se conservan siempre.
pub fn stop_service(service: &str, state_dir: &Path) -> Result<StopOutcome> {
    let kind = parse_kind(service)?;
    let dir = service_dir(state_dir, kind);
    let Some(mut info) = read_record(&dir) else {
        bail!(
            "no se encontró servicio registrado «{}» en {}",
            kind.canonical_name(),
            dir.display()
        );
    };

    let outcome = match info.backend {
        ServiceBackend::External => {
            let _ = fs::remove_file(dir.join("service.json"));
            return Ok(StopOutcome::ExternalUnregistered);
        }
        ServiceBackend::System | ServiceBackend::Nix => match info.pid {
            // Solo se señala un PID que sigue siendo el proceso que
            // arrancamos; si lo reutilizó otro, no es nuestro y no se toca.
            Some(pid)
                if pid != std::process::id()
                    && backend::pid_belongs_to(pid, kind.expected_process_names()) =>
            {
                let forced = backend::terminate(pid, Duration::from_secs(10))?;
                StopOutcome::Terminated { pid, forced }
            }
            _ => StopOutcome::AlreadyGone,
        },
        ServiceBackend::Unknown => StopOutcome::AlreadyGone,
    };
    info.status = "stopped".into();
    info.health = Health::Unhealthy;
    info.pid = None;
    write_record(&dir, &info)?;
    Ok(outcome)
}

/// Estado real de uno o todos los servicios registrados: re-sondea el
/// puerto y comprueba el PID. No escribe nada (`env.service_status` declara
/// `writes = []`); el registro en disco lo actualizan `up` y `down`.
pub fn get_service_status(
    service_filter: Option<&str>,
    state_dir: &Path,
) -> Result<Vec<ServiceInfo>> {
    let base_dir = state_dir.join("services");
    let Ok(read_dir) = fs::read_dir(&base_dir) else {
        return Ok(Vec::new());
    };
    let filter_kind = match service_filter {
        Some(f) => Some(parse_kind(f)?),
        None => None,
    };

    let mut list = Vec::new();
    for entry in read_dir.flatten() {
        let Some(mut info) = read_record(&entry.path()) else {
            continue;
        };
        let Some(kind) = ServiceKind::parse(&info.name) else {
            continue;
        };
        if let Some(fk) = filter_kind {
            if fk != kind {
                continue;
            }
        }
        refresh(&mut info, kind);
        list.push(info);
    }
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(list)
}

/// Últimas `lines` líneas del log de un servicio arrancado por antOS.
pub fn service_logs(service: &str, state_dir: &Path, lines: usize) -> Result<String> {
    let kind = parse_kind(service)?;
    let dir = service_dir(state_dir, kind);
    let Some(info) = read_record(&dir) else {
        bail!("no hay registro de «{}»", kind.canonical_name());
    };
    let Some(log) = info.log_path else {
        bail!(
            "«{}» es un servicio adoptado ({}): antOS no tiene su log",
            kind.canonical_name(),
            info.backend.label()
        );
    };
    Ok(backend::log_tail(Path::new(&log), lines))
}

/// Endpoint HTTP de un servicio registrado **y sano**, para que el demonio
/// use el Ollama que arrancó `service up` aunque no esté en el puerto por
/// defecto. `None` si no hay registro o la sonda falla.
pub fn registered_endpoint(state_dir: &Path, service: &str) -> Option<String> {
    let kind = ServiceKind::parse(service)?;
    let info = read_record(&service_dir(state_dir, kind))?;
    let healthy = backend::probe_once(&kind.probe(), info.port, Duration::from_millis(300));
    if !healthy {
        return None;
    }
    match kind.probe() {
        Probe::Http { .. } => Some(format!("http://127.0.0.1:{}", info.port)),
        Probe::Tcp => None,
    }
}

/// Inserta o actualiza una variable en el `.env` del workspace.
///
/// Si el fichero existe pero no se puede leer, falla en vez de sobrescribirlo
/// con una sola línea: bajo Seatbelt la lectura de `.env` está denegada salvo
/// concesión, y antes de T34.1 eso borraba en silencio el resto del fichero.
pub fn inject_env_variable(workspace: &Path, key: &str, value: &str) -> Result<()> {
    let env_path = workspace.join(".env");
    let mut lines = Vec::new();
    let mut updated = false;

    if env_path.is_file() {
        let content = fs::read_to_string(&env_path).with_context(|| {
            format!(
                "no puedo leer {} para actualizarlo sin perder su contenido",
                env_path.display()
            )
        })?;
        for line in content.lines() {
            if let Some((existing_key, _)) = line.split_once('=') {
                if existing_key.trim() == key {
                    lines.push(format!("{key}={value}"));
                    updated = true;
                    continue;
                }
            }
            lines.push(line.to_string());
        }
    }

    if !updated {
        lines.push(format!("{key}={value}"));
    }

    fs::write(&env_path, lines.join("\n") + "\n")
        .with_context(|| format!("escribiendo {}", env_path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests;
