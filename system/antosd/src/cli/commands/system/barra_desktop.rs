//! `antos barra` y `antos desktop`.
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

pub fn cmd_barra(_ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    let manager = crate::barra::BarraManager::global();

    match sub {
        "alert" | "notif" | "notify" | "alerta" => {
            let clean_parts: Vec<&str> = args[1..]
                .iter()
                .filter(|a| *a != "--urgent" && *a != "-u")
                .map(|s| s.as_str())
                .collect();
            let msg = if clean_parts.is_empty() {
                "Prueba de alerta visual".to_string()
            } else {
                clean_parts.join(" ")
            };
            let alert = antos_protocol::BarraAlert {
                category: "cli".into(),
                message: msg.clone(),
                urgent: args.iter().any(|a| a == "--urgent" || a == "-u"),
            };
            manager.emit_alert(alert)?;
            println!(
                "\n{} Alerta visual emitida a la barra de escritorio: «{}»\n",
                paint("antOS Barra ·", BOLD),
                paint(&msg, GREEN)
            );
        }
        _ => {
            let t = manager.get_telemetry();
            let mb = t.profiler_rss_bytes as f64 / (1024.0 * 1024.0);
            println!(
                "\n{} Telemetría en Tiempo Real de la Barra de Escritorio:",
                paint("antOS Barra ·", BOLD)
            );
            println!(
                "  • eBPF LSM Guard:       {}",
                if t.ebpf_lsm_active {
                    paint("Activo", GREEN)
                } else {
                    paint("Auditoría", YELLOW)
                }
            );
            println!(
                "  • Violaciones LSM:      {}",
                if t.ebpf_violations_count > 0 {
                    paint(&t.ebpf_violations_count.to_string(), RED)
                } else {
                    paint("0", GREEN)
                }
            );
            println!("  • Consumo RSS Pico:     {:.2} MB", mb);
            println!("  • CPU Estimada:         {:.1}%", t.profiler_cpu_percent);
            println!(
                "  • Sesión de Pair:       {}",
                paint(t.active_pair_session.as_deref().unwrap_or("inactiva"), CYAN)
            );
            println!(
                "  • Nodos antMesh:        {} registrados manualmente",
                t.mesh_peers_count
            );
            println!(
                "  • Notificaciones:       {} pendientes",
                t.active_notifications_count
            );
            println!("\n  Alertas recientes en cola:");
            let alerts = manager.get_alerts(3);
            if alerts.is_empty() {
                println!("    (sin alertas recientes)");
            } else {
                for a in alerts {
                    let u = if a.urgent {
                        paint("[URGENTE]", RED)
                    } else {
                        paint("[INFO]", CYAN)
                    };
                    println!("    • {u} {}: {}", a.category, a.message);
                }
            }
            println!();
        }
    }
    Ok(())
}

// --------------------------------------------------------------------- boot

pub fn cmd_desktop(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    match sub {
        "start" | "iniciar" | "run" => {
            let nested = args.iter().any(|a| a == "--nested" || a == "-n");
            println!(
                "\n{} Inicializando entorno gráfico Wayland de antOS...",
                paint("antOS Desktop ·", BOLD)
            );
            let _ = crate::desktop::DesktopManager::sync_configuration(&ctx.workspace)?;
            let out = crate::desktop::DesktopManager::start_session(&ctx.workspace, nested)?;
            println!("{out}");
        }
        "keys" | "hotkeys" | "atajos" => {
            let keys = crate::desktop::DesktopManager::get_hotkeys();
            println!(
                "\n{} Atajos de Teclado Globales del Entorno de Escritorio:",
                paint("antOS Desktop ·", BOLD)
            );
            println!(
                "  {:<16} {:<24} {}",
                paint("ATAJO", BOLD),
                paint("ACCIÓN", BOLD),
                paint("DESCRIPCIÓN", BOLD)
            );
            println!("  {}", "─".repeat(78));
            for k in keys {
                println!(
                    "  {:<16} {:<24} {}",
                    paint(&k.key, CYAN),
                    paint(&k.action, YELLOW),
                    k.description
                );
            }
            println!();
        }
        _ => {
            let status = crate::desktop::DesktopManager::get_status();
            let st = if status.running {
                paint("En ejecución", GREEN)
            } else {
                paint("Inactivo / Headless", DIM)
            };
            println!(
                "\n{} Diagnóstico de Sesión Gráfica Wayland:",
                paint("antOS Desktop ·", BOLD)
            );
            println!("  Estado:                {}", st);
            println!(
                "  Compositor:            {}",
                paint(&status.compositor_name, CYAN)
            );
            println!(
                "  WAYLAND_DISPLAY:       {}",
                paint(
                    status.wayland_display.as_deref().unwrap_or("ninguno"),
                    YELLOW
                )
            );
            println!("  Clientes de capa:      {}", status.active_clients_count);
            println!(
                "  Atajos registrados:    {} combinaciones globales\n",
                status.registered_hotkeys.len()
            );
            println!("  Uso:");
            println!("    antos desktop start       Arranca la sesión de escritorio");
            println!("    antos desktop keys        Muestra todos los atajos de teclado globales");
            println!("    antos desktop status      Diagnostica la sesión activa\n");
        }
    }
    Ok(())
}

// -------------------------------------------------------------------- barra
