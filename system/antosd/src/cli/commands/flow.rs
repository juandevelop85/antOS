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

pub fn cmd_agent(ctx: &Ctx, args: &[String], opts: &crate::cli::args::Opts) -> Result<()> {
    if args.is_empty() || args[0] == "list" || args[0] == "roles" {
        println!(
            "\n{}",
            paint("antOS · Roles de Agentes Especializados (antFlow)", BOLD)
        );
        let roles = [
            antos_protocol::AgentRole::Architect,
            antos_protocol::AgentRole::Coder,
            antos_protocol::AgentRole::QA,
            antos_protocol::AgentRole::Auditor,
        ];
        for r in roles {
            println!("\n  {} {}", paint("●", GREEN), paint(r.name(), BOLD));
            println!("    {}", paint(r.description(), DIM));
            println!(
                "    {}",
                paint(&format!("Directive: {}", r.system_prompt()), DIM)
            );
        }
        println!();
        return Ok(());
    }

    match args[0].as_str() {
        "config" | "configure" | "models" => {
            let mut config = crate::llm::LlmConfig::load_from_state(&ctx.state);
            let role_flag = args
                .iter()
                .position(|a| a == "--role" || a == "-r")
                .and_then(|i| args.get(i + 1))
                .map(String::as_str);
            let llm_flag = args
                .iter()
                .position(|a| a == "--llm" || a == "-m" || a == "--model")
                .and_then(|i| args.get(i + 1))
                .map(String::as_str);

            if let (Some(role), Some(model_spec)) = (role_flag, llm_flag) {
                config.set_role_model(role, model_spec);
                config.save_to_state(&ctx.state)?;
                println!(
                    "\n{} Rol de agente '{}' asignado al modelo: {}\n",
                    paint("antOS antFlow ·", BOLD),
                    paint(role, CYAN),
                    paint(model_spec, GREEN)
                );
                return Ok(());
            }

            println!(
                "\n{}",
                paint(
                    "antOS antFlow · Matriz de Modelos Asignados por Rol de Agente (T19.4)",
                    BOLD
                )
            );
            println!("  Personaliza qué motor y modelo ejecuta cada fase del ciclo de vida multi-agente:\n");
            println!(
                "  {:<14} {:<36} {:<15} {}",
                paint("ROL", BOLD),
                paint("MODELO ASIGNADO", BOLD),
                paint("PROVEEDOR", BOLD),
                paint("ESPECIALIZACIÓN", BOLD)
            );
            println!("  {}", paint(&"─".repeat(82), DIM));

            let roles_meta = [
                ("architect", "Arquitecto 📐", "Razonamiento Profundo"),
                ("coder", "Coder 💻", "Generación de Código"),
                ("qa", "QA / Tester 🧪", "Validación y Ejecución"),
                ("auditor", "Auditor 🛡️", "Seguridad y Diffs"),
            ];

            for (role_key, role_label, spec_desc) in roles_meta {
                let assigned = config.get_role_model(role_key);
                let (prov, model_part) = match assigned.split_once(':') {
                    Some((p, m)) => (p, m),
                    None => (assigned.as_str(), "default"),
                };
                println!(
                    "  {:<14} {:<36} {:<15} {}",
                    paint(role_label, CYAN),
                    paint(model_part, GREEN),
                    paint(prov, YELLOW),
                    paint(spec_desc, DIM)
                );
            }

            println!("\n  Para cambiar el modelo de un rol:");
            println!(
                "    {}",
                paint(
                    "antos agent config --role coder --llm ollama:qwen2.5-coder:latest",
                    CYAN
                )
            );
            println!("    {}", paint("antos agent config --role architect --llm openrouter:deepseek/deepseek-r1:free", CYAN));
            println!(
                "    {}",
                paint(
                    "antos agent config --role qa --llm groq:llama-3.3-70b-versatile",
                    CYAN
                )
            );
            println!(
                "    Para ver catálogo de modelos gratuitos: {}\n",
                paint("antos llm free", YELLOW)
            );
            return Ok(());
        }
        "do" => cmd_agent_do(ctx, &args[1..], opts)?,
        "report" => cmd_agent_report(ctx)?,
        "run" => {
            let ticket_id = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("uso: antos agent run <ticket_id> [--auto]"))?;
            let auto = args.iter().any(|a| a == "--auto" || a == "-a");
            let node_target = args
                .iter()
                .position(|a| a == "--node" || a == "-n" || a == "--remote")
                .and_then(|i| args.get(i + 1));

            println!(
                "\n{}",
                paint(
                    &format!("antOS · Orquestador antFlow para {ticket_id}"),
                    BOLD
                )
            );

            if let Some(target) = node_target {
                println!(
                    "  {} Despachando rol a nodo remoto Swarm: {}",
                    paint("🌐", CYAN),
                    paint(target, BOLD)
                );
                let _ = crate::distributed::SwarmEngine::global().dispatch_remote_role(
                    &ctx.workspace,
                    ticket_id,
                    antos_protocol::AgentRole::Coder,
                    Some(target),
                )?;
            }

            let engine = crate::flow::FlowEngine::global();

            let task = if auto {
                println!(
                    "  {} Ejecutando pipeline automatizado de agentes con modelos asignados...",
                    paint("▶", GREEN)
                );
                engine.run_worktree_pipeline(&ctx.workspace, &ctx.state, ticket_id, &[])?
            } else {
                engine.start_task(&ctx.workspace, &ctx.state, ticket_id)?
            };

            println!("  Tarea ID:       {}", paint(&task.id, YELLOW));
            println!("  Ticket:         {}", paint(&task.ticket_id, BOLD));
            println!("  Estado:         {}", task.state.label());
            println!("  Ejecución:      {}", backend_label(&task));
            if let Some(wt) = &task.worktree_path {
                println!("  Worktree:       {}", paint(wt, DIM));
            }
            if let Some(br) = &task.branch_name {
                println!("  Rama de Agente: {}", paint(br, GREEN));
            }
            if let Some(resumen) = &task.audit_summary {
                println!("  Auditoría:      {}", paint(resumen, GREEN));
            }

            println!(
                "\n  {}",
                paint("Historial de Transiciones de Agentes:", BOLD)
            );
            for t in &task.history {
                let rol_fmt = t
                    .role
                    .map(|r| format!(" [{}]", r.name_es()))
                    .unwrap_or_default();
                // El modelo solo se imprime cuando corrió de verdad (T33.1):
                // en una simulación es la etiqueta del modelo asignado.
                let model_fmt = if t.simulated {
                    String::new()
                } else {
                    t.model
                        .as_deref()
                        .map(|m| format!(" ({})", paint(m, CYAN)))
                        .unwrap_or_default()
                };
                println!(
                    "    • {}{}{}: {}",
                    paint(t.new_state.label(), BOLD),
                    paint(&rol_fmt, DIM),
                    model_fmt,
                    t.detail
                );
            }

            if let Some(diff) = &task.diff_preview {
                if !diff.is_empty() {
                    println!(
                        "\n  {}",
                        paint("Previsualización de Diff Consolidado:", BOLD)
                    );
                    println!("    {}", diff.replace('\n', "\n    "));
                }
            }

            println!("\n  {} Tarea procesada correctamente.\n", paint("✓", GREEN));
        }
        "status" => {
            let ticket_id = args.get(1);
            let engine = crate::flow::FlowEngine::global();
            if let Some(tid) = ticket_id {
                if let Some(task) = engine.get_task(tid) {
                    println!(
                        "\n{}",
                        paint(
                            &format!("antOS · Estado de Tarea antFlow [{}]", task.ticket_id),
                            BOLD
                        )
                    );
                    println!("  Estado:     {}", task.state.label());
                    println!("  Ejecución:  {}", backend_label(&task));
                    println!(
                        "  Rol Activo: {}",
                        task.current_role.map(|r| r.name_es()).unwrap_or("Ninguno")
                    );
                    if let Some(wt) = &task.worktree_path {
                        println!("  Worktree:   {}", paint(wt, DIM));
                    }
                    if let Some(br) = &task.branch_name {
                        println!("  Rama:       {}", paint(br, GREEN));
                    }
                    if let Some(diff) = &task.diff_preview {
                        println!("\n  Previsualización Diff:\n    {diff}");
                    }
                    println!("\n  Transiciones:");
                    for h in &task.history {
                        let model_fmt = if h.simulated {
                            String::new()
                        } else {
                            h.model
                                .as_deref()
                                .map(|m| format!(" [{}]", paint(m, CYAN)))
                                .unwrap_or_default()
                        };
                        println!("    • [{}] {}{}", h.new_state.label(), h.detail, model_fmt);
                    }
                    println!();
                } else {
                    println!("\n  No hay tarea activa para el ticket «{tid}».\n");
                }
            } else {
                let tasks = engine.list_tasks();
                println!("\n{}", paint("antOS · Tareas antFlow", BOLD));
                if tasks.is_empty() {
                    println!("  No hay tareas en curso.\n");
                } else {
                    for t in tasks {
                        println!(
                            "  • {:<8} {:<30} {} (reintentos QA: {})",
                            paint(&t.ticket_id, BOLD),
                            t.state.label(),
                            backend_label(&t),
                            t.qa_retries
                        );
                    }
                    println!();
                }
            }
        }
        "swarm" => {
            cmd_swarm(ctx, &args[1..])?;
        }
        _ => {
            bail!("subcomando desconocido para agent. Usa: antos agent do \"<objetivo>\" [--provider p] [--budget N] [--tools a,b] [--dry-run] [-y] | antos agent report | antos agent run <ticket_id> [--node <id>] | antos agent status [ticket_id] | antos agent swarm | antos agents");
        }
    }
    Ok(())
}

pub fn cmd_panel(ctx: &Ctx, args: &[String]) -> Result<()> {
    if let Some(pos) = args.iter().position(|a| a == "--dispatch" || a == "-d") {
        if let Some(target_ticket) = args.get(pos + 1) {
            println!(
                "\n{} Despachando ticket {} al equipo multi-agente antFlow...",
                paint("🚀", BOLD),
                paint(target_ticket, YELLOW)
            );
            let run_args = vec![
                "run".to_string(),
                target_ticket.clone(),
                "--auto".to_string(),
            ];
            return cmd_agent(ctx, &run_args, &crate::cli::args::Opts::default());
        }
    }

    let spec_engine = crate::spec::SpecEngine::global();
    let target_ws = ctx.current_project.as_deref().unwrap_or(&ctx.workspace);
    let tickets = spec_engine.list_tickets(target_ws)?;
    let flow_engine = crate::flow::FlowEngine::global();
    let tasks = flow_engine.list_tasks();

    let banner_text = if let Some(ref cur) = ctx.current_project {
        let name = cur
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("proyecto");
        format!("antOS · KANBAN - PROYECTO: {} (Super + A)", name)
    } else {
        "antOS · CENTRO DE CONTROL DE AGENTES Y TABLERO KANBAN (Super + A)".to_string()
    };

    println!("\n{}", paint("╔══════════════════════════════════════════════════════════════════════════════════════╗", BOLD));
    println!("║ {:^84} ║", paint(&banner_text, BOLD));
    println!("{}\n", paint("╚══════════════════════════════════════════════════════════════════════════════════════╝", BOLD));

    // Monitor de Agentes
    println!(
        "  {}",
        paint("● MONITOR DE AGENTES ACTIVOS (antFlow)", BOLD)
    );
    let roles = [
        ("📐 Arquitecto", antos_protocol::AgentRole::Architect),
        ("💻 Coder", antos_protocol::AgentRole::Coder),
        ("🧪 QA / Tester", antos_protocol::AgentRole::QA),
        ("🛡️ Auditor", antos_protocol::AgentRole::Auditor),
    ];

    for (etiqueta_rol, rol) in roles {
        let active_tasks: Vec<_> = tasks
            .iter()
            .filter(|t| t.current_role == Some(rol))
            .collect();
        if active_tasks.is_empty() {
            println!(
                "    {} {:<18} {}",
                paint("○", DIM),
                etiqueta_rol,
                paint("[Inactivo / En espera]", DIM)
            );
        } else {
            for t in active_tasks {
                println!(
                    "    {} {:<18} {} → Tarea: {} ({})",
                    paint("●", GREEN),
                    paint(etiqueta_rol, BOLD),
                    paint(t.state.label(), YELLOW),
                    paint(&t.ticket_id, BOLD),
                    t.worktree_path.as_deref().unwrap_or("sandbox")
                );
            }
        }
    }
    println!();

    // Columnas Kanban
    let pending: Vec<_> = tickets
        .iter()
        .filter(|t| t.status == antos_protocol::TicketStatus::Pending)
        .collect();
    let in_progress: Vec<_> = tickets
        .iter()
        .filter(|t| t.status == antos_protocol::TicketStatus::InProgress)
        .collect();
    let in_review: Vec<_> = tickets
        .iter()
        .filter(|t| t.status == antos_protocol::TicketStatus::InReview)
        .collect();
    let completed: Vec<_> = tickets
        .iter()
        .filter(|t| t.status == antos_protocol::TicketStatus::Completed)
        .collect();

    println!("  {}", paint("● TABLERO DE TICKETS (docs/tickets/)", BOLD));
    println!("  ┌────────────────────────┬────────────────────────┬────────────────────────┬────────────────────────┐");
    let header_backlog = format!("⏳ BACKLOG ({})", pending.len());
    let header_in_progress = format!("🔄 EN CURSO ({})", in_progress.len());
    let header_in_review = format!("🔍 REVISIÓN ({})", in_review.len());
    let header_done = format!("✅ HECHO ({})", completed.len());
    println!(
        "  │ {:<22} │ {:<22} │ {:<22} │ {:<22} │",
        paint(&header_backlog, BOLD),
        paint(&header_in_progress, BOLD),
        paint(&header_in_review, BOLD),
        paint(&header_done, BOLD)
    );
    println!("  ├────────────────────────┼────────────────────────┼────────────────────────┼────────────────────────┤");

    let max_rows = [
        pending.len(),
        in_progress.len(),
        in_review.len(),
        completed.len(),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);

    for i in 0..max_rows {
        let col1 = pending
            .get(i)
            .map(|t| format!("{} {}", t.id, ellipsis(&t.title, 14)))
            .unwrap_or_default();
        let col2 = in_progress
            .get(i)
            .map(|t| format!("{} {}", t.id, ellipsis(&t.title, 14)))
            .unwrap_or_default();
        let col3 = in_review
            .get(i)
            .map(|t| format!("{} {}", t.id, ellipsis(&t.title, 14)))
            .unwrap_or_default();
        let col4 = completed
            .get(i)
            .map(|t| format!("{} {}", t.id, ellipsis(&t.title, 14)))
            .unwrap_or_default();

        println!(
            "  │ {:<22} │ {:<22} │ {:<22} │ {:<22} │",
            col1, col2, col3, col4
        );
    }
    println!("  └────────────────────────┴────────────────────────┴────────────────────────┴────────────────────────┘");

    println!(
        "\n  {} Usa {} para despachar un ticket al equipo de agentes.",
        paint("💡", YELLOW),
        paint("antos panel --dispatch <TID>", BOLD)
    );
    println!(
        "  {} Usa {} para lanzar la interfaz gráfica Wayland/GTK4.\n",
        paint("🖥️", BOLD),
        paint("antos-barra", BOLD)
    );

    Ok(())
}

pub fn cmd_swarm(ctx: &Ctx, args: &[String]) -> Result<()> {
    let engine = crate::distributed::SwarmEngine::global();
    let sub = args.first().map(String::as_str);

    match sub {
        Some("dispatch" | "despacha") => {
            let ticket_id = args.get(1).ok_or_else(|| {
                anyhow::anyhow!("uso: antos swarm dispatch <TID> [--role <coder|qa>] [--node <ID>]")
            })?;
            let role = if args.iter().any(|a| a == "--qa") {
                antos_protocol::AgentRole::QA
            } else {
                antos_protocol::AgentRole::Coder
            };
            let node_target = args
                .iter()
                .position(|a| a == "--node" || a == "-n")
                .and_then(|i| args.get(i + 1))
                .map(String::as_str);
            let task = engine.dispatch_remote_role(&ctx.workspace, ticket_id, role, node_target)?;
            println!(
                "\n{} Tarea distribuida despachada al Swarm.",
                paint("✓", GREEN)
            );
            println!("  • Tarea ID:   {}", paint(&task.task_id, BOLD));
            println!("  • Ticket:     {}", paint(&task.ticket_id, YELLOW));
            println!("  • Rol:        {}", paint(task.role.name_es(), BOLD));
            println!("  • Nodo:       {}", paint(&task.assigned_node_id, GREEN));
            println!("  • Rama:       {}", paint(&task.worktree_branch, DIM));
            if let Some(m) = task.target_model {
                println!("  • Modelo LLM: {}", paint(&m, CYAN));
            }
            println!();
        }
        _ => {
            let status = engine.status(&ctx.workspace)?;
            println!(
                "\n{}",
                paint("antOS · Centro de Control Swarm Multi-Nodo (T9.2)", BOLD)
            );
            println!(
                "  Espacio de trabajo: {}\n",
                paint(&ctx.workspace.display().to_string(), DIM)
            );

            println!(
                "  Nodos en el clúster: {} · Tareas activas: {}\n",
                paint(&status.nodes.len().to_string(), BOLD),
                paint(&status.total_tasks.to_string(), GREEN)
            );

            for n in &status.nodes {
                let badge = if n.is_local {
                    paint("● LOCAL", GREEN)
                } else {
                    paint("🌐 REMOTO", CYAN)
                };
                let vram_str = n
                    .vram_available_mb
                    .map(|v| format!("{} MB VRAM", v))
                    .unwrap_or_else(|| "N/A".into());
                println!(
                    "  {} [{}] {} · {} ({} CPUs · {})",
                    badge,
                    paint(&n.node_id, BOLD),
                    paint(&n.hostname, BOLD),
                    paint(&n.address, YELLOW),
                    n.cpu_cores,
                    vram_str
                );

                if n.running_tasks.is_empty() {
                    println!("     {} Sin tareas en ejecución.", paint("○", DIM));
                } else {
                    for t in &n.running_tasks {
                        println!(
                            "     └─ Tarea {}: rol {:?} en rama {} [{}]",
                            paint(&t.task_id, BOLD),
                            t.role,
                            paint(&t.worktree_branch, DIM),
                            paint(&t.status, YELLOW)
                        );
                    }
                }
                println!();
            }
            println!("  Usa «antos swarm dispatch <TID> [--node <ID>]» para delegar trabajo a un nodo.\n");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------- vfs

/// Etiqueta de ejecución de una tarea antFlow (T33.1): deja claro cuando el
/// pipeline es la simulación de T3.1 y no un agente con modelo.
fn backend_label(task: &antos_protocol::FlowTask) -> String {
    match task.backend {
        antos_protocol::FlowBackend::Simulated => {
            paint("SIMULACIÓN · ningún modelo participó (T33.1)", YELLOW)
        }
        antos_protocol::FlowBackend::Agent => paint("agente con modelo", GREEN),
    }
}

/// `antos agent do "<objetivo>"` (T33.2): un run de agente con el proveedor
/// activo (o `--provider`), el toolset por defecto (o `--tools a,b,c`) y el
/// presupuesto por defecto (o `--budget <pasos>`). Con el demonio en marcha
/// va por IPC; si no, en proceso — las garantías son las mismas.
fn cmd_agent_do(ctx: &Ctx, args: &[String], opts: &crate::cli::args::Opts) -> Result<()> {
    let mut goal: Vec<String> = Vec::new();
    let mut provider: Option<String> = None;
    let mut tools: Option<Vec<String>> = None;
    let mut budget = antos_protocol::AgentBudget::default();
    // `-y`/`--dry-run` globales los consume `Opts`; aquí se aceptan también
    // tras el subcomando por comodidad.
    let mut dry_run = opts.dry_run;
    let mut assume_yes = opts.assume_yes;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--provider" | "-p" => {
                provider = args.get(i + 1).cloned();
                i += 1;
            }
            "--tools" | "-t" => {
                tools = args
                    .get(i + 1)
                    .map(|t| t.split(',').map(|s| s.trim().to_string()).collect());
                i += 1;
            }
            "--budget" | "-b" => {
                budget.max_steps = args
                    .get(i + 1)
                    .and_then(|b| b.parse().ok())
                    .ok_or_else(|| anyhow::anyhow!("--budget espera un número de pasos"))?;
                i += 1;
            }
            "--dry-run" | "--seco" => dry_run = true,
            "--yes" | "-y" | "--si" => assume_yes = true,
            other => goal.push(other.to_string()),
        }
        i += 1;
    }
    let goal = goal.join(" ");
    if goal.trim().is_empty() {
        bail!("uso: antos agent do \"<objetivo>\" [--provider p] [--budget N] [--tools a,b] [--dry-run] [-y]");
    }

    let force_local =
        crate::util::env_with_legacy_fallback("ANTOS_SIN_DEMONIO", "SYSO_SIN_DEMONIO").is_some();
    if !force_local && crate::ipc::daemon_is_running(ctx) {
        crate::ipc::remote_agent_run(
            &crate::ipc::socket_path(ctx),
            &goal,
            provider.as_deref(),
            tools,
            Some(budget),
            dry_run,
            assume_yes,
        )?;
        return Ok(());
    }

    let catalog = crate::capability::Catalog::load(&ctx.caps_dir)?;
    let mut prov = crate::agent::providers::resolve(&ctx.state, provider.as_deref())?;
    let mut cfg = crate::agent::RunConfig::new(goal);
    if let Some(t) = tools {
        cfg.toolset = t;
    }
    cfg.budget = budget;
    cfg.dry_run = dry_run;
    let mut terminal = crate::terminal::Terminal::new(assume_yes);
    crate::agent::run(ctx, &catalog, &mut *prov, &cfg, &mut terminal)?;
    Ok(())
}

/// `antos agent report`: los runs de agente registrados en el journal, del
/// más reciente al más antiguo, con su instantánea (deshacible con `undo`).
fn cmd_agent_report(ctx: &Ctx) -> Result<()> {
    let records = crate::journal::read_all(&ctx.journal_path()).unwrap_or_default();
    let runs: Vec<_> = records
        .iter()
        .rev()
        .filter(|r| r.planner.starts_with("agent:"))
        .collect();
    println!("\n{}", paint("antOS · Runs de agente (journal)", BOLD));
    if runs.is_empty() {
        println!("  Ninguno todavía. Prueba: antos agent do \"haz que pase el test X\"\n");
        return Ok(());
    }
    for r in runs {
        println!(
            "  • {} {} · {} pasos · {} · {}{}",
            paint(&r.id, YELLOW),
            paint(&r.at, DIM),
            r.plan.steps.len(),
            r.detail.as_deref().unwrap_or("-"),
            r.intent,
            if r.reverted {
                paint(" (revertido)", DIM)
            } else {
                String::new()
            }
        );
        for s in &r.plan.steps {
            println!(
                "      {} {}",
                paint(&s.capability, CYAN),
                crate::agent::tools::summarize_args(&s.args)
            );
        }
    }
    println!();
    Ok(())
}
