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

pub fn cmd_pkg(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "install" | "add" | "i" => {
            let pkg_or_recipe = args.get(1).ok_or_else(|| anyhow::anyhow!("Uso: antos pkg install <paquete|receta.toml> [--dry-run]"))?;
            let dry_run = args.iter().any(|a| a == "--dry-run" || a == "-d");

            println!("\n{} Instalando paquete en el almacén inmutable...", paint("antOS antpkg ·", BOLD));
            let rep = crate::pkg::PackageEngine::install(&ctx.state, pkg_or_recipe, dry_run)?;
            let status_badge = if rep.success { paint("INSTALADO", GREEN) } else { paint("ERROR", RED) };
            println!("  Resultado:     {}", status_badge);
            println!("  Paquete:       {} v{}", paint(&rep.name, BOLD), rep.version);
            println!("  Generación:    {}", paint(&rep.generation.to_string(), CYAN));
            println!("  Almacén:       {}", rep.store_path);
            if !rep.binaries_linked.is_empty() {
                println!("  Binarios:      {}", rep.binaries_linked.join(", "));
            }
            println!("  Mensaje:       {}\n", rep.message);
        }
        "remove" | "rm" | "uninstall" => {
            let pkg = args.get(1).ok_or_else(|| anyhow::anyhow!("Uso: antos pkg remove <nombre_paquete>"))?;
            println!("\n{} Desvinculando paquete del perfil activo...", paint("antOS antpkg ·", BOLD));
            let rep = crate::pkg::PackageEngine::remove(&ctx.state, pkg)?;
            println!("  {} Paquete «{}» desvinculado.", paint("✓", GREEN), paint(pkg, BOLD));
            println!("  Nueva generación activa: {}\n", paint(&rep.generation.to_string(), CYAN));
        }
        "list" | "ls" => {
            println!("\n{} Paquetes en el Perfil Activo:", paint("antOS antpkg ·", BOLD));
            let pkgs = crate::pkg::PackageEngine::list(&ctx.state)?;
            if pkgs.is_empty() {
                println!("  (no hay paquetes instalados en el perfil activo)\n");
            } else {
                for p in &pkgs {
                    let kb = p.installed_size_bytes / 1024;
                    println!("  • {} v{} ({} KiB, gen {}) [bin: {}]",
                        paint(&p.name, BOLD),
                        p.version,
                        kb,
                        p.generation,
                        paint(&p.binaries.join(", "), CYAN)
                    );
                }
                println!();
            }

            let gens = crate::pkg::PackageEngine::list_generations(&ctx.state)?;
            if !gens.is_empty() {
                println!("{} Historial de Generaciones:", paint("antOS antpkg ·", BOLD));
                for g in &gens {
                    let mark = if g.active { paint(" (activa)", GREEN) } else { "".to_string() };
                    println!("  • Generación {}{}: {} paquetes [{}] ({})",
                        g.generation,
                        mark,
                        g.packages.len(),
                        g.packages.join(", "),
                        g.timestamp
                    );
                }
                println!();
            }
        }
        "rollback" | "revert" => {
            let target_gen = args.get(1).and_then(|g| g.parse::<u64>().ok());
            println!("\n{} Revirtiendo perfil de paquetes de forma atómica...", paint("antOS antpkg ·", BOLD));
            let rep = crate::pkg::PackageEngine::rollback(&ctx.state, target_gen)?;
            println!("  {} Rollback exitoso a la generación {}.", paint("✓", GREEN), paint(&rep.generation.to_string(), CYAN));
            println!("  Binarios activos: {}\n", rep.binaries_linked.join(", "));
        }
        "verify" | "check" => {
            println!("\n{} Verificando integridad criptográfica y sumas SHA-256...", paint("antOS antpkg ·", BOLD));
            let (all_valid, count, details) = crate::pkg::PackageEngine::verify(&ctx.state)?;
            let badge = if all_valid { paint("INTEGRIDAD VERIFICADA", GREEN) } else { paint("FALLO DE INTEGRIDAD", RED) };
            println!("  Estado: {} ({} paquetes comprobados)", badge, count);
            for d in &details {
                println!("    {d}");
            }
            println!();
        }
        "status" | _ => {
            println!("\n{} Estado del Almacén Inmutable y Perfiles:", paint("antOS antpkg ·", BOLD));
            let st = crate::pkg::PackageEngine::status(&ctx.state)?;
            let mb = st.total_store_bytes as f64 / (1024.0 * 1024.0);
            println!("  • Directorio de Almacén:   {}", paint(&st.store_path, CYAN));
            println!("  • Directorio de Perfil:    {}", paint(&st.current_profile_path, CYAN));
            println!("  • Generación Activa:       {}", paint(&st.current_generation.to_string(), BOLD));
            println!("  • Paquetes Instalados:     {}", st.total_packages);
            println!("  • Generaciones Totales:    {}", st.generations_count);
            println!("  • Espacio Ocupado Store:   {:.2} MB", mb);
            println!("\n  Uso:");
            println!("    antos pkg install <paquete|receta.toml> [--dry-run]  Instala un paquete en el store");
            println!("    antos pkg remove <paquete>                         Desvincula un paquete del perfil");
            println!("    antos pkg list                                     Lista paquetes y generaciones");
            println!("    antos pkg rollback [generacion]                    Restaura una generación previa");
            println!("    antos pkg verify                                   Verifica hashes y firmas ed25519");
            println!("    antos pkg status                                   Muestra estado del almacén\n");
        }
    }
    Ok(())
}

// ------------------------------------------------------- autopilot (T16.3)

