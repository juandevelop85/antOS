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

pub fn cmd_intent(ctx: &Ctx, catalog: &Catalog, intent: &str, opts: &Opts) -> Result<()> {
    // Si hay demonio, se le habla. Si no, se hace en proceso.
    //
    // El resultado es idéntico porque ambos caminos usan el MISMO recorrido y
    // el MISMO dibujado: lo único que cambia es dónde corre cada mitad.
    let forzar_local = std::env::var_os("ANTOS_SIN_DEMONIO")
        .or_else(|| std::env::var_os("SYSO_SIN_DEMONIO"))
        .is_some();
    if !forzar_local && crate::ipc::hay_demonio(ctx) {
        return crate::ipc::intencion_remota(
            &crate::ipc::ruta_socket(ctx),
            intent,
            opts.planner.as_deref(),
            opts.dry_run,
            opts.assume_yes,
        );
    }

    let planificador = crate::pick_planner(Some(ctx), opts.planner.as_deref())?;
    let mut terminal = crate::terminal::Terminal::new(opts.assume_yes);
    crate::sesion::intencion(
        ctx,
        catalog,
        intent,
        &*planificador,
        opts.dry_run,
        &mut terminal,
    )
}

// --------------------------------------------------------------------- voz

/// Escuchar es capturar y transcribir. A partir de ahí, el recorrido es
/// exactamente el mismo que si lo hubieras tecleado — incluidos el diff y la
/// confirmación. La voz no salta ningún control: hablar es más cómodo, no
/// más privilegiado.

pub fn cmd_escuchar(ctx: &Ctx, catalog: &Catalog, args: &[String], opts: &Opts) -> Result<()> {
    if args.iter().any(|a| a == "--dispositivos") {
        println!();
        println!("{}", paint("dispositivos de audio", BOLD));
        for linea in crate::voz::Voz::dispositivos()?.lines() {
            println!("  {linea}");
        }
        println!();
        println!(
            "{}",
            paint("elige uno con: antos escucha --dispositivo N", DIM)
        );
        return Ok(());
    }

    let voz = crate::voz::Voz::discover()?;

    let segundos = args
        .iter()
        .position(|a| a == "--segundos")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(5);
    let desde = args
        .iter()
        .position(|a| a == "--desde")
        .and_then(|i| args.get(i + 1));
    let dispositivo = args
        .iter()
        .position(|a| a == "--dispositivo")
        .and_then(|i| args.get(i + 1))
        .map(|d| format!(":{d}"))
        .unwrap_or_else(|| ":default".to_string());

    println!();
    println!("{}", paint("antOS · escucha", BOLD));
    println!(
        "  {}",
        paint(&format!("modelo local: {}", voz.modelo().display()), DIM)
    );

    let captura = ctx.state.join("captura.wav");
    match desde {
        Some(fichero) => {
            voz.normalizar(std::path::Path::new(fichero), &captura)?;
            println!("  {}", paint(&format!("desde el fichero {fichero}"), DIM));
        }
        None => {
            println!(
                "  {}",
                paint(
                    &format!("grabando {segundos} s desde {dispositivo} · habla ahora"),
                    BOLD
                )
            );
            voz.grabar(segundos, &captura, &dispositivo)?;
        }
    }

    let intencion = voz.transcribir(&captura, &vocabulario(catalog))?;
    let _ = std::fs::remove_file(&captura);

    println!(
        "  {} {}",
        paint("he entendido:", DIM),
        paint(&format!("«{intencion}»"), BOLD)
    );

    // Y desde aquí, todo igual que si lo hubieras escrito.
    cmd_intent(ctx, catalog, &intencion, opts)
}

/// Construye la frase con la que se ceba el transcriptor.
///
/// Sale del catálogo, no de una lista escrita a mano: si mañana aparece una
/// capacidad que acepta un lenguaje nuevo, el transcriptor lo aprende solo.

fn vocabulario(catalog: &Catalog) -> String {
    let mut opciones: std::collections::BTreeSet<&str> = Default::default();
    for cap in catalog.caps.values() {
        for spec in cap.params.values() {
            opciones.extend(spec.of.iter().map(String::as_str));
        }
    }

    format!(
        "Órdenes para syso. Crear un proyecto {}. \
         Declarar una dependencia en un proyecto. \
         Leer, escribir o borrar un fichero del espacio de trabajo.",
        opciones.into_iter().collect::<Vec<_>>().join(", ")
    )
}

// ------------------------------------------------------------------ deshacer

