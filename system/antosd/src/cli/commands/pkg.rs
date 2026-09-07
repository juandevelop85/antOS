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
            if !rep.desktop_entries_linked.is_empty() {
                println!("  Accesos XDG:   {}", paint(&rep.desktop_entries_linked.join(", "), GREEN));
            }
            if !rep.icons_linked.is_empty() {
                println!("  Iconos XDG:    {}", rep.icons_linked.join(", "));
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
            let gui_only = args.iter().any(|a| a == "--gui" || a == "-g");
            let title = if gui_only {
                "Aplicaciones Gráficas (GUI) en el Perfil Activo:"
            } else {
                "Paquetes en el Perfil Activo:"
            };
            println!("\n{} {}", paint("antOS antpkg ·", BOLD), title);

            let mut pkgs = crate::pkg::PackageEngine::list(&ctx.state)?;
            if gui_only {
                pkgs.retain(|p| p.app_type == antos_protocol::PackageAppType::Gui || p.desktop_entry.is_some());
            }

            if pkgs.is_empty() {
                if gui_only {
                    println!("  (no hay aplicaciones gráficas instaladas en el perfil activo)\n");
                } else {
                    println!("  (no hay paquetes instalados en el perfil activo)\n");
                }
            } else {
                for p in &pkgs {
                    let kb = p.installed_size_bytes / 1024;
                    let type_badge = match p.app_type {
                        antos_protocol::PackageAppType::Gui => paint("[GUI]", GREEN),
                        antos_protocol::PackageAppType::Cli => paint("[CLI]", CYAN),
                    };
                    let desktop_info = p.desktop_file.as_deref().map(|df| format!(" desktop: {}", df)).unwrap_or_default();
                    println!("  • {} {} v{} ({} KiB, gen {}) [bin: {}]{}",
                        type_badge,
                        paint(&p.name, BOLD),
                        p.version,
                        kb,
                        p.generation,
                        paint(&p.binaries.join(", "), CYAN),
                        desktop_info
                    );
                }
                println!();
            }

            if !gui_only {
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
        }
        "apps" | "desktop" | "gui" => {
            println!("\n{} Aplicaciones de Escritorio Registradas:", paint("antOS antpkg ·", BOLD));
            let apps = crate::pkg::PackageEngine::list_desktop_apps(&ctx.state)?;
            if apps.is_empty() {
                println!("  (no hay aplicaciones de escritorio con archivo .desktop registradas)\n");
            } else {
                for app in &apps {
                    println!("  • {} ({})", paint(&app.name, BOLD), paint(&app.id, CYAN));
                    if let Some(gn) = &app.generic_name {
                        println!("      Categoría / Tipo:  {}", gn);
                    }
                    if let Some(comm) = &app.comment {
                        println!("      Comentario:        {}", comm);
                    }
                    println!("      Ejecutable:        {}", app.exec);
                    if let Some(ic) = &app.icon {
                        println!("      Icono:             {}", ic);
                    }
                    if !app.categories.is_empty() {
                        println!("      Categorías XDG:    {}", app.categories.join(", "));
                    }
                    println!("      Archivo .desktop:  {}\n", app.desktop_file_path);
                }
            }
        }
        "info" | "show" => {
            let pkg_name = args.get(1).ok_or_else(|| anyhow::anyhow!("Uso: antos pkg info <nombre_paquete>"))?;
            let pkgs = crate::pkg::PackageEngine::list(&ctx.state)?;
            let found = pkgs.into_iter().find(|p| p.name == *pkg_name);

            if let Some(p) = found {
                println!("\n{} Información de Paquete:", paint("antOS antpkg ·", BOLD));
                println!("  Nombre:          {}", paint(&p.name, BOLD));
                println!("  Versión:         {}", p.version);
                println!("  Tipo:            {:?}", p.app_type);
                println!("  Descripción:     {}", p.description);
                println!("  Hash Store:      {}", p.store_hash);
                println!("  Tamaño:          {:.2} KiB", p.installed_size_bytes as f64 / 1024.0);
                println!("  Generación:      {}", p.generation);
                println!("  Instalado el:    {}", p.installed_at);
                println!("  Binarios:        {}", p.binaries.join(", "));
                if let Some(df) = &p.desktop_file {
                    println!("  Archivo Desktop: {}", paint(df, GREEN));
                }
                if let Some(d) = &p.desktop_entry {
                    println!("  Desktop Name:    {}", d.name);
                    if let Some(gn) = &d.generic_name { println!("  Generic Name:    {}", gn); }
                    if let Some(c) = &d.comment { println!("  Comentario:      {}", c); }
                    println!("  Exec:            {}", d.exec);
                    if let Some(ic) = &d.icon { println!("  Icono:           {}", ic); }
                    println!("  Categorías:      {}", d.categories.join(", "));
                    if !d.mime_types.is_empty() { println!("  Tipos MIME:      {}", d.mime_types.join(", ")); }
                    println!("  Terminal:        {}", d.terminal);
                }
                if !p.icons_linked.is_empty() {
                    println!("  Iconos:          {}", p.icons_linked.join(", "));
                }
                println!();
            } else {
                println!("\n{} El paquete «{}» no se encuentra en el perfil activo.\n", paint("antOS antpkg ·", BOLD), pkg_name);
            }
        }
        "validate" => {
            let file_or_content = args.get(1).ok_or_else(|| anyhow::anyhow!("Uso: antos pkg validate <archivo.desktop>"))?;
            let content = if std::path::Path::new(file_or_content).exists() {
                std::fs::read_to_string(file_or_content)?
            } else {
                file_or_content.clone()
            };

            println!("\n{} Validando archivo .desktop según especificación Freedesktop...", paint("antOS antpkg ·", BOLD));
            let report = crate::pkg::PackageEngine::validate_desktop_entry(&content);
            if report.valid {
                println!("  Resultado:  {}\n", paint("VÁLIDO (cumple especificación XDG)", GREEN));
            } else {
                println!("  Resultado:  {}\n", paint("INVÁLIDO", RED));
                for err in &report.errors {
                    println!("  • Error: {}", paint(err, RED));
                }
            }
            for warn in &report.warnings {
                println!("  • Advertencia: {}", paint(warn, YELLOW));
            }
            println!();
        }
        "rollback" | "revert" => {
            let target_gen = args.get(1).and_then(|g| g.parse::<u64>().ok());
            println!("\n{} Revirtiendo perfil de paquetes de forma atómica...", paint("antOS antpkg ·", BOLD));
            let rep = crate::pkg::PackageEngine::rollback(&ctx.state, target_gen)?;
            println!("  {} Rollback exitoso a la generación {}.", paint("✓", GREEN), paint(&rep.generation.to_string(), CYAN));
            println!("  Binarios activos: {}", rep.binaries_linked.join(", "));
            if !rep.desktop_entries_linked.is_empty() {
                println!("  Accesos XDG:      {}", paint(&rep.desktop_entries_linked.join(", "), GREEN));
            }
            println!();
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
            let apps = crate::pkg::PackageEngine::list_desktop_apps(&ctx.state).unwrap_or_default();
            println!("  • Directorio de Almacén:   {}", paint(&st.store_path, CYAN));
            println!("  • Directorio de Perfil:    {}", paint(&st.current_profile_path, CYAN));
            println!("  • Generación Activa:       {}", paint(&st.current_generation.to_string(), BOLD));
            println!("  • Paquetes Instalados:     {}", st.total_packages);
            println!("  • Aplicaciones GUI:        {}", apps.len());
            println!("  • Generaciones Totales:    {}", st.generations_count);
            println!("  • Espacio Ocupado Store:   {:.2} MB", mb);
            println!("\n  Uso:");
            println!("    antos pkg install <paquete|receta.toml> [--dry-run]  Instala un paquete en el store");
            println!("    antos pkg remove <paquete>                         Desvincula un paquete del perfil");
            println!("    antos pkg list [--gui]                             Lista paquetes (o solo apps GUI)");
            println!("    antos pkg apps                                     Lista aplicaciones de escritorio XDG");
            println!("    antos pkg info <paquete>                           Muestra información detallada de un paquete");
            println!("    antos pkg validate <archivo.desktop>               Valida sintaxis de archivo .desktop");
            println!("    antos pkg rollback [generacion]                    Restaura una generación previa");
            println!("    antos pkg verify                                   Verifica hashes y firmas ed25519");
            println!("    antos pkg status                                   Muestra estado del almacén\n");
        }
    }
    Ok(())
}

// ------------------------------------------------------- autopilot (T16.3)

