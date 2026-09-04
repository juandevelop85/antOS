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

pub fn cmd_vfs(ctx: &Ctx, args: &[String]) -> Result<()> {
    let engine = crate::vfs::VfsEngine::global();
    let sub = args.first().map(String::as_str);

    match sub {
        Some("mount" | "monta") => {
            let target = args.get(1).map(String::as_str);
            let path = engine.mount(&ctx.workspace, target)?;
            println!(
                "\n{} Sistema de ficheros semántico /antfs montado con éxito.",
                paint("✓", GREEN)
            );
            println!(
                "  • Punto de montaje: {}",
                paint(&path.display().to_string(), BOLD)
            );
            println!(
                "  • Inspección:       {} o {}\n",
                paint(&format!("ls {}", path.display()), YELLOW),
                paint(&format!("cat {}/README.antfs", path.display()), YELLOW)
            );
        }
        Some("unmount" | "umount" | "desmonta") => {
            let target = args.get(1).map(String::as_str);
            engine.unmount(&ctx.workspace, target)?;
            println!(
                "\n{} Sistema de ficheros semántico /antfs desmontado correctamente.\n",
                paint("✓", GREEN)
            );
        }
        Some("ls" | "list") => {
            let vpath = args.get(1).map(String::as_str).unwrap_or("/antfs");
            let entries = engine.list_dir(&ctx.workspace, vpath)?;
            println!(
                "\n{} Listado de {}",
                paint("antOS VFS ·", BOLD),
                paint(vpath, YELLOW)
            );
            if entries.is_empty() {
                println!("  (directorio vacío)\n");
            } else {
                for e in entries {
                    let mark = if e.is_dir {
                        paint("📁", BLUE)
                    } else {
                        paint("📄", GREEN)
                    };
                    println!(
                        "  {} {:<26} {:<15} ({} bytes)",
                        mark,
                        paint(&e.name, BOLD),
                        paint(&e.node_type, DIM),
                        e.size
                    );
                }
                println!();
            }
        }
        Some("cat" | "read" | "lee") => {
            let vpath = args.get(1).ok_or_else(|| {
                anyhow::anyhow!(
                    "uso: antos vfs read <ruta_virtual> (ej. /antfs/symbols/structs/MeshStatus)"
                )
            })?;
            let content = engine.read_path(&ctx.workspace, vpath)?;
            println!("\n{}\n", content);
        }
        Some("symbols" | "simbolos" | "símbolos") => {
            let symbols = engine.discover_symbols(&ctx.workspace)?;
            println!(
                "\n{} ({} descubiertos)\n",
                paint("antOS VFS · Símbolos Semánticos del Proyecto", BOLD),
                paint(&symbols.len().to_string(), GREEN)
            );
            for cat in &["structs", "functions", "enums", "traits"] {
                let cat_syms: Vec<_> = symbols.iter().filter(|s| &s.category == cat).collect();
                if !cat_syms.is_empty() {
                    println!(
                        "  {} {} ({}):",
                        paint("●", YELLOW),
                        paint(*cat, BOLD),
                        cat_syms.len()
                    );
                    for s in cat_syms {
                        println!(
                            "    • {:<28} {}:{}",
                            paint(&s.name, BOLD),
                            paint(&s.file_path, DIM),
                            s.line_number
                        );
                    }
                    println!();
                }
            }
        }
        Some("validate" | "check" | "valida") => {
            let rel = args.get(1).ok_or_else(|| {
                anyhow::anyhow!("uso: antos vfs validate <archivo> (ej. src/main.rs)")
            })?;
            let abs_path = ctx.workspace.join(rel);
            if !abs_path.exists() {
                bail!(
                    "el archivo «{}» no existe en el espacio de trabajo",
                    abs_path.display()
                );
            }
            let text = std::fs::read_to_string(&abs_path)?;
            let guard = crate::vfs_guard::VfsGuardEngine::global();
            let res = guard.validate_content(rel, &text);
            println!(
                "\n{} Validación de integridad sintáctica VFS",
                paint("antOS ·", BOLD)
            );
            println!("  Archivo:  {}", paint(rel, YELLOW));
            println!(
                "  Lenguaje: {} ({} líneas)\n",
                paint(&res.language, BOLD),
                res.line_count
            );
            if res.is_valid {
                println!(
                    "  {} El archivo es sintácticamente válido y seguro para persistir.\n",
                    paint("✓ Aprobado:", GREEN)
                );
            } else {
                println!(
                    "  {} Se detectaron {} problema(s) sintáctico(s):",
                    paint("✗ Rechazado:", RED),
                    res.errors.len()
                );
                for err in res.errors {
                    println!(
                        "    • Línea {}, columna {}: {}",
                        paint(&err.line.to_string(), YELLOW),
                        err.column,
                        err.message
                    );
                }
                println!();
            }
        }
        Some("guard" | "guardia" | "interceptor") => {
            let guard = crate::vfs_guard::VfsGuardEngine::global();
            let status = guard.status()?;
            println!(
                "\n{}",
                paint(
                    "antOS VFS · Interceptor de Escrituras Semánticas (T10.2)",
                    BOLD
                )
            );
            let state_str = if status.enabled {
                paint("● ACTIVO (ENFORCING)", GREEN)
            } else {
                paint("○ INACTIVO", DIM)
            };
            println!("  Estado del interceptor:   {}", state_str);
            println!(
                "  Escrituras interceptadas: {}",
                paint(&status.total_intercepted.to_string(), BOLD)
            );
            println!(
                "  Escrituras rechazadas:    {}",
                paint(
                    &status.total_rejected.to_string(),
                    if status.total_rejected > 0 {
                        RED
                    } else {
                        GREEN
                    }
                )
            );
            if !status.rejected_paths.is_empty() {
                println!("\n  Ficheros protegidos contra corrupción sintáctica:");
                for p in status.rejected_paths {
                    println!("    • {}", paint(&p, YELLOW));
                }
            }
            println!("\n  Usa «antos vfs validate <archivo>» para probar validación previa.\n");
        }
        _ => {
            let status = engine.status(&ctx.workspace)?;
            println!(
                "\n{}",
                paint(
                    "antOS · Sistema de Ficheros Virtual FUSE (/antfs) - T10.1 & T10.2",
                    BOLD
                )
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            let mnt_badge = if status.is_mounted {
                paint("● MONTADO", GREEN)
            } else {
                paint("○ NO MONTADO", DIM)
            };
            println!("  Estado del VFS:     {}", mnt_badge);
            if let Some(mnt) = status.mount_point {
                println!("  Punto de montaje:   {}", paint(&mnt, YELLOW));
            }
            println!(
                "  Símbolos AST:       {} indexados",
                paint(&status.total_symbols.to_string(), BOLD)
            );
            println!(
                "  Módulos navegables: {} en /antfs/graph\n",
                paint(&status.total_modules.to_string(), BOLD)
            );

            println!("  Subcomandos disponibles:");
            println!("    • antos vfs symbols          Lista símbolos AST (structs, functions, enums, traits)");
            println!("    • antos vfs ls [ruta]        Explora la jerarquía /antfs (symbols, graph, git)");
            println!("    • antos vfs read <ruta>      Lee el código o diff de un inodo virtual");
            println!("    • antos vfs mount [ruta]     Proyecta /antfs en el disco local");
            println!("    • antos vfs unmount [ruta]   Desmonta la proyección /antfs");
            println!("    • antos vfs validate <file>  Valida la integridad sintáctica antes de persistir");
            println!(
                "    • antos vfs guard            Muestra métricas del interceptor de escrituras\n"
            );
        }
    }
    Ok(())
}

// --------------------------------------------------------------------- ebpf

