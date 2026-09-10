//! `antos memory` (búsqueda semántica) y `antos env` (perfiles declarativos).
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
        _ => {
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
                None => crate::env::EnvEngine::detect_stack(&ctx.workspace)
                    .unwrap_or(crate::env::EnvProfile::Base),
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
        _ => {
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
