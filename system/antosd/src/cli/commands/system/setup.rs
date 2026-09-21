//! `antos setup`: el primer arranque de antOS Linux (T36.5).
//!
//! Deja la máquina lista para trabajar en pocos minutos, y se puede volver
//! a ejecutar cuando se quiera: cada paso detecta si ya está hecho y lo
//! dice (`→ saltado (ya hecho)`), de modo que una segunda pasada no toca
//! nada. Los pasos, en orden:
//!
//! 1. **Identidad git** — `user.name` / `user.email` en
//!    `~/.config/git/config` (escritura directa del fichero, sin depender
//!    del `PATH`).
//! 2. **Clave SSH** — `ssh-keygen -t ed25519` si no hay `~/.ssh/id_ed25519`;
//!    muestra la pública para pegarla en la forja.
//! 3. **Flathub** — `flatpak remote-add --if-not-exists --user flathub …`
//!    (también lo hace, sin este comando, el servicio de usuario
//!    `antos-flathub` de `desktop.nix`).
//! 4. **Modelos** — perfil `local` / `hybrid` / `cloud` (T34.4) y, solo con
//!    confirmación, la descarga del modelo por defecto (`antos llm setup`).
//! 5. **Claves API** — para `hybrid` / `cloud`, a la bóveda cifrada
//!    (`antos secrets`), tecleadas sin eco; nunca a un fichero en claro.
//! 6. **`gh auth login`** — opcional, en primer plano.
//! 7. **`antos doctor --desktop`** — el resumen final.
//!
//! El marcador `$ANTOS_STATE/setup.toml` guarda qué se completó y cuándo;
//! la sesión de escritorio lo mira para abrir este asistente solo la
//! primera vez.
//!
//! ## Modelo de usuario
//!
//! antOS Linux es **monousuario** en esta fase: el usuario del escritorio
//! es el dueño del recinto (`ANTOS_STATE`, `ANTOS_WORKSPACE`) y del socket
//! `0600` del demonio. `antos doctor --desktop` lo comprueba y lo dice;
//! `module.nix` lo exige con una `assertion`. Un demonio por usuario es
//! un ticket futuro, no algo que este código finja.
//!
//! Ningún comando pasa por `sh -c` (T31.4).

use crate::ctx::Ctx;
use crate::installer::cli::read_hidden_line;
use crate::terminal::{paint, BOLD, CYAN, DIM, GREEN, RED, YELLOW};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Nombre del marcador dentro de `ANTOS_STATE`.
pub const SETUP_MARKER: &str = "setup.toml";
/// Remoto de Flathub.
pub const FLATHUB_URL: &str = "https://dl.flathub.org/repo/flathub.flatpakrepo";

/// Respuestas para el modo no interactivo (`antos setup --config setup.toml`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SetupToml {
    #[serde(default)]
    pub git_name: Option<String>,
    #[serde(default)]
    pub git_email: Option<String>,
    /// Generar la clave SSH si falta (por defecto sí).
    #[serde(default = "default_true")]
    pub ssh_key: bool,
    /// Añadir el remoto Flathub si `flatpak` existe (por defecto sí).
    #[serde(default = "default_true")]
    pub flathub: bool,
    /// `local` / `hybrid` / `cloud`; sin él, el paso se salta.
    #[serde(default)]
    pub llm_profile: Option<String>,
    /// Descargar el modelo por defecto (gigabytes): solo si se pide.
    #[serde(default)]
    pub pull_model: bool,
    /// Lanzar `gh auth login` (interactivo): solo si se pide.
    #[serde(default)]
    pub gh_login: bool,
}

fn default_true() -> bool {
    true
}

/// Lo que quedó de cada paso, en el marcador.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum StepStatus {
    Done,
    Skipped { reason: String },
    Failed { error: String },
}

/// El marcador `setup.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SetupMarker {
    #[serde(default)]
    pub completed_at: String,
    #[serde(default)]
    pub steps: BTreeMap<String, StepStatus>,
}

impl SetupMarker {
    pub fn path(state_dir: &Path) -> PathBuf {
        state_dir.join(SETUP_MARKER)
    }

    pub fn load(state_dir: &Path) -> Option<Self> {
        let raw = fs::read_to_string(Self::path(state_dir)).ok()?;
        toml::from_str(&raw).ok()
    }

    pub fn save(&self, state_dir: &Path) -> Result<()> {
        fs::create_dir_all(state_dir)?;
        let raw = toml::to_string_pretty(self).context("serializando setup.toml")?;
        fs::write(Self::path(state_dir), raw).context("escribiendo setup.toml")
    }
}

/// Opciones de una pasada de `antos setup`.
#[derive(Debug, Clone, Default)]
pub struct SetupOptions {
    /// `--yes`: sin preguntas; lo que no venga en `config` se salta.
    pub assume_yes: bool,
    pub config: SetupToml,
}

/// Resultado de una pasada.
#[derive(Debug, Clone)]
pub struct SetupReport {
    pub steps: Vec<(String, StepStatus)>,
}

impl SetupReport {
    pub fn failed(&self) -> usize {
        self.steps
            .iter()
            .filter(|(_, s)| matches!(s, StepStatus::Failed { .. }))
            .count()
    }
}

fn tool_in_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|d| d.join(name).is_file())
}

fn ask_line<R: BufRead, W: Write>(reader: &mut R, writer: &mut W, prompt: &str) -> Result<String> {
    write!(writer, "{prompt}")?;
    writer.flush()?;
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(line.trim().to_string())
}

fn ask_yes_no<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    prompt: &str,
    default_yes: bool,
) -> Result<bool> {
    let hint = if default_yes { "[S/n]" } else { "[s/N]" };
    let answer = ask_line(reader, writer, &format!("{prompt} {hint}: "))?;
    Ok(match answer.to_ascii_lowercase().as_str() {
        "" => default_yes,
        "s" | "si" | "sí" | "y" | "yes" => true,
        _ => false,
    })
}

fn print_step<W: Write>(writer: &mut W, name: &str, status: &StepStatus) -> Result<()> {
    match status {
        StepStatus::Done => writeln!(writer, "  {} {name}", paint("✓", GREEN))?,
        StepStatus::Skipped { reason } => writeln!(
            writer,
            "  {} {name} {}",
            paint("→", YELLOW),
            paint(&format!("(saltado: {reason})"), DIM)
        )?,
        StepStatus::Failed { error } => writeln!(writer, "  {} {name}: {error}", paint("✗", RED))?,
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────── pasos

/// ¿Hay `user.name` y `user.email` en la configuración global de git?
pub fn git_identity_present(home: &Path) -> bool {
    for path in [home.join(".config/git/config"), home.join(".gitconfig")] {
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let (mut name, mut email, mut in_user) = (false, false, false);
        for line in raw.lines() {
            let l = line.trim();
            if l.starts_with('[') {
                in_user = l == "[user]";
                continue;
            }
            if in_user {
                if l.starts_with("name") && l.contains('=') {
                    name = true;
                }
                if l.starts_with("email") && l.contains('=') {
                    email = true;
                }
            }
        }
        if name && email {
            return true;
        }
    }
    false
}

/// Escribe `[user] name/email` en `~/.config/git/config` (añadiendo la
/// sección si el fichero ya tiene otras).
pub fn write_git_identity(home: &Path, name: &str, email: &str) -> Result<PathBuf> {
    let dir = home.join(".config/git");
    fs::create_dir_all(&dir)?;
    let path = dir.join("config");
    let mut raw = fs::read_to_string(&path).unwrap_or_default();
    if !raw.is_empty() && !raw.ends_with('\n') {
        raw.push('\n');
    }
    raw.push_str(&format!("[user]\n\tname = {name}\n\temail = {email}\n"));
    fs::write(&path, raw)?;
    Ok(path)
}

fn step_git_identity<R: BufRead, W: Write>(
    home: &Path,
    reader: &mut R,
    writer: &mut W,
    opts: &SetupOptions,
) -> Result<StepStatus> {
    if git_identity_present(home) {
        return Ok(StepStatus::Skipped {
            reason: "ya hecho".into(),
        });
    }
    let (name, email) = match (&opts.config.git_name, &opts.config.git_email) {
        (Some(n), Some(e)) => (n.clone(), e.clone()),
        _ if opts.assume_yes => {
            return Ok(StepStatus::Skipped {
                reason: "sin git_name/git_email en el TOML".into(),
            })
        }
        _ => {
            let name = ask_line(reader, writer, "Tu nombre para los commits de git: ")?;
            let email = ask_line(reader, writer, "Tu correo para los commits de git: ")?;
            (name, email)
        }
    };
    if name.is_empty() || email.is_empty() || !email.contains('@') {
        return Ok(StepStatus::Skipped {
            reason: "nombre o correo vacíos".into(),
        });
    }
    let path = write_git_identity(home, &name, &email)?;
    writeln!(
        writer,
        "    escrito {}",
        paint(&path.display().to_string(), DIM)
    )?;
    Ok(StepStatus::Done)
}

fn step_ssh_key<R: BufRead, W: Write>(
    home: &Path,
    reader: &mut R,
    writer: &mut W,
    opts: &SetupOptions,
) -> Result<StepStatus> {
    let key = home.join(".ssh/id_ed25519");
    if key.exists() {
        return Ok(StepStatus::Skipped {
            reason: "ya hecho".into(),
        });
    }
    if !tool_in_path("ssh-keygen") {
        return Ok(StepStatus::Skipped {
            reason: "ssh-keygen no está en PATH".into(),
        });
    }
    let wanted = if opts.assume_yes {
        opts.config.ssh_key
    } else {
        ask_yes_no(reader, writer, "¿Generar una clave SSH ed25519?", true)?
    };
    if !wanted {
        return Ok(StepStatus::Skipped {
            reason: "el usuario no quiso".into(),
        });
    }
    let comment = opts
        .config
        .git_email
        .clone()
        .unwrap_or_else(|| "antos".to_string());
    fs::create_dir_all(home.join(".ssh"))?;
    let status = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-C", &comment, "-f"])
        .arg(&key)
        .status()
        .context("lanzando ssh-keygen")?;
    if !status.success() {
        return Ok(StepStatus::Failed {
            error: format!("ssh-keygen terminó con {status}"),
        });
    }
    if let Ok(public) = fs::read_to_string(home.join(".ssh/id_ed25519.pub")) {
        writeln!(writer, "    clave pública (pégala en tu forja):")?;
        writeln!(writer, "    {}", paint(public.trim(), CYAN))?;
    }
    Ok(StepStatus::Done)
}

/// ¿Está el remoto `flathub` del usuario?
fn flathub_present() -> bool {
    Command::new("flatpak")
        .args(["remotes", "--user", "--columns=name"])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| l.trim() == "flathub")
        })
        .unwrap_or(false)
}

fn step_flathub<W: Write>(writer: &mut W, opts: &SetupOptions) -> Result<StepStatus> {
    if !tool_in_path("flatpak") {
        return Ok(StepStatus::Skipped {
            reason: "flatpak no está en PATH".into(),
        });
    }
    if flathub_present() {
        return Ok(StepStatus::Skipped {
            reason: "ya hecho".into(),
        });
    }
    if opts.assume_yes && !opts.config.flathub {
        return Ok(StepStatus::Skipped {
            reason: "flathub = false en el TOML".into(),
        });
    }
    let status = Command::new("flatpak")
        .args([
            "remote-add",
            "--if-not-exists",
            "--user",
            "flathub",
            FLATHUB_URL,
        ])
        .status()
        .context("lanzando flatpak remote-add")?;
    if !status.success() {
        return Ok(StepStatus::Failed {
            error: format!("flatpak remote-add terminó con {status} (¿sin red?)"),
        });
    }
    writeln!(writer, "    remoto flathub añadido para el usuario")?;
    Ok(StepStatus::Done)
}

fn step_models<R: BufRead, W: Write>(
    ctx: &Ctx,
    reader: &mut R,
    writer: &mut W,
    opts: &SetupOptions,
) -> Result<StepStatus> {
    let profile = match &opts.config.llm_profile {
        Some(p) => Some(p.clone()),
        None if opts.assume_yes => None,
        None => {
            let answer = ask_line(
                reader,
                writer,
                "Perfil de modelos [local: solo Ollama · hybrid: Ollama + nube · cloud: solo nube · saltar] (predeterminado: local): ",
            )?;
            match answer.to_ascii_lowercase().as_str() {
                "" | "local" => Some("local".into()),
                "hybrid" | "híbrido" | "hibrido" => Some("hybrid".into()),
                "cloud" | "nube" => Some("cloud".into()),
                _ => None,
            }
        }
    };
    let Some(profile) = profile else {
        return Ok(StepStatus::Skipped {
            reason: "sin perfil".into(),
        });
    };
    if !matches!(profile.as_str(), "local" | "hybrid" | "cloud") {
        return Ok(StepStatus::Skipped {
            reason: format!("perfil desconocido «{profile}»"),
        });
    }
    let config = crate::llm::LlmConfig::load_from_state(&ctx.state);
    let already = config
        .profile
        .as_ref()
        .map(|p| format!("{p:?}").to_ascii_lowercase() == profile)
        .unwrap_or(false);
    if !already {
        crate::cli::commands::tools::cmd_llm(ctx, &["profile".into(), profile.clone()], true)?;
    }
    let pull = if opts.assume_yes {
        opts.config.pull_model
    } else if profile == "cloud" {
        false
    } else {
        ask_yes_no(
            reader,
            writer,
            "¿Descargar ahora el modelo local por defecto? (son varios GB)",
            false,
        )?
    };
    if pull {
        crate::cli::commands::tools::cmd_llm(ctx, &["setup".into()], true)?;
    } else {
        writeln!(
            writer,
            "    perfil «{profile}»; el modelo se descarga cuando quieras con {}",
            paint("antos llm setup", BOLD)
        )?;
    }
    Ok(StepStatus::Done)
}

fn step_api_keys<R: BufRead, W: Write>(
    ctx: &Ctx,
    reader: &mut R,
    writer: &mut W,
    opts: &SetupOptions,
) -> Result<StepStatus> {
    if opts.assume_yes {
        return Ok(StepStatus::Skipped {
            reason: "las claves no van en un TOML: `antos secrets set <NOMBRE>`".into(),
        });
    }
    let profile = opts.config.llm_profile.as_deref().unwrap_or("");
    let existing: Vec<String> = crate::vault::list_secrets(&ctx.state)
        .map(|v| v.into_iter().map(|s| s.key).collect())
        .unwrap_or_default();
    let mut stored = 0;
    for (provider, key_name) in [
        ("Anthropic (Claude)", "ANTHROPIC_API_KEY"),
        ("OpenAI", "OPENAI_API_KEY"),
    ] {
        if existing.iter().any(|k| k == key_name) {
            writeln!(writer, "    {key_name}: ya en la bóveda")?;
            continue;
        }
        let default_yes = profile != "local" && key_name == "ANTHROPIC_API_KEY";
        if !ask_yes_no(
            reader,
            writer,
            &format!("¿Guardar una clave API de {provider} en la bóveda?"),
            default_yes,
        )? {
            continue;
        }
        write!(writer, "{key_name} (no se muestra): ")?;
        writer.flush()?;
        let value = read_hidden_line(reader, writer)?;
        if value.trim().is_empty() {
            writeln!(writer, "    vacía; se omite")?;
            continue;
        }
        crate::vault::set_secret(&ctx.state, key_name, value.trim())?;
        stored += 1;
        writeln!(writer, "    {key_name}: guardada cifrada en la bóveda")?;
    }
    Ok(if stored > 0 {
        StepStatus::Done
    } else {
        StepStatus::Skipped {
            reason: "ninguna clave nueva".into(),
        }
    })
}

fn gh_logged_in() -> bool {
    Command::new("gh")
        .args(["auth", "status"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn step_gh<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    opts: &SetupOptions,
) -> Result<StepStatus> {
    if !tool_in_path("gh") {
        return Ok(StepStatus::Skipped {
            reason: "gh no está en PATH".into(),
        });
    }
    if gh_logged_in() {
        return Ok(StepStatus::Skipped {
            reason: "ya hecho".into(),
        });
    }
    let wanted = if opts.assume_yes {
        opts.config.gh_login
    } else {
        ask_yes_no(reader, writer, "¿Autenticar `gh` con GitHub ahora?", false)?
    };
    if !wanted {
        return Ok(StepStatus::Skipped {
            reason: "el usuario no quiso".into(),
        });
    }
    let status = Command::new("gh")
        .args(["auth", "login"])
        .status()
        .context("lanzando gh auth login")?;
    Ok(if status.success() {
        StepStatus::Done
    } else {
        StepStatus::Failed {
            error: format!("gh auth login terminó con {status}"),
        }
    })
}

/// Una pasada completa de `antos setup` sobre `home`, con las respuestas
/// leídas de `reader` (o del TOML con `--yes`). Guarda el marcador en
/// `ctx.state` y devuelve el informe.
pub fn run_setup<R: BufRead, W: Write>(
    ctx: &Ctx,
    home: &Path,
    reader: &mut R,
    writer: &mut W,
    opts: &SetupOptions,
) -> Result<SetupReport> {
    writeln!(writer)?;
    writeln!(writer, "{}", paint("antOS · configura tu antOS", BOLD))?;
    writeln!(
        writer,
        "{}",
        paint(
            "  Cada paso se salta solo si ya está hecho; puedes repetir `antos setup` cuando quieras.",
            DIM
        )
    )?;
    writeln!(writer)?;

    let mut steps: Vec<(String, StepStatus)> = Vec::new();
    let mut record = |name: &str, result: Result<StepStatus>, writer: &mut W| -> Result<()> {
        let status = match result {
            Ok(s) => s,
            Err(e) => StepStatus::Failed {
                error: format!("{e:#}"),
            },
        };
        print_step(writer, name, &status)?;
        steps.push((name.to_string(), status));
        Ok(())
    };

    let r = step_git_identity(home, reader, writer, opts);
    record("identidad git", r, writer)?;
    let r = step_ssh_key(home, reader, writer, opts);
    record("clave SSH", r, writer)?;
    let r = step_flathub(writer, opts);
    record("flathub", r, writer)?;
    let r = step_models(ctx, reader, writer, opts);
    record("modelos", r, writer)?;
    let r = step_api_keys(ctx, reader, writer, opts);
    record("claves API", r, writer)?;
    let r = step_gh(reader, writer, opts);
    record("gh auth", r, writer)?;

    let marker = SetupMarker {
        completed_at: chrono::Local::now().to_rfc3339(),
        steps: steps.iter().cloned().collect(),
    };
    marker.save(&ctx.state)?;
    writeln!(
        writer,
        "\n  marcador: {}",
        paint(&SetupMarker::path(&ctx.state).display().to_string(), DIM)
    )?;
    Ok(SetupReport { steps })
}

/// `antos setup [--yes] [--config setup.toml] [--status]`.
pub fn cmd_setup(ctx: &Ctx, args: &[String], assume_yes: bool) -> Result<()> {
    let mut opts = SetupOptions {
        assume_yes,
        config: SetupToml::default(),
    };
    let mut show_status = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--yes" | "-y" => opts.assume_yes = true,
            "--status" | "status" => show_status = true,
            "--config" | "-c" if i + 1 < args.len() => {
                let raw = fs::read_to_string(&args[i + 1])
                    .with_context(|| format!("leyendo {}", args[i + 1]))?;
                opts.config =
                    toml::from_str(&raw).with_context(|| format!("analizando {}", args[i + 1]))?;
                i += 1;
            }
            "--help" | "-h" | "help" => {
                println!(
                    "\n{} primer arranque de antOS Linux\n",
                    paint("antos setup ·", BOLD)
                );
                println!("  antos setup                 asistente interactivo (idempotente)");
                println!("  antos setup --yes           sin preguntas: solo lo que no necesita respuesta");
                println!("  antos setup --config f.toml respuestas desde TOML (git_name, git_email, ssh_key,");
                println!("                              flathub, llm_profile, pull_model, gh_login) + --yes");
                println!("  antos setup --status        qué quedó hecho la última vez\n");
                return Ok(());
            }
            _ => {}
        }
        i += 1;
    }

    if show_status {
        return match SetupMarker::load(&ctx.state) {
            Some(m) => {
                println!(
                    "\n{} última pasada: {}",
                    paint("antos setup ·", BOLD),
                    m.completed_at
                );
                let mut out = std::io::stdout().lock();
                for (name, status) in &m.steps {
                    print_step(&mut out, name, status)?;
                }
                println!();
                Ok(())
            }
            None => {
                println!(
                    "\n{} nunca se ha ejecutado (no hay {})\n",
                    paint("antos setup ·", BOLD),
                    SetupMarker::path(&ctx.state).display()
                );
                Ok(())
            }
        };
    }

    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("HOME no está definido"))?;
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    let report = run_setup(ctx, &home, &mut reader, &mut writer, &opts)?;
    drop(writer);

    // 7. El resumen final: qué está vivo y qué falta.
    println!();
    let doctor = super::cmd_doctor_desktop(ctx);
    match (report.failed(), doctor) {
        (0, Ok(())) => {
            println!("{}", paint("✓ tu antOS está listo", GREEN));
            Ok(())
        }
        (0, Err(e)) => {
            println!("{} {e:#}", paint("doctor --desktop:", YELLOW));
            Ok(())
        }
        (n, _) => bail!("{n} paso(s) del setup fallaron (ver arriba)"),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::io::Cursor;

    fn temp_dir(tag: &str) -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("antos-setup-{tag}-{now}"));
        fs::create_dir_all(&p).unwrap();
        p
    }

    /// Un `Ctx` con estado y workspace temporales (el catálogo real).
    fn ctx_in(root: &Path) -> Ctx {
        let discovered = Ctx::discover().expect("ctx");
        let state = root.join("state");
        let workspace = root.join("workspace");
        fs::create_dir_all(&state).unwrap();
        fs::create_dir_all(&workspace).unwrap();
        Ctx {
            workspace,
            state,
            current_project: None,
            ..discovered
        }
    }

    /// Sin códigos de color, para comparar texto.
    fn plain(bytes: &[u8]) -> String {
        let text = String::from_utf8_lossy(bytes);
        let mut out = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for n in chars.by_ref() {
                    if n == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    /// Huella de todo `home`: rutas + tamaños + mtime, para comprobar que
    /// una segunda pasada no toca nada.
    fn fingerprint(dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        fn walk(dir: &Path, out: &mut Vec<String>) {
            if let Ok(rd) = fs::read_dir(dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        walk(&p, out);
                    } else if let Ok(m) = fs::metadata(&p) {
                        out.push(format!(
                            "{} {} {:?}",
                            p.display(),
                            m.len(),
                            m.modified().ok()
                        ));
                    }
                }
            }
        }
        walk(dir, &mut out);
        out.sort();
        out
    }

    #[test]
    fn test_git_identity_detection_and_write() {
        let home = temp_dir("git");
        assert!(!git_identity_present(&home));
        // Un ~/.gitconfig con solo el nombre no basta.
        fs::write(home.join(".gitconfig"), "[user]\n\tname = Ana\n").unwrap();
        assert!(!git_identity_present(&home));
        let path = write_git_identity(&home, "Ana", "ana@example.org").unwrap();
        assert_eq!(path, home.join(".config/git/config"));
        assert!(git_identity_present(&home));
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("[user]"));
        assert!(raw.contains("email = ana@example.org"));
        let _ = fs::remove_dir_all(&home);
    }

    /// Modo no interactivo con TOML: identidad git escrita, clave SSH
    /// generada, el resto saltado con motivo, marcador guardado. La
    /// segunda pasada salta todo «ya hecho» y no cambia un solo fichero.
    #[test]
    fn test_run_setup_is_idempotent() {
        let root = temp_dir("run");
        let home = root.join("home");
        fs::create_dir_all(&home).unwrap();
        let ctx = ctx_in(&root);
        let opts = SetupOptions {
            assume_yes: true,
            config: SetupToml {
                git_name: Some("Ana".into()),
                git_email: Some("ana@example.org".into()),
                ssh_key: tool_in_path("ssh-keygen"),
                flathub: false,
                llm_profile: None,
                pull_model: false,
                gh_login: false,
            },
        };

        let mut out = Vec::new();
        let report = run_setup(&ctx, &home, &mut Cursor::new(b""), &mut out, &opts).unwrap();
        assert_eq!(report.failed(), 0, "{}", String::from_utf8_lossy(&out));
        let status = |name: &str| {
            report
                .steps
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, s)| s.clone())
                .unwrap()
        };
        assert_eq!(status("identidad git"), StepStatus::Done);
        assert!(git_identity_present(&home));
        if tool_in_path("ssh-keygen") {
            assert_eq!(status("clave SSH"), StepStatus::Done);
            assert!(home.join(".ssh/id_ed25519.pub").exists());
            assert!(String::from_utf8_lossy(&out).contains("ssh-ed25519"));
        }
        assert!(matches!(status("modelos"), StepStatus::Skipped { .. }));
        assert!(matches!(status("claves API"), StepStatus::Skipped { .. }));
        assert!(matches!(status("gh auth"), StepStatus::Skipped { .. }));
        let marker = SetupMarker::load(&ctx.state).expect("marcador");
        assert_eq!(marker.steps.len(), report.steps.len());
        assert!(!marker.completed_at.is_empty());

        // Segunda pasada: todo «ya hecho», ningún fichero de $HOME cambia.
        let before = fingerprint(&home);
        let mut out2 = Vec::new();
        let report2 = run_setup(&ctx, &home, &mut Cursor::new(b""), &mut out2, &opts).unwrap();
        assert_eq!(report2.failed(), 0);
        let text = plain(&out2);
        assert!(text.contains("identidad git (saltado: ya hecho)"), "{text}");
        if tool_in_path("ssh-keygen") {
            assert!(text.contains("clave SSH (saltado: ya hecho)"), "{text}");
        }
        assert_eq!(fingerprint(&home), before, "la segunda pasada tocó $HOME");
        let _ = fs::remove_dir_all(&root);
    }

    /// Interactivo: las respuestas vienen por `reader`; sin TOML ni `--yes`
    /// se pregunta nombre y correo, y un «n» a la clave SSH la salta.
    #[test]
    fn test_run_setup_interactive_answers() {
        let root = temp_dir("interactive");
        let home = root.join("home");
        fs::create_dir_all(&home).unwrap();
        let ctx = ctx_in(&root);
        let opts = SetupOptions::default();
        // nombre, correo, ssh? n, perfil: saltar, claves: n, n, gh (si existe): n
        let input = "Ana\nana@example.org\nn\nsaltar\nn\nn\nn\n";
        let mut out = Vec::new();
        let report = run_setup(
            &ctx,
            &home,
            &mut Cursor::new(input.as_bytes()),
            &mut out,
            &opts,
        )
        .unwrap();
        let text = plain(&out);
        assert!(text.contains("Tu nombre para los commits"), "{text}");
        assert!(git_identity_present(&home));
        assert!(!home.join(".ssh/id_ed25519").exists());
        assert_eq!(report.failed(), 0, "{text}");
        let _ = fs::remove_dir_all(&root);
    }

    /// `doctor --desktop` sobre un recinto propio: la comprobación del dueño
    /// pasa y las demás describen el estado real sin mentir (fuera de la
    /// sesión gráfica, Wayland y el demonio fallan; Ollama y flathub avisan).
    #[test]
    fn test_desktop_checks_report_owner_and_session() {
        let root = temp_dir("doctor");
        let ctx = ctx_in(&root);
        let checks = super::super::desktop_checks(&ctx);
        let by_name = |n: &str| checks.iter().find(|c| c.name == n).unwrap().clone();
        assert_eq!(by_name("recinto").ok, Some(true));
        assert!(by_name("recinto").critical);
        let daemon = by_name("demonio");
        assert_eq!(daemon.ok, Some(false), "sin demonio en el estado temporal");
        assert!(daemon.detail.contains("antos.sock") || daemon.detail.contains("demonio"));
        assert!(!by_name("ollama").critical);
        assert!(!by_name("flathub").critical);
        assert!(!by_name("identidad git").critical);
        // 7 fijas; las de labwc/* solo aparecen si hay configuración editada.
        assert!(checks.len() >= 7);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_setup_marker_round_trip() {
        let root = temp_dir("marker");
        let mut m = SetupMarker {
            completed_at: "2026-09-21T10:00:00+02:00".into(),
            ..SetupMarker::default()
        };
        m.steps.insert("identidad git".into(), StepStatus::Done);
        m.steps.insert(
            "flathub".into(),
            StepStatus::Skipped {
                reason: "flatpak no está en PATH".into(),
            },
        );
        m.save(&root).unwrap();
        let back = SetupMarker::load(&root).unwrap();
        assert_eq!(back.steps, m.steps);
        assert_eq!(back.completed_at, m.completed_at);
        let _ = fs::remove_dir_all(&root);
    }
}
