//! `antos system update` / `rollback` / `generations` (T36.6).
//!
//! Actualizar antOS Linux es una intención más: se ve el diff, se
//! aprueba, se aplica, y se puede deshacer. Todo sobre `/etc/nixos` (el
//! `flake.nix` que dejó `antos install`, T36.1) y con las herramientas que
//! ya trae `nix`: `nix flake update`, `nix build`, `nix store
//! diff-closures` (sin `nvd`), `nixos-rebuild switch` / `--rollback` /
//! `list-generations`. Ningún comando pasa por `sh -c` (T31.4); `sudo` lo
//! invoca este CLI en primer plano, nunca el demonio.
//!
//! ## Cómo
//!
//! 1. `/etc/nixos` se **copia** a `$ANTOS_STATE/system-update/` y todo
//!    (refresco del `flake.lock`, cambio de `antos.url`) ocurre sobre la
//!    copia; el original no se toca hasta aprobar.
//! 2. `--check`: evalúa el `outPath` del sistema nuevo y lo compara con
//!    `/run/current-system`. Sin construir ni descargar nada. Sale con `0`
//!    si no hay cambios y con [`EXIT_UPDATE_AVAILABLE`] si los hay, y deja
//!    `$ANTOS_STATE/update-available.json` para la barra.
//! 3. Construcción (`nix build`, descargando del caché de T36.3 si lo hay)
//!    y diff de closures frente al sistema actual, presentado como el diff
//!    de una intención: qué sube de versión, qué entra, qué sale.
//! 4. Confirmación y `sudo nixos-rebuild switch`. Si va bien, el
//!    `flake.lock` (y el `flake.nix` si cambió la fuente) vuelven a
//!    `/etc/nixos` con `sudo cp` + `sudo mv` (rename en el mismo sistema de
//!    ficheros: atómico por fichero). Bitácora: `system.update` con la
//!    generación anterior y la nueva.
//! 5. `rollback`: `sudo nixos-rebuild switch --rollback` (o
//!    `--switch-generation N`), bitácora `system.rollback`, y marca la
//!    entrada `system.update` como revertida — es lo que `antos undo` hace
//!    cuando lo último ejecutado fue una actualización.
//!
//! ## Estado de implementación
//!
//! El motor ([`SystemUpdater`]) corre sobre un [`InstallRunner`], igual que
//! el instalador: `SystemRunner` en la máquina real y un runner de tests
//! que fija la secuencia exacta de comandos. Lo que se comprueba aquí: la
//! secuencia, el parseo de `diff-closures` y `list-generations --json`, la
//! reescritura de `antos.url`, los códigos de salida y la bitácora. Lo que
//! solo se comprueba en una máquina antOS Linux: que `nixos-rebuild`
//! haga lo suyo. La barra no muestra todavía el indicador de
//! «actualización disponible»: lee `update-available.json` cuando un
//! ticket de barra lo implemente; aquí se deja el fichero y el
//! temporizador que lo mantiene al día.

use crate::ctx::Ctx;
use crate::installer::runner::{InstallRunner, SystemRunner};
use crate::journal::{Outcome, Record};
use crate::terminal::{paint, BOLD, CYAN, DIM, GREEN, RED, YELLOW};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

/// Código de salida de `--check` cuando hay una actualización.
pub const EXIT_UPDATE_AVAILABLE: i32 = 10;
/// Fichero que deja `--check` para la barra.
pub const UPDATE_AVAILABLE_FILE: &str = "update-available.json";
/// Subdirectorio del estado con la copia de trabajo de `/etc/nixos`.
pub const WORK_DIR: &str = "system-update";
/// Intento con el que se anota una actualización en la bitácora.
pub const INTENT_UPDATE: &str = "system.update";
/// Intento con el que se anota un rollback.
pub const INTENT_ROLLBACK: &str = "system.rollback";

/// Un cambio de la closure según `nix store diff-closures`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosureChange {
    pub name: String,
    /// `None` = no estaba (`∅`).
    pub before: Option<String>,
    /// `None` = desaparece (`∅`).
    pub after: Option<String>,
    /// Texto del tamaño tal cual lo da nix (`+2.3 MiB`), si lo hay.
    pub size: Option<String>,
}

impl ClosureChange {
    pub fn kind(&self) -> &'static str {
        match (&self.before, &self.after) {
            (None, Some(_)) => "nuevo",
            (Some(_), None) => "eliminado",
            _ => "cambia",
        }
    }
}

/// Parsea la salida de `nix store diff-closures A B`: líneas como
/// `antosd: 0.1.0 → 0.1.1, +2.3 MiB`, `foo: ∅ → 1.2` o `bar: 1.0 → ∅`.
pub fn parse_diff_closures(output: &str) -> Vec<ClosureChange> {
    let mut out = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        let Some((name, rest)) = line.split_once(": ") else {
            continue;
        };
        let (versions, size) = match rest.split_once(", ") {
            Some((v, s)) => (v, Some(s.trim().to_string())),
            None => (rest, None),
        };
        let Some((before, after)) = versions.split_once('→') else {
            continue;
        };
        let norm = |s: &str| {
            let s = s.trim();
            if s == "∅" || s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        };
        out.push(ClosureChange {
            name: name.trim().to_string(),
            before: norm(before),
            after: norm(after),
            size,
        });
    }
    out
}

/// Una generación de `nixos-rebuild list-generations --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Generation {
    pub generation: u32,
    #[serde(default)]
    pub date: String,
    #[serde(default, rename = "nixosVersion")]
    pub nixos_version: String,
    #[serde(default, rename = "kernelVersion")]
    pub kernel_version: String,
    #[serde(default)]
    pub current: bool,
}

pub fn parse_generations(json: &str) -> Result<Vec<Generation>> {
    serde_json::from_str(json).context("nixos-rebuild list-generations --json ilegible")
}

/// Lo que `--check` deja para la barra.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateAvailable {
    pub available: bool,
    pub checked_at: String,
    pub current: String,
    pub candidate: String,
}

/// Resultado de una pasada de `update`.
#[derive(Debug, Clone)]
pub struct UpdateReport {
    pub changes: Vec<ClosureChange>,
    pub local_changes: bool,
    pub applied: bool,
    pub from_generation: Option<u32>,
    pub to_generation: Option<u32>,
}

/// El motor: `/etc/nixos`, el estado, el sistema actual y el runner.
pub struct SystemUpdater<'a> {
    pub etc_nixos: PathBuf,
    pub state: PathBuf,
    /// `/run/current-system` (o lo que haga sus veces en tests).
    pub current_system: PathBuf,
    pub runner: &'a mut dyn InstallRunner,
}

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

impl<'a> SystemUpdater<'a> {
    pub fn new(
        etc_nixos: &Path,
        state: &Path,
        current_system: &Path,
        runner: &'a mut dyn InstallRunner,
    ) -> Self {
        Self {
            etc_nixos: etc_nixos.to_path_buf(),
            state: state.to_path_buf(),
            current_system: current_system.to_path_buf(),
            runner,
        }
    }

    pub fn work_dir(&self) -> PathBuf {
        self.state.join(WORK_DIR)
    }

    /// El hostname de `nixosConfigurations."<host>"` del `flake.nix`.
    pub fn hostname_from_flake(flake_nix: &str) -> Option<String> {
        let idx = flake_nix.find("nixosConfigurations.\"")?;
        let rest = &flake_nix[idx + "nixosConfigurations.\"".len()..];
        let end = rest.find('"')?;
        let host = &rest[..end];
        (!host.is_empty()).then(|| host.to_string())
    }

    /// La línea `antos.url = "…";` del `flake.nix`.
    pub fn source_of(flake_nix: &str) -> Option<String> {
        flake_nix.lines().find_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix("antos.url = \"")?;
            rest.split_once('"').map(|(url, _)| url.to_string())
        })
    }

    /// Reescribe `antos.url` en el texto del `flake.nix`.
    pub fn with_source(flake_nix: &str, url: &str) -> String {
        flake_nix
            .lines()
            .map(|l| {
                if l.trim().starts_with("antos.url = \"") {
                    let indent: String = l.chars().take_while(|c| c.is_whitespace()).collect();
                    format!("{indent}antos.url = \"{url}\";")
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    /// `/etc/nixos` → `$ANTOS_STATE/system-update/` (limpiando lo anterior).
    pub fn prepare_copy(&self) -> Result<PathBuf> {
        let flake = self.etc_nixos.join("flake.nix");
        if !flake.is_file() {
            bail!(
                "{} no tiene flake.nix: esto no es una máquina instalada por `antos install`",
                self.etc_nixos.display()
            );
        }
        let work = self.work_dir();
        if work.exists() {
            fs::remove_dir_all(&work).context("limpiando la copia anterior")?;
        }
        crate::installer::DeployEngine::copy_tree(&self.etc_nixos, &work)
            .context("copiando /etc/nixos")?;
        Ok(work)
    }

    /// ¿Se alcanza el host de una fuente `github:…`? Para `path:` no hace
    /// falta red. Sondeo TCP al 443 con tiempo de espera corto.
    pub fn source_reachable(url: &str) -> bool {
        let host = if url.starts_with("github:") {
            "github.com"
        } else if let Some(rest) = url.strip_prefix("https://") {
            rest.split('/').next().unwrap_or("")
        } else {
            return true; // path:, git+file:, …
        };
        if host.is_empty() {
            return false;
        }
        use std::net::ToSocketAddrs;
        let Ok(mut addrs) = (host, 443u16).to_socket_addrs() else {
            return false;
        };
        addrs.any(|a| {
            std::net::TcpStream::connect_timeout(&a, std::time::Duration::from_secs(3)).is_ok()
        })
    }

    /// `outPath` del sistema nuevo sin construir nada.
    pub fn candidate_out_path(&mut self, work: &Path, host: &str) -> Result<String> {
        let attr = format!(
            "path:{}#nixosConfigurations.{host}.config.system.build.toplevel.outPath",
            work.display()
        );
        let out = self
            .runner
            .run("nix", &args(&["eval", "--raw", &attr]), None)?;
        Ok(out.trim().to_string())
    }

    /// Ruta real de `/run/current-system`.
    pub fn current_out_path(&self) -> String {
        fs::read_link(&self.current_system)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| self.current_system.display().to_string())
    }

    /// `--check`: ¿hay algo nuevo? Deja `update-available.json`.
    pub fn check(&mut self, host: &str) -> Result<UpdateAvailable> {
        let work = self.prepare_copy()?;
        let flake = fs::read_to_string(work.join("flake.nix"))?;
        let source = Self::source_of(&flake).unwrap_or_default();
        if !Self::source_reachable(&source) {
            bail!("sin acceso a la fuente de actualizaciones ({source}); nada que comprobar");
        }
        self.runner.run(
            "nix",
            &args(&["flake", "update", "--flake", &work.display().to_string()]),
            None,
        )?;
        let candidate = self.candidate_out_path(&work, host)?;
        let current = self.current_out_path();
        let result = UpdateAvailable {
            available: candidate != current,
            checked_at: chrono::Local::now().to_rfc3339(),
            current,
            candidate,
        };
        fs::create_dir_all(&self.state)?;
        fs::write(
            self.state.join(UPDATE_AVAILABLE_FILE),
            serde_json::to_string_pretty(&result)?,
        )?;
        Ok(result)
    }

    /// Generaciones del perfil del sistema.
    pub fn generations(&mut self) -> Result<Vec<Generation>> {
        let out = self.runner.run(
            "nixos-rebuild",
            &args(&["list-generations", "--json"]),
            None,
        )?;
        parse_generations(&out)
    }

    /// ¿`configuration.nix` cambió después de la generación actual?
    pub fn has_local_changes(&self, generations: &[Generation]) -> bool {
        let Some(current) = generations.iter().find(|g| g.current) else {
            return false;
        };
        let Ok(gen_date) =
            chrono::NaiveDateTime::parse_from_str(&current.date, "%Y-%m-%d %H:%M:%S")
        else {
            return false;
        };
        let Ok(meta) = fs::metadata(self.etc_nixos.join("configuration.nix")) else {
            return false;
        };
        let Ok(modified) = meta.modified() else {
            return false;
        };
        let modified: chrono::DateTime<chrono::Local> = modified.into();
        modified.naive_local() > gen_date
    }

    /// Construye el sistema nuevo y devuelve su `outPath`.
    pub fn build(
        &mut self,
        work: &Path,
        host: &str,
        on_line: &mut dyn FnMut(&str),
    ) -> Result<String> {
        let attr = format!(
            "path:{}#nixosConfigurations.{host}.config.system.build.toplevel",
            work.display()
        );
        let link = work.join("result");
        let mut captured = String::new();
        self.runner.run_streaming(
            "nix",
            &args(&[
                "build",
                &attr,
                "-o",
                &link.display().to_string(),
                "--print-out-paths",
                "--print-build-logs",
            ]),
            &mut |l| {
                if l.starts_with("/nix/store/") {
                    captured = l.trim().to_string();
                }
                on_line(l);
            },
        )?;
        if captured.is_empty() {
            captured = fs::read_link(&link)
                .map(|p| p.display().to_string())
                .unwrap_or_default();
        }
        if captured.is_empty() {
            bail!("nix build no devolvió la ruta del sistema nuevo");
        }
        Ok(captured)
    }

    /// Diff de closures entre el sistema actual y `new_system`.
    pub fn diff(&mut self, new_system: &str) -> Result<Vec<ClosureChange>> {
        let current = self.current_system.display().to_string();
        let out = self.runner.run(
            "nix",
            &args(&["store", "diff-closures", &current, new_system]),
            None,
        )?;
        Ok(parse_diff_closures(&out))
    }

    /// `sudo nixos-rebuild switch --flake <copia>#<host>` y, si va bien,
    /// `flake.lock`/`flake.nix` de vuelta a `/etc/nixos` (por fichero:
    /// `sudo cp` a `.new` + `sudo mv`, rename atómico en el mismo sistema
    /// de ficheros).
    pub fn switch(&mut self, work: &Path, host: &str, on_line: &mut dyn FnMut(&str)) -> Result<()> {
        let flake_ref = format!("{}#{host}", work.display());
        self.runner.run_streaming(
            "sudo",
            &args(&["nixos-rebuild", "switch", "--flake", &flake_ref]),
            on_line,
        )?;
        for name in ["flake.lock", "flake.nix"] {
            let src = work.join(name);
            if !src.is_file() {
                continue;
            }
            let dst = self.etc_nixos.join(name);
            let unchanged = fs::read(&src).ok() == fs::read(&dst).ok();
            if unchanged {
                continue;
            }
            let tmp = self.etc_nixos.join(format!(".{name}.new"));
            self.runner.run(
                "sudo",
                &args(&["cp", &src.display().to_string(), &tmp.display().to_string()]),
                None,
            )?;
            self.runner.run(
                "sudo",
                &args(&["mv", &tmp.display().to_string(), &dst.display().to_string()]),
                None,
            )?;
        }
        Ok(())
    }

    /// `sudo nixos-rebuild switch --rollback` o `--switch-generation N`.
    pub fn rollback(
        &mut self,
        generation: Option<u32>,
        on_line: &mut dyn FnMut(&str),
    ) -> Result<()> {
        let mut a = args(&["nixos-rebuild", "switch"]);
        match generation {
            Some(n) => {
                a.push("--switch-generation".into());
                a.push(n.to_string());
            }
            None => a.push("--rollback".into()),
        }
        self.runner.run_streaming("sudo", &a, on_line)
    }
}

/// Entrada de bitácora de una actualización o un rollback.
pub fn journal_record(intent: &str, detail: String, outcome: Outcome) -> Record {
    Record {
        id: crate::plan::new_id(),
        at: chrono::Local::now().to_rfc3339(),
        intent: intent.to_string(),
        ticket_id: None,
        planner: "antos".into(),
        plan: antos_protocol::Plan {
            id: crate::plan::new_id(),
            intent: intent.to_string(),
            planner: "antos".into(),
            steps: Vec::new(),
        },
        tier: antos_protocol::Tier::Confirm,
        reasons: vec!["nixos-rebuild switch: cambio de generación del sistema".into()],
        outcome,
        detail: Some(detail),
        sandbox: "host".into(),
        snapshot: None,
        reverted: false,
    }
}

fn print_changes<W: Write>(writer: &mut W, changes: &[ClosureChange]) -> Result<()> {
    if changes.is_empty() {
        writeln!(writer, "  {}", paint("sin cambios en la closure", DIM))?;
        return Ok(());
    }
    for c in changes {
        let (symbol, color) = match c.kind() {
            "nuevo" => ("+", GREEN),
            "eliminado" => ("-", RED),
            _ => ("~", YELLOW),
        };
        writeln!(
            writer,
            "  {} {:<28} {} → {}{}",
            paint(symbol, color),
            c.name,
            c.before.as_deref().unwrap_or("∅"),
            c.after.as_deref().unwrap_or("∅"),
            c.size
                .as_deref()
                .map(|s| format!("  {}", paint(s, DIM)))
                .unwrap_or_default()
        )?;
    }
    Ok(())
}

/// Una pasada completa de `antos system update` (sin `--check`).
pub fn run_update<R: BufRead, W: Write>(
    updater: &mut SystemUpdater<'_>,
    journal: &Path,
    host: &str,
    source: Option<&str>,
    assume_yes: bool,
    reader: &mut R,
    writer: &mut W,
) -> Result<UpdateReport> {
    let work = updater.prepare_copy()?;
    let flake_path = work.join("flake.nix");
    let mut flake = fs::read_to_string(&flake_path)?;

    // 1. Fuente.
    if let Some(url) = source {
        let old = SystemUpdater::source_of(&flake).unwrap_or_default();
        if old != url {
            writeln!(
                writer,
                "  fuente de antOS: {} → {}",
                paint(&old, DIM),
                paint(url, CYAN)
            )?;
            flake = SystemUpdater::with_source(&flake, url);
            fs::write(&flake_path, &flake)?;
        }
    }
    let current_source = SystemUpdater::source_of(&flake).unwrap_or_default();
    if !SystemUpdater::source_reachable(&current_source) {
        bail!(
            "sin acceso a la fuente de actualizaciones ({current_source}); /etc/nixos no se ha tocado"
        );
    }

    // 2. flake.lock nuevo, sobre la copia.
    writeln!(
        writer,
        "  {} nix flake update ({})",
        paint("→", DIM),
        current_source
    )?;
    updater.runner.run(
        "nix",
        &args(&["flake", "update", "--flake", &work.display().to_string()]),
        None,
    )?;

    // 3. Construir y comparar.
    let generations = updater.generations().unwrap_or_default();
    let from_generation = generations.iter().find(|g| g.current).map(|g| g.generation);
    let local_changes = updater.has_local_changes(&generations);
    writeln!(
        writer,
        "  {} nix build (del caché si lo hay; si no, compila antosd y antos-barra)",
        paint("→", DIM)
    )?;
    let new_system = updater.build(&work, host, &mut |l| {
        let _ = writeln!(writer, "      {l}");
    })?;
    let changes = updater.diff(&new_system)?;
    writeln!(writer)?;
    writeln!(
        writer,
        "{}",
        paint("cambios respecto al sistema actual", BOLD)
    )?;
    print_changes(writer, &changes)?;
    if local_changes {
        writeln!(
            writer,
            "  {} además de la actualización, se aplicarán tus cambios locales de /etc/nixos/configuration.nix",
            paint("!", YELLOW)
        )?;
    }
    if changes.is_empty() && !local_changes {
        writeln!(writer, "\n{}", paint("✓ el sistema ya está al día", GREEN))?;
        return Ok(UpdateReport {
            changes,
            local_changes,
            applied: false,
            from_generation,
            to_generation: from_generation,
        });
    }

    // 4. Confirmación y switch.
    if !assume_yes {
        write!(
            writer,
            "\n¿Aplicar con `sudo nixos-rebuild switch`? [s/N]: "
        )?;
        writer.flush()?;
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let yes = matches!(
            line.trim().to_ascii_lowercase().as_str(),
            "s" | "si" | "sí" | "y" | "yes"
        );
        if !yes {
            writeln!(
                writer,
                "{}",
                paint("cancelado; /etc/nixos no se ha tocado", DIM)
            )?;
            return Ok(UpdateReport {
                changes,
                local_changes,
                applied: false,
                from_generation,
                to_generation: from_generation,
            });
        }
    }
    updater.switch(&work, host, &mut |l| {
        let _ = writeln!(writer, "      {l}");
    })?;
    let to_generation = updater
        .generations()
        .ok()
        .and_then(|gs| gs.iter().find(|g| g.current).map(|g| g.generation));

    // 5. Bitácora.
    let summary = changes
        .iter()
        .map(|c| {
            format!(
                "{}: {} → {}",
                c.name,
                c.before.as_deref().unwrap_or("∅"),
                c.after.as_deref().unwrap_or("∅")
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let detail = format!(
        "generación {} → {}; fuente {}; {}{}",
        from_generation
            .map(|g| g.to_string())
            .unwrap_or_else(|| "?".into()),
        to_generation
            .map(|g| g.to_string())
            .unwrap_or_else(|| "?".into()),
        current_source,
        summary,
        if local_changes {
            "; incluye cambios locales"
        } else {
            ""
        }
    );
    crate::journal::append(
        journal,
        &journal_record(INTENT_UPDATE, detail, Outcome::Executed),
    )?;
    writeln!(
        writer,
        "\n{} generación {} → {}; `antos system rollback` (o `antos undo`) la deshace",
        paint("✓ actualizado", GREEN),
        from_generation
            .map(|g| g.to_string())
            .unwrap_or_else(|| "?".into()),
        to_generation
            .map(|g| g.to_string())
            .unwrap_or_else(|| "?".into()),
    )?;
    Ok(UpdateReport {
        changes,
        local_changes,
        applied: true,
        from_generation,
        to_generation,
    })
}

/// Rollback con bitácora: marca la última `system.update` como revertida.
pub fn run_rollback<W: Write>(
    updater: &mut SystemUpdater<'_>,
    journal: &Path,
    generation: Option<u32>,
    writer: &mut W,
) -> Result<()> {
    let before = updater
        .generations()
        .ok()
        .and_then(|gs| gs.iter().find(|g| g.current).map(|g| g.generation));
    updater.rollback(generation, &mut |l| {
        let _ = writeln!(writer, "      {l}");
    })?;
    let after = updater
        .generations()
        .ok()
        .and_then(|gs| gs.iter().find(|g| g.current).map(|g| g.generation));

    let mut records = crate::journal::read_all(journal).unwrap_or_default();
    if let Some(r) = records
        .iter_mut()
        .rev()
        .find(|r| r.intent == INTENT_UPDATE && r.outcome == Outcome::Executed && !r.reverted)
    {
        r.reverted = true;
        crate::journal::rewrite(journal, &records)?;
    }
    let detail = format!(
        "generación {} → {}{}",
        before.map(|g| g.to_string()).unwrap_or_else(|| "?".into()),
        after.map(|g| g.to_string()).unwrap_or_else(|| "?".into()),
        generation
            .map(|g| format!(" (--switch-generation {g})"))
            .unwrap_or_else(|| " (--rollback)".into())
    );
    crate::journal::append(
        journal,
        &journal_record(INTENT_ROLLBACK, detail.clone(), Outcome::Reverted),
    )?;
    writeln!(writer, "{} {detail}", paint("✓ revertido", GREEN))?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────── CLI

fn etc_nixos_dir() -> PathBuf {
    std::env::var_os("ANTOS_SYSTEM_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/nixos"))
}

fn tool_in_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|d| d.join(name).is_file())
}

fn require_antos_linux(etc_nixos: &Path) -> Result<()> {
    if !cfg!(target_os = "linux") {
        bail!("`antos system` solo tiene sentido en antOS Linux (NixOS)");
    }
    if !etc_nixos.join("flake.nix").is_file() {
        bail!(
            "{} no tiene flake.nix: esto no es una máquina instalada por `antos install`",
            etc_nixos.display()
        );
    }
    for t in ["nix", "nixos-rebuild", "sudo"] {
        if !tool_in_path(t) {
            bail!("falta `{t}` en PATH");
        }
    }
    Ok(())
}

/// `antos system update|rollback|generations|status`.
pub fn cmd_system(ctx: &Ctx, args: &[String], assume_yes: bool) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("help");
    let etc_nixos = etc_nixos_dir();
    match sub {
        "update" | "upgrade" | "actualizar" => {
            require_antos_linux(&etc_nixos)?;
            let mut check = false;
            let mut yes = assume_yes;
            let mut source: Option<String> = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--check" => check = true,
                    "--yes" | "-y" => yes = true,
                    "--source" if i + 1 < args.len() => {
                        source = Some(args[i + 1].clone());
                        i += 1;
                    }
                    _ => {}
                }
                i += 1;
            }
            let flake = fs::read_to_string(etc_nixos.join("flake.nix"))?;
            let host = SystemUpdater::hostname_from_flake(&flake)
                .ok_or_else(|| anyhow::anyhow!("flake.nix sin nixosConfigurations.\"<host>\""))?;
            let mut runner = SystemRunner;
            let mut updater = SystemUpdater::new(
                &etc_nixos,
                &ctx.state,
                Path::new("/run/current-system"),
                &mut runner,
            );
            println!(
                "\n{} {}",
                paint("antOS · system update", BOLD),
                paint(&host, DIM)
            );
            if check {
                let r = updater.check(&host)?;
                if r.available {
                    println!(
                        "  {} actualización disponible ({} → {})",
                        paint("!", YELLOW),
                        paint(&r.current, DIM),
                        paint(&r.candidate, CYAN)
                    );
                    std::process::exit(EXIT_UPDATE_AVAILABLE);
                }
                println!("  {} sin cambios", paint("✓", GREEN));
                return Ok(());
            }
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            let mut reader = stdin.lock();
            let mut writer = stdout.lock();
            run_update(
                &mut updater,
                &ctx.journal_path(),
                &host,
                source.as_deref(),
                yes,
                &mut reader,
                &mut writer,
            )?;
            Ok(())
        }
        "rollback" | "deshacer" => {
            require_antos_linux(&etc_nixos)?;
            let generation = args.get(1).and_then(|g| g.parse::<u32>().ok());
            let mut runner = SystemRunner;
            let mut updater = SystemUpdater::new(
                &etc_nixos,
                &ctx.state,
                Path::new("/run/current-system"),
                &mut runner,
            );
            let stdout = std::io::stdout();
            let mut writer = stdout.lock();
            println!("\n{}", paint("antOS · system rollback", BOLD));
            run_rollback(&mut updater, &ctx.journal_path(), generation, &mut writer)
        }
        "generations" | "generaciones" => {
            require_antos_linux(&etc_nixos)?;
            let mut runner = SystemRunner;
            let mut updater = SystemUpdater::new(
                &etc_nixos,
                &ctx.state,
                Path::new("/run/current-system"),
                &mut runner,
            );
            let gens = updater.generations()?;
            println!("\n{}", paint("antOS · generaciones del sistema", BOLD));
            for g in &gens {
                println!(
                    "  {} {:>4}  {}  NixOS {}  kernel {}",
                    if g.current {
                        paint("●", GREEN)
                    } else {
                        paint("○", DIM)
                    },
                    g.generation,
                    g.date,
                    g.nixos_version,
                    g.kernel_version
                );
            }
            println!("\n  `antos system rollback [N]` vuelve a la anterior (o a la N).\n");
            Ok(())
        }
        "status" => {
            let path = ctx.state.join(UPDATE_AVAILABLE_FILE);
            match fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str::<UpdateAvailable>(&s).ok())
            {
                Some(u) if u.available => println!(
                    "\n{} actualización disponible (comprobado {})\n",
                    paint("!", YELLOW),
                    u.checked_at
                ),
                Some(u) => println!(
                    "\n{} al día (comprobado {})\n",
                    paint("✓", GREEN),
                    u.checked_at
                ),
                None => println!("\n  sin comprobación todavía (`antos system update --check`)\n"),
            }
            Ok(())
        }
        _ => {
            println!(
                "\n{} actualizar y deshacer antOS Linux\n",
                paint("antos system ·", BOLD)
            );
            println!("  antos system update [--check] [--source <url>] [--yes]");
            println!("      refresca el flake.lock sobre una copia, construye, muestra el diff de closures,");
            println!("      pide confirmación y aplica con sudo nixos-rebuild switch. --check: solo evalúa");
            println!("      (salida 0 sin cambios, {EXIT_UPDATE_AVAILABLE} con cambios) y deja update-available.json.");
            println!("  antos system rollback [N]   vuelve a la generación anterior (o a la N)");
            println!("  antos system generations    lista las generaciones del sistema");
            println!("  antos system status         lo que dejó el último --check\n");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
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
}
