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
                "\n{} Arrancando servicio efímero «{}»...",
                paint("⚡", BOLD),
                paint(svc, YELLOW)
            );
            let info = crate::service::start_service(svc, port, db, &ctx.state, &ctx.workspace)?;
            let origin = match info.backend {
                crate::service::ServiceBackend::External => {
                    "adoptado: ya escuchaba antes; antOS no lo arrancó y no lo parará"
                }
                crate::service::ServiceBackend::System => "binario de la máquina",
                crate::service::ServiceBackend::Nix => "nix shell nixpkgs#…",
                crate::service::ServiceBackend::Unknown => "desconocido",
            };
            println!(
                "  {} Servicio:      {}",
                paint("●", GREEN),
                paint(&info.name, BOLD)
            );
            println!(
                "  {} Escucha en:    127.0.0.1:{}  ({})",
                paint("●", GREEN),
                paint(&info.port.to_string(), YELLOW),
                paint(
                    match info.health {
                        crate::service::Health::Healthy => "sano",
                        crate::service::Health::Unhealthy => "sin respuesta",
                        crate::service::Health::Unknown => "sin sondear",
                    },
                    GREEN
                )
            );
            println!(
                "  {} Origen:        {} — {}",
                paint("●", GREEN),
                paint(info.backend.label(), BOLD),
                paint(origin, DIM)
            );
            if let Some(pid) = info.pid {
                println!("  {} PID:           {}", paint("●", GREEN), pid);
            }
            if let Some(cmd) = &info.command {
                println!("  {} Comando:       {}", paint("●", GREEN), paint(cmd, DIM));
            }
            println!(
                "  {} Variable .env: {}={}",
                paint("●", GREEN),
                paint(&info.env_var_key, BOLD),
                paint(&info.env_var_value, CYAN)
            );
            println!(
                "  {} Datos:         {}",
                paint("●", GREEN),
                paint(&info.data_dir, DIM)
            );
            if let Some(log) = &info.log_path {
                println!(
                    "  {} Log:           {}  (antos service logs {})\n",
                    paint("●", GREEN),
                    paint(log, DIM),
                    info.name
                );
            } else {
                println!();
            }
        }
        "down" | "stop" => {
            use crate::service::StopOutcome;
            let svc = args.get(1).ok_or_else(|| {
                anyhow::anyhow!(
                    "debes especificar el nombre del servicio (ej. antos service down postgres)"
                )
            })?;
            let msg = match crate::service::stop_service(svc, &ctx.state)? {
                StopOutcome::Terminated { pid, forced: false } => {
                    format!("detenido (PID {pid}). Los datos se conservan en $STATE/services/.")
                }
                StopOutcome::Terminated { pid, forced: true } => format!(
                    "detenido con SIGKILL (PID {pid} no atendió SIGTERM en 10 s). Los datos se conservan."
                ),
                StopOutcome::AlreadyGone => {
                    "ya no estaba corriendo; registro actualizado.".to_string()
                }
                StopOutcome::ExternalUnregistered => {
                    "era externo: se retira el registro; el proceso sigue porque antOS no lo arrancó."
                        .to_string()
                }
            };
            println!(
                "\n{} Servicio «{}» {msg}\n",
                paint("✓", GREEN),
                paint(svc, BOLD)
            );
        }
        "logs" | "log" => {
            let svc = args.get(1).ok_or_else(|| {
                anyhow::anyhow!("debes especificar el servicio (ej. antos service logs ollama)")
            })?;
            let n = args
                .get(2)
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(40);
            let tail = crate::service::service_logs(svc, &ctx.state, n)?;
            if tail.trim().is_empty() {
                println!("\n  (el log de «{svc}» está vacío)\n");
            } else {
                println!("\n{tail}\n");
            }
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
                paint("antOS · Servicios Locales Efímeros de Desarrollo", BOLD)
            );
            if services.is_empty() {
                println!("  No hay servicios efímeros registrados.");
                println!(
                    "  Arranca uno con: antos service up <ollama|postgres|redis|meilisearch>\n"
                );
            } else {
                // Se rellena ANTES de colorear: los códigos ANSI no ocupan
                // columnas pero `{:<n}` los cuenta y descuadra la tabla.
                let cell = |text: &str, width: usize, color: &str| {
                    let padded = format!("{text:<width$}");
                    if color.is_empty() {
                        padded
                    } else {
                        paint(&padded, color)
                    }
                };
                let bar = "  ├────────────────┼────────┼─────────────┼──────────┼───────┼────────┼──────────────────────────────────────────┤";
                println!("  ┌────────────────┬────────┬─────────────┬──────────┬───────┬────────┬──────────────────────────────────────────┐");
                println!(
                    "  │ {} │ {} │ {} │ {} │ {} │ {} │ {} │",
                    cell("SERVICIO", 14, BOLD),
                    cell("PUERTO", 6, BOLD),
                    cell("ESTADO", 11, BOLD),
                    cell("ORIGEN", 8, BOLD),
                    cell("SALUD", 5, BOLD),
                    cell("PID", 6, BOLD),
                    cell("VARIABLE DE ENTORNO (.env)", 40, BOLD)
                );
                println!("{bar}");
                for s in services {
                    let (st_txt, st_color) = match s.status.as_str() {
                        "running" => ("● running", GREEN),
                        "external" => ("● external", CYAN),
                        "unhealthy" => ("▲ unhealthy", YELLOW),
                        _ => ("○ stopped", DIM),
                    };
                    let hcolor = match s.health {
                        crate::service::Health::Healthy => GREEN,
                        crate::service::Health::Unhealthy => DIM,
                        crate::service::Health::Unknown => YELLOW,
                    };
                    let health = s.health.label();
                    let pid = s.pid.map(|p| p.to_string()).unwrap_or_else(|| "-".into());
                    let var = format!("{}={}", s.env_var_key, ellipsis(&s.env_var_value, 40));
                    println!(
                        "  │ {:<14} │ {:<6} │ {} │ {:<8} │ {} │ {:<6} │ {} │",
                        s.name,
                        s.port,
                        cell(st_txt, 11, st_color),
                        s.backend.label(),
                        cell(health, 5, hcolor),
                        pid,
                        cell(&ellipsis(&var, 40), 40, "")
                    );
                }
                println!("  └────────────────┴────────┴─────────────┴──────────┴───────┴────────┴──────────────────────────────────────────┘\n");
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
