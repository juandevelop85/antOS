//! CLI commands for Flatpak and desktop application management (T25.2).

use crate::apps::AppEngine;
use crate::ctx::Ctx;
use antos_protocol::AppSource;
use anyhow::Result;
use std::path::Path;

pub fn cmd_app(ctx: &Ctx, args: &[String]) -> Result<()> {
    if args.is_empty() {
        print_app_help();
        return Ok(());
    }

    let state_dir = &ctx.state;

    match args[0].as_str() {
        "list" | "ls" => {
            let mut filter_source = None;
            for arg in &args[1..] {
                match arg.as_str() {
                    "--flatpak" => filter_source = Some(AppSource::Flatpak),
                    "--pkg" | "--antpkg" | "--native" => filter_source = Some(AppSource::NativePkg),
                    "--all" => filter_source = None,
                    _ => {}
                }
            }

            let apps = AppEngine::list_apps(state_dir, filter_source)?;
            if apps.is_empty() {
                println!("No hay aplicaciones instaladas. Usa 'antos app search <término>' para encontrar aplicaciones.");
            } else {
                let filter_desc = match filter_source {
                    Some(AppSource::Flatpak) => " (Solo Flatpak)",
                    Some(AppSource::NativePkg) => " (Solo antpkg)",
                    _ => "",
                };
                println!(
                    "📦 antOS Apps · Aplicaciones Instaladas{} ({}):",
                    filter_desc,
                    apps.len()
                );
                for a in apps {
                    let src_badge = match a.source {
                        AppSource::Flatpak => "[\x1b[34mFlatpak\x1b[0m]",
                        AppSource::NativePkg => "[\x1b[32mantpkg\x1b[0m]",
                        AppSource::Nix => "[\x1b[36mNix\x1b[0m]",
                    };
                    println!("  • {:<32} {} v{} {}", a.id, src_badge, a.version, a.name);
                    println!("    Comando:    {}", a.exec_cmd);
                    if !a.categories.is_empty() {
                        println!("    Categorías: {}", a.categories.join(", "));
                    }
                    if !a.permissions.is_empty() {
                        println!("    Permisos:   {}", a.permissions.join(", "));
                    }
                }
            }
        }
        "search" | "find" => {
            let query = if args.len() > 1 {
                args[1..].join(" ")
            } else {
                "".to_string()
            };

            let results = AppEngine::search_apps(state_dir, &query)?;
            if results.is_empty() {
                println!(
                    "No se encontraron aplicaciones coincidentes con '{}'.",
                    query
                );
            } else {
                println!(
                    "🔍 antOS Apps · Resultados de Búsqueda ({}) para '{}':",
                    results.len(),
                    query
                );
                for r in results {
                    let status_badge = if r.installed {
                        "[\x1b[32mInstalada\x1b[0m]"
                    } else {
                        "[\x1b[90mDisponible\x1b[0m]"
                    };
                    let src_badge = match r.source {
                        AppSource::Flatpak => "[\x1b[34mFlathub\x1b[0m]",
                        AppSource::NativePkg => "[\x1b[32mantpkg\x1b[0m]",
                        AppSource::Nix => "[\x1b[36mNix\x1b[0m]",
                    };
                    println!(
                        "  • {:<32} {} {} v{}",
                        r.id, src_badge, status_badge, r.version
                    );
                    println!("    {} - {}", r.name, r.description);
                }
            }
        }
        "install" | "add" => {
            if args.len() < 2 {
                eprintln!("Uso: antos app install <id> [--source flatpak|pkg]");
                return Ok(());
            }

            let id = &args[1];
            let mut source = None;
            for arg in &args[2..] {
                if arg == "--flatpak" {
                    source = Some(AppSource::Flatpak);
                } else if arg == "--pkg" || arg == "--antpkg" {
                    source = Some(AppSource::NativePkg);
                }
            }

            println!("🚀 Iniciando instalación de aplicación '{}'...", id);
            let result = AppEngine::install_app(state_dir, id, source, |p| {
                println!("  [{:>3.0}%] {}", p.percentage, p.status);
            })?;

            if result.success {
                println!("✅ {}", result.message);
            } else {
                eprintln!("❌ Error al instalar: {}", result.message);
            }
        }
        "uninstall" | "remove" | "rm" => {
            if args.len() < 2 {
                eprintln!("Uso: antos app remove <id>");
                return Ok(());
            }

            let id = &args[1];
            println!("🗑️  Desinstalando aplicación '{}'...", id);
            let result = AppEngine::uninstall_app(state_dir, id)?;
            if result.success {
                println!("✅ {}", result.message);
            } else {
                eprintln!("❌ Error al desinstalar: {}", result.message);
            }
        }
        "run" | "launch" | "start" => {
            if args.len() < 2 {
                eprintln!("Uso: antos app run <id> [--workspace <dir>] [argumentos...]");
                return Ok(());
            }

            let id = &args[1];
            let mut workspace = None;
            let mut pass_args = Vec::new();

            let mut iter = args[2..].iter();
            while let Some(arg) = iter.next() {
                if arg == "--workspace" {
                    if let Some(w) = iter.next() {
                        workspace = Some(Path::new(w));
                    }
                } else {
                    pass_args.push(arg.clone());
                }
            }

            let effective_ws = workspace.unwrap_or(&ctx.workspace);
            let launch_res = AppEngine::launch_app(state_dir, id, Some(effective_ws), &pass_args)?;

            if launch_res.success {
                println!("🚀 {}", launch_res.message);
                if let Some(pid) = launch_res.pid {
                    println!("  • PID del Proceso: {}", pid);
                }
                if let Some(ws) = launch_res.workspace {
                    println!("  • Workspace Context: {}", ws);
                }
            } else {
                eprintln!("❌ Error al ejecutar: {}", launch_res.message);
            }
        }
        "help" | "--help" | "-h" => {
            print_app_help();
        }
        other => {
            eprintln!(
                "Subcomando desconocido: 'antos app {}'. Usa 'antos app help'.",
                other
            );
        }
    }

    Ok(())
}

fn print_app_help() {
    println!(
        "\
Gestor y Puente de Aplicaciones Flatpak y Contenedores Gráficos de antOS (T25.2)

Uso:
  antos app list [--flatpak|--pkg|--all]     Lista aplicaciones de escritorio instaladas
  antos app search <término>                 Busca aplicaciones en Flathub y recetas antOS
  antos app install <id> [--source <src>]    Instala aplicación (ej. com.visualstudio.code)
  antos app run <id> [--workspace <dir>]     Ejecuta aplicación inyectando Wayland y workspace
  antos app remove <id>                      Desinstala la aplicación y revoca accesos
"
    );
}
