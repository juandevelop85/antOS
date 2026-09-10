//! `antos project`, `antos use` y `antos git`.
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

use super::quota_diff::cmd_diff;

pub fn cmd_project(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("list");

    match sub {
        "init" => cmd_project_init(ctx, &args[1..]),
        "list" | "ls" => cmd_project_list(ctx),
        "use" => cmd_use(ctx, &args[1..]),
        "current" => cmd_use(ctx, &[]),
        _ => {
            println!(
                "\n{}\n",
                paint("antOS · Gestión de Proyectos en Workspace (T17.3)", BOLD)
            );
            println!("  Uso:");
            println!("    antos use <nombre>            Fija el proyecto activo en el workspace");
            println!("    antos use --clear             Limpia la selección del proyecto activo");
            println!("    antos project init <nombre> [--branch <rama>] [--lang <lenguaje>]");
            println!("    antos project list            Lista proyectos en workspace/");
            println!("    antos git init [nombre]");
            println!();
            Ok(())
        }
    }
}

pub fn cmd_project_init(ctx: &Ctx, args: &[String]) -> Result<()> {
    let mut project_name: Option<String> = None;
    let mut branch = "main".to_string();
    let mut language_hint: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-b" | "--branch" => {
                if let Some(b) = args.get(i + 1) {
                    branch = b.clone();
                    i += 1;
                }
            }
            "-l" | "--lang" | "--language" => {
                if let Some(l) = args.get(i + 1) {
                    language_hint = Some(l.clone());
                    i += 1;
                }
            }
            other if !other.starts_with('-') && project_name.is_none() => {
                project_name = Some(other.to_string());
            }
            _ => {}
        }
        i += 1;
    }

    let project_dir = if let Some(ref name) = project_name {
        let candidate = std::path::Path::new(name);
        if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            ctx.workspace.join(name)
        }
    } else if let Some(ref current) = ctx.current_project {
        current.clone()
    } else {
        bail!(
            "Especifica el nombre del proyecto a inicializar:\n    antos project init <nombre>\n    antos git init <nombre>"
        );
    };

    println!(
        "\n{}",
        paint(
            "antOS · Inicialización Declarativa de Proyecto Git (T17.3)",
            BOLD
        )
    );
    println!(
        "  Espacio de trabajo: {}",
        paint(&ctx.workspace.display().to_string(), DIM)
    );

    let display_name = project_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| project_dir.display().to_string());

    println!("  Proyecto:           {}", paint(&display_name, CYAN));
    println!(
        "  Ruta física:        {}",
        paint(&project_dir.display().to_string(), DIM)
    );
    println!("  Rama principal:     {}", paint(&branch, GREEN));

    let res = crate::exec::init_project_git_repo(&project_dir, &branch, language_hint.as_deref())?;

    println!("\n  {} {}\n", paint("✓", GREEN), res);
    println!("  Para inspeccionar los cambios del proyecto, ejecuta:");
    println!(
        "      {}\n",
        paint(&format!("antos diff {}", display_name), DIM)
    );

    Ok(())
}

pub fn cmd_project_list(ctx: &Ctx) -> Result<()> {
    println!(
        "\n{}",
        paint("antOS · Proyectos en Espacio de Trabajo (T17.3)", BOLD)
    );
    println!(
        "  Espacio de trabajo: {}\n",
        paint(&ctx.workspace.display().to_string(), DIM)
    );

    let projects = crate::exec::scan_workspace_projects(&ctx.workspace);
    if projects.is_empty() {
        println!("  (no se encontraron proyectos en workspace/)\n");
        println!(
            "  Crea uno con: {}\n",
            paint("antos project init <nombre>", CYAN)
        );
        return Ok(());
    }

    let antos_root = ctx.antos_root.as_deref();

    for proj in &projects {
        let name = proj
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| proj.display().to_string());

        let has_git = crate::git::find_git_root_with_ceiling(proj, antos_root).is_some();
        let lang = crate::exec::detect_project_language(proj);
        let file_count = crate::exec::collect_project_files(proj, 100).len();

        let git_badge = if has_git {
            paint("● Git activo", GREEN)
        } else {
            paint("○ Sin Git", YELLOW)
        };

        let is_active = ctx.current_project.as_ref() == Some(proj);
        let active_badge = if is_active {
            format!(" {}", paint("[ACTIVO]", GREEN))
        } else {
            String::new()
        };

        println!(
            "  • {}{}  [{}]  (stack: {}, {} archivos)",
            paint(&name, BOLD),
            active_badge,
            git_badge,
            paint(&lang, CYAN),
            file_count
        );
    }
    println!();
    println!("  Para fijar el proyecto activo en todos los comandos:");
    println!("    {}\n", paint("antos use <nombre>", CYAN));

    Ok(())
}

// ------------------------------------------------------------------ terminal / vte

pub fn cmd_use(ctx: &Ctx, args: &[String]) -> Result<()> {
    let active_file = ctx.state.join("active_project");

    if let Some(target) = args.first() {
        if target == "--clear" || target == "clear" || target == "none" || target == "system" {
            if active_file.exists() {
                let _ = std::fs::remove_file(&active_file);
            }
            println!(
                "\n{} Selección de proyecto restablecida. antOS operará en ámbito global / automático.\n",
                paint("antOS ·", BOLD)
            );
            return Ok(());
        }

        // Validar si el proyecto existe en workspace
        let project_dir = ctx.workspace.join(target);
        if !project_dir.exists() || !project_dir.is_dir() {
            println!(
                "\n{} El proyecto '{}' no existe en {}",
                paint("antOS Error ·", RED),
                paint(target, YELLOW),
                ctx.workspace.display()
            );
            let projects = crate::exec::scan_workspace_projects(&ctx.workspace);
            if !projects.is_empty() {
                println!("\n  Proyectos disponibles en el workspace:");
                for p in projects {
                    let pname = p.file_name().and_then(|n| n.to_str()).unwrap_or("proyecto");
                    println!("    • {}", paint(pname, CYAN));
                }
            } else {
                println!("  (no hay proyectos creados aún en workspace/)");
            }
            println!(
                "\n  Puedes inicializarlo con: {}\n",
                paint(&format!("antos project init {}", target), GREEN)
            );
            return Ok(());
        }

        // Guardar proyecto activo en state
        std::fs::create_dir_all(&ctx.state)?;
        std::fs::write(&active_file, target.trim())?;

        println!(
            "\n{} Proyecto activo fijado en: {}\n  Directorio: {}\n  Todos los comandos de antOS (tickets, git, agentes, panel, etc.) operarán sobre este proyecto por defecto.\n",
            paint("antOS ·", BOLD),
            paint(target, GREEN),
            project_dir.display()
        );
    } else {
        // Mostrar proyecto activo actual
        println!(
            "\n{} Estado del Proyecto Activo en Workspace:",
            paint("antOS ·", BOLD)
        );
        if let Some(ref cur) = ctx.current_project {
            let name = cur
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("desconocido");
            println!("  • Proyecto seleccionado: {}", paint(name, GREEN));
            println!("  • Ruta en disco:         {}", cur.display());
            if let Ok(active_name) = std::fs::read_to_string(&active_file) {
                if active_name.trim() == name {
                    println!(
                        "  • Origen:                Configurado persistentemente vía 'antos use'"
                    );
                } else {
                    println!("  • Origen:                Detectado automáticamente por directorio actual (CWD)");
                }
            } else {
                println!("  • Origen:                Detectado automáticamente por directorio actual (CWD)");
            }
        } else {
            println!(
                "  • Proyecto seleccionado: {}",
                paint("(ninguno / ámbito del sistema)", YELLOW)
            );
            println!("  • Espacio de trabajo:    {}", ctx.workspace.display());
        }
        println!("\n  Uso:");
        println!("    antos use <nombre-proyecto>   Selecciona el proyecto activo para todos los comandos");
        println!("    antos use --clear             Limpia la selección activa (vuelve a detección automática)");
        println!("    antos project list            Lista todos los proyectos en workspace/\n");
    }
    Ok(())
}

pub fn cmd_git(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "init" => cmd_project_init(ctx, &args[1..]),
        "diff" => cmd_diff(ctx, &args[1..]),
        "status" | "st" => {
            let target_dir = ctx.current_project.as_deref().unwrap_or(&ctx.workspace);
            let proj_name = target_dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| target_dir.display().to_string());

            println!(
                "\n{} {}",
                paint("antOS Git · Estado de", BOLD),
                paint(&proj_name, CYAN)
            );
            println!(
                "  Directorio: {}",
                paint(&target_dir.display().to_string(), DIM)
            );

            let antos_root = ctx.antos_root.as_deref();
            if crate::git::find_git_root_with_ceiling(target_dir, antos_root).is_none() {
                println!(
                    "  {} No es un repositorio Git propio.\n  Inicialízalo con: {}\n",
                    paint("⚠", YELLOW),
                    paint(&format!("antos project init {}", proj_name), CYAN)
                );
                return Ok(());
            }

            if let Some(status) = crate::git::GitAnalyzer::global().consultar_estado(target_dir)? {
                let branch_str = status.branch.unwrap_or_else(|| "HEAD desacoplado".into());
                println!("  Rama activa:    {}", paint(&branch_str, GREEN));
                println!("  Sincronización: +{} / -{}", status.ahead, status.behind);
                println!("  Modificados:    {}", status.modified.len());
                println!("  Staged:         {}", status.staged.len());
                println!("  Sin seguimiento: {}\n", status.untracked.len());
            } else {
                println!("  (no se pudo determinar el estado)\n");
            }
            Ok(())
        }
        _ => {
            println!(
                "\n{}\n",
                paint("antOS · Integración Git Aislada (T17.3)", BOLD)
            );
            println!("  Subcomandos:");
            println!("    antos git init [nombre]");
            println!("    antos git status");
            println!("    antos git diff [ref]");
            println!();
            Ok(())
        }
    }
}
