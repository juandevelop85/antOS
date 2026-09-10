//! `antos autopilot`.
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

/// Argumentos de `antos autopilot start` (T31.13).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AutopilotStartArgs {
    pub interval: u64,
    pub auto_merge: bool,
}

/// Analiza los argumentos de `antos autopilot start` (T31.13).
fn parse_autopilot_start_args(args: &[String]) -> AutopilotStartArgs {
    let mut interval = 5u64;
    let mut auto_merge = false;
    let mut i = 1;
    while i < args.len() {
        if (args[i] == "--interval" || args[i] == "-i") && i + 1 < args.len() {
            if let Ok(v) = args[i + 1].parse::<u64>() {
                interval = v;
            }
            i += 2;
        } else if args[i] == "--auto-merge" || args[i] == "-m" {
            auto_merge = true;
            i += 1;
        } else {
            i += 1;
        }
    }
    AutopilotStartArgs {
        interval,
        auto_merge,
    }
}

pub fn cmd_autopilot(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "start" => {
            let AutopilotStartArgs {
                interval,
                auto_merge,
            } = parse_autopilot_start_args(args);

            let config = antos_protocol::AutopilotConfig {
                enabled: true,
                poll_interval_secs: interval,
                watch_paths: Vec::new(),
                auto_merge,
                target_branch: "master".to_string(),
            };

            println!(
                "\n{} Iniciando centinela continuo en segundo plano...",
                paint("antOS Autopilot ·", BOLD)
            );
            let st = crate::autopilot::AutopilotEngine::start(&ctx.state, &ctx.workspace, config)?;
            println!(
                "  Estado:                {}",
                paint("ACTIVO (Vigilando)", GREEN)
            );
            println!("  Intervalo de sondeo:   {}s", st.poll_interval_secs);
            println!("  Espacio de trabajo:    {}", st.workspace_path);
            println!(
                "  Incidentes detectados: {}\n",
                paint(
                    &st.active_incidents_count.to_string(),
                    if st.active_incidents_count > 0 {
                        YELLOW
                    } else {
                        CYAN
                    }
                )
            );
        }

        "stop" => {
            println!(
                "\n{} Deteniendo centinela...",
                paint("antOS Autopilot ·", BOLD)
            );
            let st = crate::autopilot::AutopilotEngine::stop(&ctx.state, &ctx.workspace)?;
            println!("  Estado:               {}", paint("DETENIDO", RED));
            println!("  Incidentes resueltos: {}\n", st.resolved_incidents_count);
        }

        "scan" => {
            println!(
                "\n{} Escaneando el workspace en busca de errores y fallos...",
                paint("antOS Autopilot ·", BOLD)
            );
            let incs =
                crate::autopilot::AutopilotEngine::scan_workspace(&ctx.state, &ctx.workspace)?;
            if incs.is_empty() {
                println!(
                    "  ✓ {} Repositorio limpio, cero incidencias.",
                    paint("OK", GREEN)
                );
            } else {
                println!(
                    "  ⚠ Detectadas {} incidencias con propuestas generadas:",
                    paint(&incs.len().to_string(), YELLOW)
                );
                for inc in incs {
                    println!(
                        "    • [{}] {} en «{}» — {}",
                        paint(&inc.id, BOLD),
                        paint(&inc.incident_type, CYAN),
                        inc.file_path,
                        inc.error_message
                    );
                    if let Some(ref prop) = inc.fix_proposal {
                        println!(
                            "      Rama: {} | QA: {}",
                            prop.branch,
                            prop.test_output.lines().next().unwrap_or("")
                        );
                    }
                }
            }
            println!();
        }

        "list" | "log" | "incidents" => {
            let incs = crate::autopilot::AutopilotEngine::list_incidents(&ctx.state)?;
            if incs.is_empty() {
                println!(
                    "\n{} No hay incidencias registradas.",
                    paint("antOS Autopilot ·", BOLD)
                );
            } else {
                println!(
                    "\n{} Historial de Incidencias ({}):",
                    paint("antOS Autopilot ·", BOLD),
                    incs.len()
                );
                for inc in incs {
                    let st_badge = match inc.status.as_str() {
                        "resolved" => paint("RESUELTO", GREEN),
                        "ready_for_approval" => paint("PENDIENTE", YELLOW),
                        "dismissed" => paint("DESCARTADO", DIM),
                        other => paint(other, CYAN),
                    };
                    println!(
                        "  • [{}] {} en «{}» [{}]",
                        paint(&inc.id, BOLD),
                        paint(&inc.incident_type, CYAN),
                        inc.file_path,
                        st_badge
                    );
                    println!("    Detalle: {}", inc.error_message);
                    if let Some(ref prop) = inc.fix_proposal {
                        println!("    Fix:     {}", prop.title);
                    }
                }
            }
            println!();
        }

        "approve" | "merge" => {
            let incident_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Uso: antos autopilot approve <incident_id>"))?;
            println!(
                "\n{} Aprobando propuesta para incidente «{}»...",
                paint("antOS Autopilot ·", BOLD),
                incident_id
            );
            let inc = crate::autopilot::AutopilotEngine::resolve_incident(
                &ctx.state,
                &ctx.workspace,
                incident_id,
                true,
            )?;
            println!(
                "  ✓ {} Corrección aplicada en {}",
                paint("APROBADO", GREEN),
                inc.file_path
            );
            println!("  Estado: {}\n", paint(&inc.status, BOLD));
        }

        "reject" | "dismiss" => {
            let incident_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Uso: antos autopilot reject <incident_id>"))?;
            println!(
                "\n{} Descartando propuesta para incidente «{}»...",
                paint("antOS Autopilot ·", BOLD),
                incident_id
            );
            let _inc = crate::autopilot::AutopilotEngine::resolve_incident(
                &ctx.state,
                &ctx.workspace,
                incident_id,
                false,
            )?;
            println!("  ✓ {} Incidente descartado.\n", paint("DESCARTADO", DIM));
        }

        _ => {
            let st = crate::autopilot::AutopilotEngine::status(&ctx.state, &ctx.workspace)?;
            let status_badge = if st.active {
                paint("ACTIVO (Vigilando)", GREEN)
            } else {
                paint("DETENIDO", RED)
            };
            println!(
                "\n{} Estado del Centinela Autónomo:",
                paint("antOS Autopilot ·", BOLD)
            );
            println!("  • Estado:                 {}", status_badge);
            println!("  • Espacio de Trabajo:     {}", st.workspace_path);
            println!("  • Intervalo de Sondeo:    {}s", st.poll_interval_secs);
            println!(
                "  • Incidencias Activas:    {}",
                paint(
                    &st.active_incidents_count.to_string(),
                    if st.active_incidents_count > 0 {
                        YELLOW
                    } else {
                        GREEN
                    }
                )
            );
            println!(
                "  • Incidencias Resueltas:  {}",
                st.resolved_incidents_count
            );
            if let Some(ts) = st.last_scan_timestamp {
                println!("  • Último Escaneo:         {}", ts);
            }
            println!("\n  Uso:");
            println!("    antos autopilot start [--interval <secs>] [--auto-merge]  Arranca el centinela");
            println!("    antos autopilot stop                                      Detiene el centinela");
            println!("    antos autopilot status                                    Muestra métricas y estado");
            println!("    antos autopilot scan                                      Escaneo manual reactivo");
            println!("    antos autopilot list                                      Historial de incidentes");
            println!("    antos autopilot approve <incident_id>                     Aprueba y aplica fix");
            println!("    antos autopilot reject <incident_id>                      Descarta la propuesta\n");
        }
    }
    Ok(())
}

// ----------------------------------------------------- web console (T16.4)

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn args_of(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // -- antos autopilot start -----------------------------------------------

    #[test]
    fn test_parse_autopilot_start_args_reads_both_flags() {
        let args = args_of(&["start", "--interval", "30", "--auto-merge"]);
        let parsed = parse_autopilot_start_args(&args);
        assert_eq!(
            parsed,
            AutopilotStartArgs {
                interval: 30,
                auto_merge: true,
            }
        );
    }

    #[test]
    fn test_parse_autopilot_start_args_keeps_default_interval_on_garbage_input() {
        let args = args_of(&["start", "--interval", "not-a-number"]);
        let parsed = parse_autopilot_start_args(&args);
        assert_eq!(parsed.interval, 5);
        assert!(!parsed.auto_merge);
    }

    #[test]
    fn test_parse_autopilot_start_args_ignores_a_trailing_flag_with_no_value() {
        let args = args_of(&["start", "--interval"]);
        let parsed = parse_autopilot_start_args(&args);
        assert_eq!(parsed.interval, 5);
    }
}
