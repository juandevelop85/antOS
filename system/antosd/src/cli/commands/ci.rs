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

pub fn cmd_ci(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("run");
    match sub {
        "status" | "estado" => {
            let report = crate::ci::CiEngine::get_last_report(&ctx.state)?;
            println!("\n{}", paint("antOS CI · Reporte del Último Pipeline Ejecutado (T20.3)", BOLD));
            if let Some(r) = report {
                println!("  ID de Ejecución:      {}", paint(&r.id, CYAN));
                println!("  Estado General:       {}", if r.success { paint("✅ EXITOSO", GREEN) } else { paint("❌ FALLIDO", RED) });
                println!("  Duración Total:       {} ms", r.total_duration_ms);
                println!("  Seguridad (Secretos): {}", if r.security_clean { paint("Limpio (0 detectados)", GREEN) } else { paint(&format!("ALERTA: {} detectados", r.secrets_found.len()), RED) });
                println!("\n  Etapas ejecutadas:");
                for s in &r.stages {
                    println!("    • {:<12} {:<15} ({} ms) - {}", s.name, s.status.label(), s.duration_ms, s.command);
                }
                println!();
            } else {
                println!("  No hay reportes de CI previos registrados en .antos/ci/\n");
            }
            Ok(())
        }
        _ => {
            let mut stage_filter = None;
            let mut fast_mode = false;
            let mut i = 0;
            while i < args.len() {
                match args[i].as_str() {
                    "run" => {}
                    "--stage" | "-s" => {
                        if let Some(s) = args.get(i + 1) {
                            stage_filter = Some(s.as_str());
                            i += 1;
                        }
                    }
                    "--fast" | "-f" => fast_mode = true,
                    val if !val.starts_with('-') && stage_filter.is_none() && val != "run" => {
                        stage_filter = Some(val);
                    }
                    _ => {}
                }
                i += 1;
            }

            println!("\n{}", paint("⚙️ antOS CI · Ejecutando Matriz Local Paralela en Sandboxes (T20.3)", BOLD));
            if let Some(st) = stage_filter {
                println!("  Filtrando por etapa: {}", paint(st, CYAN));
            }
            if fast_mode {
                println!("  Modo rápido activado (--fast)");
            }

            let report = crate::ci::CiEngine::run_pipeline(
                &ctx.workspace,
                &ctx.state,
                stage_filter,
                fast_mode,
            )?;

            println!("\n  {} [{}]", paint("ID Pipeline:", CYAN), report.id);
            println!("  {} {} ms", paint("Duración Total:", BOLD), report.total_duration_ms);
            println!(
                "  {} {}",
                paint("Resultado:", BOLD),
                if report.success {
                    paint("✅ PASARON TODAS LAS ETAPAS", GREEN)
                } else {
                    paint("❌ FALLÓ AL MENOS UNA ETAPA", RED)
                }
            );

            println!("\n  Desglose de Etapas:");
            for s in &report.stages {
                let badge = match s.status {
                    antos_protocol::CiStageStatus::Passed => paint("✅ PASÓ", GREEN),
                    antos_protocol::CiStageStatus::Failed => paint("❌ FALLÓ", RED),
                    antos_protocol::CiStageStatus::Skipped => paint("⏭️ OMITIDO", DIM),
                    _ => paint("⏳", YELLOW),
                };
                println!("    • {:<12} {} ({} ms) — {}", s.name, badge, s.duration_ms, s.command);
                if s.status == antos_protocol::CiStageStatus::Failed && !s.output_snippet.is_empty() {
                    for line in s.output_snippet.lines().take(4) {
                        println!("        {}", paint(line, DIM));
                    }
                }
            }

            if !report.security_clean {
                println!("\n  {}", paint("🚨 FUGAS DE SEGURIDAD DETECTADAS:", RED));
                for sec in &report.secrets_found {
                    println!("    • {}", paint(sec, YELLOW));
                }
            }
            println!();

            if !report.success {
                bail!("El pipeline local de CI ha fallado.");
            }
            Ok(())
        }
    }
}

pub fn cmd_hook(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    match sub {
        "install" | "instalar" => {
            println!("\n{}", paint("🪝 antOS Git Hooks · Instalando Protección Automatizada (T20.3)", BOLD));
            let status = crate::ci::CiEngine::install_git_hooks(&ctx.workspace)?;
            println!("  Pre-commit hook: {}", if status.pre_commit_installed { paint("✅ INSTALADO", GREEN) } else { paint("❌ NO INSTALADO", RED) });
            println!("  Pre-push hook:   {}", if status.pre_push_installed { paint("✅ INSTALADO", GREEN) } else { paint("❌ NO INSTALADO", RED) });
            println!("  Ubicación:       {}", status.hook_dir);
            println!("  Guardias activas:");
            for g in &status.active_guards {
                println!("    • {g}");
            }
            println!("\n  Protección activa contra fuga de secretos y fallos de CI.\n");
            Ok(())
        }
        "uninstall" | "desinstalar" | "remove" => {
            println!("\n{}", paint("🪝 antOS Git Hooks · Desinstalando Protección (T20.3)", BOLD));
            let status = crate::ci::CiEngine::uninstall_git_hooks(&ctx.workspace)?;
            println!("  Pre-commit hook: {}", if status.pre_commit_installed { paint("ACTIVO", YELLOW) } else { paint("DESINSTALADO", DIM) });
            println!("  Pre-push hook:   {}", if status.pre_push_installed { paint("ACTIVO", YELLOW) } else { paint("DESINSTALADO", DIM) });
            println!("\n  Hooks de antOS eliminados correctamente.\n");
            Ok(())
        }
        "check" | "audit" => {
            println!("\n{}", paint("🪝 antOS Git Hook Auditor · Verificación de Pre-Commit (T20.3)", BOLD));
            let (passed, errors) = crate::ci::CiEngine::run_pre_commit_check(&ctx.workspace)?;
            if passed {
                println!("  {}", paint("✅ Auditoría superada: sin fugas de secretos detectadas.", GREEN));
                println!();
                Ok(())
            } else {
                println!("  {}", paint("❌ AUDITORÍA RECHAZADA: se detectaron problemas en el código:", RED));
                for e in &errors {
                    println!("  {e}");
                }
                println!("\n  El commit ha sido bloqueado por el Auditor de antOS.");
                println!("  Corrige las violaciones o secretos antes de commitear.\n");
                bail!("Pre-commit hook rechazó la operación.");
            }
        }
        _ => {
            let status = crate::ci::CiEngine::query_git_hooks_status(&ctx.workspace)?;
            println!("\n{}", paint("🪝 antOS Git Hooks · Estado de Protección (T20.3)", BOLD));
            println!("  Pre-commit hook: {}", if status.pre_commit_installed { paint("ACTIVO (Vigilando commits)", GREEN) } else { paint("INACTIVO", DIM) });
            println!("  Pre-push hook:   {}", if status.pre_push_installed { paint("ACTIVO (Vigilando push)", GREEN) } else { paint("INACTIVO", DIM) });
            println!("  Directorio:      {}", status.hook_dir);
            if !status.active_guards.is_empty() {
                println!("  Mecanismos de protección:");
                for g in &status.active_guards {
                    println!("    • {g}");
                }
            }
            println!("\n  Uso:");
            println!("    antos hook install     Instala pre-commit y pre-push hooks");
            println!("    antos hook uninstall   Elimina los hooks gestionados por antOS");
            println!("    antos hook check       Ejecuta la auditoría manualmente\n");
            Ok(())
        }
    }
}

// ------------------------------------------------------------------ snapshot / time machine (T20.4)

