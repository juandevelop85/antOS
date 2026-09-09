#![allow(unused_imports, dead_code)]

extern crate antos_protocol;

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

pub fn cmd_mesh(ctx: &Ctx, args: &[String]) -> Result<()> {
    let engine = crate::mesh::MeshEngine::global();
    let sub = args.first().map(String::as_str);

    match sub {
        Some("connect" | "conectar" | "add") => {
            let addr = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("uso: antos mesh connect <IP:PUERTO|MULTIADDR>"))?;
            let peer = engine.connect_peer(&ctx.workspace, addr)?;
            println!(
                "\n{} Nodo peer registrado en antMesh (registro local; sin cifrado ni verificación de identidad — T31.5).",
                paint("✓", GREEN)
            );
            println!("  • Nodo ID:    {}", paint(&peer.id, BOLD));
            println!("  • Hostname:   {}", paint(&peer.hostname, BOLD));
            println!("  • Dirección:  {}", paint(&peer.address, YELLOW));
            println!(
                "  • Latencia:   {} ms",
                paint(&peer.latency_ms.to_string(), GREEN)
            );
            println!(
                "  • Recursos:   {} cores CPU, {} MB RAM, VRAM: {:?}",
                peer.resources.cpu_cores, peer.resources.memory_mb, peer.resources.vram_mb
            );
            println!(
                "  • Modelos:    {}\n",
                peer.resources.available_models.join(", ")
            );
        }
        Some("pair" | "token" | "emparejar") => {
            let token = engine.generate_pairing_token(&ctx.workspace)?;
            let expires_mins = (token.expires_at.saturating_sub(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            ) / 60)
                .max(1);

            println!(
                "\n{}",
                paint("antOS · Token de Emparejamiento antMesh (T9.1)", BOLD)
            );
            println!("  Token de enlace:    {}", paint(&token.token, GREEN));
            println!("  Identidad del nodo: {}", paint(&token.node_id, BOLD));
            println!(
                "  Válido durante:     {} minutos",
                paint(&expires_mins.to_string(), YELLOW)
            );
            println!(
                "\n  Usa «{}» en el nodo remoto para unirte a este clúster.\n",
                paint(&format!("antos mesh connect <ESTA_IP>:9042"), BOLD)
            );
        }
        Some("status" | "list") | _ => {
            let status = engine.status(&ctx.workspace)?;
            println!(
                "\n{}",
                paint("antOS · Registro de Nodos antMesh (T9.1 — sin transporte cifrado aún, T31.5)", BOLD)
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            println!("  {}", paint("● NODO LOCAL", BOLD));
            println!(
                "    ID criptográfico: {}",
                paint(&status.local_node.id, BOLD)
            );
            println!(
                "    Hostname:         {}",
                paint(&status.local_node.hostname, BOLD)
            );
            println!(
                "    Dirección escucha:{}",
                paint(&status.local_node.address, YELLOW)
            );
            println!(
                "    Capacidades:      {} cores CPU · {} MB RAM · VRAM: {:?}",
                status.local_node.resources.cpu_cores,
                status.local_node.resources.memory_mb,
                status.local_node.resources.vram_mb
            );
            println!(
                "    Modelos locales:  {}\n",
                status.local_node.resources.available_models.join(", ")
            );

            println!("  {}", paint("● PEERS CONECTADOS EN LA MALLA", BOLD));
            if status.peers.is_empty() {
                println!("    {} No hay nodos vecinos conectados.", paint("○", DIM));
                println!("    Usa «antos mesh pair» para generar un token de invitación.");
                println!("    Usa «antos mesh connect <IP:9042>» para vincular un nodo remoto.\n");
            } else {
                for p in &status.peers {
                    let conn_mark = if p.connected {
                        paint("● conectado", GREEN)
                    } else {
                        paint("○ desconectado", DIM)
                    };
                    println!(
                        "    {} [{}] {} · {} ({} ms)",
                        conn_mark,
                        paint(&p.id, BOLD),
                        paint(&p.hostname, BOLD),
                        paint(&p.address, YELLOW),
                        paint(&p.latency_ms.to_string(), GREEN)
                    );
                    println!(
                        "       Recursos: {} cores, {} MB RAM, modelos: {}",
                        p.resources.cpu_cores,
                        p.resources.memory_mb,
                        p.resources.available_models.join(", ")
                    );
                }
                println!();
            }
        }
    }

    Ok(())
}

// -------------------------------------------------------------------- swarm

