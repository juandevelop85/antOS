//! `antos llm pull|rm|list` (T34.2): ciclo de vida de los modelos de Ollama
//! por su API HTTP — funciona aunque el backend sea `Nix` y no exista el
//! binario `ollama` en `PATH`.

use crate::ctx::Ctx;
use crate::llm::ollama_api::{human_size, ModelShow, OllamaClient, PullProgress};
use crate::llm::LlmConfig;
use crate::terminal::{paint, BOLD, CYAN, DIM, GREEN, YELLOW};
use anyhow::{bail, Result};
use std::io::Write;

/// El Ollama con el que trabaja `antos llm`: el configurado explícitamente,
/// si no el registrado por `service up`, si no el del entorno/por defecto.
pub fn client_for(ctx: &Ctx, config: &LlmConfig) -> OllamaClient {
    let endpoint = config
        .get_provider_settings("ollama")
        .and_then(|s| s.endpoint.clone())
        .or_else(|| ctx.local_llm.preferred_local_endpoint.clone())
        .or_else(|| {
            crate::planner::ollama::OllamaPlanner::from_env()
                .ok()
                .map(|p| p.endpoint)
        })
        .unwrap_or_else(|| crate::llm::ollama_api::DEFAULT_ENDPOINT.to_string());
    OllamaClient::new(endpoint)
}

fn require_available(client: &OllamaClient) -> Result<()> {
    if client.is_available() {
        return Ok(());
    }
    bail!(
        "Ollama no responde en {}. Arráncalo con `antos service up ollama` (o `antos llm setup`).",
        client.endpoint()
    )
}

/// Espacio libre en el volumen del directorio de estado, si se puede saber
/// (`df -k`, sin intérprete). Solo para avisar antes de una descarga larga.
fn free_disk_bytes(path: &std::path::Path) -> Option<u64> {
    let out = std::process::Command::new("df")
        .arg("-k")
        .arg(path)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().nth(1)?;
    let avail_kb: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    Some(avail_kb * 1024)
}

pub fn cmd_pull(ctx: &Ctx, config: &LlmConfig, model: &str) -> Result<()> {
    let client = client_for(ctx, config);
    require_available(&client)?;
    pull_with_progress(&client, model, Some(&ctx.state))
}

/// Descarga con barra de progreso en la terminal. Compartido con `setup`.
pub fn pull_with_progress(
    client: &OllamaClient,
    model: &str,
    state_for_disk_check: Option<&std::path::Path>,
) -> Result<()> {
    if let Some(free) = state_for_disk_check.and_then(free_disk_bytes) {
        println!(
            "  {} Espacio libre en disco: {}",
            paint("·", DIM),
            human_size(free)
        );
    }
    println!(
        "  {} Descargando {} desde {} …",
        paint("⬇", BOLD),
        paint(model, CYAN),
        paint(client.endpoint(), DIM)
    );
    let mut last_status = String::new();
    let mut last_pct: i64 = -1;
    let mut warned_disk = false;
    let mut on_progress = |p: &PullProgress| {
        if p.status != last_status {
            if !last_status.is_empty() {
                println!();
            }
            print!("    {} ", paint(&p.status, DIM));
            last_status = p.status.clone();
            last_pct = -1;
        }
        if let (Some(total), Some(done)) = (p.total, p.completed) {
            if !warned_disk {
                if let Some(free) = state_for_disk_check.and_then(free_disk_bytes) {
                    if free < total {
                        print!(
                            "\n    {} el modelo ocupa {} y quedan {} libres ",
                            paint("⚠", YELLOW),
                            human_size(total),
                            human_size(free)
                        );
                    }
                }
                warned_disk = true;
            }
            let pct = done.saturating_mul(100).checked_div(total).unwrap_or(0) as i64;
            if pct != last_pct && (pct % 5 == 0 || pct == 100) {
                print!(
                    "\r    {} {:>3}% de {}   ",
                    paint(&p.status, DIM),
                    pct,
                    human_size(total)
                );
                last_pct = pct;
            }
        }
        let _ = std::io::stdout().flush();
    };
    client.pull(model, &mut on_progress)?;
    println!(
        "\n  {} Modelo {} descargado.\n",
        paint("✓", GREEN),
        paint(model, BOLD)
    );
    Ok(())
}

pub fn cmd_rm(ctx: &Ctx, config: &LlmConfig, model: &str, assume_yes: bool) -> Result<()> {
    let client = client_for(ctx, config);
    require_available(&client)?;
    let size = client
        .tags()?
        .into_iter()
        .find(|t| t.name == model)
        .map(|t| t.size);
    let Some(size) = size else {
        bail!("el modelo «{model}» no está descargado (mira `antos llm list`)");
    };
    if !assume_yes {
        print!(
            "  ¿Borrar {} ({})? [y/N] ",
            paint(model, BOLD),
            human_size(size)
        );
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        if !matches!(
            line.trim().to_lowercase().as_str(),
            "s" | "si" | "sí" | "y" | "yes"
        ) {
            println!("  Cancelado.\n");
            return Ok(());
        }
    }
    client.delete(model)?;
    println!(
        "\n  {} Modelo {} borrado ({} liberados).\n",
        paint("✓", GREEN),
        paint(model, BOLD),
        human_size(size)
    );
    Ok(())
}

/// La tabla de modelos descargados con lo que un agente necesita saber:
/// tamaño, parámetros, cuantización, `tools` y contexto máximo.
pub fn print_local_models(client: &OllamaClient, active_model: Option<&str>) -> Result<()> {
    let tags = client.tags()?;
    if tags.is_empty() {
        println!(
            "  ● Ollama responde en {} pero no tiene modelos. Descarga uno: {}\n",
            client.endpoint(),
            paint("antos llm pull qwen2.5-coder:7b", YELLOW)
        );
        return Ok(());
    }
    println!(
        "  ● Modelos descargados en Ollama ({}, {}):",
        client.endpoint(),
        tags.len()
    );
    // Se rellena antes de colorear: los códigos ANSI descuadran `{:<n}`.
    println!(
        "    {}",
        paint(
            &format!(
                "{:<28} {:>8} {:<8} {:<10} {:<6} {:>8}",
                "MODELO", "TAMAÑO", "PARAMS", "CUANT.", "TOOLS", "CTX"
            ),
            BOLD
        )
    );
    for t in tags {
        let show = client.show(&t.name).ok();
        let (tools, tools_color, ctx) = describe_show(show.as_ref());
        let mark = if active_model == Some(t.name.as_str()) {
            paint(" ◀ activo", GREEN)
        } else {
            String::new()
        };
        println!(
            "    {} {:>8} {:<8} {:<10} {} {:>8}{mark}",
            paint(&format!("{:<28}", t.name), CYAN),
            human_size(t.size),
            t.details.parameter_size,
            t.details.quantization_level,
            if tools_color.is_empty() {
                format!("{tools:<6}")
            } else {
                paint(&format!("{tools:<6}"), tools_color)
            },
            ctx
        );
    }
    println!();
    Ok(())
}

fn describe_show(show: Option<&ModelShow>) -> (&'static str, &'static str, String) {
    match show {
        Some(s) => (
            match s.supports_tools() {
                Some(true) => "sí",
                Some(false) => "no",
                None => "?",
            },
            match s.supports_tools() {
                Some(true) => GREEN,
                Some(false) => DIM,
                None => "",
            },
            s.context_length
                .map(|c| c.to_string())
                .unwrap_or_else(|| "?".into()),
        ),
        None => ("?", "", "?".to_string()),
    }
}
