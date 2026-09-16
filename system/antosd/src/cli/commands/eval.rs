//! `antos eval`: evaluación reproducible de agentes (T33.5).
//!
//!   antos eval agent [--case <nombre>]            smoke determinista (fake)
//!   antos eval agent --live [--provider p]        contra el proveedor real
//!   antos eval diff                                última ejecución vs anterior

use crate::agent::eval;
use crate::ctx::Ctx;
use crate::terminal::{paint, BOLD, CYAN, DIM, GREEN, RED, YELLOW};
use anyhow::{bail, Result};

pub fn cmd_eval(ctx: &Ctx, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("agent") | Some("agente") => cmd_eval_agent(ctx, &args[1..]),
        Some("diff") => cmd_eval_diff(ctx),
        _ => {
            bail!("uso: antos eval agent [--live] [--provider p] [--case nombre] | antos eval diff")
        }
    }
}

fn cmd_eval_agent(ctx: &Ctx, args: &[String]) -> Result<()> {
    let mut live = false;
    let mut only: Option<String> = None;
    let mut provider: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--live" | "--real" => live = true,
            "--case" | "-c" => {
                only = args.get(i + 1).cloned();
                i += 1;
            }
            "--provider" | "-p" => {
                provider = args.get(i + 1).cloned();
                i += 1;
            }
            other => bail!("argumento desconocido: {other}"),
        }
        i += 1;
    }
    let cases_dir = eval::default_cases_dir(ctx)?;
    let catalog = crate::capability::Catalog::load(&ctx.caps_dir)?;
    println!(
        "\n{}",
        paint(
            &format!(
                "antOS · evaluación de agentes ({}) · {}",
                if live {
                    "modelo real"
                } else {
                    "determinista, proveedor fake"
                },
                cases_dir.display()
            ),
            BOLD
        )
    );
    let run = eval::run_all(
        ctx,
        &catalog,
        &cases_dir,
        only.as_deref(),
        live,
        provider.as_deref(),
        crate::agent::Executor::Confined,
    )?;
    for c in &run.cases {
        let mark = match (&c.skipped, c.passed) {
            (Some(why), _) => paint(&format!("· omitido ({why})"), DIM),
            (None, true) => paint("✓", GREEN),
            (None, false) => paint("✗", RED),
        };
        println!(
            "  {mark} {:<28} {:>2} pasos · {:>6} tokens · {:>3} s · {}{}",
            paint(&c.name, CYAN),
            c.steps,
            c.tokens,
            c.seconds,
            c.stop_reason,
            match c.tests_green {
                Some(true) => " · tests verdes",
                Some(false) => " · tests ROJOS",
                None => "",
            }
        );
        for f in &c.failures {
            println!("      {}", paint(f, RED));
        }
    }
    let path = eval::save(&ctx.state, &run)?;
    println!(
        "\n  guardado en {}",
        paint(&path.display().to_string(), DIM)
    );
    if run.passed() {
        println!("  {}\n", paint("EVAL OK", GREEN));
        Ok(())
    } else {
        println!("  {}\n", paint("EVAL FALLÓ", RED));
        bail!("evaluación con fallos")
    }
}

fn cmd_eval_diff(ctx: &Ctx) -> Result<()> {
    let (before, after) = eval::last_two(&ctx.state)?;
    println!(
        "\n{}",
        paint(
            &format!(
                "antOS · eval diff · {} ({}) → {} ({})",
                before.at, before.provider, after.at, after.provider
            ),
            BOLD
        )
    );
    let regressions = eval::diff(&before, &after);
    if regressions.is_empty() {
        println!("  {}\n", paint("sin regresiones", GREEN));
        return Ok(());
    }
    for r in &regressions {
        println!(
            "  {} {}: {}",
            paint("▼", YELLOW),
            paint(&r.case, CYAN),
            r.what
        );
    }
    println!();
    bail!("{} regresión(es)", regressions.len())
}
