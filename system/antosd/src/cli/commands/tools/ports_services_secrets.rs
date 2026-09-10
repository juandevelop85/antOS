//! `antos ports`, `antos services` y `antos secrets`.
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

pub fn cmd_ports(args: &[String]) -> Result<()> {
    let filtro = args.first().and_then(|a| a.parse::<u16>().ok());
    let puertos = crate::net::diagnose_ports(filtro)?;

    println!(
        "\n{}",
        paint("antOS · Diagnóstico de Puertos y Procesos", BOLD)
    );
    if puertos.is_empty() {
        if let Some(p) = filtro {
            println!(
                "  El puerto {} está libre.\n",
                paint(&format!(":{p}"), GREEN)
            );
        } else {
            println!("  No se detectaron puertos de desarrollo en escucha activa.\n");
        }
        return Ok(());
    }

    println!(
        "\n  {:<8} {:<8} {:<16} {:<32} CARPETA",
        paint("PUERTO", DIM),
        paint("PID", DIM),
        paint("PROCESO", DIM),
        paint("COMANDO", DIM)
    );
    println!("  {}", "─".repeat(88));

    for p in &puertos {
        let puerto_fmt = format!(":{}", p.port);
        let dir_fmt = p.working_dir.as_deref().unwrap_or("-");
        let cmd_recortado = if p.command.len() > 30 {
            format!("{}…", &p.command[..29])
        } else {
            p.command.clone()
        };

        println!(
            "  {:<8} {:<8} {:<16} {:<32} {}",
            paint(&puerto_fmt, GREEN),
            paint(&p.pid.to_string(), YELLOW),
            p.process_name,
            cmd_recortado,
            paint(dir_fmt, DIM)
        );
    }

    println!("  {}", "─".repeat(88));
    println!("  Total: {} proceso(s) en escucha\n", puertos.len());
    Ok(())
}

pub fn cmd_services(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    match sub {
        "up" | "start" => {
            let svc = args.get(1).ok_or_else(|| {
                anyhow::anyhow!(
                    "debes especificar el nombre del servicio (ej. antos service up postgres)"
                )
            })?;
            let port = args.get(2).and_then(|p| p.parse::<u16>().ok());
            let db = args.get(3).map(String::as_str);

            println!(
                "\n{} Aprovisionando servicio efímero «{}»...",
                paint("⚡", BOLD),
                paint(svc, YELLOW)
            );
            let info = crate::service::start_service(svc, port, db, &ctx.state, &ctx.workspace)?;
            println!(
                "  {} Servicio:      {}",
                paint("●", GREEN),
                paint(&info.name, BOLD)
            );
            println!(
                "  {} Puerto:        {}",
                paint("●", GREEN),
                paint(&info.port.to_string(), YELLOW)
            );
            println!(
                "  {} Estado:        {}",
                paint("●", GREEN),
                paint(&info.status, GREEN)
            );
            println!(
                "  {} Variable .env: {}={}",
                paint("●", GREEN),
                paint(&info.env_var_key, BOLD),
                paint(&info.env_var_value, CYAN)
            );
            println!(
                "  {} Almacenamiento: {}\n",
                paint("●", GREEN),
                paint(&info.data_dir, DIM)
            );
        }
        "down" | "stop" => {
            let svc = args.get(1).ok_or_else(|| {
                anyhow::anyhow!(
                    "debes especificar el nombre del servicio (ej. antos service down postgres)"
                )
            })?;
            crate::service::stop_service(svc, &ctx.state)?;
            println!(
                "\n{} Servicio «{}» detenido y limpiado.\n",
                paint("✓", GREEN),
                paint(svc, BOLD)
            );
        }
        _ => {
            let svc_filter = if sub != "status" && sub != "list" {
                Some(sub)
            } else {
                args.get(1).map(String::as_str)
            };

            let services = crate::service::get_service_status(svc_filter, &ctx.state)?;
            println!(
                "\n{}",
                paint(
                    "antOS · Servicios Locales Efímeros de Desarrollo (T5.1)",
                    BOLD
                )
            );
            if services.is_empty() {
                println!("  No hay servicios efímeros aprovisionados.");
                println!(
                    "  Inicia uno con: antos service up <postgres|redis|mariadb|meilisearch>\n"
                );
            } else {
                println!("  ┌────────────────┬────────┬───────────┬─────────────────────────────────────────────────────────┐");
                println!(
                    "  │ {:<14} │ {:<6} │ {:<9} │ {:<55} │",
                    paint("SERVICIO", BOLD),
                    paint("PUERTO", BOLD),
                    paint("ESTADO", BOLD),
                    paint("VARIABLE DE ENTORNO (.env)", BOLD)
                );
                println!("  ├────────────────┼────────┼───────────┼─────────────────────────────────────────────────────────┤");
                for s in services {
                    let st_fmt = if s.status == "running" {
                        paint("● running", GREEN)
                    } else {
                        paint("○ stopped", DIM)
                    };
                    println!(
                        "  │ {:<14} │ {:<6} │ {:<20} │ {}={} │",
                        s.name,
                        s.port,
                        st_fmt,
                        paint(&s.env_var_key, BOLD),
                        ellipsis(&s.env_var_value, 38)
                    );
                }
                println!("  └────────────────┴────────┴───────────┴─────────────────────────────────────────────────────────┘\n");
            }
        }
    }
    Ok(())
}

pub fn cmd_secrets(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    let grants = Grants::load(&ctx.grants_path())?;

    match sub {
        "set" => {
            let key = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("uso: antos secret set <CLAVE> <VALOR>"))?;
            let val = args
                .get(2)
                .ok_or_else(|| anyhow::anyhow!("uso: antos secret set <CLAVE> <VALOR>"))?;
            crate::vault::set_secret(&ctx.state, key, val)?;
            println!(
                "\n{} Secreto «{}» almacenado de forma segura en la bóveda de antOS.\n",
                paint("✓", GREEN),
                paint(key, BOLD)
            );
        }
        "get" => {
            let key = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("uso: antos secret get <CLAVE>"))?;
            match crate::vault::get_secret(&ctx.state, key, &grants) {
                Ok(Some(v)) => {
                    println!("\n{} {key} = {}\n", paint("🔑", BOLD), paint(&v, GREEN));
                }
                Ok(None) => {
                    println!(
                        "\n{} El secreto «{key}» no existe en la bóveda.\n",
                        paint("○", DIM)
                    );
                }
                Err(e) => {
                    println!(
                        "\n{} {e}\n",
                        paint("🛡️ Cero Autoridad Ambiental (Bloqueado):", RED)
                    );
                }
            }
        }
        _ => {
            let list = crate::vault::list_secrets(&ctx.state)?;
            let active_grants = grants.list_active();

            println!(
                "\n{}",
                paint(
                    "antOS · Bóveda de Secretos y Blindaje Zero Environmental Authority (T5.2)",
                    BOLD
                )
            );

            // Concesiones activas
            println!("  {}", paint("● CONCESIONES ACTIVAS", BOLD));
            if active_grants.is_empty() {
                println!(
                    "    {} No hay concesiones activas. Blindaje al 100%.",
                    paint("○", DIM)
                );
            } else {
                for g in active_grants {
                    let mins_left = ((g.expires_at - chrono::Local::now().timestamp()) / 60).max(1);
                    let reason_str = g
                        .reason
                        .as_deref()
                        .map(|r| format!(" (motivo: «{r}»)"))
                        .unwrap_or_default();
                    println!(
                        "    {} {:<20} expira en {:>2} min{reason_str}",
                        paint("●", GREEN),
                        paint(&g.cap, BOLD),
                        paint(&mins_left.to_string(), YELLOW)
                    );
                }
            }
            println!();

            // Secretos almacenados
            println!(
                "  {}",
                paint("● SECRETOS EN BÓVEDA ($STATE/vault.json)", BOLD)
            );
            if list.is_empty() {
                println!("    No hay secretos en la bóveda.");
                println!("    Guarda uno con: antos secret set <CLAVE> <VALOR>\n");
            } else {
                println!("    ┌──────────────────────────────┬──────────────┬────────────────────────────┐");
                println!(
                    "    │ {:<28} │ {:<12} │ {:<26} │",
                    paint("CLAVE", BOLD),
                    paint("LONGITUD", BOLD),
                    paint("ESTADO DE ACCESO", BOLD)
                );
                println!("    ├──────────────────────────────┼──────────────┼────────────────────────────┤");
                for s in list {
                    let has_grant = grants.is_granted("secret.read")
                        || grants.is_granted(&format!("secret.{}", s.key));
                    let acc_str = if has_grant {
                        paint("🔓 Concedido", GREEN)
                    } else {
                        paint("🔒 Protegido (Grant req)", YELLOW)
                    };
                    println!(
                        "    │ {:<28} │ {:<12} │ {:<37} │",
                        paint(&s.key, BOLD),
                        format!("{} bytes", s.length),
                        acc_str
                    );
                }
                println!("    └──────────────────────────────┴──────────────┴────────────────────────────┘\n");
            }
        }
    }
    Ok(())
}
