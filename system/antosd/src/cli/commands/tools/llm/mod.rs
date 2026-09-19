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

mod doctor;
mod models;
mod profile;
mod setup;

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
                } else if let Some(ep) = registered_ollama_endpoint(ctx) {
                    o.endpoint = ep;
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

    // 3. Jerarquía automática (modo "auto", T34.2): local primero.
    // 1. Ollama local si responde (el registrado por `service up` manda).
    // 2. OpenCode / llama.cpp local si está disponible en puerto 8080.
    // 3. Proveedores de red solo si hay clave configurada.
    // 4. Planificador local determinista sin dependencias externas.
    if let Ok(mut o) = OllamaPlanner::from_env() {
        if let Some(ep) = registered_ollama_endpoint(ctx) {
            o.endpoint = ep;
        }
        if o.is_available() {
            return Ok(Box::new(o));
        }
    }
    if let Ok(oc) = OpenAiCompatPlanner::from_preset("opencode") {
        if oc.is_available() {
            return Ok(Box::new(oc));
        }
    }
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
    Ok(Box::new(LocalPlanner))
}

/// El Ollama que registró `antos service up` (T34.1), que puede no estar en
/// el puerto por defecto. Si solo se sondeó `11434`, `from_env` ya apunta ahí
/// y se devuelve `None`.
fn registered_ollama_endpoint(ctx: Option<&Ctx>) -> Option<String> {
    let c = ctx?;
    if c.local_llm.source == crate::ctx::LocalLlmSource::Registered {
        c.local_llm.preferred_local_endpoint.clone()
    } else {
        None
    }
}

#[allow(dead_code)]
pub fn pick_planner_by_name(nombre: Option<&str>) -> Result<Box<dyn Planner>> {
    pick_planner(None, nombre)
}

// ------------------------------------------------------------------ llm (T19.2)

/// `assume_yes` es la bandera global `-y`/`--yes`, que el parser de opciones
/// retira de los argumentos antes de llegar aquí.
pub fn cmd_llm(ctx: &Ctx, args: &[String], assume_yes: bool) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    let mut config = crate::llm::LlmConfig::load_from_state(&ctx.state);

    match sub {
        "setup" | "init" => {
            setup::cmd_setup(ctx, &mut config, &args[1..], assume_yes)?;
            return Ok(());
        }
        "doctor" => {
            doctor::cmd_doctor(ctx, &config)?;
            return Ok(());
        }
        "profile" | "perfil" => {
            profile::cmd_profile(ctx, &mut config, &args[1..])?;
            return Ok(());
        }
        "pull" | "download" => {
            let model = args.get(1).ok_or_else(|| {
                anyhow::anyhow!(
                    "uso: antos llm pull <modelo>  (ej. antos llm pull qwen2.5-coder:7b)"
                )
            })?;
            models::cmd_pull(ctx, &config, model)?;
            return Ok(());
        }
        "rm" | "remove" | "delete" => {
            let model = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("uso: antos llm rm <modelo> [--yes]"))?;
            models::cmd_rm(ctx, &config, model, assume_yes)?;
            return Ok(());
        }
        "temperature" | "temp" => {
            // antos llm temperature [<valor>]
            let value = args.get(1).and_then(|a| a.parse::<f32>().ok());
            match value {
                Some(t) => {
                    if !(0.0..=2.0).contains(&t) {
                        bail!("la temperatura debe estar entre 0.0 y 2.0");
                    }
                    config
                        .providers
                        .entry("ollama".into())
                        .or_default()
                        .temperature = Some(t);
                    config.save_to_state(&ctx.state)?;
                    println!(
                        "\n{} Temperatura del agente con Ollama: {t}\n",
                        paint("✓", GREEN)
                    );
                }
                None => {
                    let current = config
                        .providers
                        .get("ollama")
                        .and_then(|p| p.temperature)
                        .unwrap_or(crate::llm::DEFAULT_OLLAMA_TEMPERATURE);
                    println!(
                        "\n  Temperatura del agente con Ollama: {current} (por defecto {}; el planificador va a 0.0)",
                        crate::llm::DEFAULT_OLLAMA_TEMPERATURE
                    );
                    println!("  Cambiar: antos llm temperature 0.7\n");
                }
            }
            return Ok(());
        }
        "ctx" | "context" => {
            // antos llm ctx [<tokens>] [--role <rol>]
            let role = args
                .iter()
                .position(|a| a == "--role" || a == "-r")
                .and_then(|i| args.get(i + 1))
                .map(String::as_str);
            let value = args.iter().skip(1).find_map(|a| a.parse::<u32>().ok());
            match value {
                Some(n) => {
                    if !(1024..=1_048_576).contains(&n) {
                        bail!("el contexto debe estar entre 1024 y 1048576 tokens");
                    }
                    match role {
                        Some(r) => config.set_role_num_ctx(r, n),
                        None => {
                            config.providers.entry("ollama".into()).or_default().num_ctx = Some(n);
                        }
                    }
                    config.save_to_state(&ctx.state)?;
                    println!(
                        "\n{} Contexto pedido a Ollama{}: {} tokens (se acota al máximo del modelo y a la RAM al arrancar un agente)\n",
                        paint("✓", GREEN),
                        role.map(|r| format!(" para el rol {r}")).unwrap_or_default(),
                        paint(&n.to_string(), CYAN)
                    );
                }
                None => {
                    println!(
                        "\n{}",
                        paint("antOS · Ventana de contexto pedida a Ollama", BOLD)
                    );
                    println!(
                        "  Por defecto: {} tokens{}",
                        config.requested_num_ctx(None),
                        if config
                            .providers
                            .get("ollama")
                            .and_then(|p| p.num_ctx)
                            .is_some()
                        {
                            " (configurado)"
                        } else {
                            " (valor de antOS; Ollama solo usaría 4096)"
                        }
                    );
                    for (r, n) in &config.role_num_ctx {
                        println!("  Rol {r:<10} {n} tokens");
                    }
                    println!("  Cambiar: antos llm ctx 32768 [--role coder]\n");
                }
            }
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

            // Modelos de Ollama con lo que un agente necesita saber (T34.2).
            let client = models::client_for(ctx, &config);
            if client.is_available() {
                let active = config
                    .get_provider_settings("ollama")
                    .and_then(|s| s.model.clone());
                if let Err(e) = models::print_local_models(&client, active.as_deref()) {
                    println!("  ○ No pude listar los modelos de Ollama: {e:#}\n");
                }
            } else {
                println!(
                    "  ○ Ollama no responde en {} (arráncalo: antos service up ollama)\n",
                    client.endpoint()
                );
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
            println!("    antos llm list               Lista modelos descargados y configurados");
            println!("    antos llm setup              Deja Ollama listo para agentes sin claves");
            println!(
                "    antos llm doctor             RAM, tier, modelos, tools y contexto efectivo"
            );
            println!("    antos llm pull|rm <modelo>   Descarga o borra un modelo de Ollama");
            println!("    antos llm ctx <tokens>       Ventana de contexto pedida a Ollama");
            println!("    antos llm temperature <t>    Temperatura del agente con Ollama");
            println!("    antos llm profile <p>        Roles antFlow: local | hybrid | cloud\n");
        }
    }

    Ok(())
}
