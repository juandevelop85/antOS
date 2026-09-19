//! `project.run` (T35.2): ejecutar los comandos que declara el stack de un
//! proyecto (`install`, `test`, `build`) **sin abrir una shell**, dentro
//! del recinto, con las cachés de los gestores dentro del proyecto.
//!
//! El `argv` viene de `.antos/project.toml` (escrito por `project.scaffold`
//! desde el catálogo de `system/stacks/`), nunca del texto del usuario ni
//! del modelo. El programa tiene que estar en `stacks::ALLOWED_PROGRAMS` y
//! ningún argumento puede llevar metacaracteres de shell: se comprueba al
//! cargar el catálogo y **otra vez aquí**, porque el manifiesto del proyecto
//! es un fichero del workspace que cualquiera (un agente con `fs.patch`
//! incluido) puede editar.
//!
//! ## Estado de implementación
//!
//! - **Red: todo o nada.** El recinto (`Policy.network: bool`; Seatbelt
//!   `(deny network*)`, Landlock ABI 4) no filtra por dominio. `project.run`
//!   declara red y la previsualización muestra los hosts del stack, pero con
//!   la red abierta el comando puede hablar con cualquier destino. Un filtro
//!   por dominio es otro ticket.
//! - **Toolchain:** binario en `PATH` (o en los directorios habituales) y,
//!   si no lo hay pero sí `nix`, `nix shell nixpkgs#<toolchain> -c <prog>`.
//!   El camino `nix` solo se ha verificado por construcción del comando; en
//!   la máquina de desarrollo no hay `nix`. Bajo Landlock además `/nix` tiene
//!   que ser legible desde el recinto (`SYSTEM_READ_ROOTS`, T35.2).
//! - **Rust bajo Landlock:** `~/.rustup` no es legible desde el recinto
//!   (limitación conocida de T33.2), así que `cargo` por `rustup` falla ahí
//!   igual que en `test.run`; con `cargo` de nixpkgs no.
//! - `dev` (servidor de larga vida) no se ejecuta aquí a propósito: un
//!   proceso que no termina no cabe en un paso con cuota. Es trabajo de
//!   `env.service_up` para proyectos (fase siguiente).

use super::{Change, PendingChanges};
use crate::ctx::Ctx;
use crate::stacks::{ProjectManifest, ALLOWED_PROGRAMS};
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const TAIL_LINES: usize = 60;

/// Lee `.antos/project.toml` de un proyecto del workspace. Si un paso
/// anterior del mismo plan lo va a escribir (`project.scaffold` +
/// `project.run` en una sola propuesta), se lee de ahí: el plan se calcula
/// antes de tocar el disco.
pub fn read_manifest(project_dir: &Path, pending: &PendingChanges) -> Result<ProjectManifest> {
    let path = project_dir.join(".antos").join("project.toml");
    let text = match pending.read(&path) {
        Some(t) => t,
        None => std::fs::read_to_string(&path).with_context(|| {
            format!(
                "el proyecto no tiene {} (lo escribe `project.scaffold`; T35.3 añade `antos project adopt`)",
                path.display()
            )
        })?,
    };
    toml::from_str(&text).with_context(|| format!("{} ilegible", path.display()))
}

/// Comprueba lo mismo que el catálogo, sobre un `argv` que ya está en disco.
pub fn validate_argv(argv: &[String]) -> Result<()> {
    let Some(program) = argv.first() else {
        bail!("comando vacío");
    };
    if !ALLOWED_PROGRAMS.contains(&program.as_str()) {
        bail!(
            "el programa «{program}» no está en la lista blanca ({})",
            ALLOWED_PROGRAMS.join(", ")
        );
    }
    if let Some(bad) = argv.iter().find(|a| {
        a.chars()
            .any(|c| matches!(c, ';' | '|' | '&' | '`' | '$' | '\n' | '\r'))
    }) {
        bail!("el argumento «{bad}» lleva metacaracteres de shell; los argumentos son literales");
    }
    Ok(())
}

pub fn changes_for(
    cap: &str,
    a: &BTreeMap<String, String>,
    ctx: &Ctx,
    pending: &PendingChanges,
) -> Result<Option<Vec<Change>>> {
    if cap != "project.run" {
        return Ok(None);
    }
    let project = a["project"].clone();
    let command = a["command"].clone();
    let project_dir = ctx.workspace.join(&project);
    let manifest = read_manifest(&project_dir, pending)?;
    let argv = match command.as_str() {
        "install" => manifest.commands.install.clone(),
        "test" | "verify" => manifest.commands.test.clone(),
        "build" => manifest.commands.build.clone(),
        other => bail!("comando «{other}» no soportado (install | test | verify | build)"),
    };
    if argv.is_empty() {
        bail!(
            "el stack «{}» del proyecto «{project}» no declara el comando `{command}`",
            manifest.stack
        );
    }
    validate_argv(&argv)?;
    Ok(Some(vec![Change::StackCommand {
        project_dir,
        project,
        command,
        argv,
        toolchain_nix: manifest.toolchain.nix.clone(),
        hosts: manifest.network.hosts.clone(),
    }]))
}

/// Cómo se va a lanzar el programa.
enum Launch {
    /// Binario resuelto a ruta absoluta.
    System(PathBuf),
    /// `nix shell nixpkgs#… -c <programa>`.
    Nix { nix: PathBuf },
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Directorios donde suelen vivir los toolchains sin estar en el `PATH` del
/// demonio (la barra arranca con un entorno mínimo).
fn extra_tool_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/run/current-system/sw/bin"),
    ];
    if let Some(home) = home_dir() {
        dirs.push(home.join(".cargo/bin"));
        dirs.push(home.join(".local/bin"));
        dirs.extend(crate::service::backend::expand_single_wildcard(
            &home.join(".nvm/versions/node/*/bin"),
        ));
    }
    dirs
}

fn resolve_launch(program: &str, toolchain_nix: &[String]) -> Result<Launch> {
    let mut dirs = crate::service::backend::path_dirs();
    dirs.extend(extra_tool_dirs());
    if let Some(p) = crate::service::backend::locate_in(&dirs, program) {
        return Ok(Launch::System(p));
    }
    if let Some(nix) = crate::service::backend::find_nix() {
        if toolchain_nix.is_empty() {
            bail!("no encuentro «{program}» y el stack no declara toolchain para `nix shell`");
        }
        return Ok(Launch::Nix { nix });
    }
    bail!(
        "no encuentro «{program}» en esta máquina y no hay `nix`: instala el toolchain \
         ({program}) o `nix` (con él, antOS lo trae con `nix shell nixpkgs#{}`)",
        toolchain_nix.join(" nixpkgs#")
    )
}

/// Entorno del comando: mínimo, sin credenciales, y con TODO lo que un gestor
/// de paquetes escribe fuera del proyecto redirigido a `.antos/` dentro de él
/// (el recinto no deja escribir en `~/.npm`, `~/.cargo`, `~/.cache`…).
fn project_env(cmd: &mut Command, project_dir: &Path, nix: bool) {
    let antos = project_dir.join(".antos");
    let cache = antos.join("cache");
    let home = antos.join("home");
    let tmp = antos.join("tmp");
    for d in [&cache, &home, &tmp] {
        let _ = std::fs::create_dir_all(d);
    }
    cmd.env_clear();
    let mut path_entries = crate::service::backend::path_dirs();
    path_entries.extend(extra_tool_dirs());
    if let Ok(joined) = std::env::join_paths(path_entries.iter().filter(|p| p.is_dir())) {
        cmd.env("PATH", joined);
    }
    for key in ["USER", "LOGNAME", "LANG", "LC_ALL", "SSL_CERT_FILE", "TERM"] {
        if let Some(v) = std::env::var_os(key) {
            cmd.env(key, v);
        }
    }
    if nix {
        for (key, v) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("NIX_") {
                cmd.env(key, v);
            }
        }
    }
    cmd.env("HOME", &home)
        .env("TMPDIR", &tmp)
        .env("TMP", &tmp)
        .env("TEMP", &tmp)
        .env("XDG_CACHE_HOME", cache.join("xdg"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        // npm / pnpm / yarn
        .env("npm_config_cache", cache.join("npm"))
        .env("npm_config_update_notifier", "false")
        .env("npm_config_fund", "false")
        .env("npm_config_audit", "false")
        .env("PNPM_HOME", cache.join("pnpm"))
        .env("YARN_CACHE_FOLDER", cache.join("yarn"))
        // cargo (el índice del registro va al proyecto; RUSTUP_HOME se
        // hereda si existe: es solo lectura)
        .env("CARGO_HOME", cache.join("cargo"))
        .env("CARGO_TERM_COLOR", "never")
        // uv / pip
        .env("UV_CACHE_DIR", cache.join("uv"))
        .env("UV_PYTHON_INSTALL_DIR", cache.join("uv-python"))
        .env("PIP_CACHE_DIR", cache.join("pip"))
        .env("PYTHONDONTWRITEBYTECODE", "1")
        // go
        .env("GOPATH", cache.join("go"))
        .env("GOMODCACHE", cache.join("go/pkg/mod"))
        .env("GOCACHE", cache.join("go-build"))
        .env("NO_COLOR", "1")
        .env("CI", "1");
    if let Some(rustup) = std::env::var_os("RUSTUP_HOME") {
        cmd.env("RUSTUP_HOME", rustup);
    } else if let Some(home) = home_dir() {
        cmd.env("RUSTUP_HOME", home.join(".rustup"));
    }
}

pub fn apply(change: &Change) -> Result<Option<String>> {
    let Change::StackCommand {
        project_dir,
        project,
        command,
        argv,
        toolchain_nix,
        ..
    } = change
    else {
        return Ok(None);
    };
    // Segunda línea de defensa: el manifiesto es editable.
    validate_argv(argv)?;
    let program = &argv[0];
    let rest = &argv[1..];
    let launch = resolve_launch(program, toolchain_nix)?;
    let (mut cmd, shown) = match &launch {
        Launch::System(path) => {
            let mut c = Command::new(path);
            c.args(rest);
            (c, format!("{} {}", path.display(), rest.join(" ")))
        }
        Launch::Nix { nix } => {
            let mut c = Command::new(nix);
            c.arg("--extra-experimental-features")
                .arg("nix-command flakes")
                .arg("shell");
            for pkg in toolchain_nix {
                c.arg(format!("nixpkgs#{pkg}"));
            }
            c.arg("-c").arg(program).args(rest);
            (
                c,
                format!(
                    "nix shell nixpkgs#{} -c {}",
                    toolchain_nix.join(" nixpkgs#"),
                    argv.join(" ")
                ),
            )
        }
    };
    project_env(&mut cmd, project_dir, matches!(launch, Launch::Nix { .. }));
    cmd.current_dir(project_dir).stdin(Stdio::null());
    let out = cmd
        .output()
        .with_context(|| format!("ejecutando {shown} en {}", project_dir.display()))?;

    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    let logs = project_dir.join(".antos").join("logs");
    let _ = std::fs::create_dir_all(&logs);
    let log_path = logs.join(format!("{command}.log"));
    let _ = std::fs::write(&log_path, format!("$ {shown}\n{text}"));
    let lines: Vec<&str> = text.lines().collect();
    let tail = lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n");
    let code = out.status.code().unwrap_or(-1);

    match command.as_str() {
        // Como `test.run`: el resultado vuelve como texto para que un agente
        // pueda leer el rojo y corregir.
        "test" => Ok(Some(format!(
            "{} ({shown}, código {code}) · log en {}\n{tail}",
            if out.status.success() {
                "TESTS EN VERDE"
            } else {
                "TESTS EN ROJO"
            },
            log_path.display()
        ))),
        // `verify` (T35.3): el andamio no se da por bueno con la suite en
        // rojo; el fallo del paso deja el snapshot para `antos undo`.
        "verify" => {
            if !out.status.success() {
                bail!(
                    "el proyecto «{project}» no pasa su propia suite ({shown}, código {code}; log en {}):\n{tail}",
                    log_path.display()
                );
            }
            Ok(Some(format!(
                "proyecto «{project}» verificado: {shown} en verde · log en {}\n{}",
                log_path.display(),
                lines[lines.len().saturating_sub(6)..].join("\n")
            )))
        }
        _ => {
            if !out.status.success() {
                bail!(
                    "`{command}` de «{project}» falló (código {code}; log en {}):\n{tail}",
                    log_path.display()
                );
            }
            Ok(Some(format!(
                "`{command}` de «{project}» OK ({shown}) · log en {}\n{}",
                log_path.display(),
                lines[lines.len().saturating_sub(8)..].join("\n")
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn argv_validation_rejects_shells_and_metacharacters() {
        assert!(validate_argv(&["npm".into(), "install".into()]).is_ok());
        let sh = validate_argv(&["sh".into(), "-c".into(), "echo".into()]).unwrap_err();
        assert!(sh.to_string().contains("lista blanca"));
        let meta = validate_argv(&["npm".into(), "install && rm -rf /".into()]).unwrap_err();
        assert!(meta.to_string().contains("metacaracteres"));
        assert!(validate_argv(&[]).is_err());
    }

    /// Sin binario y sin `nix` el error nombra las dos salidas; y con el
    /// manifiesto solo en `PendingChanges` (andamio + install en un plan) el
    /// paso se calcula igual.
    #[test]
    fn missing_toolchain_is_explained_and_pending_manifest_is_read() {
        if crate::service::backend::find_nix().is_none() {
            let err = match resolve_launch("antos-programa-inexistente", &["nodejs_22".into()]) {
                Ok(_) => panic!("no debería encontrar el programa"),
                Err(e) => e.to_string(),
            };
            assert!(err.contains("no hay `nix`"), "{err}");
            assert!(err.contains("nix shell nixpkgs#nodejs_22"), "{err}");
        }

        let ctx = Ctx::discover().unwrap();
        let temp = std::env::temp_dir().join(format!("antos_prun_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        let ctx = Ctx {
            workspace: temp.clone(),
            ..ctx
        };
        let stack = crate::stacks::StackCatalog::embedded()
            .unwrap()
            .get("express")
            .unwrap()
            .clone();
        let mut pending = PendingChanges::default();
        pending.apply(&Change::Write {
            path: temp.join("shop/.antos/project.toml"),
            content: stack.project_manifest("shop").unwrap(),
        });
        let mut args = BTreeMap::new();
        args.insert("project".into(), "shop".into());
        args.insert("command".into(), "install".into());
        let changes = changes_for("project.run", &args, &ctx, &pending)
            .unwrap()
            .unwrap();
        match &changes[0] {
            Change::StackCommand {
                argv,
                hosts,
                toolchain_nix,
                ..
            } => {
                assert_eq!(argv, &["npm", "install"]);
                assert_eq!(hosts, &["registry.npmjs.org"]);
                assert_eq!(toolchain_nix, &["nodejs_22"]);
            }
            other => panic!("{other:?}"),
        }
        // Sin manifiesto (ni pendiente ni en disco): error claro.
        let none = PendingChanges::default();
        let err = changes_for("project.run", &args, &ctx, &none)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(err.contains("project.toml"), "{err}");
        let _ = std::fs::remove_dir_all(&temp);
    }

    /// Andamio + install + test de verdad, con el stack `express`, aplicando
    /// los cambios en este proceso (el ejecutor confinado relanza el binario
    /// de `antos`, que no existe dentro de `cargo test`; el camino por el
    /// recinto lo ejercita CI con el binario real, job `antos-linux-desktop`).
    /// `#[ignore]` porque descarga de la red.
    #[test]
    #[ignore = "necesita node/npm y red; lo ejecuta CI Linux"]
    fn real_express_scaffold_install_and_test() {
        let ctx = Ctx::discover().unwrap();
        let temp = std::env::temp_dir().join(format!("antos_prun_real_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        let temp = temp.canonicalize().unwrap();
        let ctx = Ctx {
            workspace: temp.clone(),
            state: temp.join("state"),
            current_project: None,
            ..ctx
        };
        std::fs::create_dir_all(&ctx.state).unwrap();
        let catalog = crate::capability::Catalog::load(&ctx.caps_dir).unwrap();
        let mut pending = PendingChanges::default();
        let step = |cap: &str, kv: &[(&str, &str)]| crate::plan::Step {
            capability: cap.into(),
            args: kv
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        let mut all = Vec::new();
        for st in [
            step(
                "project.scaffold",
                &[
                    ("language", "javascript"),
                    ("framework", "express"),
                    ("name", "shop"),
                ],
            ),
            step(
                "project.run",
                &[("project", "shop"), ("command", "install")],
            ),
            step("project.run", &[("project", "shop"), ("command", "test")]),
        ] {
            let cap = catalog.get(&st.capability).unwrap();
            let changes = super::super::changes_for(&st, cap, &ctx, &pending).unwrap();
            for c in &changes {
                pending.apply(c);
            }
            all.extend(changes);
        }
        let outputs = super::super::apply(&all).expect("andamio + install + test");
        assert!(temp.join("shop/node_modules/express").is_dir());
        assert!(
            temp.join("shop/.antos/cache/npm").is_dir(),
            "caché de npm dentro del proyecto"
        );
        assert!(
            outputs.iter().any(|o| o.starts_with("TESTS EN VERDE")),
            "{outputs:?}"
        );
        let _ = std::fs::remove_dir_all(&temp);
    }

    /// T35.3: `test.run` usa `commands.test` del manifiesto en vez de
    /// adivinar por ficheros (un directorio sin Cargo.toml/package.json que
    /// aun así sabe probarse), y rechaza un manifiesto con un programa fuera
    /// de la lista blanca.
    #[test]
    fn test_run_uses_the_manifest_command() {
        let dir = std::env::temp_dir().join(format!("antos_trun_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".antos")).unwrap();
        let program =
            if crate::service::backend::locate_in(&crate::service::backend::path_dirs(), "node")
                .is_some()
            {
                ("node", vec!["-e", "process.exit(0)"])
            } else {
                ("python3", vec!["-c", "raise SystemExit(0)"])
            };
        let stack = crate::stacks::StackCatalog::embedded()
            .unwrap()
            .get("rust")
            .unwrap()
            .clone();
        let mut manifest: crate::stacks::ProjectManifest =
            toml::from_str(&stack.project_manifest("x").unwrap()).unwrap();
        manifest.commands.test = std::iter::once(program.0.to_string())
            .chain(program.1.iter().map(|a| a.to_string()))
            .collect();
        manifest.save(&dir).unwrap();
        let out = crate::exec::fs::run_tests(&dir, None).expect("run_tests con manifiesto");
        assert!(out.starts_with("TESTS EN VERDE"), "{out}");

        manifest.commands.test = vec!["sh".into(), "-c".into(), "true".into()];
        manifest.save(&dir).unwrap();
        let err = crate::exec::fs::run_tests(&dir, None)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(err.contains("lista blanca"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_env_keeps_caches_inside_the_project_and_drops_secrets() {
        let dir = std::env::temp_dir().join(format!("antos_penv_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut cmd = Command::new("true");
        project_env(&mut cmd, &dir, false);
        let envs: BTreeMap<String, String> = cmd
            .get_envs()
            .filter_map(|(k, v)| {
                Some((
                    k.to_string_lossy().to_string(),
                    v?.to_string_lossy().to_string(),
                ))
            })
            .collect();
        let inside = |key: &str| envs[key].starts_with(&dir.to_string_lossy().to_string());
        for key in [
            "HOME",
            "TMPDIR",
            "npm_config_cache",
            "CARGO_HOME",
            "UV_CACHE_DIR",
            "GOMODCACHE",
            "XDG_CACHE_HOME",
        ] {
            assert!(inside(key), "{key} fuera del proyecto: {}", envs[key]);
        }
        assert!(!envs.contains_key("ANTHROPIC_API_KEY"));
        assert!(dir.join(".antos/cache").is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
