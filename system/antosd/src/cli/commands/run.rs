#![allow(unused_imports, dead_code, deprecated)]

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

pub fn cmd_intent(ctx: &Ctx, catalog: &Catalog, intent: &str, opts: &Opts) -> Result<()> {
    let force_local =
        crate::util::env_with_legacy_fallback("ANTOS_SIN_DEMONIO", "SYSO_SIN_DEMONIO").is_some();
    if !force_local && crate::ipc::hay_demonio(ctx) {
        return crate::ipc::intencion_remota(
            &crate::ipc::socket_path(ctx),
            intent,
            opts.planner.as_deref(),
            opts.dry_run,
            opts.assume_yes,
        );
    }

    let planner_instance = crate::pick_planner(Some(ctx), opts.planner.as_deref())?;
    let mut terminal = crate::terminal::Terminal::new(opts.assume_yes);
    crate::session::intent_session(
        ctx,
        catalog,
        intent,
        &*planner_instance,
        opts.dry_run,
        &mut terminal,
    )
}

// --------------------------------------------------------------------- voice

pub fn cmd_listen(ctx: &Ctx, catalog: &Catalog, args: &[String], opts: &Opts) -> Result<()> {
    if args
        .iter()
        .any(|a| a == "--dispositivos" || a == "--devices")
    {
        println!();
        println!("{}", paint("audio devices", BOLD));
        for line in crate::voice::Voice::devices()?.lines() {
            println!("  {line}");
        }
        println!();
        println!("{}", paint("pick one with: antos listen --device N", DIM));
        return Ok(());
    }

    let voice = crate::voice::Voice::discover()?;

    let seconds = args
        .iter()
        .position(|a| a == "--segundos" || a == "--seconds")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(5);
    let from = args
        .iter()
        .position(|a| a == "--desde" || a == "--from")
        .and_then(|i| args.get(i + 1));
    let device = args
        .iter()
        .position(|a| a == "--dispositivo" || a == "--device")
        .and_then(|i| args.get(i + 1))
        .map(|d| format!(":{d}"))
        .unwrap_or_else(|| ":default".to_string());

    println!();
    println!("{}", paint("antOS · listen", BOLD));
    println!(
        "  {}",
        paint(
            &format!("local model: {}", voice.model_path().display()),
            DIM
        )
    );

    let capture = ctx.state.join("captura.wav");
    match from {
        Some(file) => {
            voice.normalize(std::path::Path::new(file), &capture)?;
            println!("  {}", paint(&format!("from file {file}"), DIM));
        }
        None => {
            println!(
                "  {}",
                paint(
                    &format!("recording {seconds} s from {device} · speak now"),
                    BOLD
                )
            );
            voice.record(seconds, &capture, &device)?;
        }
    }

    let intent_text = voice.transcribe(&capture, &vocabulary(catalog))?;
    let _ = std::fs::remove_file(&capture);

    println!(
        "  {} {}",
        paint("understood:", DIM),
        paint(&format!("«{intent_text}»"), BOLD)
    );

    // From here on, same as if you had typed it.
    cmd_intent(ctx, catalog, &intent_text, opts)
}

/// Builds the prompt that primes the transcriber.
///
/// Comes from the catalog, not a hand-written list: if tomorrow a capability
/// that accepts a new language appears, the transcriber learns it on its own.
fn vocabulary(catalog: &Catalog) -> String {
    let mut options: std::collections::BTreeSet<&str> = Default::default();
    for cap in catalog.caps.values() {
        for spec in cap.params.values() {
            options.extend(spec.of.iter().map(String::as_str));
        }
    }

    format!(
        "Commands for antOS. Create a project {}. \
         Declare a dependency. \
         Read, write or delete a file in the workspace.",
        options.into_iter().collect::<Vec<_>>().join(", ")
    )
}

// ------------------------------------------------------------------ undo
