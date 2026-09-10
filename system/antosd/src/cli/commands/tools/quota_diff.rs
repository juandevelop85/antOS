//! `antos quota` (cuotas de recursos del sandbox) y `antos diff`.
#![allow(unused_imports, dead_code)]

extern crate antos_protocol;

use crate::capability::{Catalog, Tier};
use crate::cli::args::Opts;
use crate::ctx::Ctx;
use crate::grants::Grants;
use crate::journal::{Outcome, Record};
use crate::planner::{
    claude::ClaudePlanner, local::LocalPlanner, ollama::OllamaPlanner,
    openai_compat::OpenAiCompatPlanner, Planner,
};
use crate::terminal::{ellipsis, paint, tier_color, BLUE, BOLD, CYAN, DIM, GREEN, RED, YELLOW};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

// ------------------------------------------------------------------ quota

/// Sobrescrituras analizadas de `antos quota set [--timeout N] [--memory N]
/// [--cpu N] [--pids N]`. Un campo en `None` significa «no se especificó»,
/// así que `apply_to` solo toca los campos de la cuota que el usuario pidió
/// cambiar; el resto conserva el valor previamente cargado (T31.13).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct QuotaSetArgs {
    pub timeout_secs: Option<u64>,
    pub max_memory_mb: Option<u64>,
    pub cpu_quota_percent: Option<u32>,
    pub max_pids: Option<u32>,
}

impl QuotaSetArgs {
    /// Aplica las sobrescrituras presentes sobre una cuota existente.
    fn apply_to(&self, q: &mut crate::sandbox::quota::ResourceQuota) {
        if let Some(v) = self.timeout_secs {
            q.timeout_secs = v;
        }
        if let Some(v) = self.max_memory_mb {
            q.max_memory_mb = v;
        }
        if let Some(v) = self.cpu_quota_percent {
            q.cpu_quota_percent = v;
        }
        if let Some(v) = self.max_pids {
            q.max_pids = v;
        }
    }
}

/// Analiza los argumentos de `antos quota set`. Un valor no numérico
/// (`--cpu abc`) se ignora en silencio, igual que antes de esta extracción
/// (T31.13).
fn parse_quota_set_args(args: &[String]) -> QuotaSetArgs {
    let mut parsed = QuotaSetArgs::default();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--timeout" | "-t" => {
                if let Some(val) = args.get(i + 1).and_then(|v| v.parse::<u64>().ok()) {
                    parsed.timeout_secs = Some(val);
                    i += 1;
                }
            }
            "--memory" | "-m" => {
                if let Some(val) = args.get(i + 1).and_then(|v| v.parse::<u64>().ok()) {
                    parsed.max_memory_mb = Some(val);
                    i += 1;
                }
            }
            "--cpu" | "-c" => {
                if let Some(val) = args.get(i + 1).and_then(|v| v.parse::<u32>().ok()) {
                    parsed.cpu_quota_percent = Some(val);
                    i += 1;
                }
            }
            "--pids" | "-p" => {
                if let Some(val) = args.get(i + 1).and_then(|v| v.parse::<u32>().ok()) {
                    parsed.max_pids = Some(val);
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    parsed
}

pub fn cmd_quota(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "set" => {
            let mut q = crate::sandbox::quota::load_quota(&ctx.workspace)?;
            parse_quota_set_args(args).apply_to(&mut q);

            crate::sandbox::quota::save_quota(&ctx.workspace, &q)?;
            println!(
                "\n{} Cuotas de recursos de sandbox actualizadas.",
                paint("✓", GREEN)
            );
            println!(
                "  • Timeout:   {}s",
                paint(&q.timeout_secs.to_string(), BOLD)
            );
            println!(
                "  • Memoria:   {} MB",
                paint(&q.max_memory_mb.to_string(), BOLD)
            );
            println!(
                "  • CPU:       {}%",
                paint(&q.cpu_quota_percent.to_string(), BOLD)
            );
            println!(
                "  • Max PIDs:  {} procesos\n",
                paint(&q.max_pids.to_string(), BOLD)
            );
        }
        "reset" => {
            let def = crate::sandbox::quota::ResourceQuota::default();
            crate::sandbox::quota::save_quota(&ctx.workspace, &def)?;
            println!(
                "\n{} Cuotas de sandbox restablecidas a los valores por defecto del sistema.\n",
                paint("✓", GREEN)
            );
        }
        _ => {
            println!(
                "\n{}",
                paint(
                    "antOS · Cuotas y Límites de Recursos para Sandboxes (T7.2)",
                    BOLD
                )
            );
            let q = crate::sandbox::quota::load_quota(&ctx.workspace)?;
            let cgroup_avail = crate::sandbox::quota::CgroupV2Manager::is_available();

            let backend = if cgroup_avail {
                paint("● cgroups v2 (Linux)", GREEN)
            } else {
                paint("● Seatbelt + Supervisor de Procesos (macOS)", BLUE)
            };

            println!("  Mecanismo:          {backend}");
            println!(
                "  Timeout de agente:  {}",
                paint(&format!("{}s", q.timeout_secs), GREEN)
            );
            println!(
                "  Límite de memoria:  {}",
                paint(&format!("{} MB", q.max_memory_mb), GREEN)
            );
            println!(
                "  Cuota de CPU:       {}",
                paint(&format!("{}%", q.cpu_quota_percent), GREEN)
            );
            println!(
                "  Límite de procesos: {}\n",
                paint(&format!("{} PIDs", q.max_pids), GREEN)
            );
            println!(
                "  Usa antos quota set [--timeout N] [--memory N] [--cpu N] para modificar.\n"
            );
        }
    }

    Ok(())
}

// ------------------------------------------------------------------ diff

pub fn cmd_diff(ctx: &Ctx, args: &[String]) -> Result<()> {
    // T17.2 argument parsing:
    //   antos diff                  — detect project from cwd or scan workspace
    //   antos diff <project>        — diff the named project
    //   antos diff <project> <ref>  — diff the named project against <ref>
    //   antos diff <ref>            — backwards-compat: diff active project / workspace against <ref>
    //
    // A token is treated as a project name when it matches a directory under workspace/.
    // Otherwise it is treated as a git target ref.

    let antos_root = ctx.antos_root.clone();
    let workspace = &ctx.workspace;

    println!(
        "\n{}",
        paint(
            "antOS · Visor Interactivo de Diffs y Parches (T8.1 / T17.2)",
            BOLD
        )
    );
    println!(
        "  Espacio de trabajo: {}",
        paint(&workspace.display().to_string(), DIM)
    );

    // Parse arguments into (project_path, target_ref).
    let (project_path, target) = parse_diff_args(args, workspace, ctx.current_project.as_deref());

    if let Some(ref p) = project_path {
        println!(
            "  Proyecto:           {}",
            paint(&p.display().to_string(), DIM)
        );
    } else if let Some(ref p) = ctx.current_project {
        println!(
            "  Proyecto activo:    {}",
            paint(&p.display().to_string(), DIM)
        );
    }
    println!("  Objetivo:           {}\n", paint(&target, BOLD));

    // Case A: a specific project was requested — diff only that project.
    if let Some(proj) = project_path {
        diff_single_project(&proj, &target, antos_root.as_deref());
        return Ok(());
    }

    // Case B: we are inside a project (contextual detection from T17.1).
    if let Some(ref proj) = ctx.current_project {
        diff_single_project(proj, &target, antos_root.as_deref());
        return Ok(());
    }

    // Case C: no project context — scan all projects in workspace/.
    let projects = crate::exec::scan_workspace_projects(workspace);
    if projects.is_empty() {
        println!(
            "  {} No se encontraron proyectos en el espacio de trabajo.\n",
            paint("ℹ Sin proyectos:", DIM)
        );
        return Ok(());
    }

    println!(
        "  {} {} proyecto(s) detectado(s)\n",
        paint("↓ Escaneando:", CYAN),
        projects.len()
    );

    for proj in &projects {
        let proj_name = proj
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| proj.display().to_string());
        println!(
            "  {} {}",
            paint("┌ proyecto:", BOLD),
            paint(&proj_name, CYAN)
        );
        diff_single_project(proj, &target, antos_root.as_deref());
    }

    Ok(())
}

/// Parses CLI arguments into (optional project path, target ref).
///
/// Logic:
/// - If the first arg is a directory under `workspace/`, it is the project.
///   The second arg (if present) is the target ref.
/// - Otherwise the first arg (if present) is the target ref.
/// - Falls back to (current_project, "HEAD").
fn parse_diff_args(
    args: &[String],
    workspace: &std::path::Path,
    current_project: Option<&std::path::Path>,
) -> (Option<std::path::PathBuf>, String) {
    match args {
        [] => (None, "HEAD".into()),
        [first] => {
            let candidate = workspace.join(first);
            if candidate.is_dir() {
                (Some(candidate), "HEAD".into())
            } else {
                // Treat as a target ref, keep detected project (or None).
                (current_project.map(|p| p.to_path_buf()), first.clone())
            }
        }
        [first, second, ..] => {
            let candidate = workspace.join(first);
            if candidate.is_dir() {
                (Some(candidate), second.clone())
            } else {
                // first is a ref, not a project name.
                (current_project.map(|p| p.to_path_buf()), first.clone())
            }
        }
    }
}

/// Diffs a single project directory, printing the result to stdout.
///
/// Uses ceiling-aware git root detection (T17.1) to verify the project has its
/// own `.git` before invoking `git diff`. If it does not, prints an informative
/// file listing and actionable guidance.
fn diff_single_project(proj: &std::path::Path, target: &str, antos_root: Option<&std::path::Path>) {
    let project_name = proj
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| proj.display().to_string());

    let has_git = crate::git::find_git_root_with_ceiling(proj, antos_root).is_some();

    if !has_git {
        let files = crate::exec::collect_project_files(proj, 20);
        if files.is_empty() {
            println!(
                "  {} {} — directorio vacío (sin archivos ni repositorio Git)\n",
                paint("⚠", YELLOW),
                paint(&project_name, BOLD)
            );
        } else {
            println!(
                "  {} {} — {} archivo(s) detectado(s), sin repositorio Git:",
                paint("⚠", YELLOW),
                paint(&project_name, BOLD),
                files.len()
            );
            for f in &files {
                println!("    {}", paint(f, DIM));
            }
            println!();
            println!(
                "  {} Inicializa el repositorio con: {}",
                paint("→", CYAN),
                paint(&format!("antos project init {}", project_name), DIM)
            );
            println!();
        }
        return;
    }

    // Build git diff with ceiling.
    let ceiling_val = antos_root
        .and_then(|r| r.parent())
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    // Check if HEAD exists. If not, the repo is newly initialized with no commits.
    let mut check_head = std::process::Command::new("git");
    check_head
        .current_dir(proj)
        .args(["rev-parse", "--verify", "HEAD"]);
    if !ceiling_val.is_empty() {
        check_head.env("GIT_CEILING_DIRECTORIES", &ceiling_val);
    }
    let has_commits = check_head
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !has_commits && target == "HEAD" {
        let mut status_cmd = std::process::Command::new("git");
        status_cmd.current_dir(proj).args(["status", "--porcelain"]);
        if !ceiling_val.is_empty() {
            status_cmd.env("GIT_CEILING_DIRECTORIES", &ceiling_val);
        }
        if let Ok(st_out) = status_cmd.output() {
            let st_str = String::from_utf8_lossy(&st_out.stdout);
            let lines: Vec<&str> = st_str.lines().filter(|l| !l.trim().is_empty()).collect();
            if lines.is_empty() {
                println!(
                    "  {} {} — repositorio Git inicializado (árbol limpio, sin commits aún).
",
                    paint("✓", GREEN),
                    paint(&project_name, BOLD)
                );
            } else {
                println!(
                    "  {} {} — repositorio Git inicializado ({} archivo(s) pendientes de commit inicial):",
                    paint("●", CYAN),
                    paint(&project_name, BOLD),
                    lines.len()
                );
                for line in &lines {
                    println!("    {}", paint(line.trim(), DIM));
                }
                println!();
            }
        }
        return;
    }

    let mut git_cmd = std::process::Command::new("git");
    git_cmd.current_dir(proj).args(["diff", target]);
    if !ceiling_val.is_empty() {
        git_cmd.env("GIT_CEILING_DIRECTORIES", &ceiling_val);
    }

    match git_cmd.output() {
        Ok(out) if out.status.success() => {
            let diff_str = String::from_utf8_lossy(&out.stdout);
            let files = crate::diff_view::DiffEngine::parse_unified_diff(&diff_str);
            if files.is_empty() {
                println!(
                    "  {} {} — sin cambios pendientes contra «{}».\n",
                    paint("✓", GREEN),
                    paint(&project_name, BOLD),
                    target
                );
            } else {
                println!(
                    "  {} {} — {} archivo(s) modificado(s):",
                    paint("~", CYAN),
                    paint(&project_name, BOLD),
                    files.len()
                );
                let rendered = crate::diff_view::DiffEngine::render_terminal(&files);
                print!("{rendered}");
            }
        }
        _ => {
            println!(
                "  {} No se pudo ejecutar git diff en «{}».\n",
                paint("✗", RED),
                proj.display()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn args_of(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // -- antos quota set ------------------------------------------------

    #[test]
    fn test_parse_quota_set_args_reads_every_flag() {
        let args = args_of(&[
            "set",
            "--timeout",
            "120",
            "--memory",
            "2048",
            "--cpu",
            "80",
            "--pids",
            "64",
        ]);
        let parsed = parse_quota_set_args(&args);
        assert_eq!(
            parsed,
            QuotaSetArgs {
                timeout_secs: Some(120),
                max_memory_mb: Some(2048),
                cpu_quota_percent: Some(80),
                max_pids: Some(64),
            }
        );
    }

    #[test]
    fn test_parse_quota_set_args_leaves_unparseable_values_unset() {
        // «--cpu abc» no es un u32 válido: el campo queda en `None`, no se
        // aplica ninguna sobrescritura para él (T31.13).
        let args = args_of(&["set", "--cpu", "abc", "--pids", "nope"]);
        let parsed = parse_quota_set_args(&args);
        assert_eq!(parsed.cpu_quota_percent, None);
        assert_eq!(parsed.max_pids, None);
    }

    #[test]
    fn test_parse_quota_set_args_ignores_a_trailing_flag_with_no_value() {
        let args = args_of(&["set", "--timeout"]);
        let parsed = parse_quota_set_args(&args);
        assert_eq!(parsed.timeout_secs, None);
    }

    #[test]
    fn test_quota_set_args_apply_to_only_touches_specified_fields() {
        let mut q = crate::sandbox::quota::ResourceQuota {
            timeout_secs: 30,
            max_memory_mb: 512,
            cpu_quota_percent: 50,
            max_pids: 32,
        };
        let overrides = QuotaSetArgs {
            timeout_secs: Some(90),
            max_memory_mb: None,
            cpu_quota_percent: None,
            max_pids: Some(16),
        };
        overrides.apply_to(&mut q);

        assert_eq!(q.timeout_secs, 90);
        assert_eq!(q.max_memory_mb, 512); // sin cambios: no se pidió
        assert_eq!(q.cpu_quota_percent, 50); // sin cambios: no se pidió
        assert_eq!(q.max_pids, 16);
    }
}
