//! Selección de backend de planificación (`pick_planner`) y `antos llm`.
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

pub fn pick_planner(ctx: Option<&Ctx>, nombre: Option<&str>) -> Result<Box<dyn Planner>> {
    // 1. Si el usuario solicitó explícitamente un planificador por CLI/flag (--planner):
    if let Some(target) = nombre {
        return match target {
            "local" => Ok(Box::new(LocalPlanner)),
            "claude" => Ok(Box::new(ClaudePlanner::from_env()?)),
            "ollama" | "local-llm" | "local_llm" => Ok(Box::new(OllamaPlanner::from_env()?)),
            "groq" => Ok(Box::new(OpenAiCompatPlanner::from_preset("groq")?)),
            "openrouter" | "open-router" => {
                Ok(Box::new(OpenAiCompatPlanner::from_preset("openrouter")?))
            }
            "gemini" | "google" => Ok(Box::new(OpenAiCompatPlanner::from_preset("gemini")?)),
            "opencode" | "localai" | "vllm" => {
                Ok(Box::new(OpenAiCompatPlanner::from_preset("opencode")?))
            }
            "openai" | "openai_compat" | "compat" => {
                Ok(Box::new(OpenAiCompatPlanner::from_preset("openai")?))
            }
            other => {
                if other.starts_with("http://") || other.starts_with("https://") {
                    Ok(Box::new(OpenAiCompatPlanner::new(
                        "custom", other, "default", None,
                    )))
                } else {
                    bail!("planificador desconocido: {other} (usa «local», «ollama», «groq», «openrouter», «gemini», «opencode» o «claude»)")
                }
            }
        };
    }

    // 2. Si hay configuración persistente guardada (vía 'antos llm use <proveedor>'):
    let config = if let Some(c) = ctx {
        crate::llm::LlmConfig::load_from_state(&c.state)
    } else {
        let state = std::env::var_os("ANTOS_STATE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".antos"));
        crate::llm::LlmConfig::load_from_state(&state)
    };

    if config.active_provider != "auto" {
        let p_name = config.active_provider.as_str();
        let custom_settings = config.get_provider_settings(p_name);
        let custom_model = custom_settings.and_then(|s| s.model.as_deref());
        let custom_endpoint = custom_settings.and_then(|s| s.endpoint.as_deref());

        let planner_res: Result<Box<dyn Planner>> = match p_name {
            "local" => Ok(Box::new(LocalPlanner)),
            "claude" => Ok(Box::new(ClaudePlanner::from_env()?)),
            "ollama" => {
                let mut o = OllamaPlanner::from_env()?;
                if let Some(m) = custom_model {
                    o.model = m.to_string();
                }
                if let Some(e) = custom_endpoint {
                    o.endpoint = e.to_string();
                }
                Ok(Box::new(o))
            }
            "groq" | "openrouter" | "gemini" | "opencode" | "openai" => {
                let mut p = OpenAiCompatPlanner::from_preset(p_name)?;
                if let Some(m) = custom_model {
                    p.model = m.to_string();
                }
                if let Some(e) = custom_endpoint {
                    p.endpoint = e.to_string();
                }
                Ok(Box::new(p))
            }
            other => {
                if let Some(endpoint) = custom_endpoint {
                    let model = custom_model.unwrap_or("default");
                    Ok(Box::new(OpenAiCompatPlanner::new(
                        other, endpoint, model, None,
                    )))
                } else {
                    OpenAiCompatPlanner::from_preset(other).map(|p| Box::new(p) as Box<dyn Planner>)
                }
            }
        };

        if let Ok(p) = planner_res {
            return Ok(p);
        }
    }

    // 3. Jerarquía de fallback automático (modo "auto"):
    // 1. Proveedores en la nube con free tiers o claves configuradas.
    // 2. Claude si hay clave de API configurada.
    // 3. Ollama local si está disponible en la máquina.
    // 4. OpenCode / llama.cpp local si está disponible en puerto 8080.
    // 5. Planificador local determinista sin dependencias externas.
    if let Ok(p) = OpenAiCompatPlanner::from_preset("groq") {
        return Ok(Box::new(p));
    }
    if let Ok(p) = OpenAiCompatPlanner::from_preset("openrouter") {
        return Ok(Box::new(p));
    }
    if let Ok(p) = ClaudePlanner::from_env() {
        return Ok(Box::new(p));
    }
    if let Ok(p) = OpenAiCompatPlanner::from_preset("gemini") {
        return Ok(Box::new(p));
    }
    if let Ok(o) = OllamaPlanner::from_env() {
        if o.is_available() {
            return Ok(Box::new(o));
        }
    }
    if let Ok(oc) = OpenAiCompatPlanner::from_preset("opencode") {
        if oc.is_available() {
            return Ok(Box::new(oc));
        }
    }
    Ok(Box::new(LocalPlanner))
}

#[allow(dead_code)]
pub fn pick_planner_by_name(nombre: Option<&str>) -> Result<Box<dyn Planner>> {
    pick_planner(None, nombre)
}

// ------------------------------------------------------------------ llm (T19.2)

pub fn cmd_llm(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    let mut config = crate::llm::LlmConfig::load_from_state(&ctx.state);

    match sub {
        "setup" | "init" => {
            println!(
                "\n{}",
                paint(
                    "antOS · Asistente de Configuración de Motor LLM Local (T19.3)",
                    BOLD
                )
            );
            println!("  Este asistente verifica y prepara el motor local Ollama/OpenCode para desarrollo sin conexión.\n");

            // 1. Verificar si Ollama o OpenCode está escuchando
            let status = &ctx.local_llm;
            let target_model = args
                .iter()
                .position(|a| a == "--model" || a == "-m")
                .and_then(|i| args.get(i + 1))
                .map(String::as_str)
                .unwrap_or("qwen2.5-coder:latest");

            println!("  [1/3] Detección de servicios locales:");
            if status.ollama_available {
                println!(
                    "    ✓ Ollama detectado y respondiendo en {}",
                    paint("http://127.0.0.1:11434", GREEN)
                );
            } else if status.opencode_available {
                println!(
                    "    ✓ OpenCode detectado y respondiendo en {}",
                    paint("http://127.0.0.1:8080/v1", GREEN)
                );
            } else {
                println!("    ○ Ningún motor local está corriendo actualmente.");
                println!(
                    "      • Para instalar Ollama con antpkg ejecuta: {}",
                    paint("antos pkg install recipes/ollama.toml", YELLOW)
                );
                println!(
                    "      • O inicia el servicio si ya lo tienes:     {}",
                    paint("ollama serve &", CYAN)
                );
                println!("      • O descarga Ollama directamente desde:     https://ollama.com\n");
            }

            // 2. Recomendaciones de modelos de desarrollo
            println!("  [2/3] Modelos recomendados para antOS antFlow:");
            println!(
                "    • {} (Recomendado: balance perfecto velocidad y sintaxis de código)",
                paint("qwen2.5-coder:7b", GREEN)
            );
            println!(
                "    • {} (Especializado en refactorización y depuración)",
                paint("deepseek-coder:6.7b", CYAN)
            );
            println!(
                "    • {} (Ultraligero para portátiles sin GPU dedicada)",
                paint("qwen2.5-coder:1.5b", DIM)
            );

            // 3. Configuración persistente del motor
            println!("\n  [3/3] Aplicando configuración:");
            config.set_active_provider("ollama", Some(target_model), None);
            config.save_to_state(&ctx.state)?;

            println!(
                "    ✓ Motor predeterminado fijado en: {}",
                paint("ollama", GREEN)
            );
            println!(
                "    ✓ Modelo de código seleccionado:  {}",
                paint(target_model, CYAN)
            );
            println!("\n  Pasos siguientes:");
            println!(
                "    1. Si aún no tienes el modelo descargado, ejecuta: {}",
                paint(&format!("ollama pull {target_model}"), YELLOW)
            );
            println!(
                "    2. Verifica la inferencia con:                    {}",
                paint("antos llm test", CYAN)
            );
            println!(
                "    3. Explora alternativas gratuitas con:            {}\n",
                paint("antos llm free", CYAN)
            );
            return Ok(());
        }
        "use" | "set" | "select" => {
            let target = args.get(1).map(String::as_str);
            match target {
                None => {
                    println!(
                        "\n{} Uso: antos llm use <proveedor> [--model <modelo>] [--endpoint <url>]",
                        paint("antOS ·", BOLD)
                    );
                    println!("  Proveedores soportados: groq, openrouter, gemini, ollama, opencode, claude, local, auto");
                    println!("  Ejemplo: antos llm use groq --model llama-3.3-70b-versatile");
                    println!(
                        "  Ejemplo: antos llm use openrouter --model deepseek/deepseek-r1:free"
                    );
                    println!("  Ejemplo: antos llm use ollama --model qwen2.5-coder");
                    println!("  Para ver opciones gratuitas: antos llm free\n");
                    return Ok(());
                }
                Some("--clear" | "clear" | "auto" | "reset") => {
                    config.clear_active();
                    config.save_to_state(&ctx.state)?;
                    println!(
                        "\n{} Selección de motor restablecida a '{}'. El sistema elegirá automáticamente el mejor motor disponible.\n",
                        paint("antOS ·", BOLD),
                        paint("auto", GREEN)
                    );
                    return Ok(());
                }
                Some(provider) => {
                    let model_flag = args
                        .iter()
                        .position(|a| a == "--model" || a == "-m")
                        .and_then(|i| args.get(i + 1))
                        .map(String::as_str);
                    let endpoint_flag = args
                        .iter()
                        .position(|a| a == "--endpoint" || a == "-e" || a == "--url")
                        .and_then(|i| args.get(i + 1))
                        .map(String::as_str);

                    config.set_active_provider(provider, model_flag, endpoint_flag);
                    config.save_to_state(&ctx.state)?;

                    let active_settings = config.get_provider_settings(provider);
                    let model_disp = active_settings
                        .and_then(|s| s.model.as_deref())
                        .unwrap_or("por defecto");
                    let endpoint_disp = active_settings
                        .and_then(|s| s.endpoint.as_deref())
                        .unwrap_or("estándar");

                    println!(
                        "\n{} Motor LLM Activo fijado en: {}\n  Modelo:   {}\n  Endpoint: {}\n  Todos los comandos de antOS y agentes usarán este motor por defecto.\n",
                        paint("antOS ·", BOLD),
                        paint(provider, GREEN),
                        paint(model_disp, CYAN),
                        paint(endpoint_disp, DIM)
                    );
                }
            }
        }
        "free" | "gratis" => {
            println!(
                "\n{}",
                paint(
                    "antOS · Catálogo de Modelos y Proveedores 100% Gratuitos (T19.2)",
                    BOLD
                )
            );
            println!("  antOS está diseñado para funcionar con coste $0 usando modelos locales o cloud tiers gratuitos:\n");

            let recs = crate::planner::openai_compat::get_free_recommendations();
            for r in recs {
                let badge = if r.provider_type == "local" {
                    paint("● LOCAL / OFFLINE", GREEN)
                } else {
                    paint("⚡ CLOUD FREE TIER", CYAN)
                };

                println!("  ┌─ {}  [{}]", paint(r.display_name, BOLD), badge);
                println!("  │  Descripción:   {}", r.description);
                println!("  │  Modelo base:   {}", paint(r.default_model, GREEN));
                println!("  │  Alternativos:  {}", r.alternative_models.join(", "));
                println!("  │  Límites:       {}", paint(r.rate_limits, DIM));
                println!(
                    "  │  Activar:       {}",
                    paint(
                        &format!(
                            "antos llm use {} --model {}",
                            r.provider_id, r.default_model
                        ),
                        YELLOW
                    )
                );
                if !r.env_key.is_empty() && r.provider_type != "local" {
                    println!("  │  Clave API:     Obtener clave gratuita y guardar con:");
                    println!(
                        "  │                 {}",
                        paint(&format!("antos secret set {} <tu-clave>", r.env_key), DIM)
                    );
                }
                println!("  └─────────────────────────────────────────────────────────────────────────────\n");
            }
            println!(
                "  Para probar el motor actual: {}\n",
                paint("antos llm test", CYAN)
            );
        }
        "test" | "ping" => {
            let prompt = args
                .iter()
                .position(|a| a == "--prompt" || a == "-p")
                .and_then(|i| args.get(i + 1))
                .map(String::as_str)
                .unwrap_or("crea un proyecto rust llamado demo");

            println!(
                "\n{} Probando inferencia y Tool Calling con el motor activo...",
                paint("antOS LLM Test ·", BOLD)
            );
            println!("  Intención de prueba: «{}»", paint(prompt, CYAN));

            let catalog = Catalog::load(&ctx.caps_dir).unwrap_or_else(|_| Catalog {
                caps: std::collections::BTreeMap::new(),
            });
            let planner = pick_planner(Some(ctx), None)?;

            println!("  Motor seleccionado:  {}", paint(planner.name(), GREEN));
            let start = std::time::Instant::now();
            match planner.plan(prompt, &catalog) {
                Ok(proposal) => {
                    let elapsed = start.elapsed();
                    println!(
                        "  Latencia de respuesta: {} ms",
                        paint(&elapsed.as_millis().to_string(), GREEN)
                    );
                    println!(
                        "  Pasos generados:       {}",
                        paint(&proposal.steps.len().to_string(), BOLD)
                    );
                    for (idx, step) in proposal.steps.iter().enumerate() {
                        println!(
                            "    {}. Capacidad: {} ({:?})",
                            idx + 1,
                            paint(&step.capability, YELLOW),
                            step.args
                        );
                    }
                    if let Some(ref note) = proposal.note {
                        println!("  Nota del modelo:       {}", paint(note, DIM));
                    }
                    println!(
                        "\n  {} Prueba de inferencia y Tool Calling superada con éxito.\n",
                        paint("✓", GREEN)
                    );
                }
                Err(e) => {
                    let elapsed = start.elapsed();
                    println!("  Latencia: {} ms", elapsed.as_millis());
                    println!(
                        "\n  {} Fallo al ejecutar la prueba: {:#}\n",
                        paint("✗ Error:", RED),
                        e
                    );
                }
            }
        }
        "list" | "models" => {
            println!(
                "\n{}",
                paint("antOS · Modelos y Motores Configurados (T19.2)", BOLD)
            );
            println!(
                "  Motor Activo configurado: {}\n",
                paint(&config.active_provider, GREEN)
            );

            // Intentar listar modelos de Ollama si está disponible
            if let Ok(ollama) = crate::planner::ollama::OllamaPlanner::from_env() {
                if ollama.is_available() {
                    if let Ok(models) = ollama.list_models() {
                        println!(
                            "  ● Modelos descargados en Ollama local ({}):",
                            models.len()
                        );
                        for m in models {
                            println!("    • {}", paint(&m, CYAN));
                        }
                        println!();
                    }
                }
            }

            println!("  ● Proveedores preconfigurados en antOS:");
            for (prov, s) in &config.providers {
                let m = s.model.as_deref().unwrap_or("predeterminado");
                let e = s.endpoint.as_deref().unwrap_or("estándar");
                println!(
                    "    • {:<12} -> modelo: {:<32} (endpoint: {})",
                    paint(prov, BOLD),
                    paint(m, GREEN),
                    paint(e, DIM)
                );
            }
            println!(
                "\n  Para cambiar de motor: {}\n",
                paint("antos llm use <proveedor>", CYAN)
            );
        }
        _ => {
            println!(
                "\n{}",
                paint(
                    "antOS · Estado de Motores de Inferencia Multi-LLM (T19.2)",
                    BOLD
                )
            );

            let active_disp = if config.active_provider == "auto" {
                format!(
                    "{} (selección inteligente por disponibilidad)",
                    paint("auto", GREEN)
                )
            } else {
                paint(&config.active_provider, GREEN)
            };
            println!("  ● Configuración Activa: {}\n", active_disp);

            // 1. Groq Cloud (Free)
            let groq_ok = crate::planner::openai_compat::OpenAiCompatPlanner::from_preset("groq")
                .map(|p| p.is_available())
                .unwrap_or(false);
            let groq_badge = if groq_ok {
                paint("● Conectado (Free Tier)", GREEN)
            } else {
                paint(
                    "○ Sin API Key (define GROQ_API_KEY o usa 'antos secret set GROQ_API_KEY ...')",
                    DIM,
                )
            };
            println!("  1. Groq Cloud (Free Tier - Ultra rápido >300 t/s):");
            println!("     Estado: {}", groq_badge);
            println!("     Modelo: {}", paint("llama-3.3-70b-versatile", BOLD));

            // 2. OpenRouter (Free)
            let or_ok =
                crate::planner::openai_compat::OpenAiCompatPlanner::from_preset("openrouter")
                    .map(|p| p.is_available())
                    .unwrap_or(false);
            let or_badge = if or_ok {
                paint("● Conectado (Free Tier)", GREEN)
            } else {
                paint("○ Sin API Key (define OPENROUTER_API_KEY)", DIM)
            };
            println!("\n  2. OpenRouter (Free Tier - DeepSeek-R1 / Qwen2.5):");
            println!("     Estado: {}", or_badge);
            println!("     Modelo: {}", paint("deepseek/deepseek-r1:free", BOLD));

            // 3. Google Gemini (Free)
            let gem_ok = crate::planner::openai_compat::OpenAiCompatPlanner::from_preset("gemini")
                .map(|p| p.is_available())
                .unwrap_or(false);
            let gem_badge = if gem_ok {
                paint("● Conectado (Free Tier)", GREEN)
            } else {
                paint("○ Sin API Key (define GEMINI_API_KEY)", DIM)
            };
            println!("\n  3. Google Gemini API (Free Tier):");
            println!("     Estado: {}", gem_badge);
            println!("     Modelo: {}", paint("gemini-2.0-flash", BOLD));

            // 4. Ollama Local (Offline)
            let ollama_inst = crate::planner::ollama::OllamaPlanner::from_env();
            let ollama_ok = ollama_inst
                .as_ref()
                .map(|o| o.is_available())
                .unwrap_or(false);
            let ollama_badge = if ollama_ok {
                paint("● Online (Local Offline)", GREEN)
            } else {
                paint("○ Desconectado (ejecuta 'ollama serve')", YELLOW)
            };
            println!("\n  4. Ollama (Local Offline - Privacidad Total):");
            println!("     Estado: {}", ollama_badge);
            let ollama_ep = ollama_inst
                .as_ref()
                .map(|o| o.endpoint.as_str())
                .unwrap_or("http://127.0.0.1:11434");
            println!("     Endpoint: {}", paint(ollama_ep, BOLD));

            // 5. OpenCode / llama.cpp (Local)
            let oc_inst =
                crate::planner::openai_compat::OpenAiCompatPlanner::from_preset("opencode");
            let oc_ok = oc_inst.as_ref().map(|p| p.is_available()).unwrap_or(false);
            let oc_badge = if oc_ok {
                paint("● Online (Local /v1)", GREEN)
            } else {
                paint("○ No detectado en http://127.0.0.1:8080/v1", DIM)
            };
            println!("\n  5. OpenCode / llama.cpp / LocalAI (Local):");
            println!("     Estado: {}", oc_badge);

            // 6. Claude (Anthropic)
            let claude_ok = crate::planner::claude::ClaudePlanner::from_env().is_ok();
            let claude_badge = if claude_ok {
                paint("● Conectado (API Key presente)", GREEN)
            } else {
                paint("○ Sin ANTHROPIC_API_KEY", DIM)
            };
            println!("\n  6. Claude API (Anthropic):");
            println!("     Estado: {}", claude_badge);

            // 7. Determinista Local
            println!("\n  7. Planificador Determinista antOS:");
            println!(
                "     Estado: {}",
                paint("● Siempre Activo (Cero Dependencias / Offline)", GREEN)
            );

            println!("\n  Comandos disponibles:");
            println!("    antos llm free               Explora modelos y servicios 100% gratuitos");
            println!("    antos llm use <proveedor>    Cambia el motor activo");
            println!("    antos llm test               Prueba interactiva del motor en uso");
            println!("    antos llm list               Lista modelos descargados y configurados\n");
        }
    }

    Ok(())
}
