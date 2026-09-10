//! `antos reproduce`, `antos testgen` y `antos snapshot`.
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

pub fn cmd_reproduce(ctx: &Ctx, args: &[String]) -> Result<()> {
    let mut log_path = None;
    let mut target_file = None;
    let mut positional = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--log" | "-l" => {
                if let Some(p) = args.get(i + 1) {
                    log_path = Some(p.clone());
                    i += 1;
                }
            }
            "--target" | "-t" => {
                if let Some(t) = args.get(i + 1) {
                    target_file = Some(t.clone());
                    i += 1;
                }
            }
            val if !val.starts_with('-') => {
                positional.push(val.to_string());
            }
            _ => {}
        }
        i += 1;
    }

    let error_input = if let Some(ref path) = log_path {
        let p = std::path::Path::new(path);
        let resolved = if p.is_absolute() {
            p.to_path_buf()
        } else {
            ctx.workspace.join(p)
        };
        std::fs::read_to_string(&resolved)
            .with_context(|| format!("No se pudo leer el archivo de log {}", resolved.display()))?
    } else if !positional.is_empty() {
        positional.join(" ")
    } else {
        bail!("Uso: antos reproduce \"<stack trace / log de error>\" [--target <archivo>] [--log <ruta_log>]");
    };

    println!(
        "\n{}",
        paint(
            "🧪 antOS TDD Engine · Reproducción Autónoma de Bugs (T20.2)",
            BOLD
        )
    );
    println!("  Analizando traza y aislando contexto de falla...");

    let report = crate::reproduce::TddEngine::run_reproduce_pipeline(
        &error_input,
        target_file.as_deref(),
        &ctx.workspace,
        &ctx.state,
    )?;

    println!("\n  {} [{}]", paint("ID del Caso:", CYAN), report.id);
    println!(
        "  {} {:?}",
        paint("Lenguaje Detectado:", BOLD),
        report.diagnostic.language
    );
    println!(
        "  {} {}",
        paint("Tipo de Error:", YELLOW),
        report.diagnostic.error_type
    );
    println!("  {} {}", paint("Mensaje:", RED), report.diagnostic.message);

    if let Some(ref f) = report.diagnostic.target_file {
        println!(
            "  {} {}:{}",
            paint("Ubicación:", BOLD),
            f,
            report.diagnostic.target_line.unwrap_or(0)
        );
    }
    if let Some(ref fn_name) = report.diagnostic.target_function {
        println!("  {} {}", paint("Función / Símbolo:", BOLD), fn_name);
    }
    if !report.diagnostic.frames.is_empty() {
        println!(
            "  {} ({} niveles capturados):",
            paint("Pila de Llamadas:", DIM),
            report.diagnostic.frames.len()
        );
        for frame in report.diagnostic.frames.iter().take(3) {
            println!(
                "    • {}:{} [{}]",
                frame.file,
                frame.line.unwrap_or(0),
                frame.function.as_deref().unwrap_or("fn")
            );
        }
    }

    println!(
        "\n  {} {}",
        paint("Fase del Ciclo:", BOLD),
        paint(report.phase.label(), GREEN)
    );
    println!(
        "  {} {}",
        paint("Test de Regresión:", CYAN),
        report.test_file
    );
    if let Some(ref fix) = report.fix_summary {
        println!("  {} {}", paint("Propuesta Correctiva:", GREEN), fix);
    }
    println!(
        "  {} {}",
        paint("Auditoría antOS:", BOLD),
        if report.audited {
            paint(
                "✅ Aislado en sandbox y certificado contra regresiones",
                GREEN,
            )
        } else {
            paint("⚠️ Pendiente de validación", YELLOW)
        }
    );
    println!(
        "  {} .antos/reproduce/{}/\n",
        paint("Artefactos:", DIM),
        report.id
    );

    Ok(())
}

pub fn cmd_testgen(ctx: &Ctx, args: &[String]) -> Result<()> {
    let mut target = None;
    let mut suite = "unit".to_string();
    let mut cases = 3usize;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--target" | "-t" => {
                if let Some(t) = args.get(i + 1) {
                    target = Some(t.clone());
                    i += 1;
                }
            }
            "--suite" | "-s" => {
                if let Some(s) = args.get(i + 1) {
                    suite = s.clone();
                    i += 1;
                }
            }
            "--cases" | "-c" => {
                if let Some(c) = args.get(i + 1).and_then(|v| v.parse::<usize>().ok()) {
                    cases = c;
                    i += 1;
                }
            }
            val if !val.starts_with('-') && target.is_none() => {
                target = Some(val.to_string());
            }
            _ => {}
        }
        i += 1;
    }

    let target = target.unwrap_or_else(|| "src/lib.rs".into());

    println!(
        "\n{}",
        paint(
            "⚡ antOS TestGen · Generador Autónomo de Tests (T20.2)",
            BOLD
        )
    );
    println!("  Generando suite de tests para «{}»...", target);

    let report = crate::reproduce::TddEngine::generate_tests_for_target(
        &target,
        &suite,
        cases,
        &ctx.workspace,
    )?;

    println!("\n  {} [{}]", paint("ID de Suite:", CYAN), report.id);
    println!(
        "  {} {:?}",
        paint("Lenguaje:", BOLD),
        report.diagnostic.language
    );
    println!("  {} {}", paint("Objetivo:", CYAN), target);
    println!(
        "  {} {} ({} casos)",
        paint("Tipo de Suite:", YELLOW),
        suite,
        cases
    );
    println!(
        "  {} {}",
        paint("Archivo Generado:", GREEN),
        report.test_file
    );
    if let Some(ref summary) = report.fix_summary {
        println!("  {} {}\n", paint("Resumen:", DIM), summary);
    }

    Ok(())
}

// ------------------------------------------------------------------ ci / hooks (T20.3)

pub fn cmd_snapshot(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    match sub {
        "create" | "crear" | "save" => {
            let mut label = None;
            let mut author = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--author" | "-a" => {
                        if let Some(a) = args.get(i + 1) {
                            author = Some(a.as_str());
                            i += 1;
                        }
                    }
                    val if !val.starts_with('-') && label.is_none() => {
                        label = Some(val);
                    }
                    _ => {}
                }
                i += 1;
            }

            println!(
                "\n{}",
                paint(
                    "📸 antOS Time Machine · Creando Instantánea Atómica (T20.4)",
                    BOLD
                )
            );
            let meta = crate::time_machine::TimeMachineEngine::create_snapshot(
                &ctx.workspace,
                &ctx.state,
                label,
                author,
            )?;

            println!("  {} [{}]", paint("ID Instantánea:", CYAN), meta.id);
            if let Some(ref l) = meta.label {
                println!("  {} {}", paint("Etiqueta:", GREEN), l);
            }
            println!("  {} {}", paint("Autor:", BOLD), meta.author);
            if let Some(ref br) = meta.git_branch {
                println!(
                    "  {} {} ({})",
                    paint("Rama Git:", BOLD),
                    br,
                    meta.git_commit.as_deref().unwrap_or("?")
                );
            }
            println!(
                "  {} {} archivos ({} KiB)",
                paint("Árbol de Código:", BOLD),
                meta.files_count,
                meta.total_bytes / 1024
            );
            if !meta.services_included.is_empty() {
                println!(
                    "  {} {}",
                    paint("Servicios Respaldados:", YELLOW),
                    meta.services_included.join(", ")
                );
            }
            if meta.memory_graph_included {
                println!(
                    "  {} Grafo de contexto preservado",
                    paint("Memoria Semántica:", CYAN)
                );
            }
            println!("  {} {}", paint("Mecanismo CoW:", DIM), meta.method);
            println!("  Estado del entorno congelado y protegido con capacidad de rollback.\n");
            Ok(())
        }
        "list" | "ls" | "status" => {
            let list = crate::time_machine::TimeMachineEngine::list_snapshots(&ctx.state)?;
            println!(
                "\n{}",
                paint(
                    "⏱️ antOS Time Machine · Cronología de Instantáneas de Estado (T20.4)",
                    BOLD
                )
            );
            if list.is_empty() {
                println!("  No hay instantáneas registradas en .antos/snapshots/dev/\n");
                println!("  Crea una nueva con: antos snapshot create [etiqueta]\n");
                return Ok(());
            }

            println!(
                "  {:<26} {:<18} {:<10} {:<12} {:<10}",
                paint("ID", BOLD),
                paint("ETIQUETA", BOLD),
                paint("ARCHIVOS", BOLD),
                paint("TAMAÑO", BOLD),
                paint("RAMA", BOLD)
            );
            println!("  {}", "─".repeat(80));
            for s in &list {
                let lbl = s.label.as_deref().unwrap_or("—");
                let size_str = format!("{} KiB", s.total_bytes / 1024);
                let branch_str = s.git_branch.as_deref().unwrap_or("—");
                println!(
                    "  {:<26} {:<18} {:<10} {:<12} {:<10}",
                    paint(&s.id, CYAN),
                    lbl,
                    s.files_count,
                    size_str,
                    branch_str
                );
            }
            println!(
                "\n  Total: {} instantánea(s) disponibles para restauración inmediata.\n",
                list.len()
            );
            Ok(())
        }
        "restore" | "restaurar" | "revert" => {
            let target = args.get(1).map(String::as_str);
            let Some(id_or_label) = target else {
                bail!("Uso: antos snapshot restore <id|etiqueta> [--no-rescue]");
            };
            let create_rescue = !args.iter().any(|a| a == "--no-rescue");

            println!(
                "\n{}",
                paint(
                    "⏪ antOS Time Machine · Restaurando Estado del Entorno (T20.4)",
                    BOLD
                )
            );
            println!("  Objetivo: «{}»", paint(id_or_label, CYAN));

            let res = crate::time_machine::TimeMachineEngine::restore_snapshot(
                &ctx.workspace,
                &ctx.state,
                id_or_label,
                create_rescue,
            )?;

            println!(
                "  {} {}",
                paint("Instantánea Restaurada:", GREEN),
                res.snapshot_id
            );
            if let Some(ref rescue) = res.rescue_snapshot_id {
                println!(
                    "  {} [{}]",
                    paint("Snapshot de Rescate Creado:", YELLOW),
                    rescue
                );
            }
            println!(
                "  {} {} archivos actualizados / {}",
                paint("Operaciones de Archivo:", BOLD),
                res.files_restored,
                paint(
                    &format!("{} archivos eliminados (untracked)", res.files_deleted),
                    DIM
                )
            );
            if !res.services_restored.is_empty() {
                println!(
                    "  {} {}",
                    paint("Servicios Restaurados:", YELLOW),
                    res.services_restored.join(", ")
                );
            }
            if res.memory_graph_restored {
                println!(
                    "  {} Grafo vectorial reestablecido",
                    paint("Memoria Semántica:", CYAN)
                );
            }
            println!(
                "  {} {} ms",
                paint("Tiempo de Inversión:", BOLD),
                res.duration_ms
            );
            println!("  ✅ Entorno revertido con éxito al punto exacto capturado.\n");
            Ok(())
        }
        "delete" | "del" | "rm" | "eliminar" => {
            let target = args.get(1).map(String::as_str);
            let Some(id_or_label) = target else {
                bail!("Uso: antos snapshot delete <id|etiqueta>");
            };
            println!(
                "\n{}",
                paint(
                    "🗑️ antOS Time Machine · Eliminando Instantánea (T20.4)",
                    BOLD
                )
            );
            let deleted =
                crate::time_machine::TimeMachineEngine::delete_snapshot(&ctx.state, id_or_label)?;
            println!(
                "  Instantánea [{}] eliminada correctamente y espacio liberado.\n",
                paint(&deleted, CYAN)
            );
            Ok(())
        }
        _ => {
            println!("\n{}", paint("antOS Time Machine · Ayuda (T20.4)", BOLD));
            println!("  Uso:");
            println!("    antos snapshot create [etiqueta]     Crea una instantánea completa");
            println!("    antos snapshot list                  Lista instantáneas disponibles");
            println!("    antos snapshot restore <id|etiqueta> Revierte el entorno completo");
            println!("    antos snapshot delete <id|etiqueta>  Elimina una instantánea\n");
            Ok(())
        }
    }
}
