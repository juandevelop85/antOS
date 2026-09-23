//! Tests de `antos system update` (`update/mod.rs`), separados por
//! tamaño (T31.15): movimiento de código, no cambio de comportamiento.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use antos_protocol::InstallConfig;
use std::io::Cursor;

const DIFF_OUTPUT: &str = "antosd: 0.1.0 → 0.1.1, +2.3 MiB\n\
antos-barra: 0.1.0 → 0.1.1\n\
firefox: 131.0 → 132.0, +1.2 MiB\n\
linux: 6.11.2 → 6.11.5, +0.0 MiB\n\
some-lib: ∅ → 2.0\n\
old-tool: 1.0 → ∅, -4.0 MiB\n";

const GENERATIONS_OLD: &str = r#"[{"generation":3,"date":"2026-09-20 10:00:00","nixosVersion":"25.11","kernelVersion":"6.11.2","current":true},{"generation":2,"date":"2026-09-10 10:00:00","nixosVersion":"25.11","kernelVersion":"6.11.0","current":false}]"#;
const GENERATIONS_NEW: &str = r#"[{"generation":4,"date":"2026-09-21 12:00:00","nixosVersion":"25.11","kernelVersion":"6.11.5","current":true},{"generation":3,"date":"2026-09-20 10:00:00","nixosVersion":"25.11","kernelVersion":"6.11.2","current":false}]"#;

/// Un runner que contesta como la máquina antOS Linux lo haría y graba
/// cada comando. `sudo cp/mv` se ejecutan de verdad sobre el directorio
/// temporal, para comprobar que el lock vuelve a `/etc/nixos`.
struct FakeRunner {
    commands: Vec<String>,
    switched: bool,
    candidate: String,
    diff: String,
    /// Fecha de la generación actual (para simular «cambios locales»).
    current_date: String,
}

impl FakeRunner {
    fn new(candidate: &str) -> Self {
        Self {
            commands: Vec::new(),
            switched: false,
            candidate: candidate.into(),
            diff: DIFF_OUTPUT.into(),
            current_date: "2026-09-20 10:00:00".into(),
        }
    }
    fn record(&mut self, program: &str, args: &[String]) -> String {
        let line = std::iter::once(program.to_string())
            .chain(args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ");
        self.commands.push(line.clone());
        line
    }
}

impl InstallRunner for FakeRunner {
    fn run(&mut self, program: &str, args: &[String], _stdin: Option<&str>) -> Result<String> {
        let line = self.record(program, args);
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        Ok(match (program, a.as_slice()) {
            ("nix", ["eval", "--raw", _]) => self.candidate.clone(),
            ("nix", ["flake", "update", ..]) => {
                // El lock nuevo aparece en la copia.
                let work = a[3];
                fs::write(Path::new(work).join("flake.lock"), "{ \"nuevo\": true }").unwrap();
                String::new()
            }
            ("nix", ["store", "diff-closures", ..]) => self.diff.clone(),
            ("nixos-rebuild", ["list-generations", "--json"]) => {
                if self.switched {
                    GENERATIONS_NEW.into()
                } else {
                    GENERATIONS_OLD.replace("2026-09-20 10:00:00", &self.current_date)
                }
            }
            ("sudo", ["cp", src, dst]) => {
                fs::copy(src, dst).unwrap();
                String::new()
            }
            ("sudo", ["mv", src, dst]) => {
                fs::rename(src, dst).unwrap();
                String::new()
            }
            _ => panic!("comando inesperado: {line}"),
        })
    }
    fn run_streaming(
        &mut self,
        program: &str,
        args: &[String],
        on_line: &mut dyn FnMut(&str),
    ) -> Result<()> {
        let line = self.record(program, args);
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        match (program, a.as_slice()) {
            ("nix", ["build", ..]) => {
                on_line("construyendo…");
                on_line(&self.candidate.clone());
            }
            ("sudo", ["nixos-rebuild", "switch", ..]) => {
                self.switched = true;
                on_line("activating the configuration...");
            }
            _ => panic!("comando de streaming inesperado: {line}"),
        }
        Ok(())
    }
    fn is_mountpoint(&self, _path: &Path) -> bool {
        false
    }
    fn is_real(&self) -> bool {
        false
    }
}

fn temp_dir(tag: &str) -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("antos-update-{tag}-{now}"));
    fs::create_dir_all(&p).unwrap();
    p
}

/// Una máquina «instalada»: `/etc/nixos` tal cual lo genera el
/// instalador (mismo formato que la realidad), estado y
/// `/run/current-system` como enlace.
fn installed_machine(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let etc = root.join("etc/nixos");
    let cfg = InstallConfig {
        hostname: "antos-lap".into(),
        ..InstallConfig::default()
    };
    crate::installer::DeployEngine::generate_nixos_config(&cfg, &etc, None).unwrap();
    fs::write(etc.join("flake.lock"), "{ \"viejo\": true }").unwrap();
    let state = root.join("state");
    fs::create_dir_all(&state).unwrap();
    let current = root.join("current-system");
    std::os::unix::fs::symlink("/nix/store/aaaa-nixos-system-antos-lap-old", &current).unwrap();
    (etc, state, current)
}

#[test]
fn test_parse_diff_closures() {
    let changes = parse_diff_closures(DIFF_OUTPUT);
    assert_eq!(changes.len(), 6);
    assert_eq!(changes[0].name, "antosd");
    assert_eq!(changes[0].before.as_deref(), Some("0.1.0"));
    assert_eq!(changes[0].after.as_deref(), Some("0.1.1"));
    assert_eq!(changes[0].size.as_deref(), Some("+2.3 MiB"));
    assert_eq!(changes[1].size, None);
    let new = changes.iter().find(|c| c.name == "some-lib").unwrap();
    assert_eq!(new.before, None);
    assert_eq!(new.kind(), "nuevo");
    let gone = changes.iter().find(|c| c.name == "old-tool").unwrap();
    assert_eq!(gone.after, None);
    assert_eq!(gone.kind(), "eliminado");
    assert!(parse_diff_closures("").is_empty());
}

#[test]
fn test_parse_generations() {
    let g = parse_generations(GENERATIONS_OLD).unwrap();
    assert_eq!(g.len(), 2);
    assert!(g[0].current);
    assert_eq!(g[0].generation, 3);
    assert_eq!(g[1].kernel_version, "6.11.0");
}

/// El flake que escribe el instalador es el que `update` lee: hostname
/// y fuente se extraen de él, y la fuente se reescribe sin tocar nada más.
#[test]
fn test_flake_helpers_match_the_generated_flake() {
    let root = temp_dir("flake");
    let (etc, _, _) = installed_machine(&root);
    let flake = fs::read_to_string(etc.join("flake.nix")).unwrap();
    assert_eq!(
        SystemUpdater::hostname_from_flake(&flake).as_deref(),
        Some("antos-lap")
    );
    assert_eq!(
        SystemUpdater::source_of(&flake).as_deref(),
        Some("path:/etc/nixos/antos")
    );
    let rewritten = SystemUpdater::with_source(&flake, "github:juandevelop85/antOS/v0.2.0");
    assert_eq!(
        SystemUpdater::source_of(&rewritten).as_deref(),
        Some("github:juandevelop85/antOS/v0.2.0")
    );
    assert_eq!(rewritten.lines().count(), flake.lines().count());
    assert!(rewritten.contains("antos.inputs.nixpkgs.follows"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn test_check_reports_availability_and_writes_json() {
    let root = temp_dir("check");
    let (etc, state, current) = installed_machine(&root);
    let mut runner = FakeRunner::new("/nix/store/bbbb-nixos-system-antos-lap-new");
    let mut up = SystemUpdater::new(&etc, &state, &current, &mut runner);
    let r = up.check("antos-lap").unwrap();
    assert!(r.available);
    assert_eq!(r.current, "/nix/store/aaaa-nixos-system-antos-lap-old");
    let json: UpdateAvailable =
        serde_json::from_str(&fs::read_to_string(state.join(UPDATE_AVAILABLE_FILE)).unwrap())
            .unwrap();
    assert!(json.available);
    // Solo evaluó: ni build, ni switch, ni diff.
    assert!(runner
        .commands
        .iter()
        .any(|c| c.starts_with("nix flake update --flake")));
    assert!(runner
        .commands
        .iter()
        .any(|c| c.starts_with("nix eval --raw path:")));
    assert!(!runner
        .commands
        .iter()
        .any(|c| c.contains("nix build") || c.contains("switch")));
    // El original no se tocó.
    assert_eq!(
        fs::read_to_string(etc.join("flake.lock")).unwrap(),
        "{ \"viejo\": true }"
    );

    let mut runner = FakeRunner::new("/nix/store/aaaa-nixos-system-antos-lap-old");
    let mut up = SystemUpdater::new(&etc, &state, &current, &mut runner);
    assert!(!up.check("antos-lap").unwrap().available);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn test_update_applies_with_yes_and_journals() {
    let root = temp_dir("apply");
    let (etc, state, current) = installed_machine(&root);
    let journal = state.join("journal.jsonl");
    let mut runner = FakeRunner::new("/nix/store/bbbb-nixos-system-antos-lap-new");
    let mut up = SystemUpdater::new(&etc, &state, &current, &mut runner);
    let mut out = Vec::new();
    let report = run_update(
        &mut up,
        &journal,
        "antos-lap",
        None,
        true,
        &mut Cursor::new(b""),
        &mut out,
    )
    .unwrap();
    assert!(report.applied);
    assert_eq!(report.from_generation, Some(3));
    assert_eq!(report.to_generation, Some(4));
    assert_eq!(report.changes.len(), 6);
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("antosd"), "{text}");
    assert!(text.contains("0.1.0 → 0.1.1"), "{text}");

    let work = state.join(WORK_DIR).display().to_string();
    let c = &runner.commands;
    assert_eq!(c[0], format!("nix flake update --flake {work}"));
    assert_eq!(c[1], "nixos-rebuild list-generations --json");
    assert!(c[2].starts_with(&format!("nix build path:{work}#nixosConfigurations.antos-lap.config.system.build.toplevel -o {work}/result")));
    assert_eq!(
        c[3],
        format!(
            "nix store diff-closures {} /nix/store/bbbb-nixos-system-antos-lap-new",
            current.display()
        )
    );
    assert_eq!(
        c[4],
        format!("sudo nixos-rebuild switch --flake {work}#antos-lap")
    );
    // El lock nuevo volvió a /etc/nixos (cp a .new + mv); el flake.nix, sin cambios, no se tocó.
    assert!(c
        .iter()
        .any(|x| x.starts_with("sudo cp ") && x.contains("flake.lock")));
    assert!(!c
        .iter()
        .any(|x| x.starts_with("sudo cp ") && x.contains("flake.nix")));
    assert_eq!(
        fs::read_to_string(etc.join("flake.lock")).unwrap(),
        "{ \"nuevo\": true }"
    );
    assert!(!etc.join(".flake.lock.new").exists());

    let records = crate::journal::read_all(&journal).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].intent, INTENT_UPDATE);
    assert_eq!(records[0].outcome, Outcome::Executed);
    assert!(records[0]
        .detail
        .as_deref()
        .unwrap()
        .contains("generación 3 → 4"));
    assert!(records[0]
        .detail
        .as_deref()
        .unwrap()
        .contains("antosd: 0.1.0 → 0.1.1"));

    // Rollback: marca la actualización como revertida y anota la suya.
    let mut up = SystemUpdater::new(&etc, &state, &current, &mut runner);
    let mut out = Vec::new();
    run_rollback(&mut up, &journal, None, &mut out).unwrap();
    assert!(runner
        .commands
        .iter()
        .any(|x| x == "sudo nixos-rebuild switch --rollback"));
    let records = crate::journal::read_all(&journal).unwrap();
    assert_eq!(records.len(), 2);
    assert!(records[0].reverted);
    assert_eq!(records[1].intent, INTENT_ROLLBACK);
    assert_eq!(records[1].outcome, Outcome::Reverted);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn test_update_declined_touches_nothing() {
    let root = temp_dir("decline");
    let (etc, state, current) = installed_machine(&root);
    let journal = state.join("journal.jsonl");
    let mut runner = FakeRunner::new("/nix/store/bbbb-nixos-system-antos-lap-new");
    let mut up = SystemUpdater::new(&etc, &state, &current, &mut runner);
    let mut out = Vec::new();
    let report = run_update(
        &mut up,
        &journal,
        "antos-lap",
        None,
        false,
        &mut Cursor::new(b"n\n"),
        &mut out,
    )
    .unwrap();
    assert!(!report.applied);
    assert!(!runner.commands.iter().any(|c| c.contains("switch")));
    assert_eq!(
        fs::read_to_string(etc.join("flake.lock")).unwrap(),
        "{ \"viejo\": true }"
    );
    assert!(!journal.exists());
    assert!(String::from_utf8_lossy(&out).contains("cancelado"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn test_update_without_changes_is_up_to_date() {
    let root = temp_dir("uptodate");
    let (etc, state, current) = installed_machine(&root);
    let journal = state.join("journal.jsonl");
    let mut runner = FakeRunner::new("/nix/store/aaaa-nixos-system-antos-lap-old");
    runner.diff = String::new();
    // La generación actual es posterior al configuration.nix recién
    // escrito: sin cambios locales.
    runner.current_date = "2999-01-01 00:00:00".into();
    let mut up = SystemUpdater::new(&etc, &state, &current, &mut runner);
    let mut out = Vec::new();
    let report = run_update(
        &mut up,
        &journal,
        "antos-lap",
        None,
        true,
        &mut Cursor::new(b""),
        &mut out,
    )
    .unwrap();
    assert!(!report.applied);
    assert!(!report.local_changes);
    assert!(report.changes.is_empty());
    assert!(String::from_utf8_lossy(&out).contains("al día"));
    assert!(!runner.commands.iter().any(|c| c.contains("switch")));
    let _ = fs::remove_dir_all(&root);
}

/// Con `configuration.nix` editado después de la generación actual, el
/// diff lo dice y, aunque la closure no cambie, se ofrece aplicar.
#[test]
fn test_update_reports_local_changes() {
    let root = temp_dir("local");
    let (etc, state, current) = installed_machine(&root);
    let journal = state.join("journal.jsonl");
    let mut runner = FakeRunner::new("/nix/store/aaaa-nixos-system-antos-lap-old");
    runner.diff = String::new();
    let mut up = SystemUpdater::new(&etc, &state, &current, &mut runner);
    let mut out = Vec::new();
    let report = run_update(
        &mut up,
        &journal,
        "antos-lap",
        None,
        true,
        &mut Cursor::new(b""),
        &mut out,
    )
    .unwrap();
    assert!(report.local_changes);
    assert!(report.applied);
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("cambios locales"), "{text}");
    let records = crate::journal::read_all(&journal).unwrap();
    assert!(records[0]
        .detail
        .as_deref()
        .unwrap()
        .contains("incluye cambios locales"));
    let _ = fs::remove_dir_all(&root);
}

/// `--source` reescribe `antos.url` en la copia y, tras aplicar, el
/// `flake.nix` nuevo vuelve a /etc/nixos; el original no se toca antes.
#[test]
fn test_update_with_new_source_rewrites_flake_after_switch() {
    let root = temp_dir("source");
    let (etc, state, current) = installed_machine(&root);
    let journal = state.join("journal.jsonl");
    let other = root.join("other-antos");
    fs::create_dir_all(&other).unwrap();
    let url = format!("path:{}", other.display());
    let mut runner = FakeRunner::new("/nix/store/bbbb-nixos-system-antos-lap-new");
    let mut up = SystemUpdater::new(&etc, &state, &current, &mut runner);
    let mut out = Vec::new();
    run_update(
        &mut up,
        &journal,
        "antos-lap",
        Some(&url),
        true,
        &mut Cursor::new(b""),
        &mut out,
    )
    .unwrap();
    let flake = fs::read_to_string(etc.join("flake.nix")).unwrap();
    assert_eq!(
        SystemUpdater::source_of(&flake).as_deref(),
        Some(url.as_str())
    );
    assert!(runner
        .commands
        .iter()
        .any(|x| x.starts_with("sudo cp ") && x.contains("flake.nix")));
    assert!(String::from_utf8_lossy(&out).contains("fuente de antOS"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn test_source_reachable_needs_no_network_for_paths() {
    assert!(SystemUpdater::source_reachable("path:/etc/nixos/antos"));
    assert!(SystemUpdater::source_reachable("git+file:///srv/antos"));
    assert!(!SystemUpdater::source_reachable("https://"));
}

#[test]
fn test_looks_offline_reconoce_los_fallos_de_red_de_nix() {
    // Lo que `nix flake update` escribió en el smoke de instalación sin red.
    assert!(SystemUpdater::looks_offline(
        "warning: you don't have Internet access; disabling some network-dependent features"
    ));
    assert!(SystemUpdater::looks_offline(
        "error: unable to download 'https://github.com/NixOS/nixpkgs/archive/34ab990.tar.gz': \
         Could not resolve hostname (6) Could not resolve host: github.com"
    ));
    // Y la guarda propia, que salta antes de invocar a nix.
    assert!(SystemUpdater::looks_offline(
        "sin acceso a la fuente de actualizaciones (github:juan/antos); nada que comprobar"
    ));
    // Un fallo de verdad no se disfraza de falta de red.
    assert!(!SystemUpdater::looks_offline(
        "error: attribute 'nixosConfigurations.antos-box' missing"
    ));
}
