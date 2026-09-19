//! `antos llm profile [local|hybrid|cloud]` (T34.4): reparte los roles de
//! antFlow entre Ollama y la nube de un golpe, con los modelos que le caben
//! a esta máquina.

use crate::ctx::Ctx;
use crate::llm::doctor::{ModelTable, SystemResources};
use crate::llm::{LlmConfig, Profile, ProfileInputs};
use crate::terminal::{paint, BOLD, CYAN, DIM, GREEN, YELLOW};
use anyhow::{bail, Result};

/// El primer proveedor de nube gratuito con clave configurada, como
/// `proveedor:modelo`. `None` si no hay ninguna clave.
fn cloud_with_key(config: &LlmConfig) -> Option<String> {
    for id in ["openrouter", "groq", "gemini"] {
        if crate::planner::openai_compat::OpenAiCompatPlanner::from_preset(id).is_ok() {
            let model = config
                .get_provider_settings(id)
                .and_then(|s| s.model.clone())
                .unwrap_or_else(|| "default".into());
            return Some(format!("{id}:{model}"));
        }
    }
    crate::planner::claude::ClaudePlanner::from_env()
        .ok()
        .map(|p| format!("claude:{}", p.credentials().1))
}

/// Los modelos locales del tier de RAM de esta máquina. Sin tabla o sin
/// RAM legible, el 7B para todo (lo que T33.5 verificó).
fn local_inputs(ctx: &Ctx) -> (ProfileInputs, Option<String>) {
    let table = ModelTable::load(ctx.antos_root.as_deref());
    let tier = SystemResources::detect().and_then(|r| table.tier_for(r.total_ram_gb()).cloned());
    match tier {
        Some(t) => (
            ProfileInputs {
                local_large: t.large,
                local_code: t.code,
                local_fast: t.fast,
                cloud: None,
            },
            Some(t.name),
        ),
        None => (
            ProfileInputs {
                local_large: "qwen2.5-coder:7b".into(),
                local_code: "qwen2.5-coder:7b".into(),
                local_fast: "qwen2.5-coder:7b".into(),
                cloud: None,
            },
            None,
        ),
    }
}

pub fn cmd_profile(ctx: &Ctx, config: &mut LlmConfig, args: &[String]) -> Result<()> {
    let Some(wanted) = args.first() else {
        println!("\n{}", paint("antOS · Perfil de roles antFlow", BOLD));
        println!(
            "  Perfil aplicado: {}",
            config
                .profile
                .map(|p| paint(p.label(), GREEN))
                .unwrap_or_else(|| paint(
                    "ninguno (instalación limpia = local: todos los roles en Ollama)",
                    DIM
                ))
        );
        for (role, model) in config.list_role_models() {
            println!("    {:<10} {}", role, paint(&model, CYAN));
        }
        println!("  Cambiar: antos llm profile local | hybrid | cloud\n");
        return Ok(());
    };
    let Some(profile) = Profile::parse(wanted) else {
        bail!("perfil desconocido «{wanted}»; los perfiles son local, hybrid y cloud");
    };

    let (mut inputs, tier_name) = local_inputs(ctx);
    let mut applied = profile;
    if profile == Profile::Hybrid {
        inputs.cloud = cloud_with_key(config);
        if inputs.cloud.is_none() {
            println!(
                "\n  {} hybrid necesita una clave de nube (OpenRouter, Groq, Gemini o Claude) y no hay ninguna: se aplica local.",
                paint("⚠", YELLOW)
            );
            applied = Profile::Local;
        }
    }
    let assigned = config.apply_profile(applied, &inputs);
    config.save_to_state(&ctx.state)?;

    println!(
        "\n{} Perfil {} aplicado{}",
        paint("✓", GREEN),
        paint(applied.label(), BOLD),
        tier_name
            .map(|t| paint(&format!(" (tier de RAM: {t})"), DIM))
            .unwrap_or_default()
    );
    for (role, model) in &assigned {
        println!("    {:<10} {}", role, paint(model, CYAN));
    }
    if applied == Profile::Local {
        println!(
            "  {} Los modelos que no estén descargados: antos llm pull <modelo>",
            paint("·", DIM)
        );
    }
    println!();
    Ok(())
}
