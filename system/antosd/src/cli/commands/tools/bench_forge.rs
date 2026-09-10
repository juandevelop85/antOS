//! `antos bench`, `antos issue` y `antos pr`.
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

pub fn cmd_bench(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("run");
    match sub {
        "diff" | "compare" => {
            let mut against = None;
            let mut threshold = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--against" | "-a" => {
                        if let Some(a) = args.get(i + 1) {
                            against = Some(a.as_str());
                            i += 1;
                        }
                    }
                    "--threshold" | "-t" => {
                        if let Some(t) = args.get(i + 1) {
                            threshold = t.parse::<f64>().ok();
                            i += 1;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }

            println!(
                "\n{}",
                paint(
                    "⚡ antOS Bench Diff · Comparador de Rendimiento en Worktrees (T21.1)",
                    BOLD
                )
            );
            println!(
                "  Comparando contra rama base: «{}» (umbral regresión: {:.1}%)",
                paint(against.unwrap_or("master"), CYAN),
                threshold.unwrap_or(15.0)
            );

            let report = crate::bench::BenchEngine::compare_benchmark(
                &ctx.workspace,
                &ctx.state,
                against,
                threshold,
            )?;

            println!("\n  {} [{}]", paint("ID Comparación:", BOLD), report.id);
            println!(
                "  {} vs {}",
                paint(&report.base_branch, DIM),
                paint(&report.target_branch, CYAN)
            );
            println!("  {}", "─".repeat(84));
            println!(
                "  {:<26} {:<14} {:<14} {:<12} {}",
                paint("BENCHMARK", BOLD),
                paint("BASE (MEDIA)", BOLD),
                paint("OBJETIVO", BOLD),
                paint("VARIACIÓN Δ", BOLD),
                paint("ESTADO", BOLD)
            );
            println!("  {}", "─".repeat(84));

            for c in &report.comparisons {
                let delta_str = format!("{:+.2}%", c.delta_pct);
                let color_delta = if c.is_regression {
                    paint(&delta_str, RED)
                } else if c.delta_pct < -5.0 {
                    paint(&delta_str, GREEN)
                } else {
                    paint(&delta_str, DIM)
                };

                let status_badge = if c.is_regression {
                    paint("REGRESIÓN", RED)
                } else {
                    paint("ÓPTIMO", GREEN)
                };

                println!(
                    "  {:<26} {:<14} {:<14} {:<12} {}",
                    paint(&c.name, CYAN),
                    format!("{} ns", c.base_mean_ns),
                    format!("{} ns", c.target_mean_ns),
                    color_delta,
                    status_badge
                );
            }
            println!("  {}", "─".repeat(84));
            println!(
                "\n  {} {}",
                paint("Veredicto Auditor antFlow:", BOLD),
                report.auditor_verdict
            );
            println!();
            Ok(())
        }
        "history" | "historial" | "hist" => {
            let history = crate::bench::BenchEngine::load_history(&ctx.state);
            println!(
                "\n{}",
                paint(
                    "📈 antOS Bench · Historial de Rendimiento Continuo (T21.1)",
                    BOLD
                )
            );
            if history.is_empty() {
                println!("  No hay registros de benchmarks previos en .antos/bench_history.json\n");
                println!("  Ejecuta uno con: antos bench\n");
                return Ok(());
            }

            println!(
                "  {:<22} {:<16} {:<10} {:<14} {}",
                paint("ID", BOLD),
                paint("RAMA", BOLD),
                paint("SUITE", BOLD),
                paint("MÉTRICAS", BOLD),
                paint("DURACIÓN", BOLD)
            );
            println!("  {}", "─".repeat(75));
            for h in &history {
                println!(
                    "  {:<22} {:<16} {:<10} {:<14} {} ms",
                    paint(&h.id, CYAN),
                    h.branch,
                    h.suite_name,
                    format!("{} pruebas", h.metrics.len()),
                    h.total_duration_ms
                );
            }
            println!("\n  Total: {} corridas registradas.\n", history.len());
            Ok(())
        }
        _ => {
            let target = if sub != "run" {
                Some(sub)
            } else {
                args.get(1).map(String::as_str)
            };
            println!(
                "\n{}",
                paint(
                    "⚡ antOS Continuous Benchmarking · Ejecución de Suite (T21.1)",
                    BOLD
                )
            );
            if let Some(t) = target {
                println!("  Objetivo específico: «{}»", paint(t, CYAN));
            }

            let report =
                crate::bench::BenchEngine::run_benchmark(&ctx.workspace, &ctx.state, target)?;

            println!("  {} [{}]", paint("ID Ejecución:", BOLD), report.id);
            println!(
                "  {} {} ({})",
                paint("Contexto Git:", BOLD),
                report.branch,
                report.commit.as_deref().unwrap_or("—")
            );
            println!(
                "  {} {} ms\n",
                paint("Tiempo Total:", BOLD),
                report.total_duration_ms
            );

            println!(
                "  {:<26} {:<12} {:<12} {:<12} {:<14}",
                paint("MÉTRICA", BOLD),
                paint("MEDIA", BOLD),
                paint("P95", BOLD),
                paint("P99", BOLD),
                paint("RENDIMIENTO", BOLD)
            );
            println!("  {}", "─".repeat(78));

            for m in &report.metrics {
                println!(
                    "  {:<26} {:<12} {:<12} {:<12} {:<14}",
                    paint(&m.name, CYAN),
                    format!("{} ns", m.mean_ns),
                    format!("{} ns", m.p95_ns),
                    format!("{} ns", m.p99_ns),
                    format!("{:.0} ops/s", m.ops_per_sec)
                );
            }
            println!("  {}\n", "─".repeat(78));
            Ok(())
        }
    }
}

// ------------------------------------------------------------------ forge: issues & pr (T21.2)

pub fn cmd_issue(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    match sub {
        "list" | "ls" => {
            println!(
                "\n{}",
                paint(
                    "🐙 antOS Git Forge · Issues Abiertos en Repositorio Remoto (T21.2)",
                    BOLD
                )
            );
            let issues = crate::forge::ForgeEngine::list_issues(&ctx.workspace, &ctx.state)?;
            if issues.is_empty() {
                println!("  No se encontraron issues abiertos en el repositorio remoto.\n");
                return Ok(());
            }

            println!(
                "  {:<8} {:<42} {:<16} {}",
                paint("NUM", BOLD),
                paint("TÍTULO", BOLD),
                paint("AUTOR", BOLD),
                paint("ETIQUETAS", BOLD)
            );
            println!("  {}", "─".repeat(84));

            for issue in &issues {
                let tags = if issue.labels.is_empty() {
                    "—".to_string()
                } else {
                    issue.labels.join(", ")
                };
                println!(
                    "  #{:<7} {:<42} @{:<15} {}",
                    paint(&issue.number.to_string(), CYAN),
                    if issue.title.len() > 40 {
                        format!("{}...", &issue.title[..37])
                    } else {
                        issue.title.clone()
                    },
                    issue.author,
                    paint(&tags, DIM)
                );
            }
            println!("  {}", "─".repeat(84));
            println!(
                "\n  Importa un issue a ticket técnico local con: antos issue import <numero>\n"
            );
            Ok(())
        }
        "import" | "sync" => {
            let id = args.get(1).map(String::as_str).unwrap_or("42");
            println!(
                "\n{}",
                paint("📥 antOS Git Forge · Importando Issue Remoto (T21.2)", BOLD)
            );

            let (ticket_id, path, title) =
                crate::forge::ForgeEngine::import_issue(&ctx.workspace, &ctx.state, id)?;

            println!("  Ticket Creado: [{}]", paint(&ticket_id, CYAN));
            println!("  Título:        {}", title);
            println!(
                "  Ubicación:     {}",
                paint(&path.display().to_string(), DIM)
            );
            println!(
                "\n  El issue ha sido estructurado con criterios de aceptación en docs/tickets/."
            );
            println!(
                "  Despáchalo al equipo con: antos agent run {}\n",
                ticket_id
            );
            Ok(())
        }
        _ => {
            println!("\nUso:");
            println!("  antos issue list                  Lista issues abiertos en GitHub/GitLab");
            println!(
                "  antos issue import <id_o_url>     Importa issue y genera ticket técnico local\n"
            );
            Ok(())
        }
    }
}

pub fn cmd_pr(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    match sub {
        "create" | "push" => {
            let mut title = None;
            let mut base = None;
            let mut draft = false;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--title" | "-t" => {
                        if let Some(t) = args.get(i + 1) {
                            title = Some(t.as_str());
                            i += 1;
                        }
                    }
                    "--base" | "-b" => {
                        if let Some(b) = args.get(i + 1) {
                            base = Some(b.as_str());
                            i += 1;
                        }
                    }
                    "--draft" | "-d" => {
                        draft = true;
                    }
                    _ => {}
                }
                i += 1;
            }

            println!(
                "\n{}",
                paint(
                    "🚀 antOS Git Forge · Publicando Pull Request / Merge Request (T21.2)",
                    BOLD
                )
            );
            let pr = crate::forge::ForgeEngine::create_pull_request(
                &ctx.workspace,
                &ctx.state,
                title,
                base,
                draft,
            )?;

            println!(
                "  PR #{}:      {}",
                paint(&pr.number.to_string(), CYAN),
                paint(&pr.title, BOLD)
            );
            println!(
                "  Rama:        {} -> {}",
                paint(&pr.head_branch, DIM),
                paint(&pr.base_branch, CYAN)
            );
            println!(
                "  Modo:        {}",
                if pr.draft {
                    "Borrador (Draft PR)"
                } else {
                    "Listo para Revisión (Ready)"
                }
            );
            println!("  Enlace Web:  {}", paint(&pr.url, CYAN));
            println!("\n  Pull Request formulado y publicado exitosamente con certificación del Auditor.\n");
            Ok(())
        }
        "status" | "info" => {
            let pr_num = args
                .get(1)
                .and_then(|n| n.trim_start_matches('#').parse::<u64>().ok());
            println!(
                "\n{}",
                paint(
                    "🔍 antOS Git Forge · Estado de Pull Request Remoto (T21.2)",
                    BOLD
                )
            );

            let status = crate::forge::ForgeEngine::get_pull_request_status(&ctx.state, pr_num)?;

            println!(
                "  PR #{}:      {}",
                paint(&status.number.to_string(), CYAN),
                status.title
            );
            println!(
                "  Estado:      {}",
                paint(&status.state.to_uppercase(), GREEN)
            );
            println!(
                "  Fusión:      {}",
                if status.mergeable {
                    "Limpia (Sin conflictos)"
                } else {
                    "Conflictos detectados"
                }
            );
            println!(
                "  CI Checks:   {}",
                status.ci_status.as_deref().unwrap_or("En progreso")
            );
            println!("  URL:         {}\n", paint(&status.url, DIM));
            Ok(())
        }
        _ => {
            println!("\nUso:");
            println!("  antos pr create [--draft] [--title <t>]   Formula y publica Pull Request certificado");
            println!("  antos pr status [numero]                  Consulta el estado y CI checks del PR\n");
            Ok(())
        }
    }
}
