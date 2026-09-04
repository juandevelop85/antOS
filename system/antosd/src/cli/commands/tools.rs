#![allow(unused_imports, dead_code)]

extern crate antos_protocol as antos_protocolo;

use std::path::{Path, PathBuf};
use anyhow::{bail, Context, Result};
use crate::capability::{Catalog, Tier};
use crate::ctx::Ctx;
use crate::grants::Grants;
use crate::journal::{Outcome, Record};
use crate::planner::{
    claude::ClaudePlanner, local::LocalPlanner, ollama::OllamaPlanner,
    openai_compat::OpenAiCompatPlanner, Planner,
};
use crate::terminal::{ellipsis, paint, tier_color, BLUE, BOLD, CYAN, DIM, GREEN, RED, YELLOW};
use crate::cli::args::Opts;

pub fn cmd_ports(args: &[String]) -> Result<()> {
    let filtro = args.first().and_then(|a| a.parse::<u16>().ok());
    let puertos = crate::net::diagnosticar_puertos(filtro)?;

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
                "\n{} Aprovisionando servicio efímero «{}»...",
                paint("⚡", BOLD),
                paint(svc, YELLOW)
            );
            let info = crate::service::start_service(svc, port, db, &ctx.state, &ctx.workspace)?;
            println!(
                "  {} Servicio:      {}",
                paint("●", GREEN),
                paint(&info.name, BOLD)
            );
            println!(
                "  {} Puerto:        {}",
                paint("●", GREEN),
                paint(&info.port.to_string(), YELLOW)
            );
            println!(
                "  {} Estado:        {}",
                paint("●", GREEN),
                paint(&info.status, GREEN)
            );
            println!(
                "  {} Variable .env: {}={}",
                paint("●", GREEN),
                paint(&info.env_var_key, BOLD),
                paint(&info.env_var_value, CYAN)
            );
            println!(
                "  {} Almacenamiento: {}\n",
                paint("●", GREEN),
                paint(&info.data_dir, DIM)
            );
        }
        "down" | "stop" => {
            let svc = args.get(1).ok_or_else(|| {
                anyhow::anyhow!(
                    "debes especificar el nombre del servicio (ej. antos service down postgres)"
                )
            })?;
            crate::service::stop_service(svc, &ctx.state)?;
            println!(
                "\n{} Servicio «{}» detenido y limpiado.\n",
                paint("✓", GREEN),
                paint(svc, BOLD)
            );
        }
        "status" | "list" | _ => {
            let svc_filter = if sub != "status" && sub != "list" {
                Some(sub)
            } else {
                args.get(1).map(String::as_str)
            };

            let services = crate::service::get_service_status(svc_filter, &ctx.state)?;
            println!(
                "\n{}",
                paint(
                    "antOS · Servicios Locales Efímeros de Desarrollo (T5.1)",
                    BOLD
                )
            );
            if services.is_empty() {
                println!("  No hay servicios efímeros aprovisionados.");
                println!(
                    "  Inicia uno con: antos service up <postgres|redis|mariadb|meilisearch>\n"
                );
            } else {
                println!("  ┌────────────────┬────────┬───────────┬─────────────────────────────────────────────────────────┐");
                println!(
                    "  │ {:<14} │ {:<6} │ {:<9} │ {:<55} │",
                    paint("SERVICIO", BOLD),
                    paint("PUERTO", BOLD),
                    paint("ESTADO", BOLD),
                    paint("VARIABLE DE ENTORNO (.env)", BOLD)
                );
                println!("  ├────────────────┼────────┼───────────┼─────────────────────────────────────────────────────────┤");
                for s in services {
                    let st_fmt = if s.status == "running" {
                        paint("● running", GREEN)
                    } else {
                        paint("○ stopped", DIM)
                    };
                    println!(
                        "  │ {:<14} │ {:<6} │ {:<20} │ {}={} │",
                        s.name,
                        s.port,
                        st_fmt,
                        paint(&s.env_var_key, BOLD),
                        ellipsis(&s.env_var_value, 38)
                    );
                }
                println!("  └────────────────┴────────┴───────────┴─────────────────────────────────────────────────────────┘\n");
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
        "list" | _ => {
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

// ------------------------------------------------------------------ apoyo

pub fn pick_planner(ctx: Option<&Ctx>, nombre: Option<&str>) -> Result<Box<dyn Planner>> {
    // 1. Si el usuario solicitó explícitamente un planificador por CLI/flag (--planner):
    if let Some(target) = nombre {
        return match target {
            "local" => Ok(Box::new(LocalPlanner)),
            "claude" => Ok(Box::new(ClaudePlanner::from_env()?)),
            "ollama" | "local-llm" | "local_llm" => Ok(Box::new(OllamaPlanner::from_env()?)),
            "groq" => Ok(Box::new(OpenAiCompatPlanner::from_preset("groq")?)),
            "openrouter" | "open-router" => Ok(Box::new(OpenAiCompatPlanner::from_preset("openrouter")?)),
            "gemini" | "google" => Ok(Box::new(OpenAiCompatPlanner::from_preset("gemini")?)),
            "opencode" | "localai" | "vllm" => Ok(Box::new(OpenAiCompatPlanner::from_preset("opencode")?)),
            "openai" | "openai_compat" | "compat" => Ok(Box::new(OpenAiCompatPlanner::from_preset("openai")?)),
            other => {
                if other.starts_with("http://") || other.starts_with("https://") {
                    Ok(Box::new(OpenAiCompatPlanner::new("custom", other, "default", None)))
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
                    Ok(Box::new(OpenAiCompatPlanner::new(other, endpoint, model, None)))
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

pub fn pick_planner_por_nombre(nombre: Option<&str>) -> Result<Box<dyn Planner>> {
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
                paint("antOS · Asistente de Configuración de Motor LLM Local (T19.3)", BOLD)
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
                println!("    ✓ Ollama detectado y respondiendo en {}", paint("http://127.0.0.1:11434", GREEN));
            } else if status.opencode_available {
                println!("    ✓ OpenCode detectado y respondiendo en {}", paint("http://127.0.0.1:8080/v1", GREEN));
            } else {
                println!("    ○ Ningún motor local está corriendo actualmente.");
                println!("      • Para instalar Ollama con antpkg ejecuta: {}", paint("antos pkg install recipes/ollama.toml", YELLOW));
                println!("      • O inicia el servicio si ya lo tienes:     {}", paint("ollama serve &", CYAN));
                println!("      • O descarga Ollama directamente desde:     https://ollama.com\n");
            }

            // 2. Recomendaciones de modelos de desarrollo
            println!("  [2/3] Modelos recomendados para antOS antFlow:");
            println!("    • {} (Recomendado: balance perfecto velocidad y sintaxis de código)", paint("qwen2.5-coder:7b", GREEN));
            println!("    • {} (Especializado en refactorización y depuración)", paint("deepseek-coder:6.7b", CYAN));
            println!("    • {} (Ultraligero para portátiles sin GPU dedicada)", paint("qwen2.5-coder:1.5b", DIM));

            // 3. Configuración persistente del motor
            println!("\n  [3/3] Aplicando configuración:");
            config.set_active_provider("ollama", Some(target_model), None);
            config.save_to_state(&ctx.state)?;

            println!("    ✓ Motor predeterminado fijado en: {}", paint("ollama", GREEN));
            println!("    ✓ Modelo de código seleccionado:  {}", paint(target_model, CYAN));
            println!("\n  Pasos siguientes:");
            println!("    1. Si aún no tienes el modelo descargado, ejecuta: {}", paint(&format!("ollama pull {target_model}"), YELLOW));
            println!("    2. Verifica la inferencia con:                    {}", paint("antos llm test", CYAN));
            println!("    3. Explora alternativas gratuitas con:            {}\n", paint("antos llm free", CYAN));
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
                    println!("  Ejemplo: antos llm use openrouter --model deepseek/deepseek-r1:free");
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
                paint("antOS · Catálogo de Modelos y Proveedores 100% Gratuitos (T19.2)", BOLD)
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
                        &format!("antos llm use {} --model {}", r.provider_id, r.default_model),
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
            println!("  Para probar el motor actual: {}\n", paint("antos llm test", CYAN));
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
                Ok(propuesta) => {
                    let elapsed = start.elapsed();
                    println!(
                        "  Latencia de respuesta: {} ms",
                        paint(&elapsed.as_millis().to_string(), GREEN)
                    );
                    println!(
                        "  Pasos generados:       {}",
                        paint(&propuesta.steps.len().to_string(), BOLD)
                    );
                    for (idx, step) in propuesta.steps.iter().enumerate() {
                        println!(
                            "    {}. Capacidad: {} ({:?})",
                            idx + 1,
                            paint(&step.capability, YELLOW),
                            step.args
                        );
                    }
                    if let Some(ref note) = propuesta.nota {
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
                    println!("\n  {} Fallo al ejecutar la prueba: {:#}\n", paint("✗ Error:", RED), e);
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
                        println!("  ● Modelos descargados en Ollama local ({}):", models.len());
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
            println!("\n  Para cambiar de motor: {}\n", paint("antos llm use <proveedor>", CYAN));
        }
        "status" | _ => {
            println!(
                "\n{}",
                paint("antOS · Estado de Motores de Inferencia Multi-LLM (T19.2)", BOLD)
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
            let or_ok = crate::planner::openai_compat::OpenAiCompatPlanner::from_preset("openrouter")
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
            let ollama_ok = ollama_inst.as_ref().map(|o| o.is_available()).unwrap_or(false);
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
            let oc_inst = crate::planner::openai_compat::OpenAiCompatPlanner::from_preset("opencode");
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

// ------------------------------------------------------------------ memory

pub fn cmd_memory(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    let db_path = crate::memory::MemoryEngine::default_db_path(&ctx.workspace);

    match sub {
        "index" | "reindex" => {
            println!(
                "\n{} Escaneando e indexando espacio de trabajo: {}",
                paint("●", GREEN),
                paint(&ctx.workspace.display().to_string(), BOLD)
            );
            let store = crate::memory::MemoryEngine::index_workspace(&ctx.workspace)?;
            crate::memory::MemoryEngine::save(&store, &db_path)?;
            println!(
                "{} Indexación completada: {} fragmentos y {} nodos de grafo guardados en {}\n",
                paint("✓", GREEN),
                paint(&store.chunks.len().to_string(), BOLD),
                paint(&store.graph.nodes.len().to_string(), BOLD),
                paint(&db_path.display().to_string(), DIM)
            );
        }
        "search" | "find" => {
            let query = args.get(1).map(String::as_str).unwrap_or_default();
            if query.is_empty() {
                bail!("uso: antos memory search <texto_de_busqueda> [--limit N]");
            }
            let limit = args
                .iter()
                .position(|a| a == "--limit" || a == "-n")
                .and_then(|i| args.get(i + 1))
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(5);

            let store = if db_path.exists() {
                crate::memory::MemoryEngine::load(&db_path)?
            } else {
                println!(
                    "{} No existe índice previo. Indexando espacio de trabajo por primera vez...",
                    paint("i", YELLOW)
                );
                let s = crate::memory::MemoryEngine::index_workspace(&ctx.workspace)?;
                crate::memory::MemoryEngine::save(&s, &db_path)?;
                s
            };

            let hits = crate::memory::MemoryEngine::search(&store, query, limit);
            println!(
                "\n{}",
                paint(
                    &format!("antOS · Búsqueda Semántica Vectorial para «{query}»"),
                    BOLD
                )
            );
            println!("  Resultados encontrados: {}\n", hits.len());

            if hits.is_empty() {
                println!("  No se encontraron coincidencias relevantes en el código o tickets.\n");
            } else {
                for (idx, h) in hits.iter().enumerate() {
                    let score_badge = paint(&format!("[{:.2}]", h.score), GREEN);
                    let kind_badge = paint(&format!("{:?}", h.kind), DIM);
                    println!(
                        "  {}. {} {} {}:{}",
                        idx + 1,
                        score_badge,
                        kind_badge,
                        paint(&h.path, BOLD),
                        h.line_start
                    );
                    println!("     Título: {}", paint(&h.title, YELLOW));
                    println!("     Extracto: {}\n", paint(&h.snippet, DIM));
                }
            }
        }
        "graph" => {
            let target = args.get(1).map(String::as_str);
            let store = if db_path.exists() {
                crate::memory::MemoryEngine::load(&db_path)?
            } else {
                let s = crate::memory::MemoryEngine::index_workspace(&ctx.workspace)?;
                crate::memory::MemoryEngine::save(&s, &db_path)?;
                s
            };

            println!(
                "\n{}",
                paint(
                    "antOS · Grafo de Contexto y Dependencias del Proyecto",
                    BOLD
                )
            );
            match target {
                Some(t) => {
                    let related = store.graph.related_to(t);
                    println!(
                        "  Relaciones para símbolo o archivo «{}»: {}\n",
                        paint(t, BOLD),
                        related.len()
                    );
                    for (node, edge) in related {
                        println!(
                            "    • {:<18} ──> {} ({})",
                            format!("{:?}", edge),
                            paint(&node.label, BOLD),
                            node.kind
                        );
                    }
                    println!();
                }
                None => {
                    println!(
                        "  Total de nodos:   {}",
                        paint(&store.graph.nodes.len().to_string(), GREEN)
                    );
                    println!(
                        "  Total de aristas: {}\n",
                        paint(&store.graph.edges.len().to_string(), GREEN)
                    );
                    println!("  Usa: antos memory graph <nodo> para inspeccionar relaciones.");
                    println!("  Ejemplo: antos memory graph ticket:T6.1 o file:system/antosd/src/main.rs\n");
                }
            }
        }
        "status" | _ => {
            let exists = db_path.exists();
            println!(
                "\n{}",
                paint("antOS · Memoria Semántica y Grafo de Contexto (T6.2)", BOLD)
            );
            println!(
                "  Ubicación: {}",
                paint(&db_path.display().to_string(), DIM)
            );
            if exists {
                if let Ok(store) = crate::memory::MemoryEngine::load(&db_path) {
                    println!("  Estado:    {}", paint("● Activo / Sincronizado", GREEN));
                    println!(
                        "  Fragmentos: {}",
                        paint(&store.chunks.len().to_string(), BOLD)
                    );
                    println!(
                        "  Nodos:      {}",
                        paint(&store.graph.nodes.len().to_string(), BOLD)
                    );
                    println!(
                        "  Aristas:    {}",
                        paint(&store.graph.edges.len().to_string(), BOLD)
                    );
                } else {
                    println!(
                        "  Estado:    {}",
                        paint("! Archivo de memoria corrupto", RED)
                    );
                }
            } else {
                println!(
                    "  Estado:    {}",
                    paint("○ Sin indexar (Ejecuta: antos memory index)", YELLOW)
                );
            }
            println!();
        }
    }

    Ok(())
}

// ------------------------------------------------------------------ env

pub fn cmd_env(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "init" | "setup" => {
            let profile_arg = args.get(1).map(String::as_str);
            let profile = match profile_arg {
                Some(p) => crate::env::EnvProfile::from_str_loose(p).ok_or_else(|| {
                    anyhow::anyhow!(
                        "perfil desconocido «{p}». Opciones válidas: rust, node, python, go, base"
                    )
                })?,
                None => {
                    crate::env::EnvEngine::detect_stack(&ctx.workspace).unwrap_or(crate::env::EnvProfile::Base)
                }
            };

            let summary = crate::env::EnvEngine::init_profile(&ctx.workspace, profile, true, true)?;
            println!(
                "\n{} Perfil de entorno declarativo inicializado exitosamente.",
                paint("✓", GREEN)
            );
            println!("  Perfil:   {}", paint(&summary.profile, BOLD));
            println!(
                "  Archivos: {}",
                paint(&summary.created_files.join(", "), GREEN)
            );
            println!("  Paquetes: {}\n", paint(&summary.packages.join(", "), DIM));
            println!(
                "  Ejecuta: antos env sync para comprobar la disponibilidad de las herramientas.\n"
            );
        }
        "sync" | "check" => {
            println!(
                "\n{}",
                paint(
                    "antOS · Sincronización y Diagnóstico de Toolchains (T7.1)",
                    BOLD
                )
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            let statuses = crate::env::EnvEngine::check_toolchains(&ctx.workspace)?;
            let mut all_ok = true;

            for s in statuses {
                if s.available {
                    let loc = s.path.unwrap_or_default();
                    println!(
                        "  {} {:<16} ({})",
                        paint("✓", GREEN),
                        paint(&s.name, BOLD),
                        paint(&loc, DIM)
                    );
                } else {
                    all_ok = false;
                    println!(
                        "  {} {:<16} ({})",
                        paint("✗", RED),
                        paint(&s.name, BOLD),
                        paint("no instalado en el sistema o nix-store", RED)
                    );
                }
            }

            println!();
            if all_ok {
                println!(
                    "  {} Todas las toolchains declaradas están disponibles y operativas.\n",
                    paint("✓ Entorno listo:", GREEN)
                );
            } else {
                println!("  {} Faltan herramientas por aprovisionar. Puedes usar devbox shell o nix develop.\n", paint("! Advertencia:", YELLOW));
            }
        }
        "status" | _ => {
            println!(
                "\n{}",
                paint("antOS · Estado del Perfil de Entorno (T7.1)", BOLD)
            );
            let cfg = crate::env::EnvEngine::load_config(&ctx.workspace)?;

            match cfg {
                Some(c) => {
                    println!("  Perfil activo:      {}", paint(&c.profile, GREEN));
                    println!(
                        "  Paquetes declarados: {}",
                        paint(&c.packages.join(", "), BOLD)
                    );
                    let statuses = crate::env::EnvEngine::check_toolchains(&ctx.workspace)?;
                    let available_count = statuses.iter().filter(|s| s.available).count();
                    println!(
                        "  Disponibilidad:     {}/{} herramientas en PATH\n",
                        available_count,
                        statuses.len()
                    );
                }
                None => {
                    let detected = crate::env::EnvEngine::detect_stack(&ctx.workspace);
                    let det_str = detected.map(|d| d.as_str()).unwrap_or("no detectado");
                    println!("  Perfil configurado: {}", paint("○ Ninguno", YELLOW));
                    println!("  Stack detectado:    {}", paint(det_str, BOLD));
                    println!("\n  Usa antos env init [{det_str}] para inicializar devbox.json y flake.nix.\n");
                }
            }
        }
    }

    Ok(())
}

// ------------------------------------------------------------------ quota

pub fn cmd_quota(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "set" => {
            let mut q = crate::sandbox::quota::load_quota(&ctx.workspace)?;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--timeout" | "-t" => {
                        if let Some(val) = args.get(i + 1).and_then(|v| v.parse::<u64>().ok()) {
                            q.timeout_secs = val;
                            i += 1;
                        }
                    }
                    "--memory" | "-m" => {
                        if let Some(val) = args.get(i + 1).and_then(|v| v.parse::<u64>().ok()) {
                            q.max_memory_mb = val;
                            i += 1;
                        }
                    }
                    "--cpu" | "-c" => {
                        if let Some(val) = args.get(i + 1).and_then(|v| v.parse::<u32>().ok()) {
                            q.cpu_quota_percent = val;
                            i += 1;
                        }
                    }
                    "--pids" | "-p" => {
                        if let Some(val) = args.get(i + 1).and_then(|v| v.parse::<u32>().ok()) {
                            q.max_pids = val;
                            i += 1;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }

            crate::sandbox::quota::save_quota(&ctx.workspace, &q)?;
            println!(
                "\n{} Cuotas de recursos de sandbox actualizadas.",
                paint("✓", GREEN)
            );
            println!(
                "  • Timeout:   {}s",
                paint(&q.timeout_secs.to_string(), BOLD)
            );
            println!(
                "  • Memoria:   {} MB",
                paint(&q.max_memory_mb.to_string(), BOLD)
            );
            println!(
                "  • CPU:       {}%",
                paint(&q.cpu_quota_percent.to_string(), BOLD)
            );
            println!(
                "  • Max PIDs:  {} procesos\n",
                paint(&q.max_pids.to_string(), BOLD)
            );
        }
        "reset" => {
            let def = crate::sandbox::quota::ResourceQuota::default();
            crate::sandbox::quota::save_quota(&ctx.workspace, &def)?;
            println!(
                "\n{} Cuotas de sandbox restablecidas a los valores por defecto del sistema.\n",
                paint("✓", GREEN)
            );
        }
        "status" | _ => {
            println!(
                "\n{}",
                paint(
                    "antOS · Cuotas y Límites de Recursos para Sandboxes (T7.2)",
                    BOLD
                )
            );
            let q = crate::sandbox::quota::load_quota(&ctx.workspace)?;
            let cgroup_avail = crate::sandbox::quota::CgroupV2Manager::is_available();

            let backend = if cgroup_avail {
                paint("● cgroups v2 (Linux)", GREEN)
            } else {
                paint("● Seatbelt + Supervisor de Procesos (macOS)", BLUE)
            };

            println!("  Mecanismo:          {backend}");
            println!(
                "  Timeout de agente:  {}",
                paint(&format!("{}s", q.timeout_secs), GREEN)
            );
            println!(
                "  Límite de memoria:  {}",
                paint(&format!("{} MB", q.max_memory_mb), GREEN)
            );
            println!(
                "  Cuota de CPU:       {}",
                paint(&format!("{}%", q.cpu_quota_percent), GREEN)
            );
            println!(
                "  Límite de procesos: {}\n",
                paint(&format!("{} PIDs", q.max_pids), GREEN)
            );
            println!(
                "  Usa antos quota set [--timeout N] [--memory N] [--cpu N] para modificar.\n"
            );
        }
    }

    Ok(())
}

// ------------------------------------------------------------------ diff

pub fn cmd_diff(ctx: &Ctx, args: &[String]) -> Result<()> {
    // T17.2 argument parsing:
    //   antos diff                  — detect project from cwd or scan workspace
    //   antos diff <project>        — diff the named project
    //   antos diff <project> <ref>  — diff the named project against <ref>
    //   antos diff <ref>            — backwards-compat: diff active project / workspace against <ref>
    //
    // A token is treated as a project name when it matches a directory under workspace/.
    // Otherwise it is treated as a git target ref.

    let antos_root = ctx.antos_root.clone();
    let workspace  = &ctx.workspace;

    println!(
        "\n{}",
        paint("antOS · Visor Interactivo de Diffs y Parches (T8.1 / T17.2)", BOLD)
    );
    println!(
        "  Espacio de trabajo: {}",
        paint(&workspace.display().to_string(), DIM)
    );

    // Parse arguments into (project_path, target_ref).
    let (project_path, target) = parse_diff_args(args, workspace, ctx.current_project.as_deref());

    if let Some(ref p) = project_path {
        println!("  Proyecto:           {}", paint(&p.display().to_string(), DIM));
    } else if let Some(ref p) = ctx.current_project {
        println!("  Proyecto activo:    {}", paint(&p.display().to_string(), DIM));
    }
    println!("  Objetivo:           {}\n", paint(&target, BOLD));

    // Case A: a specific project was requested — diff only that project.
    if let Some(proj) = project_path {
        diff_single_project(&proj, &target, antos_root.as_deref());
        return Ok(());
    }

    // Case B: we are inside a project (contextual detection from T17.1).
    if let Some(ref proj) = ctx.current_project {
        diff_single_project(proj, &target, antos_root.as_deref());
        return Ok(());
    }

    // Case C: no project context — scan all projects in workspace/.
    let projects = crate::exec::scan_workspace_projects(workspace);
    if projects.is_empty() {
        println!(
            "  {} No se encontraron proyectos en el espacio de trabajo.\n",
            paint("ℹ Sin proyectos:", DIM)
        );
        return Ok(());
    }

    println!(
        "  {} {} proyecto(s) detectado(s)\n",
        paint("↓ Escaneando:", CYAN),
        projects.len()
    );

    for proj in &projects {
        let proj_name = proj.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| proj.display().to_string());
        println!("  {} {}", paint("┌ proyecto:", BOLD), paint(&proj_name, CYAN));
        diff_single_project(proj, &target, antos_root.as_deref());
    }

    Ok(())
}

/// Parses CLI arguments into (optional project path, target ref).
///
/// Logic:
/// - If the first arg is a directory under `workspace/`, it is the project.
///   The second arg (if present) is the target ref.
/// - Otherwise the first arg (if present) is the target ref.
/// - Falls back to (current_project, "HEAD").

fn parse_diff_args(
    args: &[String],
    workspace: &std::path::Path,
    current_project: Option<&std::path::Path>,
) -> (Option<std::path::PathBuf>, String) {
    match args {
        [] => (None, "HEAD".into()),
        [first] => {
            let candidate = workspace.join(first);
            if candidate.is_dir() {
                (Some(candidate), "HEAD".into())
            } else {
                // Treat as a target ref, keep detected project (or None).
                (current_project.map(|p| p.to_path_buf()), first.clone())
            }
        }
        [first, second, ..] => {
            let candidate = workspace.join(first);
            if candidate.is_dir() {
                (Some(candidate), second.clone())
            } else {
                // first is a ref, not a project name.
                (current_project.map(|p| p.to_path_buf()), first.clone())
            }
        }
    }
}

/// Diffs a single project directory, printing the result to stdout.
///
/// Uses ceiling-aware git root detection (T17.1) to verify the project has its
/// own `.git` before invoking `git diff`. If it does not, prints an informative
/// file listing and actionable guidance.

fn diff_single_project(
    proj: &std::path::Path,
    target: &str,
    antos_root: Option<&std::path::Path>,
) {
    let project_name = proj.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| proj.display().to_string());

    let has_git = crate::git::find_git_root_with_ceiling(proj, antos_root).is_some();

    if !has_git {
        let files = crate::exec::collect_project_files(proj, 20);
        if files.is_empty() {
            println!(
                "  {} {} — directorio vacío (sin archivos ni repositorio Git)\n",
                paint("⚠", YELLOW),
                paint(&project_name, BOLD)
            );
        } else {
            println!(
                "  {} {} — {} archivo(s) detectado(s), sin repositorio Git:",
                paint("⚠", YELLOW),
                paint(&project_name, BOLD),
                files.len()
            );
            for f in &files {
                println!("    {}", paint(f, DIM));
            }
            println!();
            println!(
                "  {} Inicializa el repositorio con: {}",
                paint("→", CYAN),
                paint(&format!("antos project init {}", project_name), DIM)
            );
            println!();
        }
        return;
    }

    // Build git diff with ceiling.
    let ceiling_val = antos_root
        .and_then(|r| r.parent())
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    // Check if HEAD exists. If not, the repo is newly initialized with no commits.
    let mut check_head = std::process::Command::new("git");
    check_head.current_dir(proj).args(["rev-parse", "--verify", "HEAD"]);
    if !ceiling_val.is_empty() {
        check_head.env("GIT_CEILING_DIRECTORIES", &ceiling_val);
    }
    let has_commits = check_head.output().map(|o| o.status.success()).unwrap_or(false);

    if !has_commits && target == "HEAD" {
        let mut status_cmd = std::process::Command::new("git");
        status_cmd.current_dir(proj).args(["status", "--porcelain"]);
        if !ceiling_val.is_empty() {
            status_cmd.env("GIT_CEILING_DIRECTORIES", &ceiling_val);
        }
        if let Ok(st_out) = status_cmd.output() {
            let st_str = String::from_utf8_lossy(&st_out.stdout);
            let lines: Vec<&str> = st_str.lines().filter(|l| !l.trim().is_empty()).collect();
            if lines.is_empty() {
                println!(
                    "  {} {} — repositorio Git inicializado (árbol limpio, sin commits aún).
",
                    paint("✓", GREEN),
                    paint(&project_name, BOLD)
                );
            } else {
                println!(
                    "  {} {} — repositorio Git inicializado ({} archivo(s) pendientes de commit inicial):",
                    paint("●", CYAN),
                    paint(&project_name, BOLD),
                    lines.len()
                );
                for line in &lines {
                    println!("    {}", paint(line.trim(), DIM));
                }
                println!();
            }
        }
        return;
    }

    let mut git_cmd = std::process::Command::new("git");
    git_cmd.current_dir(proj).args(&["diff", target]);
    if !ceiling_val.is_empty() {
        git_cmd.env("GIT_CEILING_DIRECTORIES", &ceiling_val);
    }

    match git_cmd.output() {
        Ok(out) if out.status.success() => {
            let diff_str = String::from_utf8_lossy(&out.stdout);
            let files = crate::diff_view::DiffEngine::parse_unified_diff(&diff_str);
            if files.is_empty() {
                println!(
                    "  {} {} — sin cambios pendientes contra «{}».\n",
                    paint("✓", GREEN),
                    paint(&project_name, BOLD),
                    target
                );
            } else {
                println!(
                    "  {} {} — {} archivo(s) modificado(s):",
                    paint("~", CYAN),
                    paint(&project_name, BOLD),
                    files.len()
                );
                let rendered = crate::diff_view::DiffEngine::render_terminal(&files);
                print!("{rendered}");
            }
        }
        _ => {
            println!(
                "  {} No se pudo ejecutar git diff en «{}».\n",
                paint("✗", RED),
                proj.display()
            );
        }
    }
}


// ----------------------------------------------------------- project / git (T17.3)

pub fn cmd_project(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("list");

    match sub {
        "init" => cmd_project_init(ctx, &args[1..]),
        "list" | "ls" => cmd_project_list(ctx),
        "use" => cmd_use(ctx, &args[1..]),
        "current" => cmd_use(ctx, &[]),
        _ => {
            println!(
                "\n{}\n",
                paint("antOS · Gestión de Proyectos en Workspace (T17.3)", BOLD)
            );
            println!("  Uso:");
            println!("    antos use <nombre>            Fija el proyecto activo en el workspace");
            println!("    antos use --clear             Limpia la selección del proyecto activo");
            println!("    antos project init <nombre> [--branch <rama>] [--lang <lenguaje>]");
            println!("    antos project list            Lista proyectos en workspace/");
            println!("    antos git init [nombre]");
            println!();
            Ok(())
        }
    }
}

pub fn cmd_project_init(ctx: &Ctx, args: &[String]) -> Result<()> {
    let mut project_name: Option<String> = None;
    let mut branch = "main".to_string();
    let mut language_hint: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-b" | "--branch" => {
                if let Some(b) = args.get(i + 1) {
                    branch = b.clone();
                    i += 1;
                }
            }
            "-l" | "--lang" | "--language" => {
                if let Some(l) = args.get(i + 1) {
                    language_hint = Some(l.clone());
                    i += 1;
                }
            }
            other if !other.starts_with('-') && project_name.is_none() => {
                project_name = Some(other.to_string());
            }
            _ => {}
        }
        i += 1;
    }

    let project_dir = if let Some(ref name) = project_name {
        let candidate = std::path::Path::new(name);
        if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            ctx.workspace.join(name)
        }
    } else if let Some(ref current) = ctx.current_project {
        current.clone()
    } else {
        bail!(
            "Especifica el nombre del proyecto a inicializar:\n    antos project init <nombre>\n    antos git init <nombre>"
        );
    };

    println!(
        "\n{}",
        paint("antOS · Inicialización Declarativa de Proyecto Git (T17.3)", BOLD)
    );
    println!(
        "  Espacio de trabajo: {}",
        paint(&ctx.workspace.display().to_string(), DIM)
    );

    let display_name = project_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| project_dir.display().to_string());

    println!("  Proyecto:           {}", paint(&display_name, CYAN));
    println!("  Ruta física:        {}", paint(&project_dir.display().to_string(), DIM));
    println!("  Rama principal:     {}", paint(&branch, GREEN));

    let res = crate::exec::init_project_git_repo(&project_dir, &branch, language_hint.as_deref())?;

    println!("\n  {} {}\n", paint("✓", GREEN), res);
    println!("  Para inspeccionar los cambios del proyecto, ejecuta:");
    println!(
        "      {}\n",
        paint(&format!("antos diff {}", display_name), DIM)
    );

    Ok(())
}

pub fn cmd_project_list(ctx: &Ctx) -> Result<()> {
    println!(
        "\n{}",
        paint("antOS · Proyectos en Espacio de Trabajo (T17.3)", BOLD)
    );
    println!(
        "  Espacio de trabajo: {}\n",
        paint(&ctx.workspace.display().to_string(), DIM)
    );

    let projects = crate::exec::scan_workspace_projects(&ctx.workspace);
    if projects.is_empty() {
        println!("  (no se encontraron proyectos en workspace/)\n");
        println!(
            "  Crea uno con: {}\n",
            paint("antos project init <nombre>", CYAN)
        );
        return Ok(());
    }

    let antos_root = ctx.antos_root.as_deref();

    for proj in &projects {
        let name = proj
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| proj.display().to_string());

        let has_git = crate::git::find_git_root_with_ceiling(proj, antos_root).is_some();
        let lang = crate::exec::detect_project_language(proj);
        let file_count = crate::exec::collect_project_files(proj, 100).len();

        let git_badge = if has_git {
            paint("● Git activo", GREEN)
        } else {
            paint("○ Sin Git", YELLOW)
        };

        let is_active = ctx.current_project.as_ref() == Some(proj);
        let active_badge = if is_active {
            format!(" {}", paint("[ACTIVO]", GREEN))
        } else {
            String::new()
        };

        println!(
            "  • {}{}  [{}]  (stack: {}, {} archivos)",
            paint(&name, BOLD),
            active_badge,
            git_badge,
            paint(&lang, CYAN),
            file_count
        );
    }
    println!();
    println!("  Para fijar el proyecto activo en todos los comandos:");
    println!("    {}\n", paint("antos use <nombre>", CYAN));

    Ok(())
}

// ------------------------------------------------------------------ terminal / vte

pub fn cmd_use(ctx: &Ctx, args: &[String]) -> Result<()> {
    let active_file = ctx.state.join("active_project");

    if let Some(target) = args.first() {
        if target == "--clear" || target == "clear" || target == "none" || target == "system" {
            if active_file.exists() {
                let _ = std::fs::remove_file(&active_file);
            }
            println!(
                "\n{} Selección de proyecto restablecida. antOS operará en ámbito global / automático.\n",
                paint("antOS ·", BOLD)
            );
            return Ok(());
        }

        // Validar si el proyecto existe en workspace
        let project_dir = ctx.workspace.join(target);
        if !project_dir.exists() || !project_dir.is_dir() {
            println!(
                "\n{} El proyecto '{}' no existe en {}",
                paint("antOS Error ·", RED),
                paint(target, YELLOW),
                ctx.workspace.display()
            );
            let projects = crate::exec::scan_workspace_projects(&ctx.workspace);
            if !projects.is_empty() {
                println!("\n  Proyectos disponibles en el workspace:");
                for p in projects {
                    let pname = p.file_name().and_then(|n| n.to_str()).unwrap_or("proyecto");
                    println!("    • {}", paint(pname, CYAN));
                }
            } else {
                println!("  (no hay proyectos creados aún en workspace/)");
            }
            println!(
                "\n  Puedes inicializarlo con: {}\n",
                paint(&format!("antos project init {}", target), GREEN)
            );
            return Ok(());
        }

        // Guardar proyecto activo en state
        std::fs::create_dir_all(&ctx.state)?;
        std::fs::write(&active_file, target.trim())?;

        println!(
            "\n{} Proyecto activo fijado en: {}\n  Directorio: {}\n  Todos los comandos de antOS (tickets, git, agentes, panel, etc.) operarán sobre este proyecto por defecto.\n",
            paint("antOS ·", BOLD),
            paint(target, GREEN),
            project_dir.display()
        );
    } else {
        // Mostrar proyecto activo actual
        println!(
            "\n{} Estado del Proyecto Activo en Workspace:",
            paint("antOS ·", BOLD)
        );
        if let Some(ref cur) = ctx.current_project {
            let name = cur.file_name().and_then(|n| n.to_str()).unwrap_or("desconocido");
            println!("  • Proyecto seleccionado: {}", paint(name, GREEN));
            println!("  • Ruta en disco:         {}", cur.display());
            if let Ok(active_name) = std::fs::read_to_string(&active_file) {
                if active_name.trim() == name {
                    println!("  • Origen:                Configurado persistentemente vía 'antos use'");
                } else {
                    println!("  • Origen:                Detectado automáticamente por directorio actual (CWD)");
                }
            } else {
                println!("  • Origen:                Detectado automáticamente por directorio actual (CWD)");
            }
        } else {
            println!("  • Proyecto seleccionado: {}", paint("(ninguno / ámbito del sistema)", YELLOW));
            println!("  • Espacio de trabajo:    {}", ctx.workspace.display());
        }
        println!("\n  Uso:");
        println!("    antos use <nombre-proyecto>   Selecciona el proyecto activo para todos los comandos");
        println!("    antos use --clear             Limpia la selección activa (vuelve a detección automática)");
        println!("    antos project list            Lista todos los proyectos en workspace/\n");
    }
    Ok(())
}

pub fn cmd_git(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "init" => cmd_project_init(ctx, &args[1..]),
        "diff" => cmd_diff(ctx, &args[1..]),
        "status" | "st" => {
            let target_dir = ctx.current_project.as_deref().unwrap_or(&ctx.workspace);
            let proj_name = target_dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| target_dir.display().to_string());

            println!(
                "\n{} {}",
                paint("antOS Git · Estado de", BOLD),
                paint(&proj_name, CYAN)
            );
            println!("  Directorio: {}", paint(&target_dir.display().to_string(), DIM));

            let antos_root = ctx.antos_root.as_deref();
            if crate::git::find_git_root_with_ceiling(target_dir, antos_root).is_none() {
                println!(
                    "  {} No es un repositorio Git propio.\n  Inicialízalo con: {}\n",
                    paint("⚠", YELLOW),
                    paint(&format!("antos project init {}", proj_name), CYAN)
                );
                return Ok(());
            }

            if let Some(status) = crate::git::GitAnalyzer::global().consultar_estado(target_dir)? {
                let branch_str = status.branch.unwrap_or_else(|| "HEAD desacoplado".into());
                println!("  Rama activa:    {}", paint(&branch_str, GREEN));
                println!("  Sincronización: +{} / -{}", status.ahead, status.behind);
                println!("  Modificados:    {}", status.modified.len());
                println!("  Staged:         {}", status.staged.len());
                println!("  Sin seguimiento: {}\n", status.untracked.len());
            } else {
                println!("  (no se pudo determinar el estado)\n");
            }
            Ok(())
        }
        _ => {
            println!(
                "\n{}\n",
                paint("antOS · Integración Git Aislada (T17.3)", BOLD)
            );
            println!("  Subcomandos:");
            println!("    antos git init [nombre]");
            println!("    antos git status");
            println!("    antos git diff [ref]");
            println!();
            Ok(())
        }
    }
}

pub fn cmd_terminal(args: &[String]) -> Result<()> {
    let mut session = crate::vte::TerminalSession::new("vte-cli");
    println!(
        "\n{}",
        paint("antOS · Consola Terminal VTE Embebida (T8.1)", BOLD)
    );
    println!(
        "  Shell interactivo:  {}\n",
        paint(&session.active_shell, GREEN)
    );

    if args.is_empty() {
        println!("  Consola terminal interactiva lista. Para ejecutar comandos usa:");
        println!("    antos terminal \"<comando>\"\n");
    } else {
        let cmd = args.join(" ");
        session.execute_command(&cmd)?;
        for line in &session.buffer {
            println!("{}", line.raw);
        }
        println!();
    }

    Ok(())
}

// ------------------------------------------------------------------ notify / approvals

pub fn cmd_notify(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str);
    let engine = crate::notification::NotificationEngine::global();

    match sub {
        Some("approve" | "aprobar") => {
            let id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("uso: antos notify approve <ID>"))?;
            let (ok, msg) = engine.handle_action(
                &ctx.workspace,
                id,
                antos_protocolo::NotificationAction::Approve,
            )?;
            if ok {
                println!("\n{} {msg}\n", paint("✓", GREEN));
            } else {
                println!("\n{} {msg}\n", paint("✗", RED));
            }
        }
        Some("reject" | "rechazar" | "rollback") => {
            let id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("uso: antos notify reject <ID>"))?;
            let (ok, msg) = engine.handle_action(
                &ctx.workspace,
                id,
                antos_protocolo::NotificationAction::Reject,
            )?;
            if ok {
                println!("\n{} {msg}\n", paint("✓", GREEN));
            } else {
                println!("\n{} {msg}\n", paint("✗", RED));
            }
        }
        Some("dismiss" | "descartar") => {
            let id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("uso: antos notify dismiss <ID>"))?;
            let (ok, msg) = engine.handle_action(
                &ctx.workspace,
                id,
                antos_protocolo::NotificationAction::Dismiss,
            )?;
            if ok {
                println!("\n{} {msg}\n", paint("✓", GREEN));
            } else {
                println!("\n{} {msg}\n", paint("✗", RED));
            }
        }
        Some("clear" | "limpiar") => {
            let count = engine.clear(&ctx.workspace)?;
            println!(
                "\n{} Se limpiaron {} notificaciones leídas.\n",
                paint("✓", GREEN),
                count
            );
        }
        _ => {
            let list = engine.list(&ctx.workspace)?;
            println!(
                "\n{}",
                paint(
                    "antOS · Bandeja de Notificaciones y Aprobaciones Asíncronas (T8.2)",
                    BOLD
                )
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            if list.is_empty() {
                println!(
                    "  {} No hay notificaciones ni aprobaciones pendientes.\n",
                    paint("✓ Bandeja al día:", GREEN)
                );
                println!(
                    "  Los agentes multi-agente antFlow notificarán aquí cuando completen tareas."
                );
                println!("  Comandos: antos notify approve <ID> | antos notify reject <ID>\n");
            } else {
                for n in &list {
                    let mark = if n.read {
                        paint("○ leída", DIM)
                    } else {
                        paint("● NUEVA", YELLOW)
                    };
                    let kind_badge = match n.kind {
                        antos_protocolo::NotificationKind::ApprovalRequired => {
                            paint("⚠️ APROBACIÓN REQUERIDA", YELLOW)
                        }
                        antos_protocolo::NotificationKind::TaskFinished => {
                            paint("✓ TAREA COMPLETADA", GREEN)
                        }
                        antos_protocolo::NotificationKind::QAFailed => paint("✗ QA FALLIDO", RED),
                        antos_protocolo::NotificationKind::SecurityAlert => {
                            paint("🛡️ ALERTA SEGURIDAD", RED)
                        }
                        antos_protocolo::NotificationKind::System => paint("ℹ️ SISTEMA", CYAN),
                    };

                    println!(
                        "  {} [{}] {} — {}",
                        mark,
                        paint(&n.id, BOLD),
                        kind_badge,
                        paint(&n.ticket_id, BOLD)
                    );
                    println!("     {}: {}", paint("Título", DIM), n.title);
                    println!("     {}: {}\n", paint("Detalle", DIM), n.body);
                }
                println!("  Usa «antos notify approve <ID>» para autorizar o «antos notify reject <ID>» para rollback.\n");
            }
        }
    }

    Ok(())
}

// --------------------------------------------------------------------- mesh

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

            let lsm_badge = if status.lsm_enabled {
                paint("● KERNEL LSM ACTIVO (BPF Enforcing)", GREEN)
            } else {
                paint("○ EMULACIÓN ESPACIO DE USUARIO (Auditoría activa)", YELLOW)
            };
            println!("  Estado del soporte eBPF: {}", lsm_badge);
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
                        antos_protocolo::EbpfSecurityAction::Allowed => paint("✓ ALLOW", GREEN),
                        antos_protocolo::EbpfSecurityAction::Blocked => paint("⛔ BLOCK", RED),
                        antos_protocolo::EbpfSecurityAction::Audited => paint("👁 AUDIT", YELLOW),
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
                        antos_protocolo::EbpfSecurityAction::Allowed => paint("✓", GREEN),
                        antos_protocolo::EbpfSecurityAction::Blocked => paint("⛔", RED),
                        antos_protocolo::EbpfSecurityAction::Audited => paint("👁", YELLOW),
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
                "socket" | "net" | "red" => antos_protocolo::EbpfHookKind::SocketConnect,
                "bprm" | "exec" => antos_protocolo::EbpfHookKind::BprmCheckSecurity,
                "syscall" => antos_protocolo::EbpfHookKind::SyscallTrace,
                _ => antos_protocolo::EbpfHookKind::FileOpen,
            };
            let target = args
                .get(2)
                .map(String::as_str)
                .unwrap_or_else(|| match hook {
                    antos_protocolo::EbpfHookKind::SocketConnect => "192.168.1.50:4444",
                    antos_protocolo::EbpfHookKind::FileOpen => "/etc/shadow",
                    antos_protocolo::EbpfHookKind::BprmCheckSecurity => "/bin/nc",
                    antos_protocolo::EbpfHookKind::SyscallTrace => "ptrace",
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

            let lsm_badge = if status.lsm_enabled {
                paint("● KERNEL LSM ACTIVO", GREEN)
            } else {
                paint("○ EMULACIÓN ESPACIO USUARIO", YELLOW)
            };
            println!("  Soporte:            {}", lsm_badge);
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
                    paint("  Puntos Calientes de Ejecución (Hotspots):", BOLD)
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

// ---------------------------------------------------------------------- lsp

pub fn cmd_reproduce(ctx: &Ctx, args: &[String]) -> Result<()> {
    let mut log_path = None;
    let mut target_file = None;
    let mut positional = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--log" | "-l" => {
                if let Some(p) = args.get(i + 1) {
                    log_path = Some(p.clone());
                    i += 1;
                }
            }
            "--target" | "-t" => {
                if let Some(t) = args.get(i + 1) {
                    target_file = Some(t.clone());
                    i += 1;
                }
            }
            val if !val.starts_with('-') => {
                positional.push(val.to_string());
            }
            _ => {}
        }
        i += 1;
    }

    let error_input = if let Some(ref path) = log_path {
        let p = std::path::Path::new(path);
        let resolved = if p.is_absolute() { p.to_path_buf() } else { ctx.workspace.join(p) };
        std::fs::read_to_string(&resolved)
            .with_context(|| format!("No se pudo leer el archivo de log {}", resolved.display()))?
    } else if !positional.is_empty() {
        positional.join(" ")
    } else {
        bail!("Uso: antos reproduce \"<stack trace / log de error>\" [--target <archivo>] [--log <ruta_log>]");
    };

    println!("\n{}", paint("🧪 antOS TDD Engine · Reproducción Autónoma de Bugs (T20.2)", BOLD));
    println!("  Analizando traza y aislando contexto de falla...");

    let report = crate::reproduce::TddEngine::run_reproduce_pipeline(
        &error_input,
        target_file.as_deref(),
        &ctx.workspace,
        &ctx.state,
    )?;

    println!("\n  {} [{}]", paint("ID del Caso:", CYAN), report.id);
    println!("  {} {:?}", paint("Lenguaje Detectado:", BOLD), report.diagnostic.language);
    println!("  {} {}", paint("Tipo de Error:", YELLOW), report.diagnostic.error_type);
    println!("  {} {}", paint("Mensaje:", RED), report.diagnostic.message);

    if let Some(ref f) = report.diagnostic.target_file {
        println!("  {} {}:{}", paint("Ubicación:", BOLD), f, report.diagnostic.target_line.unwrap_or(0));
    }
    if let Some(ref fn_name) = report.diagnostic.target_function {
        println!("  {} {}", paint("Función / Símbolo:", BOLD), fn_name);
    }
    if !report.diagnostic.frames.is_empty() {
        println!("  {} ({} niveles capturados):", paint("Pila de Llamadas:", DIM), report.diagnostic.frames.len());
        for frame in report.diagnostic.frames.iter().take(3) {
            println!("    • {}:{} [{}]", frame.file, frame.line.unwrap_or(0), frame.function.as_deref().unwrap_or("fn"));
        }
    }

    println!("\n  {} {}", paint("Fase del Ciclo:", BOLD), paint(report.phase.label(), GREEN));
    println!("  {} {}", paint("Test de Regresión:", CYAN), report.test_file);
    if let Some(ref fix) = report.fix_summary {
        println!("  {} {}", paint("Propuesta Correctiva:", GREEN), fix);
    }
    println!(
        "  {} {}",
        paint("Auditoría antOS:", BOLD),
        if report.audited {
            paint("✅ Aislado en sandbox y certificado contra regresiones", GREEN)
        } else {
            paint("⚠️ Pendiente de validación", YELLOW)
        }
    );
    println!("  {} .antos/reproduce/{}/\n", paint("Artefactos:", DIM), report.id);

    Ok(())
}

pub fn cmd_testgen(ctx: &Ctx, args: &[String]) -> Result<()> {
    let mut target = None;
    let mut suite = "unit".to_string();
    let mut cases = 3usize;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--target" | "-t" => {
                if let Some(t) = args.get(i + 1) {
                    target = Some(t.clone());
                    i += 1;
                }
            }
            "--suite" | "-s" => {
                if let Some(s) = args.get(i + 1) {
                    suite = s.clone();
                    i += 1;
                }
            }
            "--cases" | "-c" => {
                if let Some(c) = args.get(i + 1).and_then(|v| v.parse::<usize>().ok()) {
                    cases = c;
                    i += 1;
                }
            }
            val if !val.starts_with('-') && target.is_none() => {
                target = Some(val.to_string());
            }
            _ => {}
        }
        i += 1;
    }

    let target = target.unwrap_or_else(|| "src/lib.rs".into());

    println!("\n{}", paint("⚡ antOS TestGen · Generador Autónomo de Tests (T20.2)", BOLD));
    println!("  Generando suite de tests para «{}»...", target);

    let report = crate::reproduce::TddEngine::generate_tests_for_target(
        &target,
        &suite,
        cases,
        &ctx.workspace,
    )?;

    println!("\n  {} [{}]", paint("ID de Suite:", CYAN), report.id);
    println!("  {} {:?}", paint("Lenguaje:", BOLD), report.diagnostic.language);
    println!("  {} {}", paint("Objetivo:", CYAN), target);
    println!("  {} {} ({} casos)", paint("Tipo de Suite:", YELLOW), suite, cases);
    println!("  {} {}", paint("Archivo Generado:", GREEN), report.test_file);
    if let Some(ref summary) = report.fix_summary {
        println!("  {} {}\n", paint("Resumen:", DIM), summary);
    }

    Ok(())
}

// ------------------------------------------------------------------ ci / hooks (T20.3)

pub fn cmd_snapshot(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    match sub {
        "create" | "crear" | "save" => {
            let mut label = None;
            let mut author = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--author" | "-a" => {
                        if let Some(a) = args.get(i + 1) {
                            author = Some(a.as_str());
                            i += 1;
                        }
                    }
                    val if !val.starts_with('-') && label.is_none() => {
                        label = Some(val);
                    }
                    _ => {}
                }
                i += 1;
            }

            println!("\n{}", paint("📸 antOS Time Machine · Creando Instantánea Atómica (T20.4)", BOLD));
            let meta = crate::time_machine::TimeMachineEngine::create_snapshot(
                &ctx.workspace,
                &ctx.state,
                label,
                author,
            )?;

            println!("  {} [{}]", paint("ID Instantánea:", CYAN), meta.id);
            if let Some(ref l) = meta.label {
                println!("  {} {}", paint("Etiqueta:", GREEN), l);
            }
            println!("  {} {}", paint("Autor:", BOLD), meta.author);
            if let Some(ref br) = meta.git_branch {
                println!("  {} {} ({})", paint("Rama Git:", BOLD), br, meta.git_commit.as_deref().unwrap_or("?"));
            }
            println!("  {} {} archivos ({} KiB)", paint("Árbol de Código:", BOLD), meta.files_count, meta.total_bytes / 1024);
            if !meta.services_included.is_empty() {
                println!("  {} {}", paint("Servicios Respaldados:", YELLOW), meta.services_included.join(", "));
            }
            if meta.memory_graph_included {
                println!("  {} {}", paint("Memoria Semántica:", CYAN), "Grafo de contexto preservado");
            }
            println!("  {} {}", paint("Mecanismo CoW:", DIM), meta.method);
            println!("  Estado del entorno congelado y protegido con capacidad de rollback.\n");
            Ok(())
        }
        "list" | "ls" | "status" => {
            let list = crate::time_machine::TimeMachineEngine::list_snapshots(&ctx.state)?;
            println!("\n{}", paint("⏱️ antOS Time Machine · Cronología de Instantáneas de Estado (T20.4)", BOLD));
            if list.is_empty() {
                println!("  No hay instantáneas registradas en .antos/snapshots/dev/\n");
                println!("  Crea una nueva con: antos snapshot create [etiqueta]\n");
                return Ok(());
            }

            println!("  {:<26} {:<18} {:<10} {:<12} {:<10}",
                paint("ID", BOLD), paint("ETIQUETA", BOLD), paint("ARCHIVOS", BOLD), paint("TAMAÑO", BOLD), paint("RAMA", BOLD));
            println!("  {}", "─".repeat(80));
            for s in &list {
                let lbl = s.label.as_deref().unwrap_or("—");
                let size_str = format!("{} KiB", s.total_bytes / 1024);
                let branch_str = s.git_branch.as_deref().unwrap_or("—");
                println!("  {:<26} {:<18} {:<10} {:<12} {:<10}",
                    paint(&s.id, CYAN), lbl, s.files_count, size_str, branch_str);
            }
            println!("\n  Total: {} instantánea(s) disponibles para restauración inmediata.\n", list.len());
            Ok(())
        }
        "restore" | "restaurar" | "revert" => {
            let target = args.get(1).map(String::as_str);
            let Some(id_or_label) = target else {
                bail!("Uso: antos snapshot restore <id|etiqueta> [--no-rescue]");
            };
            let create_rescue = !args.iter().any(|a| a == "--no-rescue");

            println!("\n{}", paint("⏪ antOS Time Machine · Restaurando Estado del Entorno (T20.4)", BOLD));
            println!("  Objetivo: «{}»", paint(id_or_label, CYAN));

            let res = crate::time_machine::TimeMachineEngine::restore_snapshot(
                &ctx.workspace,
                &ctx.state,
                id_or_label,
                create_rescue,
            )?;

            println!("  {} {}", paint("Instantánea Restaurada:", GREEN), res.snapshot_id);
            if let Some(ref rescue) = res.rescue_snapshot_id {
                println!("  {} [{}]", paint("Snapshot de Rescate Creado:", YELLOW), rescue);
            }
            println!("  {} {} archivos actualizados / {}", paint("Operaciones de Archivo:", BOLD), res.files_restored, paint(&format!("{} archivos eliminados (untracked)", res.files_deleted), DIM));
            if !res.services_restored.is_empty() {
                println!("  {} {}", paint("Servicios Restaurados:", YELLOW), res.services_restored.join(", "));
            }
            if res.memory_graph_restored {
                println!("  {} Grafo vectorial reestablecido", paint("Memoria Semántica:", CYAN));
            }
            println!("  {} {} ms", paint("Tiempo de Inversión:", BOLD), res.duration_ms);
            println!("  ✅ Entorno revertido con éxito al punto exacto capturado.\n");
            Ok(())
        }
        "delete" | "del" | "rm" | "eliminar" => {
            let target = args.get(1).map(String::as_str);
            let Some(id_or_label) = target else {
                bail!("Uso: antos snapshot delete <id|etiqueta>");
            };
            println!("\n{}", paint("🗑️ antOS Time Machine · Eliminando Instantánea (T20.4)", BOLD));
            let deleted = crate::time_machine::TimeMachineEngine::delete_snapshot(&ctx.state, id_or_label)?;
            println!("  Instantánea [{}] eliminada correctamente y espacio liberado.\n", paint(&deleted, CYAN));
            Ok(())
        }
        _ => {
            println!("\n{}", paint("antOS Time Machine · Ayuda (T20.4)", BOLD));
            println!("  Uso:");
            println!("    antos snapshot create [etiqueta]     Crea una instantánea completa");
            println!("    antos snapshot list                  Lista instantáneas disponibles");
            println!("    antos snapshot restore <id|etiqueta> Revierte el entorno completo");
            println!("    antos snapshot delete <id|etiqueta>  Elimina una instantánea\n");
            Ok(())
        }
    }
}

// ------------------------------------------------------------------ bench & perf diff (T21.1)

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

            println!("\n{}", paint("⚡ antOS Bench Diff · Comparador de Rendimiento en Worktrees (T21.1)", BOLD));
            println!("  Comparando contra rama base: «{}» (umbral regresión: {:.1}%)",
                paint(against.unwrap_or("master"), CYAN),
                threshold.unwrap_or(15.0));

            let report = crate::bench::BenchEngine::compare_benchmark(
                &ctx.workspace,
                &ctx.state,
                against,
                threshold,
            )?;

            println!("\n  {} [{}]", paint("ID Comparación:", BOLD), report.id);
            println!("  {} vs {}", paint(&report.base_branch, DIM), paint(&report.target_branch, CYAN));
            println!("  {}", "─".repeat(84));
            println!("  {:<26} {:<14} {:<14} {:<12} {}",
                paint("BENCHMARK", BOLD),
                paint("BASE (MEDIA)", BOLD),
                paint("OBJETIVO", BOLD),
                paint("VARIACIÓN Δ", BOLD),
                paint("ESTADO", BOLD));
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

                println!("  {:<26} {:<14} {:<14} {:<12} {}",
                    paint(&c.name, CYAN),
                    format!("{} ns", c.base_mean_ns),
                    format!("{} ns", c.target_mean_ns),
                    color_delta,
                    status_badge);
            }
            println!("  {}", "─".repeat(84));
            println!("\n  {} {}", paint("Veredicto Auditor antFlow:", BOLD), report.auditor_verdict);
            println!();
            Ok(())
        }
        "history" | "historial" | "hist" => {
            let history = crate::bench::BenchEngine::load_history(&ctx.state);
            println!("\n{}", paint("📈 antOS Bench · Historial de Rendimiento Continuo (T21.1)", BOLD));
            if history.is_empty() {
                println!("  No hay registros de benchmarks previos en .antos/bench_history.json\n");
                println!("  Ejecuta uno con: antos bench\n");
                return Ok(());
            }

            println!("  {:<22} {:<16} {:<10} {:<14} {}",
                paint("ID", BOLD), paint("RAMA", BOLD), paint("SUITE", BOLD), paint("MÉTRICAS", BOLD), paint("DURACIÓN", BOLD));
            println!("  {}", "─".repeat(75));
            for h in &history {
                println!("  {:<22} {:<16} {:<10} {:<14} {} ms",
                    paint(&h.id, CYAN),
                    h.branch,
                    h.suite_name,
                    format!("{} pruebas", h.metrics.len()),
                    h.total_duration_ms);
            }
            println!("\n  Total: {} corridas registradas.\n", history.len());
            Ok(())
        }
        _ => {
            let target = if sub != "run" { Some(sub) } else { args.get(1).map(String::as_str) };
            println!("\n{}", paint("⚡ antOS Continuous Benchmarking · Ejecución de Suite (T21.1)", BOLD));
            if let Some(t) = target {
                println!("  Objetivo específico: «{}»", paint(t, CYAN));
            }

            let report = crate::bench::BenchEngine::run_benchmark(
                &ctx.workspace,
                &ctx.state,
                target,
            )?;

            println!("  {} [{}]", paint("ID Ejecución:", BOLD), report.id);
            println!("  {} {} ({})", paint("Contexto Git:", BOLD), report.branch, report.commit.as_deref().unwrap_or("—"));
            println!("  {} {} ms\n", paint("Tiempo Total:", BOLD), report.total_duration_ms);

            println!("  {:<26} {:<12} {:<12} {:<12} {:<14}",
                paint("MÉTRICA", BOLD),
                paint("MEDIA", BOLD),
                paint("P95", BOLD),
                paint("P99", BOLD),
                paint("RENDIMIENTO", BOLD));
            println!("  {}", "─".repeat(78));

            for m in &report.metrics {
                println!("  {:<26} {:<12} {:<12} {:<12} {:<14}",
                    paint(&m.name, CYAN),
                    format!("{} ns", m.mean_ns),
                    format!("{} ns", m.p95_ns),
                    format!("{} ns", m.p99_ns),
                    format!("{:.0} ops/s", m.ops_per_sec));
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
            println!("\n{}", paint("🐙 antOS Git Forge · Issues Abiertos en Repositorio Remoto (T21.2)", BOLD));
            let issues = crate::forge::ForgeEngine::list_issues(&ctx.workspace, &ctx.state)?;
            if issues.is_empty() {
                println!("  No se encontraron issues abiertos en el repositorio remoto.\n");
                return Ok(());
            }

            println!("  {:<8} {:<42} {:<16} {}",
                paint("NUM", BOLD),
                paint("TÍTULO", BOLD),
                paint("AUTOR", BOLD),
                paint("ETIQUETAS", BOLD));
            println!("  {}", "─".repeat(84));

            for issue in &issues {
                let tags = if issue.labels.is_empty() { "—".to_string() } else { issue.labels.join(", ") };
                println!("  #{:<7} {:<42} @{:<15} {}",
                    paint(&issue.number.to_string(), CYAN),
                    if issue.title.len() > 40 { format!("{}...", &issue.title[..37]) } else { issue.title.clone() },
                    issue.author,
                    paint(&tags, DIM));
            }
            println!("  {}", "─".repeat(84));
            println!("\n  Importa un issue a ticket técnico local con: antos issue import <numero>\n");
            Ok(())
        }
        "import" | "sync" => {
            let id = args.get(1).map(String::as_str).unwrap_or("42");
            println!("\n{}", paint("📥 antOS Git Forge · Importando Issue Remoto (T21.2)", BOLD));

            let (ticket_id, path, title) = crate::forge::ForgeEngine::import_issue(&ctx.workspace, &ctx.state, id)?;

            println!("  Ticket Creado: [{}]", paint(&ticket_id, CYAN));
            println!("  Título:        {}", title);
            println!("  Ubicación:     {}", paint(&path.display().to_string(), DIM));
            println!("\n  El issue ha sido estructurado con criterios de aceptación en docs/tickets/.");
            println!("  Despáchalo al equipo con: antos agent run {}\n", ticket_id);
            Ok(())
        }
        _ => {
            println!("\nUso:");
            println!("  antos issue list                  Lista issues abiertos en GitHub/GitLab");
            println!("  antos issue import <id_o_url>     Importa issue y genera ticket técnico local\n");
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

            println!("\n{}", paint("🚀 antOS Git Forge · Publicando Pull Request / Merge Request (T21.2)", BOLD));
            let pr = crate::forge::ForgeEngine::create_pull_request(
                &ctx.workspace,
                &ctx.state,
                title,
                base,
                draft,
            )?;

            println!("  PR #{}:      {}", paint(&pr.number.to_string(), CYAN), paint(&pr.title, BOLD));
            println!("  Rama:        {} -> {}", paint(&pr.head_branch, DIM), paint(&pr.base_branch, CYAN));
            println!("  Modo:        {}", if pr.draft { "Borrador (Draft PR)" } else { "Listo para Revisión (Ready)" });
            println!("  Enlace Web:  {}", paint(&pr.url, CYAN));
            println!("\n  Pull Request formulado y publicado exitosamente con certificación del Auditor.\n");
            Ok(())
        }
        "status" | "info" => {
            let pr_num = args.get(1).and_then(|n| n.trim_start_matches('#').parse::<u64>().ok());
            println!("\n{}", paint("🔍 antOS Git Forge · Estado de Pull Request Remoto (T21.2)", BOLD));

            let status = crate::forge::ForgeEngine::get_pull_request_status(&ctx.state, pr_num)?;

            println!("  PR #{}:      {}", paint(&status.number.to_string(), CYAN), status.title);
            println!("  Estado:      {}", paint(&status.state.to_uppercase(), GREEN));
            println!("  Fusión:      {}", if status.mergeable { "Limpia (Sin conflictos)" } else { "Conflictos detectados" });
            println!("  CI Checks:   {}", status.ci_status.as_deref().unwrap_or("En progreso"));
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

// ------------------------------------------------ live architecture & mermaid (T21.3)

pub fn cmd_doc(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("arch");
    match sub {
        "arch" | "diagram" => {
            let mut kind_str = "full";
            let mut output_file = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--type" | "-t" => {
                        if let Some(k) = args.get(i + 1) {
                            kind_str = k.as_str();
                            i += 1;
                        }
                    }
                    "--output" | "-o" => {
                        if let Some(o) = args.get(i + 1) {
                            output_file = Some(o.as_str());
                            i += 1;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }

            let kind = match kind_str {
                "components" | "c4" | "comp" => antos_protocol::ArchDiagramKind::Components,
                "flow" | "ipc" => antos_protocol::ArchDiagramKind::IpcFlow,
                "antflow" | "state" => antos_protocol::ArchDiagramKind::AntFlow,
                _ => antos_protocol::ArchDiagramKind::Full,
            };

            let report = crate::doc_arch::DocArchEngine::generate_diagram(&ctx.workspace, kind);

            if let Some(out_path) = output_file {
                let p = std::path::Path::new(out_path);
                std::fs::write(p, &report.mermaid_content)?;
                println!(
                    "\n{} Diagrama Mermaid guardado en: {}\n",
                    paint("📐 antOS Doc Arch ·", BOLD),
                    paint(out_path, CYAN)
                );
            } else {
                println!("\n{}", paint("📐 antOS Living Architecture · Diagramas Vivos de Arquitectura (T21.3)", BOLD));
                println!("  Tipo:         {}", paint(report.kind.name(), CYAN));
                println!("  Topología:    {} crates | {} módulos demonio | {} capacidades",
                    paint(&report.crates_count.to_string(), CYAN),
                    paint(&report.modules_count.to_string(), CYAN),
                    paint(&report.caps_count.to_string(), CYAN)
                );
                println!("\n```mermaid\n{}\n```\n", report.mermaid_content);
            }
            Ok(())
        }
        "sync" => {
            let target_file = args.get(1).map(String::as_str);
            println!("\n{}", paint("🔄 antOS Doc Sync · Sincronizando Documentación Viva de Arquitectura (T21.3)", BOLD));

            let report = crate::doc_arch::DocArchEngine::sync_docs(&ctx.workspace, target_file)?;

            println!("  Archivos escaneados:    {}", paint(&report.files_scanned.to_string(), CYAN));
            println!("  Archivos actualizados:  {}", paint(&report.files_updated.to_string(), GREEN));
            for p in &report.updated_paths {
                println!("    • {}", paint(p, CYAN));
            }
            println!("  Resultado:              {}\n", report.message);
            Ok(())
        }
        "check" => {
            let target_file = args.get(1).map(String::as_str);
            println!("\n{}", paint("🔍 antOS Doc Check · Verificación de Sincronización Arquitectónica (T21.3)", BOLD));

            match crate::doc_arch::DocArchEngine::check_docs(&ctx.workspace, target_file) {
                Ok(report) => {
                    println!("  {}\n", paint(&report.message, GREEN));
                    Ok(())
                }
                Err(e) => {
                    println!("  {}\n", paint(&format!("❌ {}", e), RED));
                    bail!("{e}");
                }
            }
        }
        _ => {
            println!("\nUso:");
            println!("  antos doc arch [--type components|flow|antflow|all] [--output <file>]");
            println!("  antos doc sync [--file <path>]    Sincroniza e incrusta diagramas en markdown");
            println!("  antos doc check [--file <path>]   Verifica modo CI si la doc está sincronizada\n");
            Ok(())
        }
    }
}

// ------------------------------------------------------------------ desktop

pub fn cmd_screenshot(ctx: &Ctx, args: &[String]) -> Result<()> {
    let target = args.first().map(String::as_str);
    let path = if args.len() > 1 {
        Some(ctx.workspace.join(&args[1]))
    } else if let Some(t) = target {
        if t.ends_with(".png") || t.ends_with(".bmp") {
            Some(ctx.workspace.join(t))
        } else {
            None
        }
    } else {
        None
    };

    let actual_target = if let Some(t) = target {
        if t.ends_with(".png") || t.ends_with(".bmp") {
            None
        } else {
            Some(t)
        }
    } else {
        None
    };

    println!(
        "\n{} Captura de Pantalla Wayland:",
        paint("antOS Screencopy ·", BOLD)
    );
    let engine = crate::vision::VisionEngine::global();
    let res = engine.capture_screen(actual_target, path.as_deref())?;

    let saved = res.saved_path.as_deref().unwrap_or("en memoria");
    println!(
        "  {} Captura completada para «{}»",
        paint("✓", GREEN),
        res.target
    );
    println!("    Destino:      {}", saved);
    println!(
        "    Resolución:   {}x{} píxeles ({})",
        res.width,
        res.height,
        res.format.to_uppercase()
    );
    println!(
        "    Tamaño:       {} KiB ({} bytes)\n",
        res.size_bytes / 1024,
        res.size_bytes
    );
    Ok(())
}

pub fn cmd_qa(ctx: &Ctx, args: &[String]) -> Result<()> {
    let _ = ctx;
    let sub = args.first().map(String::as_str);
    match sub {
        Some("visual" | "vis" | "ui") => {
            let target = args.get(1).map(String::as_str).unwrap_or("desktop");
            let criteria: Vec<String> = if args.len() > 2 {
                args[2..].iter().map(|s| s.replace('_', " ")).collect()
            } else {
                vec![
                    "Verificar contraste de color accesible".to_string(),
                    "Comprobar márgenes y alineación de elementos".to_string(),
                    "Verificar ausencia de desbordamientos visuales".to_string(),
                ]
            };

            println!(
                "\n{} Agente Multimodal VisualQA:",
                paint("antOS QA Visual ·", BOLD)
            );
            println!("  Objetivo: {}", paint(target, CYAN));
            println!("  Criterios evaluados: {}\n", criteria.len());

            let engine = crate::vision::VisionEngine::global();
            let report = engine.inspect_visual(target, &criteria, None)?;

            let status_badge = if report.pass {
                paint("APROBADO", GREEN)
            } else {
                paint("RECHAZADO", RED)
            };

            println!("  {} {}", paint("Resultado:", BOLD), status_badge);
            println!("  Resumen:     {}", report.summary);
            println!(
                "  Resolución:  {}x{} píxeles ({} KiB)\n",
                report.image_width,
                report.image_height,
                report.image_size_bytes / 1024
            );

            println!("  {}:", paint("Hallazgos de Inspección", BOLD));
            for f in &report.findings {
                let sev = match f.severity.as_str() {
                    "critical" => paint("[CRÍTICO]", RED),
                    "warning" => paint("[ADVERTENCIA]", YELLOW),
                    _ => paint("[INFO]", CYAN),
                };
                println!(
                    "    • {} {}: {}",
                    sev,
                    paint(&f.category, BOLD),
                    f.description
                );
                if let Some(ref coords) = f.coordinates {
                    println!("      Coordenadas:   {}", coords);
                }
                println!("      Recomendación: {}", f.recommendation);
            }
            println!();

            if !report.pass {
                bail!("Inspección visual rechazada debido a fallos críticos");
            }
        }
        _ => {
            println!(
                "\n{} Inspección de Calidad Visual (antFlow QA):",
                paint("antOS QA ·", BOLD)
            );
            println!("  Uso:");
            println!("    antos qa visual [target] [criterios...]  Auditoría visual con agente multimodal\n");
        }
    }
    Ok(())
}

// -------------------------------------------------------- disk & partitioning (T15.1)

