//! `antos ebpf` (sentinela simulado, T31.14) y `antos profile`.
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

/// Etiqueta honesta del backend de eventos del sentinela eBPF (T31.14): el
/// panel de `antos ebpf` es hoy un ring buffer de espacio de usuario, nunca
/// vigilancia real del kernel — ver la cabecera de `crate::ebpf`.
fn ebpf_backend_badge(backend: antos_protocol::EbpfBackend) -> String {
    match backend {
        antos_protocol::EbpfBackend::Simulated => paint(
            "○ SIMULADO (ring buffer en espacio de usuario, sin BPF real)",
            YELLOW,
        ),
        antos_protocol::EbpfBackend::LinuxBpf => paint("● KERNEL LSM ACTIVO (BPF real)", GREEN),
    }
}

pub fn cmd_ebpf(ctx: &Ctx, args: &[String]) -> Result<()> {
    let engine = crate::ebpf::EbpfSentinelEngine::global();
    let sub = args.first().map(String::as_str);

    match sub {
        Some("status" | "info" | "estado") => {
            let status = engine.status()?;
            println!(
                "\n{}",
                paint("antOS · Supervisor Kernel eBPF LSM (T11.1)", BOLD)
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            println!(
                "  Backend de eventos:      {}",
                ebpf_backend_badge(status.backend)
            );
            println!(
                "  LSM `bpf` en el kernel:  {}",
                if status.lsm_enabled {
                    paint("disponible en el host (no usado por este backend)", DIM)
                } else {
                    paint("no anunciado por el host", DIM)
                }
            );
            println!("  Sondas activas ({}):", status.active_probes.len());
            for probe in status.active_probes {
                println!("    • {}", paint(&probe, CYAN));
            }
            println!(
                "  Capacidad del ring buffer: {} entradas (uso: {})",
                status.ring_buffer_capacity, status.ring_buffer_utilization
            );
            println!(
                "  Eventos capturados:        {}",
                paint(&status.total_events_captured.to_string(), BOLD)
            );
            println!(
                "  Violaciones bloqueadas:    {}\n",
                paint(
                    &status.total_violations_blocked.to_string(),
                    if status.total_violations_blocked > 0 {
                        RED
                    } else {
                        GREEN
                    }
                )
            );
        }
        Some("trace" | "traza") => {
            let pid_opt = args.get(1).and_then(|p| p.parse::<u32>().ok());
            let events = match pid_opt {
                Some(pid) => engine.trace_pid(pid),
                None => engine.get_audit_log(25),
            };
            println!(
                "\n{} Traza en vivo de syscalls y eventos de seguridad",
                paint("antOS eBPF ·", BOLD)
            );
            if let Some(p) = pid_opt {
                println!("  Filtro por PID: {}\n", paint(&p.to_string(), YELLOW));
            } else {
                println!("  Mostrando los últimos {} eventos:\n", events.len());
            }

            if events.is_empty() {
                println!("  (no hay eventos en el ring buffer)\n");
            } else {
                for ev in events {
                    let mark = match ev.action_taken {
                        antos_protocol::EbpfSecurityAction::Allowed => paint("✓ ALLOW", GREEN),
                        antos_protocol::EbpfSecurityAction::Blocked => paint("⛔ BLOCK", RED),
                        antos_protocol::EbpfSecurityAction::Audited => paint("👁 AUDIT", YELLOW),
                    };
                    println!(
                        "  {} [{}] PID {}:{} ➔ {} ({:?})",
                        mark,
                        paint(&ev.id, DIM),
                        ev.pid,
                        paint(&ev.comm, BOLD),
                        paint(&ev.target_resource, YELLOW),
                        ev.hook
                    );
                    if let Some(ref r) = ev.violation_reason {
                        println!("      └─ {}", paint(r, DIM));
                    }
                }
                println!();
            }
        }
        Some("audit" | "log" | "registro") => {
            let limit = args
                .get(1)
                .and_then(|l| l.parse::<usize>().ok())
                .unwrap_or(20);
            let events = engine.get_audit_log(limit);
            println!(
                "\n{} Registro de auditoría eBPF (últimos {} eventos)\n",
                paint("antOS eBPF ·", BOLD),
                events.len()
            );
            if events.is_empty() {
                println!("  (registro vacío)\n");
            } else {
                for ev in events {
                    let mark = match ev.action_taken {
                        antos_protocol::EbpfSecurityAction::Allowed => paint("✓", GREEN),
                        antos_protocol::EbpfSecurityAction::Blocked => paint("⛔", RED),
                        antos_protocol::EbpfSecurityAction::Audited => paint("👁", YELLOW),
                    };
                    println!(
                        "  {} [{}] {:<18} PID {}:{} ➔ {}",
                        mark,
                        paint(&ev.id, DIM),
                        format!("{:?}", ev.hook),
                        ev.pid,
                        paint(&ev.comm, BOLD),
                        ev.target_resource
                    );
                }
                println!();
            }
        }
        Some("simulate" | "simula" | "test") => {
            let kind_str = args.get(1).map(String::as_str).unwrap_or("file");
            let hook = match kind_str {
                "socket" | "net" | "red" => antos_protocol::EbpfHookKind::SocketConnect,
                "bprm" | "exec" => antos_protocol::EbpfHookKind::BprmCheckSecurity,
                "syscall" => antos_protocol::EbpfHookKind::SyscallTrace,
                _ => antos_protocol::EbpfHookKind::FileOpen,
            };
            let target = args
                .get(2)
                .map(String::as_str)
                .unwrap_or_else(|| match hook {
                    antos_protocol::EbpfHookKind::SocketConnect => "192.168.1.50:4444",
                    antos_protocol::EbpfHookKind::FileOpen => "/etc/shadow",
                    antos_protocol::EbpfHookKind::BprmCheckSecurity => "/bin/nc",
                    antos_protocol::EbpfHookKind::SyscallTrace => "ptrace",
                });

            let ev = engine.simulate_violation(hook, target);
            println!(
                "\n{} Simulación de intento de evasión de sandbox",
                paint("antOS eBPF ·", BOLD)
            );
            println!("  Hook interceptado: {:?}", ev.hook);
            println!(
                "  Recurso objetivo:  {}",
                paint(&ev.target_resource, YELLOW)
            );
            println!("  Acción del kernel: {}", paint("⛔ BLOQUEADO", RED));
            println!(
                "  Alerta disparada:  {} Se envió notificación prioritaria a la bandeja Wayland.\n",
                paint("✓", GREEN)
            );
        }
        _ => {
            let status = engine.status()?;
            println!(
                "\n{}",
                paint("antOS · Supervisor Kernel eBPF LSM (T11.1)", BOLD)
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            println!(
                "  Backend:            {}",
                ebpf_backend_badge(status.backend)
            );
            println!("  Sondas activas:     {}", status.active_probes.len());
            println!(
                "  Eventos capturados: {}",
                paint(&status.total_events_captured.to_string(), BOLD)
            );
            println!(
                "  Bloqueos evasión:   {}\n",
                paint(
                    &status.total_violations_blocked.to_string(),
                    if status.total_violations_blocked > 0 {
                        RED
                    } else {
                        GREEN
                    }
                )
            );

            println!("  Subcomandos disponibles:");
            println!(
                "    • antos ebpf status            Diagnóstico de sondas y soporte de kernel"
            );
            println!(
                "    • antos ebpf trace [pid]       Traza de llamadas al sistema en tiempo real"
            );
            println!("    • antos ebpf audit [limit]     Registro de auditoría del ring buffer");
            println!(
                "    • antos ebpf simulate <tipo>   Simula evasión (file|socket|bprm) y alerta\n"
            );
        }
    }
    Ok(())
}

// ------------------------------------------------------------------- profile

pub fn cmd_profile(ctx: &Ctx, args: &[String]) -> Result<()> {
    let engine = crate::profiler::ProfilerEngine::global();
    let sub = args.first().map(String::as_str);

    match sub {
        Some("run" | "ejecutar") => {
            let command = if args.len() > 1 {
                args[1..].join(" ")
            } else {
                "cargo test".to_string()
            };
            println!(
                "\n{} Ejecutando perfilado continuo para: {}",
                paint("antOS Profiler ·", BOLD),
                paint(&command, YELLOW)
            );
            let report = engine.run_and_profile(&ctx.workspace, &command)?;

            let peak_mb = report.peak_memory_bytes as f64 / (1024.0 * 1024.0);
            let status_badge = if report.exit_code == 0 {
                paint("EXIT 0 (Éxito)", GREEN)
            } else {
                paint(&format!("EXIT {}", report.exit_code), RED)
            };

            println!("\n{}", paint("Resultado del Perfilado:", BOLD));
            println!("  Estado del comando:       {}", status_badge);
            if !report.metrics_are_real {
                println!(
                    "  {} `getrusage` falló en este host: las métricas de abajo son una \
                     estimación de respaldo a partir de la duración, no una medición real \
                     (T31.14).",
                    paint("⚠", YELLOW)
                );
            }
            println!(
                "  Duración de Wall-Clock:   {} ms",
                paint(&report.duration_ms.to_string(), BOLD)
            );
            println!(
                "  Tiempo de CPU:            {} ms usuario, {} ms sistema",
                report.cpu_user_ms, report.cpu_sys_ms
            );
            println!(
                "  Memoria Pico (RSS):       {} MB",
                paint(&format!("{peak_mb:.2}"), CYAN)
            );
            println!("  Fallas de Página (Faults): {}\n", report.page_faults);

            if !report.hotspots.is_empty() {
                println!(
                    "{}",
                    paint(
                        "  Puntos Calientes Estimados (heurística por tipo de comando, no muestreo real):",
                        BOLD
                    )
                );
                for h in report.hotspots {
                    println!(
                        "    • {:<32} CPU: {:>4.1}% | Mem: {:>4.1}% ({} muestras)",
                        paint(&h.name, YELLOW),
                        h.percentage_cpu,
                        h.percentage_memory,
                        h.calls_or_samples
                    );
                }
                println!();
            }

            if !report.suggestions.is_empty() {
                println!(
                    "{}",
                    paint(
                        "  Recomendaciones de Optimización para Agentes Coder / QA:",
                        BOLD
                    )
                );
                for s in report.suggestions {
                    let impact_color = if s.potential_impact.contains("Alto") {
                        RED
                    } else {
                        YELLOW
                    };
                    println!(
                        "    ★ [{}] {}",
                        paint(&s.potential_impact, impact_color),
                        paint(&s.title, BOLD)
                    );
                    println!("      └─ {}", paint(&s.description, DIM));
                    if let Some(target) = s.target_symbol_or_path {
                        println!("         Objetivo: {}", paint(&target, CYAN));
                    }
                }
                println!();
            }
        }
        Some("top" | "hotspots" | "cuellos") => {
            let (hotspots, _) = engine.analyze_aggregate(&ctx.workspace);
            println!(
                "\n{}",
                paint("antOS Profiler · Top Cuellos de Botella (Hotspots)", BOLD)
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            if hotspots.is_empty() {
                println!(
                    "  (no hay hotspots registrados; ejecuta «antos profile run <comando>»)\n"
                );
            } else {
                for (idx, h) in hotspots.iter().enumerate() {
                    println!(
                        "  {}. {:<32} CPU: {:>5.1}% | Mem: {:>5.1}% ({} llamadas)",
                        idx + 1,
                        paint(&h.name, YELLOW),
                        h.percentage_cpu,
                        h.percentage_memory,
                        h.calls_or_samples
                    );
                }
                println!();
            }
        }
        Some("analyze" | "analiza" | "sugerencias") => {
            let (hotspots, suggestions) = engine.analyze_aggregate(&ctx.workspace);
            println!(
                "\n{}",
                paint("antOS Profiler · Análisis y Recomendaciones Técnicas", BOLD)
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            if suggestions.is_empty() {
                println!(
                    "  (sin recomendaciones activas; ejecuta «antos profile run <comando>»)\n"
                );
            } else {
                println!("  Puntos calientes consolidados: {}\n", hotspots.len());
                for s in suggestions {
                    let impact_color = if s.potential_impact.contains("Alto") {
                        RED
                    } else {
                        YELLOW
                    };
                    println!(
                        "  ★ [{}] {}",
                        paint(&s.potential_impact, impact_color),
                        paint(&s.title, BOLD)
                    );
                    println!("    └─ {}", paint(&s.description, DIM));
                }
                println!();
            }
        }
        Some("list" | "reports" | "reportes" | "historial") => {
            let reports = engine.load_reports(&ctx.workspace);
            println!(
                "\n{}",
                paint(
                    "antOS Profiler · Histórico de Reportes de Rendimiento",
                    BOLD
                )
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            if reports.is_empty() {
                println!("  (no hay reportes guardados)\n");
            } else {
                for r in reports {
                    let peak_mb = r.peak_memory_bytes as f64 / (1024.0 * 1024.0);
                    println!(
                        "  • [{}] «{}» — {} ms | {:.1} MB RSS (código {})",
                        paint(&r.id, DIM),
                        paint(&r.command, BOLD),
                        r.duration_ms,
                        peak_mb,
                        r.exit_code
                    );
                }
                println!();
            }
        }
        _ => {
            let reports = engine.load_reports(&ctx.workspace);
            println!(
                "\n{}",
                paint("antOS · Profiler Continuo de Runtime (T11.2)", BOLD)
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );
            println!(
                "  Reportes registrados:   {}",
                paint(&reports.len().to_string(), BOLD)
            );

            println!("  Subcomandos disponibles:");
            println!(
                "    • antos profile run <cmd>      Ejecuta y perfila un comando en tiempo real"
            );
            println!("    • antos profile top            Lista los principales puntos calientes (hotspots)");
            println!(
                "    • antos profile analyze        Sintetiza recomendaciones para Coder y QA"
            );
            println!(
                "    • antos profile list           Muestra el histórico de reportes guardados\n"
            );
        }
    }
    Ok(())
}
