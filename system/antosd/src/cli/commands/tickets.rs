#![allow(unused_imports, dead_code)]

extern crate antos_protocol as antos_protocolo;

use std::path::{Path, PathBuf};
use anyhow::{bail, Context, Result};
use crate::capability::{Catalog, Tier};
use crate::ctx::Ctx;
use crate::grants::Grants;
use crate::journal::{Outcome, Record};
use crate::planner::{
    claude::ClaudePlanner, local::LocalPlanner, ollama::OllamaPlanner,
    openai_compat::OpenAiCompatPlanner, Planner,
};
use crate::terminal::{ellipsis, paint, tier_color, BLUE, BOLD, CYAN, DIM, GREEN, RED, YELLOW};
use crate::cli::args::Opts;

fn render_project_tickets(proj_path: &std::path::Path, proj_name: &str, engine: &crate::spec::SpecEngine) -> Result<()> {
    if !proj_path.exists() {
        println!(
            "\n{} El proyecto «{}» no existe en el workspace ({}).\n",
            paint("✗", RED),
            paint(proj_name, BOLD),
            paint(&proj_path.display().to_string(), DIM)
        );
        return Ok(());
    }

    let tickets = engine.list_tickets(proj_path)?;
    if tickets.is_empty() {
        println!(
            "\n{} {}\n",
            paint("antOS · Catálogo de Tickets", BOLD),
            paint(&format!("— Proyecto: «{proj_name}»"), CYAN)
        );
        println!("  (no se encontraron tickets definidos en este proyecto)");
        println!("  Crea el primer ticket con:\n");
        println!("    antos ticket new T1.1 \"Título del Ticket\" --project {proj_name}\n");
        return Ok(());
    }

    println!(
        "\n{} {}\n",
        paint("antOS · Catálogo de Tickets", BOLD),
        paint(&format!("— Proyecto: «{proj_name}»"), CYAN)
    );
    println!(
        "  {:<8} {:<8} {:<55} {}",
        paint("FASE", DIM),
        paint("ID", DIM),
        paint("TÍTULO", DIM),
        paint("ESTADO", DIM)
    );
    println!("  {}", "─".repeat(88));

    let mut completados = 0;
    for t in &tickets {
        if t.status == antos_protocolo::TicketStatus::Completado {
            completados += 1;
        }
        println!(
            "  {:<8} {:<8} {:<55} {}",
            paint(&t.phase, DIM),
            paint(&t.id, BOLD),
            ellipsis(&t.title, 53),
            t.status.tag()
        );
    }
    println!("  {}", "─".repeat(88));
    println!(
        "  Total: {} tickets | {} completados | {} pendientes\n",
        tickets.len(),
        completados,
        tickets.len() - completados
    );
    Ok(())
}

pub fn cmd_tickets(ctx: &Ctx, args: &[String]) -> Result<()> {
    let engine = crate::spec::SpecEngine::global();

    // 1. Extraer flags globales de proyecto o sistema: --project <P>, -p <P>, --system, --os
    let mut project_flag: Option<String> = None;
    let mut is_system = false;
    let mut clean_args: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--project" | "-p" => {
                if i + 1 < args.len() {
                    project_flag = Some(args[i + 1].clone());
                    i += 2;
                    continue;
                }
            }
            "--system" | "--os" => {
                is_system = true;
                i += 1;
                continue;
            }
            _ => {
                clean_args.push(args[i].clone());
                i += 1;
            }
        }
    }

    let sub = clean_args.first().map(String::as_str);

    // 2. Resolver directorio objetivo base
    let default_target_ws = if is_system {
        ctx.antos_root.clone().unwrap_or_else(|| ctx.workspace.clone())
    } else if let Some(ref p) = project_flag {
        ctx.workspace.join(p)
    } else if let Some(ref cur) = ctx.current_project {
        cur.clone()
    } else {
        ctx.workspace.clone()
    };

    match sub {
        Some("new" | "create" | "add") => {
            let id = clean_args.get(1).ok_or_else(|| {
                anyhow::anyhow!(
                    "uso: antos ticket new <ID> <Título> [--project <proyecto>] [--fase \"...\"] [--desc \"...\"]"
                )
            })?;
            let title = clean_args.get(2).ok_or_else(|| {
                anyhow::anyhow!(
                    "uso: antos ticket new <ID> <Título> [--project <proyecto>] [--fase \"...\"] [--desc \"...\"]"
                )
            })?;

            let phase = clean_args
                .iter()
                .position(|a| a == "--fase" || a == "-f")
                .and_then(|idx| clean_args.get(idx + 1))
                .cloned();

            let desc = clean_args
                .iter()
                .position(|a| a == "--desc" || a == "-d")
                .and_then(|idx| clean_args.get(idx + 1))
                .cloned();

            let path = engine.create_ticket(
                &default_target_ws,
                id,
                title,
                desc.as_deref(),
                phase.as_deref(),
            )?;

            let scope_name = default_target_ws
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "sistema".to_string());

            println!(
                "\n{} Ticket {} creado exitosamente para «{}» en {}.\n",
                paint("✓", GREEN),
                paint(id, BOLD),
                paint(&scope_name, CYAN),
                paint(&path.display().to_string(), DIM)
            );
            return Ok(());
        }
        Some("status" | "set-status") => {
            let id = clean_args.get(1).ok_or_else(|| {
                anyhow::anyhow!(
                    "uso: antos ticket status <ID> <completado|progreso|revision|pendiente> [--project <proyecto>]"
                )
            })?;
            let status_raw = clean_args.get(2).ok_or_else(|| {
                anyhow::anyhow!(
                    "uso: antos ticket status <ID> <completado|progreso|revision|pendiente> [--project <proyecto>]"
                )
            })?;
            let st = match status_raw.to_lowercase().as_str() {
                "completado" | "done" | "hecho" => antos_protocolo::TicketStatus::Completado,
                "progreso" | "en_progreso" | "in_progress" => {
                    antos_protocolo::TicketStatus::EnProgreso
                }
                "revision" | "revisión" | "review" => antos_protocolo::TicketStatus::EnRevision,
                _ => antos_protocolo::TicketStatus::Pendiente,
            };
            engine.update_ticket_status(&default_target_ws, id, st)?;
            let scope_name = default_target_ws
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "sistema".to_string());

            println!(
                "\n{} Estado del ticket {} en «{}» actualizado a {}.\n",
                paint("✓", GREEN),
                paint(id, BOLD),
                paint(&scope_name, CYAN),
                st.tag()
            );
            return Ok(());
        }
        Some(arg) if arg != "list" => {
            let is_ticket_id = (arg.starts_with('T') || arg.starts_with('t'))
                && arg.chars().nth(1).map(|c| c.is_ascii_digit()).unwrap_or(false);

            if is_ticket_id {
                let detalle = engine.get_ticket(&default_target_ws, arg)?;
                match detalle {
                    Some(t) => {
                        println!(
                            "\n{} {}  {}",
                            paint(&t.id, BOLD),
                            paint(&t.phase, DIM),
                            t.status.tag()
                        );
                        println!("{}", paint(&t.title, BOLD));
                        println!();
                        println!("{}", paint("Descripción:", BOLD));
                        println!("  {}", t.description);
                        if !t.technical_scope.is_empty() {
                            println!();
                            println!("{}", paint("Alcance Técnico:", BOLD));
                            for a in &t.technical_scope {
                                println!("  • {a}");
                            }
                        }
                        if !t.acceptance_criteria.is_empty() {
                            println!();
                            println!("{}", paint("Criterios de Aceptación:", BOLD));
                            for c in &t.acceptance_criteria {
                                println!("  • {c}");
                            }
                        }
                        println!();
                    }
                    None => {
                        let scope_name = default_target_ws
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "catálogo".to_string());
                        println!("\nticket '{arg}' no encontrado en el ámbito «{scope_name}».\n");
                    }
                }
                return Ok(());
            } else if arg == "system" || arg == "os" {
                is_system = true;
            } else {
                // Es el nombre de un proyecto: `antos tickets api-service`
                let proj_target = ctx.workspace.join(arg);
                return render_project_tickets(&proj_target, arg, engine);
            }
        }
        _ => {}
    }

    // Listar tickets
    if let Some(ref p) = project_flag {
        let proj_target = ctx.workspace.join(p);
        return render_project_tickets(&proj_target, p, engine);
    }

    if !is_system {
        if let Some(ref cur) = ctx.current_project {
            let proj_name = cur.file_name().and_then(|n| n.to_str()).unwrap_or("proyecto");
            return render_project_tickets(cur, proj_name, engine);
        }
    }

    // Si el usuario está ejecutando dentro de `workspace/` (raíz del workspace)
    let cwd_canon = std::env::current_dir().ok().and_then(|c| c.canonicalize().ok());
    let ws_canon = ctx.workspace.canonicalize().ok();
    let in_workspace_root = cwd_canon == ws_canon
        && ctx.workspace.file_name().map(|n| n == "workspace").unwrap_or(false);

    if !is_system && in_workspace_root {
        let projects = crate::exec::scan_workspace_projects(&ctx.workspace);
        println!(
            "\n{}\n",
            paint("antOS · Catálogo de Tickets por Proyecto en Workspace (T17.4)", BOLD)
        );
        if projects.is_empty() {
            println!("  (no hay proyectos inicializados en el workspace)");
            println!("  Crea un nuevo proyecto con: antos project init <nombre>\n");
            return Ok(());
        }

        for p in &projects {
            let p_name = p.file_name().and_then(|n| n.to_str()).unwrap_or("proyecto");
            let p_tickets = engine.list_tickets(p).unwrap_or_default();
            if p_tickets.is_empty() {
                println!("  • {:<20} (sin catálogo de tickets)", paint(p_name, CYAN));
            } else {
                let comp = p_tickets.iter().filter(|t| t.status == antos_protocolo::TicketStatus::Completado).count();
                let pend = p_tickets.len() - comp;
                println!(
                    "  • {:<20} {} tickets ({} completados, {} pendientes)",
                    paint(p_name, CYAN),
                    p_tickets.len(),
                    comp,
                    pend
                );
            }
        }
        println!("\n  Comandos:");
        println!("    antos tickets <proyecto>               ver tickets de un proyecto");
        println!("    antos ticket new <ID> <Título> -p <p>  crear ticket en proyecto");
        println!("    antos tickets --system                 ver tickets del sistema antOS\n");
        return Ok(());
    }

    let target = if is_system {
        ctx.antos_root.clone().unwrap_or_else(|| ctx.workspace.clone())
    } else {
        ctx.workspace.clone()
    };

    let tickets = engine.list_tickets(&target)?;
    if tickets.is_empty() {
        println!("\nno se encontraron tickets en el sistema operativo.");
        println!("Crea uno con: antos ticket new <ID> <Título>\n");
        return Ok(());
    }

    println!(
        "\n{}",
        paint("antOS · Catálogo y Hoja de Ruta de Tickets (Sistema Operativo)", BOLD)
    );
    println!();
    println!(
        "  {:<8} {:<8} {:<55} {}",
        paint("FASE", DIM),
        paint("ID", DIM),
        paint("TÍTULO", DIM),
        paint("ESTADO", DIM)
    );
    println!("  {}", "─".repeat(88));

    let mut completados = 0;
    for t in &tickets {
        if t.status == antos_protocolo::TicketStatus::Completed {
            completados += 1;
        }
        println!(
            "  {:<8} {:<8} {:<55} {}",
            paint(&t.phase, DIM),
            paint(&t.id, BOLD),
            ellipsis(&t.title, 53),
            t.status.tag()
        );
    }
    println!("  {}", "─".repeat(88));
    println!(
        "  Total: {} tickets | {} completados | {} pendientes\n",
        tickets.len(),
        completados,
        tickets.len() - completados
    );
    Ok(())
}

