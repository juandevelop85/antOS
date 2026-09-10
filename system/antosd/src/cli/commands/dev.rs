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

pub fn cmd_dev(ctx: &Ctx, args: &[String]) -> Result<()> {
    let mut project = None;
    let mut is_preview = false;
    let mut is_status = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--project" | "-p" => {
                if let Some(p) = args.get(i + 1) {
                    project = Some(p.as_str());
                    i += 1;
                }
            }
            "--preview" => is_preview = true,
            "--status" => is_status = true,
            val if !val.starts_with('-') && project.is_none() => {
                project = Some(val);
            }
            _ => {}
        }
        i += 1;
    }

    if is_status {
        let status = crate::dev_tui::DevWorkspaceManager::get_status(project, &ctx.workspace);
        println!(
            "\n{}",
            paint("antOS · Espacio de Trabajo Integrado Dev TUI (T20.1)", BOLD)
        );
        println!(
            "  Proyecto activo:     {}",
            paint(
                status.active_project.as_deref().unwrap_or("workspace"),
                CYAN
            )
        );
        println!(
            "  Editor configurado:  {}",
            paint(&status.editor_command, GREEN)
        );
        println!(
            "  Dimensiones:         {}x{}",
            status.term_columns, status.term_rows
        );
        println!(
            "  Panel lateral:       {}",
            if status.side_panel_visible {
                paint("visible", GREEN)
            } else {
                paint("oculto", YELLOW)
            }
        );
        println!(
            "  Terminal inferior:   {}",
            if status.terminal_drawer_open {
                paint("abierta", GREEN)
            } else {
                paint("cerrada", DIM)
            }
        );
        println!("\n  Atajos registrados:");
        for hk in &status.registered_hotkeys {
            println!("    • {hk}");
        }
        println!();
        return Ok(());
    }

    use std::io::IsTerminal;
    let is_interactive = !is_preview
        && std::io::stdout().is_terminal()
        && std::env::var("CI").is_err()
        && std::env::var("ANTOS_TEST").is_err();

    crate::dev_tui::DevWorkspaceManager::launch(project, &ctx.workspace, is_interactive)
}

// ------------------------------------------------------------------ reproduce / testgen (T20.2)

pub fn cmd_edit(args: &[String]) -> Result<()> {
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "nvim".to_string());

    let target = args.first().map(String::as_str);
    if let Some(file) = target {
        println!(
            "{} Abriendo '{}' con el editor predeterminado ({})",
            paint("antOS ·", BOLD),
            paint(file, CYAN),
            paint(&editor, GREEN)
        );
    } else {
        println!(
            "{} Iniciando editor de texto predeterminado ({})",
            paint("antOS ·", BOLD),
            paint(&editor, GREEN)
        );
    }

    let mut cmd = std::process::Command::new(&editor);
    if let Some(file) = target {
        cmd.arg(file);
    }

    let status = cmd
        .status()
        .with_context(|| format!("No se pudo ejecutar el editor '{editor}'"))?;
    if !status.success() {
        anyhow::bail!("El editor '{}' finalizó con código no exitoso", editor);
    }
    Ok(())
}

// --------------------------------------------------------------------- pair

pub fn cmd_lsp(ctx: &Ctx, args: &[String]) -> Result<()> {
    let server = crate::lsp::LspServer::global();
    let sub = args.first().map(String::as_str);

    match sub {
        Some("status" | "info") => {
            let status = server.get_status(&ctx.workspace);
            println!(
                "\n{}",
                paint("antOS · Unified Language Server Protocol (LSP)", BOLD)
            );
            println!(
                "  Espacio de trabajo:     {}",
                paint(&ctx.workspace.display().to_string(), DIM)
            );
            println!(
                "  Estado del servidor:    {}",
                if status.running {
                    paint("Activo", GREEN)
                } else {
                    paint("En espera", YELLOW)
                }
            );
            println!(
                "  Transporte:             {}",
                paint(&status.transport, CYAN)
            );
            println!("  Clientes conectados:    {}", status.connected_clients);
            println!(
                "  Símbolos AST indexados: {}",
                paint(&status.indexed_symbols_count.to_string(), BOLD)
            );
            println!(
                "  Capacidades LSP:        {}\n",
                paint(&status.capabilities.join(", "), DIM)
            );
        }
        Some("config" | "conf") => {
            let editor_str = args.get(1).map(String::as_str).unwrap_or("neovim");
            let editor_kind = match editor_str.to_lowercase().as_str() {
                "neovim" | "nvim" | "vim" => antos_protocol::LspEditorKind::Neovim,
                "vscode" | "code" => antos_protocol::LspEditorKind::VsCode,
                "helix" | "hx" => antos_protocol::LspEditorKind::Helix,
                "emacs" => antos_protocol::LspEditorKind::Emacs,
                _ => antos_protocol::LspEditorKind::Generic,
            };

            let (snippet, target_file) = server.generate_config(editor_kind, &ctx.workspace);
            println!(
                "\n{} Configuración de antOS LSP para: {}",
                paint("antOS LSP ·", BOLD),
                paint(editor_kind.name(), YELLOW)
            );
            println!(
                "  Archivo de configuración: {}\n",
                paint(&target_file, CYAN)
            );
            println!("{}\n", snippet);
        }
        Some("stdio" | "run" | "start") => {
            server.run_stdio(&ctx.workspace)?;
        }
        None => {
            server.run_stdio(&ctx.workspace)?;
        }
        _ => {
            let status = server.get_status(&ctx.workspace);
            println!(
                "\n{}",
                paint("antOS · Unified Language Server Protocol (LSP)", BOLD)
            );
            println!(
                "  Espacio de trabajo:     {}",
                paint(&ctx.workspace.display().to_string(), DIM)
            );
            println!(
                "  Símbolos AST indexados: {}\n",
                paint(&status.indexed_symbols_count.to_string(), BOLD)
            );
            println!("  Subcomandos disponibles:");
            println!(
                "    • antos lsp [stdio]            Inicia el servidor JSON-RPC 2.0 sobre stdio"
            );
            println!("    • antos lsp status             Diagnostica el estado del servidor y conexiones");
            println!("    • antos lsp config [editor]    Genera configuración (por defecto Neovim, o vscode/helix/emacs)\n");
        }
    }
    Ok(())
}

// --------------------------------------------------------------------- edit / editor

pub fn cmd_pair(ctx: &Ctx, args: &[String]) -> Result<()> {
    let engine = crate::collab::CollabEngine::global();
    let first = args.first().map(String::as_str);

    let (ticket_id, file_path) = match first {
        Some(arg)
            if arg.starts_with('T')
                && arg
                    .chars()
                    .nth(1)
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false) =>
        {
            let file = args.get(1).map(String::as_str).unwrap_or("src/main.rs");
            (Some(arg.to_string()), file.to_string())
        }
        Some(file) => (None, file.to_string()),
        None => (None, "src/main.rs".to_string()),
    };

    println!(
        "\n{} Iniciando sesión interactiva de Pair Programming con Coder...",
        paint("antOS Pair ·", BOLD)
    );
    let status = engine.start_session(&ctx.workspace, &file_path, ticket_id)?;

    println!("\n{}", paint("Sesión de Co-Edición Activa:", BOLD));
    println!(
        "  ID de Sesión:          {}",
        paint(&status.session_id, CYAN)
    );
    println!(
        "  Archivo compartido:    {}",
        paint(&status.file_path, YELLOW)
    );
    println!(
        "  Colaboradores:         {}",
        paint(&status.collaborators.join(" & "), GREEN)
    );
    if let Some(ref t) = status.active_ticket_id {
        println!("  Ticket vinculado:      {}", paint(t, BOLD));
    }
    println!(
        "  Tamaño del buffer:     {} caracteres\n",
        status.buffer_length
    );

    println!(
        "{}",
        paint("Cursores y Sugerencias de Código (Ghost Text):", BOLD)
    );
    for c in &status.cursors {
        println!(
            "  • [{}] Línea {}, Columna {}",
            paint(&c.client_id, BOLD),
            c.line,
            c.character
        );
        if let Some(ref ghost) = c.ghost_text {
            println!("      └─ Ghost text sugerido: {}", paint(ghost, DIM));
        }
    }
    println!("\n  Consejo: Presiona <Tab> en tu editor o ejecuta «antos intent acepta el ghost text» para fusionar.\n");

    Ok(())
}

// -------------------------------------------------------------------- debug

pub fn cmd_debug(_ctx: &Ctx, args: &[String]) -> Result<()> {
    let command = if !args.is_empty() {
        args.join(" ")
    } else {
        "cargo test".to_string()
    };

    println!(
        "\n{} Maqueta de sesión DAP para: {} {}",
        paint("antOS DAP Debugger ·", BOLD),
        paint(&command, YELLOW),
        paint(
            "(SIMULADO: no se ejecuta el comando ni se adjunta ningún depurador — T31.14)",
            YELLOW
        )
    );
    let mut dap = crate::collab::DapServer::new("dap-cli".into(), command.clone());
    let bp = dap.add_breakpoint("src/main.rs", 1);

    println!(
        "\n{}",
        paint(
            "Sesión de Depuración Simulada (pila y variables son datos de ejemplo fijos):",
            BOLD
        )
    );
    println!("  ID de Sesión:          {}", paint(&dap.session_id, CYAN));
    println!("  Comando (no ejecutado):{}", paint(&dap.command, BOLD));
    println!("  Estado:                {}", paint(&dap.state, GREEN));
    println!(
        "  Punto de interrupción: {}:{} (verificado: {})\n",
        bp.file_path, bp.line, bp.verified
    );

    println!("{}", paint("Pila de Llamadas (Call Stack):", BOLD));
    for (i, frame) in dap.call_stack.iter().enumerate() {
        println!("  {}. {}", i + 1, frame);
    }
    println!();

    println!("{}", paint("Variables Locales en Alcance:", BOLD));
    for var in &dap.variables {
        println!(
            "  • {:<16} ({}) = {}",
            paint(&var.name, YELLOW),
            var.type_name,
            paint(&var.value, CYAN)
        );
    }
    println!();

    Ok(())
}

// ------------------------------------------------------------------ dev / workspace (T20.1)
