#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::backend::{self, Launcher, ServiceBackend, SpawnContext};
use super::kinds::{Cmd, LaunchDirs, LaunchPlan, ServiceKind};
use super::*;
use std::net::TcpListener;
use std::path::PathBuf;

fn temp_base(tag: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "antos_service_{tag}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(base.join("workspace")).unwrap();
    base
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Un puerto en el que seguro no escucha nadie: el 1 es privilegiado y
/// ningún test lo abre. `free_port()` no sirve para esto: macOS reparte los
/// efímeros en orden y otro test en paralelo puede recibir el mismo justo
/// después de que lo soltemos.
const CLOSED_PORT: u16 = 1;

// ------------------------------------------------------------------ kinds

#[test]
fn known_kinds_have_ports_urls_and_aliases() {
    assert_eq!(default_port("postgres"), Some(5432));
    assert_eq!(default_port("psql"), Some(5432));
    assert_eq!(default_port("redis"), Some(6379));
    assert_eq!(default_port("mysql"), Some(3306));
    assert_eq!(default_port("OLLAMA"), Some(11434));
    assert_eq!(default_port("mongo"), None, "desconocido → None, no 8080");

    let (k, v) = build_connection_url("postgres", 5432, "my_app").unwrap();
    assert_eq!(k, "DATABASE_URL");
    assert_eq!(v, "postgres://antos:antos@127.0.0.1:5432/my_app");
    let (k, v) = build_connection_url("ollama", 11500, "").unwrap();
    assert_eq!(k, "OLLAMA_HOST");
    assert_eq!(v, "http://127.0.0.1:11500");
    assert!(build_connection_url("mongo", 1, "").is_none());
}

#[test]
fn launch_plans_never_touch_a_shell_and_mark_setup() {
    let dirs = LaunchDirs {
        data: PathBuf::from("/s/data"),
        run: PathBuf::from("/s/run"),
        home: PathBuf::from("/s/home"),
    };
    let pg = ServiceKind::Postgres
        .launch_plan(5499, "db1", &dirs)
        .unwrap();
    assert_eq!(pg.setup[0].program, "initdb");
    assert_eq!(pg.setup_marker, Some(PathBuf::from("/s/data/PG_VERSION")));
    assert_eq!(pg.main.program, "postgres");
    assert!(pg
        .main
        .args
        .contains(&"listen_addresses=127.0.0.1".to_string()));
    assert_eq!(pg.post_start[0].program, "createdb");
    assert!(pg.post_start[0].args.contains(&"db1".to_string()));

    let ollama = ServiceKind::Ollama.launch_plan(11500, "", &dirs).unwrap();
    assert_eq!(ollama.main.args, vec!["serve".to_string()]);
    assert!(ollama
        .env
        .contains(&("OLLAMA_HOST".to_string(), "127.0.0.1:11500".to_string())));
    assert!(ollama
        .env
        .contains(&("OLLAMA_MODELS".to_string(), "/s/data".to_string())));

    for k in ServiceKind::ALL {
        if let Some(plan) = k.launch_plan(1, "x", &dirs) {
            for c in plan
                .setup
                .iter()
                .chain([&plan.main])
                .chain(&plan.post_start)
            {
                assert!(
                    !matches!(c.program, "sh" | "bash" | "zsh"),
                    "{k:?} usa un intérprete"
                );
            }
        }
    }
    assert!(ServiceKind::MariaDb.launch_plan(1, "x", &dirs).is_none());
    assert!(ServiceKind::RabbitMq.launch_plan(1, "x", &dirs).is_none());
}

#[test]
fn nix_launcher_wraps_the_program_without_a_shell() {
    let l = Launcher::Nix {
        nix: PathBuf::from("/nix/bin/nix"),
        package: "redis",
    };
    let cmd = Cmd {
        program: "redis-server",
        args: vec!["--port".into(), "1".into()],
    };
    assert_eq!(
        l.describe(&cmd),
        "nix shell nixpkgs#redis -c redis-server --port 1"
    );
    assert_eq!(l.backend(), ServiceBackend::Nix);
}

// --------------------------------------------------------------- adopción

#[test]
fn adopts_a_listening_port_as_external_and_never_kills_it() {
    let base = temp_base("adopt");
    let state = base.join(".antos");
    let workspace = base.join("workspace");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let info = start_service("redis", Some(port), None, &state, &workspace).unwrap();
    assert_eq!(info.backend, ServiceBackend::External);
    assert_eq!(info.status, "external");
    assert_eq!(info.health, Health::Healthy);
    assert_eq!(info.pid, None);
    let env = fs::read_to_string(workspace.join(".env")).unwrap();
    assert!(env.contains(&format!("REDIS_URL=redis://127.0.0.1:{port}")));

    let listed = get_service_status(Some("redis"), &state).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].status, "external");

    let outcome = stop_service("redis", &state).unwrap();
    assert_eq!(outcome, StopOutcome::ExternalUnregistered);
    assert!(
        !state.join("services/redis/service.json").exists(),
        "el registro de un adoptado se retira"
    );
    // El «servicio» externo sigue vivo: nadie lo mató.
    assert!(backend::probe_once(
        &super::kinds::Probe::Tcp,
        port,
        Duration::from_millis(300)
    ));
    drop(listener);
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn failing_to_start_writes_neither_record_nor_env() {
    let base = temp_base("nostart");
    let state = base.join(".antos");
    let workspace = base.join("workspace");
    // Puerto libre y un servicio sin plan de arranque: no hay nada que
    // adoptar ni forma de arrancarlo.
    let err = start_service("mariadb", Some(CLOSED_PORT), None, &state, &workspace).unwrap_err();
    assert!(
        err.to_string().contains("todavía no sabe arrancar"),
        "{err}"
    );
    assert!(!state.join("services/mariadb/service.json").exists());
    assert!(!workspace.join(".env").exists());

    let err = start_service("mongo", None, None, &state, &workspace).unwrap_err();
    assert!(err.to_string().contains("servicio desconocido"), "{err}");
    let _ = fs::remove_dir_all(&base);
}

// ------------------------------------------------------------ proceso real

fn python3() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("python3"))
            .find(|c| c.is_file())
    })
}

/// El camino `System` de punta a punta con un proceso cualquiera que abra un
/// puerto: `setsid`, log, espera de la sonda, PID vivo, `SIGTERM`.
#[test]
fn spawns_detached_waits_for_the_port_and_terminates() {
    let Some(py) = python3() else {
        eprintln!("sin python3: se omite");
        return;
    };
    let base = temp_base("spawn");
    let dirs = LaunchDirs {
        data: base.join("data"),
        run: base.join("run"),
        home: base.join("home"),
    };
    for d in [&dirs.data, &dirs.run, &dirs.home] {
        fs::create_dir_all(d).unwrap();
    }
    let port = free_port();
    let plan = LaunchPlan {
        setup: Vec::new(),
        setup_marker: None,
        // Un servidor TCP mínimo (no `http.server`: hace una resolución
        // inversa de DNS al arrancar y se queda colgado si el resolvedor
        // no responde, lo que convertía este test en una lotería).
        main: Cmd {
            program: "python3",
            args: vec![
                "-c".into(),
                format!(
                    "import socket,time\ns=socket.socket()\ns.bind(('127.0.0.1',{port}))\n\
                     s.listen(1)\ntime.sleep(600)"
                ),
            ],
        },
        env: Vec::new(),
        post_start: Vec::new(),
    };
    let launcher = Launcher::System {
        bins: vec![("python3", py)],
    };
    let log = base.join("log");
    let pid = backend::spawn_and_wait(&SpawnContext {
        launcher: &launcher,
        plan: &plan,
        // Sonda TCP: basta con que el puerto acepte.
        kind: ServiceKind::Redis,
        port,
        home: &dirs.home,
        run: &dirs.run,
        log_path: &log,
    })
    .expect("debe arrancar y escuchar");
    assert!(backend::pid_alive(pid));
    assert!(log.exists());

    let forced = backend::terminate(pid, Duration::from_secs(5)).unwrap();
    assert!(!forced, "un proceso python muere con SIGTERM");
    assert!(!backend::pid_alive(pid));
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn a_dead_pid_is_reported_as_stopped_not_running() {
    let base = temp_base("dead");
    let state = base.join(".antos");
    // Un PID que existió y ya no: lanzamos `true` y esperamos a que muera.
    let mut child = std::process::Command::new("true").spawn().unwrap();
    let dead_pid = child.id();
    child.wait().unwrap();

    let dir = state.join("services/redis");
    fs::create_dir_all(&dir).unwrap();
    let info = ServiceInfo {
        name: "redis".into(),
        port: CLOSED_PORT,
        status: "running".into(),
        env_var_key: "REDIS_URL".into(),
        env_var_value: "redis://127.0.0.1:1".into(),
        pid: Some(dead_pid),
        data_dir: String::new(),
        backend: ServiceBackend::System,
        health: Health::Healthy,
        log_path: None,
        started_at: None,
        command: None,
    };
    fs::write(
        dir.join("service.json"),
        serde_json::to_string(&info).unwrap(),
    )
    .unwrap();

    let listed = get_service_status(None, &state).unwrap();
    assert_eq!(listed[0].status, "stopped");
    assert_eq!(listed[0].health, Health::Unhealthy);
    assert_eq!(listed[0].pid, None);

    let outcome = stop_service("redis", &state).unwrap();
    assert_eq!(outcome, StopOutcome::AlreadyGone);
    let _ = fs::remove_dir_all(&base);
}

/// Un PID vivo que no es el servicio (reutilizado por otro proceso) cuenta
/// como muerto y, sobre todo, no recibe ninguna señal.
#[test]
fn a_reused_pid_is_not_ours_and_is_never_signalled() {
    let base = temp_base("reused");
    let state = base.join(".antos");
    // Un proceso ajeno vivo: `sleep`. Si `stop_service` lo señalara, el
    // `try_wait` de abajo lo vería terminado.
    let mut bystander = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let dir = state.join("services/redis");
    fs::create_dir_all(&dir).unwrap();
    let info = ServiceInfo {
        name: "redis".into(),
        port: CLOSED_PORT,
        status: "running".into(),
        env_var_key: "REDIS_URL".into(),
        env_var_value: "redis://127.0.0.1:1".into(),
        pid: Some(bystander.id()),
        data_dir: String::new(),
        backend: ServiceBackend::System,
        health: Health::Healthy,
        log_path: None,
        started_at: None,
        command: None,
    };
    fs::write(
        dir.join("service.json"),
        serde_json::to_string(&info).unwrap(),
    )
    .unwrap();

    let listed = get_service_status(Some("redis"), &state).unwrap();
    assert_eq!(listed[0].status, "stopped", "un PID ajeno no es «running»");
    assert_eq!(
        stop_service("redis", &state).unwrap(),
        StopOutcome::AlreadyGone
    );
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        bystander.try_wait().unwrap().is_none(),
        "el proceso ajeno debe seguir vivo"
    );
    let _ = bystander.kill();
    let _ = bystander.wait();
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn legacy_records_without_backend_are_unknown_and_not_running() {
    let base = temp_base("legacy");
    let state = base.join(".antos");
    let dir = state.join("services/postgres");
    fs::create_dir_all(&dir).unwrap();
    // Lo que escribía `start_service` antes de T34.1.
    let legacy = format!(
        r#"{{"name":"postgres","port":{},"status":"running","env_var_key":"DATABASE_URL",
            "env_var_value":"postgres://antos:antos@127.0.0.1:5432/antos_dev","pid":null,
            "data_dir":"/x"}}"#,
        CLOSED_PORT
    );
    fs::write(dir.join("service.json"), legacy).unwrap();

    let listed = get_service_status(Some("postgres"), &state).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].backend, ServiceBackend::Unknown);
    assert_eq!(listed[0].status, "stopped");
    assert_eq!(listed[0].health, Health::Unhealthy);
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn inject_env_preserves_other_lines_and_updates_in_place() {
    let base = temp_base("env");
    let ws = base.join("workspace");
    fs::write(ws.join(".env"), "A=1\nDATABASE_URL=old\nB=2\n").unwrap();
    inject_env_variable(&ws, "DATABASE_URL", "new").unwrap();
    inject_env_variable(&ws, "REDIS_URL", "r").unwrap();
    let env = fs::read_to_string(ws.join(".env")).unwrap();
    assert_eq!(env, "A=1\nDATABASE_URL=new\nB=2\nREDIS_URL=r\n");
    let _ = fs::remove_dir_all(&base);
}

// ------------------------------------------------ integración (binario real)

/// Arranca y para un servicio real por `System`/`Nix`. Se ejecuta con
/// `cargo test -p antosd -- --ignored service::` donde el binario exista
/// (CI Linux instala `redis-server` y `postgresql`).
fn roundtrip_real(service: &str, env_key: &str) {
    let base = temp_base(&format!("real_{service}"));
    let state = base.join(".antos");
    let workspace = base.join("workspace");
    let port = free_port();

    let info = start_service(service, Some(port), Some("antos_t"), &state, &workspace)
        .unwrap_or_else(|e| panic!("{service} debe arrancar: {e:#}"));
    assert!(matches!(
        info.backend,
        ServiceBackend::System | ServiceBackend::Nix
    ));
    assert_eq!(info.status, "running");
    assert!(info.pid.is_some());
    let env = fs::read_to_string(workspace.join(".env")).unwrap();
    assert!(env.contains(&format!("{env_key}=")));

    // Idempotente: repetir devuelve el mismo proceso, no arranca otro.
    let again = start_service(service, Some(port), Some("antos_t"), &state, &workspace).unwrap();
    assert_eq!(again.pid, info.pid);

    let listed = get_service_status(Some(service), &state).unwrap();
    assert_eq!(listed[0].status, "running");
    assert_eq!(listed[0].health, Health::Healthy);

    match stop_service(service, &state).unwrap() {
        StopOutcome::Terminated { pid, .. } => assert_eq!(Some(pid), info.pid),
        other => panic!("esperaba Terminated, fue {other:?}"),
    }
    let after = get_service_status(Some(service), &state).unwrap();
    assert_eq!(after[0].status, "stopped");
    assert!(
        after[0].data_dir.ends_with("data"),
        "los datos se conservan"
    );
    let _ = fs::remove_dir_all(&base);
}

#[test]
#[ignore = "necesita redis-server o nix en la máquina"]
fn real_redis_roundtrip() {
    roundtrip_real("redis", "REDIS_URL");
}

#[test]
#[ignore = "necesita postgres/initdb o nix en la máquina"]
fn real_postgres_roundtrip() {
    roundtrip_real("postgres", "DATABASE_URL");
}

#[test]
#[ignore = "necesita el binario ollama o nix; arranca un segundo Ollama en otro puerto"]
fn real_ollama_roundtrip() {
    roundtrip_real("ollama", "OLLAMA_HOST");
}

/// T34.3: la detección por systemd nunca inventa: una unidad inexistente es
/// `false` en cualquier plataforma, y sin registro ni unidad activa no hay
/// entrada sintética en `antos services`.
#[test]
fn systemd_detection_never_invents_a_service() {
    assert!(!backend::systemd_unit_active("antos-unidad-que-no-existe"));
    let base = temp_base("managed");
    let state = base.join(".antos");
    let listed = get_service_status(Some("rabbitmq"), &state).unwrap();
    assert!(
        listed.is_empty(),
        "sin registro ni rabbitmq.service activo no se lista nada: {listed:?}"
    );
    let _ = fs::remove_dir_all(&base);
}
