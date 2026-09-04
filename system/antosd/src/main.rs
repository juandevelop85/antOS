//! antOS — El sistema operativo personal para desarrolladores impulsado por IA.
//!
//! Entrada principal del ejecutable y demonio `antos` / `antosd`.
//! Confinamiento, inicialización de subsistemas y despacho directo a `cli::dispatch`.

extern crate antos_protocol as antos_protocolo;

pub mod autopilot;
pub mod barra;
pub mod bench;
pub mod blast;
pub mod boot;
pub mod capability;
pub mod ci;
pub mod cli;
pub mod collab;
pub mod ctx;
pub mod desktop;
pub mod dev_tui;
pub mod diff_view;
pub mod distributed;
pub mod doc_arch;
pub mod ebpf;
pub mod env;
pub mod exec;
pub mod flow;
pub mod forge;
pub mod git;
pub mod grants;
pub mod installer;
pub mod ipc;
pub mod journal;
pub mod llm;
pub mod lsp;
pub mod memory;
pub mod mesh;
pub mod net;
pub mod notification;
pub mod pkg;
pub mod plan;
pub mod planner;
pub mod preview;
pub mod profiler;
pub mod protocolo;
pub mod reproduce;
pub mod sandbox;
pub mod service;
pub mod sesion;
pub mod snapshot;
pub mod spec;
pub mod terminal;
pub mod time_machine;
pub mod vault;
pub mod vfs;
pub mod vfs_guard;
pub mod vision;
pub mod vm;
pub mod voz;
pub mod vte;
pub mod wasm;
pub mod web;

use anyhow::Result;
use capability::Catalog;
use ctx::Ctx;
use terminal::{paint, RED};

pub use cli::commands::tools::{pick_planner, pick_planner_por_nombre};

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {e:#}", paint("error:", RED));
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // El ejecutor confinado se atiende antes que nada. Corre DENTRO del
    // recinto, así que no puede crear directorios de estado ni leer el
    // catálogo: solo aplica la lista de cambios que le llega por stdin.
    match std::env::args().nth(1).unwrap_or_default().as_str() {
        sandbox::EXEC_SUBCOMMAND => return sandbox::execute_from_stdin(),
        sandbox::NET_SUBCOMMAND => return sandbox::probe_network_from_inside(),
        _ => {}
    }

    let ctx = Ctx::discover()?;
    let catalog = Catalog::load(&ctx.caps_dir)?;

    cli::dispatch(&ctx, &catalog, std::env::args().skip(1).collect())
}
