//! `antos eval`: evaluación reproducible de agentes (T33.5).
//!
//!   antos eval agent [--case <nombre>]            smoke determinista (fake)
//!   antos eval agent --live [--provider p]        contra el proveedor real
//!   antos eval agent --live --repeat N             N veces por caso: tasa y medianas
//!   antos eval diff [--margin 0.2]                 última ejecución vs anterior

use crate::agent::eval;
use crate::ctx::Ctx;
use crate::terminal::{paint, BOLD, CYAN, DIM, GREEN, RED, YELLOW};
use anyhow::{bail, Result};

pub fn cmd_eval(ctx: &Ctx, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("agent") | Some("agente") => cmd_eval_agent(ctx, &args[1..]),
        Some("diff") => cmd_eval_diff(ctx, &args[1..]),
        _ => {
            bail!(
                "uso: antos eval agent [--live] [--provider p] [--case nombre] [--repeat N] | \
                 antos eval diff [--margin 0.2]"
            )
        }
    }
}

fn cmd_eval_agent(ctx: &Ctx, args: &[String]) -> Result<()> {
    let mut live = false;
    let mut only: Option<String> = None;
    let mut provider: Option<String> = None;
    let mut repeat: u32 = 1;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--live" | "--real" => live = true,
            "--repeat" | "-n" => {
                repeat = args
                    .get(i + 1)
                    .and_then(|v| v.parse().ok())
                    .filter(|n| *n >= 1)
                    .ok_or_else(|| anyhow::anyhow!("--repeat necesita un entero ≥ 1"))?;
                i += 1;
            }
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
        eval::RunOptions {
            only: only.as_deref(),
            live,
            provider_spec: provider.as_deref(),
            executor: crate::agent::Executor::Confined,
            repeat,
        },
    )?;
    for c in &run.cases {
        let mark = match (&c.skipped, c.passed) {
            (Some(why), _) => paint(&format!("· omitido ({why})"), DIM),
            (None, true) => paint("✓", GREEN),
            (None, false) => paint("✗", RED),
        };
        let rate = if c.runs() > 1 {
            format!(" · {}/{} ok", c.successes(), c.runs())
        } else {
            String::new()
        };
        println!(
            "  {mark} {:<28} {:>2} pasos · {:>6} tokens · {:>3} s{rate} · {}{}",
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

fn cmd_eval_diff(ctx: &Ctx, args: &[String]) -> Result<()> {
    let margin = match args.iter().position(|a| a == "--margin") {
        Some(i) => args
            .get(i + 1)
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|m| (0.0..=1.0).contains(m))
            .ok_or_else(|| anyhow::anyhow!("--margin necesita un valor entre 0 y 1"))?,
        None => eval::DEFAULT_RATE_MARGIN,
    };
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
    let regressions = eval::diff_with_margin(&before, &after, margin);
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
