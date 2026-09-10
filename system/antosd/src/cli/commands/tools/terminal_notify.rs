//! `antos terminal` (VTE embebido) y `antos notify` (bandeja de aprobaciones).
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

pub fn cmd_terminal(args: &[String]) -> Result<()> {
    let mut session = crate::vte::TerminalSession::new("vte-cli");
    println!(
        "\n{}",
        paint("antOS · Consola Terminal VTE Embebida (T8.1)", BOLD)
    );
    println!(
        "  Shell interactivo:  {}\n",
        paint(&session.active_shell, GREEN)
    );

    if args.is_empty() {
        println!("  Consola terminal interactiva lista. Para ejecutar comandos usa:");
        println!("    antos terminal \"<comando>\"\n");
    } else {
        let cmd = args.join(" ");
        session.execute_command(&cmd)?;
        for line in &session.buffer {
            println!("{}", line.raw);
        }
        println!();
    }

    Ok(())
}

// ------------------------------------------------------------------ notify / approvals

pub fn cmd_notify(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str);
    let engine = crate::notification::NotificationEngine::global();

    match sub {
        Some("approve" | "aprobar") => {
            let id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("uso: antos notify approve <ID>"))?;
            let (ok, msg) = engine.handle_action(
                &ctx.workspace,
                id,
                antos_protocol::NotificationAction::Approve,
            )?;
            if ok {
                println!("\n{} {msg}\n", paint("✓", GREEN));
            } else {
                println!("\n{} {msg}\n", paint("✗", RED));
            }
        }
        Some("reject" | "rechazar" | "rollback") => {
            let id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("uso: antos notify reject <ID>"))?;
            let (ok, msg) = engine.handle_action(
                &ctx.workspace,
                id,
                antos_protocol::NotificationAction::Reject,
            )?;
            if ok {
                println!("\n{} {msg}\n", paint("✓", GREEN));
            } else {
                println!("\n{} {msg}\n", paint("✗", RED));
            }
        }
        Some("dismiss" | "descartar") => {
            let id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("uso: antos notify dismiss <ID>"))?;
            let (ok, msg) = engine.handle_action(
                &ctx.workspace,
                id,
                antos_protocol::NotificationAction::Dismiss,
            )?;
            if ok {
                println!("\n{} {msg}\n", paint("✓", GREEN));
            } else {
                println!("\n{} {msg}\n", paint("✗", RED));
            }
        }
        Some("clear" | "limpiar") => {
            let count = engine.clear(&ctx.workspace)?;
            println!(
                "\n{} Se limpiaron {} notificaciones leídas.\n",
                paint("✓", GREEN),
                count
            );
        }
        _ => {
            let list = engine.list(&ctx.workspace)?;
            println!(
                "\n{}",
                paint(
                    "antOS · Bandeja de Notificaciones y Aprobaciones Asíncronas (T8.2)",
                    BOLD
                )
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            if list.is_empty() {
                println!(
                    "  {} No hay notificaciones ni aprobaciones pendientes.\n",
                    paint("✓ Bandeja al día:", GREEN)
                );
                println!(
                    "  Los agentes multi-agente antFlow notificarán aquí cuando completen tareas."
                );
                println!("  Comandos: antos notify approve <ID> | antos notify reject <ID>\n");
            } else {
                for n in &list {
                    let mark = if n.read {
                        paint("○ leída", DIM)
                    } else {
                        paint("● NUEVA", YELLOW)
                    };
                    let kind_badge = match n.kind {
                        antos_protocol::NotificationKind::ApprovalRequired => {
                            paint("⚠️ APROBACIÓN REQUERIDA", YELLOW)
                        }
                        antos_protocol::NotificationKind::TaskFinished => {
                            paint("✓ TAREA COMPLETADA", GREEN)
                        }
                        antos_protocol::NotificationKind::QAFailed => paint("✗ QA FALLIDO", RED),
                        antos_protocol::NotificationKind::SecurityAlert => {
                            paint("🛡️ ALERTA SEGURIDAD", RED)
                        }
                        antos_protocol::NotificationKind::System => paint("ℹ️ SISTEMA", CYAN),
                    };

                    println!(
                        "  {} [{}] {} — {}",
                        mark,
                        paint(&n.id, BOLD),
                        kind_badge,
                        paint(&n.ticket_id, BOLD)
                    );
                    println!("     {}: {}", paint("Título", DIM), n.title);
                    println!("     {}: {}\n", paint("Detalle", DIM), n.body);
                }
                println!("  Usa «antos notify approve <ID>» para autorizar o «antos notify reject <ID>» para rollback.\n");
            }
        }
    }

    Ok(())
}
