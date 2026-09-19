//! `antos llm setup` (T34.2): de una instalación limpia a un agente que
//! funciona sin claves, en cuatro pasos y con confirmación en cada uno.
//!
//! 1. Servicio: si Ollama no responde, propone `service up ollama`.
//! 2. Diagnóstico: RAM, tier, modelos, contexto (`doctor`).
//! 3. Modelo: propone descargar el recomendado por RAM (o `--model`).
//! 4. Configuración: `active_provider = ollama` con ese modelo.
//!
//! Nada es silencioso; `--yes` responde que sí a todo para scripts.

use super::doctor::collect;
use super::models::{client_for, pull_with_progress};
use crate::ctx::Ctx;
use crate::llm::LlmConfig;
use crate::terminal::{paint, BOLD, CYAN, DIM, GREEN, YELLOW};
use anyhow::{bail, Result};
use std::io::Write;

fn confirm(question: &str, assume_yes: bool) -> Result<bool> {
    if assume_yes {
        println!("  {question} {}", paint("sí (--yes)", DIM));
        return Ok(true);
    }
    print!("  {question} [y/N] ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line)? == 0 {
        return Ok(false);
    }
    Ok(matches!(
        line.trim().to_lowercase().as_str(),
        "s" | "si" | "sí" | "y" | "yes"
    ))
}

pub fn cmd_setup(
    ctx: &Ctx,
    config: &mut LlmConfig,
    args: &[String],
    assume_yes: bool,
) -> Result<()> {
    let wanted_model = args
        .iter()
        .position(|a| a == "--model" || a == "-m")
        .and_then(|i| args.get(i + 1))
        .cloned();

    println!(
        "\n{}",
        paint(
            "antOS · Puesta a punto del motor local (antos llm setup)",
            BOLD
        )
    );
    println!("  Deja un antOS sin claves listo para `antos agent do …` con Ollama.\n");

    // ── 1. Servicio ──────────────────────────────────────────────────────
    println!("  {} Servicio", paint("[1/4]", BOLD));
    let mut client = client_for(ctx, config);
    if client.is_available() {
        println!(
            "    {} Ollama responde en {}",
            paint("✓", GREEN),
            paint(client.endpoint(), CYAN)
        );
    } else {
        println!(
            "    {} Ollama no responde en {}",
            paint("○", DIM),
            client.endpoint()
        );
        if !confirm(
            "¿Arrancarlo ahora con `antos service up ollama`?",
            assume_yes,
        )? {
            bail!(
                "sin Ollama no hay motor local. Arráncalo tú (`antos service up ollama` o \
                 Ollama.app) y repite `antos llm setup`."
            );
        }
        let info = crate::service::start_service("ollama", None, None, &ctx.state, &ctx.workspace)?;
        println!(
            "    {} Ollama {} en 127.0.0.1:{} ({})",
            paint("✓", GREEN),
            if info.backend == crate::service::ServiceBackend::External {
                "adoptado"
            } else {
                "arrancado"
            },
            info.port,
            info.backend.label()
        );
        client = crate::llm::ollama_api::OllamaClient::new(info.env_var_value.clone());
        // Que el resto de `setup` (y `doctor`) miren a este endpoint.
        config.set_active_provider("ollama", None, Some(client.endpoint()));
    }

    // ── 2. Diagnóstico ───────────────────────────────────────────────────
    println!("\n  {} Diagnóstico", paint("[2/4]", BOLD));
    let report = collect(ctx, config);
    for line in report.render().lines() {
        println!("    {line}");
    }

    // ── 3. Modelo ────────────────────────────────────────────────────────
    println!("\n  {} Modelo", paint("[3/4]", BOLD));
    let target = wanted_model
        .or_else(|| report.recommended_model().map(str::to_string))
        .unwrap_or_else(|| "qwen2.5-coder:7b".to_string());
    let downloaded = report.models.iter().any(|m| m.tag.name == target);
    if downloaded {
        println!(
            "    {} {} ya está descargado",
            paint("✓", GREEN),
            paint(&target, CYAN)
        );
    } else {
        let why = if report.tier.is_some() {
            "recomendado para la RAM de esta máquina"
        } else {
            "por defecto"
        };
        println!(
            "    {} {} no está descargado ({why})",
            paint("○", DIM),
            paint(&target, CYAN)
        );
        if confirm(&format!("¿Descargar {target} ahora?"), assume_yes)? {
            pull_with_progress(&client, &target, Some(&ctx.state))?;
        } else {
            println!(
                "    {} Sin modelo el agente no arranca; cuando quieras: {}",
                paint("·", DIM),
                paint(&format!("antos llm pull {target}"), YELLOW)
            );
        }
    }
    // Un modelo sin `tools` no sirve para agentes: se avisa aquí, antes de
    // fijarlo como activo.
    if let Ok(show) = client.show(&target) {
        if show.supports_tools() == Some(false) {
            println!(
                "    {} {} no soporta llamadas a herramienta: el planificador lo usará, \
                 los agentes no. Elige otro con --model.",
                paint("⚠", YELLOW),
                target
            );
        }
    }

    // ── 4. Configuración ─────────────────────────────────────────────────
    println!("\n  {} Configuración", paint("[4/4]", BOLD));
    if confirm(
        &format!("¿Fijar ollama:{target} como motor activo de antOS?"),
        assume_yes,
    )? {
        config.set_active_provider("ollama", Some(&target), None);
        config.save_to_state(&ctx.state)?;
        println!(
            "    {} Motor activo: {} · modelo: {} · contexto pedido: {}",
            paint("✓", GREEN),
            paint("ollama", GREEN),
            paint(&target, CYAN),
            config.requested_num_ctx(None)
        );
    } else {
        // Al menos que el endpoint elegido quede guardado si lo tocamos.
        config.save_to_state(&ctx.state)?;
        println!(
            "    {} Sin fijar; `auto` usará Ollama igualmente si tiene un modelo con tools.",
            paint("·", DIM)
        );
    }

    println!("\n  Siguiente paso:");
    println!(
        "    {}   un run suelto",
        paint("antos agent do \"haz que pase el test X\"", CYAN)
    );
    println!(
        "    {}                        subir o bajar el contexto si hace falta\n",
        paint("antos llm ctx <tokens>", CYAN)
    );
    Ok(())
}
