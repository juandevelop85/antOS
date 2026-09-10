//! `antos doc`, `antos screenshot` y `antos qa`.
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

pub fn cmd_doc(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("arch");
    match sub {
        "arch" | "diagram" => {
            let mut kind_str = "full";
            let mut output_file = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--type" | "-t" => {
                        if let Some(k) = args.get(i + 1) {
                            kind_str = k.as_str();
                            i += 1;
                        }
                    }
                    "--output" | "-o" => {
                        if let Some(o) = args.get(i + 1) {
                            output_file = Some(o.as_str());
                            i += 1;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }

            let kind = match kind_str {
                "components" | "c4" | "comp" => antos_protocol::ArchDiagramKind::Components,
                "flow" | "ipc" => antos_protocol::ArchDiagramKind::IpcFlow,
                "antflow" | "state" => antos_protocol::ArchDiagramKind::AntFlow,
                _ => antos_protocol::ArchDiagramKind::Full,
            };

            let report = crate::doc_arch::DocArchEngine::generate_diagram(&ctx.workspace, kind);

            if let Some(out_path) = output_file {
                let p = std::path::Path::new(out_path);
                std::fs::write(p, &report.mermaid_content)?;
                println!(
                    "\n{} Diagrama Mermaid guardado en: {}\n",
                    paint("📐 antOS Doc Arch ·", BOLD),
                    paint(out_path, CYAN)
                );
            } else {
                println!(
                    "\n{}",
                    paint(
                        "📐 antOS Living Architecture · Diagramas Vivos de Arquitectura (T21.3)",
                        BOLD
                    )
                );
                println!("  Tipo:         {}", paint(report.kind.name(), CYAN));
                println!(
                    "  Topología:    {} crates | {} módulos demonio | {} capacidades",
                    paint(&report.crates_count.to_string(), CYAN),
                    paint(&report.modules_count.to_string(), CYAN),
                    paint(&report.caps_count.to_string(), CYAN)
                );
                println!("\n```mermaid\n{}\n```\n", report.mermaid_content);
            }
            Ok(())
        }
        "sync" => {
            let target_file = args.get(1).map(String::as_str);
            println!(
                "\n{}",
                paint(
                    "🔄 antOS Doc Sync · Sincronizando Documentación Viva de Arquitectura (T21.3)",
                    BOLD
                )
            );

            let report = crate::doc_arch::DocArchEngine::sync_docs(&ctx.workspace, target_file)?;

            println!(
                "  Archivos escaneados:    {}",
                paint(&report.files_scanned.to_string(), CYAN)
            );
            println!(
                "  Archivos actualizados:  {}",
                paint(&report.files_updated.to_string(), GREEN)
            );
            for p in &report.updated_paths {
                println!("    • {}", paint(p, CYAN));
            }
            println!("  Resultado:              {}\n", report.message);
            Ok(())
        }
        "check" => {
            let target_file = args.get(1).map(String::as_str);
            println!(
                "\n{}",
                paint(
                    "🔍 antOS Doc Check · Verificación de Sincronización Arquitectónica (T21.3)",
                    BOLD
                )
            );

            match crate::doc_arch::DocArchEngine::check_docs(&ctx.workspace, target_file) {
                Ok(report) => {
                    println!("  {}\n", paint(&report.message, GREEN));
                    Ok(())
                }
                Err(e) => {
                    println!("  {}\n", paint(&format!("❌ {}", e), RED));
                    bail!("{e}");
                }
            }
        }
        _ => {
            println!("\nUso:");
            println!("  antos doc arch [--type components|flow|antflow|all] [--output <file>]");
            println!(
                "  antos doc sync [--file <path>]    Sincroniza e incrusta diagramas en markdown"
            );
            println!("  antos doc check [--file <path>]   Verifica modo CI si la doc está sincronizada\n");
            Ok(())
        }
    }
}

// ------------------------------------------------------------------ desktop

pub fn cmd_screenshot(ctx: &Ctx, args: &[String]) -> Result<()> {
    let target = args.first().map(String::as_str);
    let path = if args.len() > 1 {
        Some(ctx.workspace.join(&args[1]))
    } else if let Some(t) = target {
        if t.ends_with(".png") || t.ends_with(".bmp") {
            Some(ctx.workspace.join(t))
        } else {
            None
        }
    } else {
        None
    };

    let actual_target = target.filter(|&t| !(t.ends_with(".png") || t.ends_with(".bmp")));

    println!(
        "\n{} Captura de Pantalla Wayland:",
        paint("antOS Screencopy ·", BOLD)
    );
    let engine = crate::vision::VisionEngine::global();
    let res = engine.capture_screen(actual_target, path.as_deref())?;

    let saved = res.saved_path.as_deref().unwrap_or("en memoria");
    println!(
        "  {} Captura completada para «{}»",
        paint("✓", GREEN),
        res.target
    );
    println!("    Destino:      {}", saved);
    println!(
        "    Resolución:   {}x{} píxeles ({})",
        res.width,
        res.height,
        res.format.to_uppercase()
    );
    println!(
        "    Tamaño:       {} KiB ({} bytes)\n",
        res.size_bytes / 1024,
        res.size_bytes
    );
    Ok(())
}

pub fn cmd_qa(ctx: &Ctx, args: &[String]) -> Result<()> {
    let _ = ctx;
    let sub = args.first().map(String::as_str);
    match sub {
        Some("visual" | "vis" | "ui") => {
            let target = args.get(1).map(String::as_str).unwrap_or("desktop");
            let criteria: Vec<String> = if args.len() > 2 {
                args[2..].iter().map(|s| s.replace('_', " ")).collect()
            } else {
                vec![
                    "Verificar contraste de color accesible".to_string(),
                    "Comprobar márgenes y alineación de elementos".to_string(),
                    "Verificar ausencia de desbordamientos visuales".to_string(),
                ]
            };

            println!(
                "\n{} Agente Multimodal VisualQA:",
                paint("antOS QA Visual ·", BOLD)
            );
            println!("  Objetivo: {}", paint(target, CYAN));
            println!("  Criterios evaluados: {}\n", criteria.len());

            let engine = crate::vision::VisionEngine::global();
            let report = engine.inspect_visual(target, &criteria, None)?;

            let status_badge = if report.pass {
                paint("APROBADO", GREEN)
            } else {
                paint("RECHAZADO", RED)
            };

            println!("  {} {}", paint("Resultado:", BOLD), status_badge);
            println!("  Resumen:     {}", report.summary);
            println!(
                "  Resolución:  {}x{} píxeles ({} KiB)\n",
                report.image_width,
                report.image_height,
                report.image_size_bytes / 1024
            );

            println!("  {}:", paint("Hallazgos de Inspección", BOLD));
            for f in &report.findings {
                let sev = match f.severity.as_str() {
                    "critical" => paint("[CRÍTICO]", RED),
                    "warning" => paint("[ADVERTENCIA]", YELLOW),
                    _ => paint("[INFO]", CYAN),
                };
                println!(
                    "    • {} {}: {}",
                    sev,
                    paint(&f.category, BOLD),
                    f.description
                );
                if let Some(ref coords) = f.coordinates {
                    println!("      Coordenadas:   {}", coords);
                }
                println!("      Recomendación: {}", f.recommendation);
            }
            println!();

            if !report.pass {
                bail!("Inspección visual rechazada debido a fallos críticos");
            }
        }
        _ => {
            println!(
                "\n{} Inspección de Calidad Visual (antFlow QA):",
                paint("antOS QA ·", BOLD)
            );
            println!("  Uso:");
            println!("    antos qa visual [target] [criterios...]  Auditoría visual con agente multimodal\n");
        }
    }
    Ok(())
}
