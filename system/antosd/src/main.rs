//! antOS — The personal operating system for developers powered by AI.
//!
//! Main entry point for the `antos` / `antosd` executable and daemon.
//! Confinement, subsystem initialization and direct dispatch to `cli::dispatch`.

// Development rule 1 / T31.7: no `unwrap()` or `expect()` in production
// code — a panic in the daemon or in an IPC handler takes down whatever
// else shares its process, and (before T31.7) a poisoned `Mutex` made that
// permanent for the process's lifetime. Enforced by the compiler, not just
// the rule text, so a new call site can't slip back in unnoticed. Every
// `#[cfg(test)] mod tests` explicitly opts back in with its own
// `#![allow(...)]` — tests asserting with `.unwrap()` is normal and
// expected, and is not what this rule is about.
#![deny(clippy::unwrap_used, clippy::expect_used)]

extern crate antos_protocol;

pub mod agent;
pub mod apps;
pub mod autopilot;
pub mod barra;
pub mod bench;
pub mod blast;
pub mod boot;
pub mod capability;
pub mod ci;
pub mod cli;
pub mod collab;
pub mod crypto;
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
pub mod protocol;
pub mod reproduce;
pub mod runtime;
pub mod sandbox;
pub mod service;
pub mod session;
pub mod snapshot;
pub mod spec;
pub mod stacks;
pub mod terminal;
pub mod time_machine;
pub mod util;
pub mod vault;
pub mod vfs;
pub mod vfs_guard;
pub mod vision;
pub mod vm;
pub mod voice;
pub mod vte;
pub mod wasm;
pub mod web;

use anyhow::Result;
use capability::Catalog;
use ctx::Ctx;
use terminal::{paint, RED};

pub use cli::commands::tools::{pick_planner, pick_planner_by_name};

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {e:#}", paint("error:", RED));
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // The confined executor is handled before anything else. It runs INSIDE
    // the enclosure, so it cannot create state directories or read the
    // catalog: it only applies the list of changes that arrives via stdin.
    match std::env::args().nth(1).unwrap_or_default().as_str() {
        sandbox::EXEC_SUBCOMMAND => return sandbox::execute_from_stdin(),
        sandbox::NET_SUBCOMMAND => return sandbox::probe_network_from_inside(),
        _ => {}
    }

    let ctx = Ctx::discover()?;
    let catalog = Catalog::load(&ctx.caps_dir)?;

    cli::dispatch(&ctx, &catalog, std::env::args().skip(1).collect())
}
