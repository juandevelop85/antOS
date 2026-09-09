//! T30.4 · Integración host del canal IPC entre `antos-barra` y `antosd`.
//!
//! Arranca el demonio (`antos demonio`) con un espacio de trabajo y un estado
//! temporales, le envía una `Request::Intent` sintética por el socket UNIX
//! —exactamente igual que haría la barra de escritorio— y comprueba que
//! despacha la intención y emite el flujo de `Event` que la barra consumiría
//! (empezando por `Event::Start` y terminando limpiamente).
//!
//! No hay GUI: es una verificación a nivel de protocolo, ejecutable en
//! cualquier host donde compile el workspace.

// This whole file is test code (a Rust integration test binary, compiled
// separately from `src/`), so it's exempt from development rule 1 the same
// way any `#[cfg(test)] mod tests` is (T31.7).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use antos_protocol::{Event, Request};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("no pude resolver la raíz del repositorio")
}

/// Un directorio temporal con nombre **corto**: la ruta del socket UNIX no
/// puede pasar de `SUN_LEN` (~104 bytes en macOS), y el demonio le añade
/// `/antos.sock`. `/tmp` (→ `/private/tmp`) deja margen de sobra.
fn unique_tmp() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tag = (nanos as u64).wrapping_mul(2_654_435_761).rotate_left(13) & 0xffff_ffff;
    PathBuf::from("/tmp").join(format!("at304-{:08x}", tag))
}

/// El demonio en marcha, con limpieza garantizada al salir del test.
struct Daemon {
    child: Child,
    socket: PathBuf,
    workspace: PathBuf,
    root: PathBuf,
}

impl Daemon {
    fn start() -> Daemon {
        let repo = repo_root();
        let root = unique_tmp();
        let workspace = root.join("workspace");
        let state = root.join("state");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::create_dir_all(&state).unwrap();
        // El demonio canonicaliza `ANTOS_STATE` (en macOS `/var/...` ->
        // `/private/var/...`); hacemos lo mismo para mirar el socket correcto.
        let workspace = workspace.canonicalize().unwrap();
        let state = state.canonicalize().unwrap();

        // Un repo git vacío: algunas rutas del planificador consultan el estado
        // de git del workspace.
        let _ = Command::new("git")
            .arg("init")
            .arg("-q")
            .current_dir(&workspace)
            .status();

        let socket = state.join("antos.sock");

        let mut child = Command::new(env!("CARGO_BIN_EXE_antos"))
            .arg("demonio")
            .current_dir(&repo)
            .env("ANTOS_WORKSPACE", &workspace)
            .env("ANTOS_STATE", &state)
            .env("ANTOS_CAPABILITIES", repo.join("system/capabilities"))
            .env("ANTOS_SYSTEM_CONFIG", state.join("etc-nixos"))
            // Sin red ni claves: el planificador heurístico local.
            .env_remove("ANTHROPIC_API_KEY")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("no pude lanzar `antos demonio`");

        let deadline = Instant::now() + Duration::from_secs(30);
        while !socket.exists() {
            if let Ok(Some(status)) = child.try_wait() {
                let mut err = String::new();
                if let Some(mut s) = child.stderr.take() {
                    use std::io::Read;
                    let _ = s.read_to_string(&mut err);
                }
                panic!("`antos demonio` terminó antes de escuchar ({status}):\n{err}");
            }
            assert!(
                Instant::now() < deadline,
                "el demonio no creó el socket {} en 30 s",
                socket.display()
            );
            std::thread::sleep(Duration::from_millis(50));
        }

        Daemon {
            child,
            socket,
            workspace,
            root,
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Envía una intención por el socket y devuelve el flujo de eventos recibido.
fn run_intent(socket: &Path, text: &str) -> Vec<Event> {
    let mut stream = UnixStream::connect(socket).expect("no pude conectar al socket del demonio");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();

    let request = Request::Intent {
        text: text.to_string(),
        planner: Some("local".to_string()),
        dry_run: true,
    };
    let line = serde_json::to_string(&request).unwrap();
    stream.write_all(line.as_bytes()).unwrap();
    stream.write_all(b"\n").unwrap();
    stream.flush().unwrap();
    // No más peticiones por esta conexión.
    let _ = stream.shutdown(Shutdown::Write);

    let reader = BufReader::new(stream);
    let mut events = Vec::new();
    for line in reader.lines() {
        let line = line.expect("error leyendo del socket del demonio");
        if line.trim().is_empty() {
            continue;
        }
        let event: Event = serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("evento IPC ilegible para la barra: {e}\n  línea: {line}"));
        events.push(event);
    }
    events
}

/// Un solo test: arrancar `antos demonio` es caro (carga del catálogo,
/// `git init`, sondeo de planificadores), así que ambas comprobaciones
/// —despacho de intención y consulta de estado de git— comparten un demonio.
#[test]
fn daemon_speaks_the_bar_protocol_over_the_ipc_socket() {
    let daemon = Daemon::start();

    // ── 1. Despacho de una intención (lo que hace la barra al escribir texto).
    let intent = "compilar el proyecto y ejecutar los tests";
    let events = run_intent(&daemon.socket, intent);

    // El invariante que le importa a la barra: el demonio DESPACHÓ la intención
    // —el primer evento es `Start` y refleja el texto y el planificador— y todo
    // lo que llegó por el canal es un `Event` que la barra sabe pintar (lo
    // garantiza el parseo en `run_intent`, que hace `panic!` si algo es
    // ilegible), y la conexión se cerró limpiamente (el bucle terminó en EOF).
    assert!(
        !events.is_empty(),
        "el demonio no emitió ningún evento por el canal IPC"
    );
    match &events[0] {
        Event::Start {
            intent: got,
            planner,
        } => {
            assert_eq!(
                got, intent,
                "el primer evento no refleja la intención enviada"
            );
            assert_eq!(planner, "local");
        }
        other => panic!("el primer evento debería ser Event::Start, fue {other:?}"),
    }
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::Start { .. }))
            .count(),
        1,
        "la sesión emitió más de un Event::Start; eventos: {events:?}"
    );

    // ── 2. Consulta de estado de git (la insignia de git de la barra).
    let mut stream = UnixStream::connect(&daemon.socket).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    let request = Request::QueryGitStatus {
        workspace_path: daemon.workspace.to_string_lossy().into_owned(),
    };
    writeln!(stream, "{}", serde_json::to_string(&request).unwrap()).unwrap();
    stream.flush().unwrap();
    let _ = stream.shutdown(Shutdown::Write);

    let mut first = String::new();
    let read = BufReader::new(stream).read_line(&mut first).unwrap();
    assert!(read > 0, "el demonio no respondió a QueryGitStatus");
    let event: Event = serde_json::from_str(first.trim())
        .unwrap_or_else(|e| panic!("respuesta ilegible: {e}\n  línea: {first}"));
    assert!(
        matches!(event, Event::GitStatus(_) | Event::NotGitRepo),
        "se esperaba GitStatus o NotGitRepo, se recibió {event:?}"
    );
}
