//! `antos web` (consola web remota).
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

/// Argumentos de `antos web start` (T31.13).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WebStartArgs {
    pub bind: String,
    pub port: u16,
}

/// Analiza los argumentos de `antos web start` (T31.13).
fn parse_web_start_args(args: &[String]) -> WebStartArgs {
    let mut bind = "127.0.0.1".to_string();
    let mut port = 8088u16;
    let mut i = 1;
    while i < args.len() {
        if (args[i] == "--bind" || args[i] == "-b") && i + 1 < args.len() {
            bind = args[i + 1].clone();
            i += 2;
        } else if (args[i] == "--port" || args[i] == "-p") && i + 1 < args.len() {
            if let Ok(p) = args[i + 1].parse::<u16>() {
                port = p;
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    WebStartArgs { bind, port }
}

/// Argumentos de `antos web token` (T31.13).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WebTokenArgs {
    pub label: Option<String>,
    pub ttl: Option<u64>,
}

/// Analiza los argumentos de `antos web token` (T31.13).
fn parse_web_token_args(args: &[String]) -> WebTokenArgs {
    let mut label = None;
    let mut ttl = None;
    let mut i = 1;
    while i < args.len() {
        if (args[i] == "--label" || args[i] == "-l") && i + 1 < args.len() {
            label = Some(args[i + 1].clone());
            i += 2;
        } else if (args[i] == "--ttl" || args[i] == "-t") && i + 1 < args.len() {
            if let Ok(v) = args[i + 1].parse::<u64>() {
                ttl = Some(v);
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    WebTokenArgs { label, ttl }
}

pub fn cmd_web(ctx: &Ctx, args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "start" => {
            let WebStartArgs { bind, port } = parse_web_start_args(args);

            let config = antos_protocol::WebConsoleConfig {
                bind_addr: bind.clone(),
                port,
                auth_required: true,
                ws_ping_interval_secs: 30,
            };

            println!(
                "\n{} Iniciando consola web remota y bridge WebSocket...",
                paint("antOS Web Console ·", BOLD)
            );
            let token_sess = crate::web::WebEngine::generate_token(
                &ctx.state,
                Some("admin".into()),
                Some(86400),
            )?;
            let url = format!("http://{}:{}", bind, port);
            println!(
                "  Estado:               {}",
                paint("ACTIVO (En línea)", GREEN)
            );
            println!("  URL de Acceso:        {}", paint(&url, CYAN));
            println!(
                "  URL con Token:        {}",
                paint(&format!("{}?token={}", url, token_sess.token), BOLD)
            );
            println!("  WebSocket Bridge:     {}/ws/events", url);
            println!("  Presiona Ctrl+C para detener el servidor.\n");

            crate::web::WebEngine::serve_blocking(&ctx.state, &ctx.workspace, config)?;
        }

        "stop" => {
            println!(
                "\n{} Deteniendo servidor de consola web...",
                paint("antOS Web Console ·", BOLD)
            );
            let st = crate::web::WebEngine::stop(&ctx.state)?;
            println!(
                "  Estado:               {}\n",
                paint(if st.running { "ACTIVO" } else { "DETENIDO" }, RED)
            );
        }

        "token" => {
            let WebTokenArgs { label, ttl } = parse_web_token_args(args);

            println!(
                "\n{} Generando token de autenticación...",
                paint("antOS Web Console ·", BOLD)
            );
            let session = crate::web::WebEngine::generate_token(&ctx.state, label, ttl)?;
            let st = crate::web::WebEngine::status(&ctx.state)?;
            println!("  Token:                {}", paint(&session.token, BOLD));
            println!(
                "  Expira en:            {}s",
                session.expires_at.saturating_sub(session.created_at)
            );
            if let Some(ref l) = session.client_label {
                println!("  Cliente / Dispositivo: {}", l);
            }
            println!(
                "  Enlace de Conexión:   {}\n",
                paint(&format!("{}?token={}", st.url, session.token), CYAN)
            );
        }

        _ => {
            let st = crate::web::WebEngine::status(&ctx.state)?;
            let status_badge = if st.running {
                paint("ACTIVO (En línea)", GREEN)
            } else {
                paint("DETENIDO", RED)
            };
            println!(
                "\n{} Estado del Servidor Web y Bridge WebSocket:",
                paint("antOS Web Console ·", BOLD)
            );
            println!("  • Estado:                 {}", status_badge);
            println!("  • Dirección y Puerto:     {}:{}", st.bind_addr, st.port);
            println!("  • URL de Acceso:          {}", paint(&st.url, CYAN));
            println!("  • Clientes Conectados:    {}", st.connected_clients);
            println!("  • Sesiones Token Activas: {}", st.active_sessions_count);
            println!("\n  Uso:");
            println!("    antos web start [--port <puerto>] [--bind <ip>]  Inicia el servidor web");
            println!("    antos web stop                                  Detiene el servidor");
            println!(
                "    antos web status                                Muestra estado y métricas"
            );
            println!("    antos web token [--label <nombre>] [--ttl <s]>  Genera enlace seguro\n");
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ tickets

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn args_of(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // -- antos web start / token ---------------------------------------------

    #[test]
    fn test_parse_web_start_args_reads_bind_and_port() {
        let args = args_of(&["start", "--bind", "0.0.0.0", "--port", "9090"]);
        let parsed = parse_web_start_args(&args);
        assert_eq!(
            parsed,
            WebStartArgs {
                bind: "0.0.0.0".into(),
                port: 9090,
            }
        );
    }

    #[test]
    fn test_parse_web_start_args_keeps_default_port_on_an_out_of_range_value() {
        // 99999 no cabe en un u16: se conserva el puerto por defecto.
        let args = args_of(&["start", "--port", "99999"]);
        let parsed = parse_web_start_args(&args);
        assert_eq!(parsed.port, 8088);
    }

    #[test]
    fn test_parse_web_start_args_ignores_a_trailing_flag_with_no_value() {
        let args = args_of(&["start", "--bind"]);
        let parsed = parse_web_start_args(&args);
        assert_eq!(parsed.bind, "127.0.0.1");
    }

    #[test]
    fn test_parse_web_token_args_reads_label_and_ttl() {
        let args = args_of(&["token", "--label", "laptop", "--ttl", "3600"]);
        let parsed = parse_web_token_args(&args);
        assert_eq!(
            parsed,
            WebTokenArgs {
                label: Some("laptop".into()),
                ttl: Some(3600),
            }
        );
    }

    #[test]
    fn test_parse_web_token_args_keeps_ttl_none_on_an_unparseable_value() {
        let args = args_of(&["token", "--ttl", "forever"]);
        let parsed = parse_web_token_args(&args);
        assert_eq!(parsed.ttl, None);
    }

    #[test]
    fn test_parse_web_token_args_ignores_a_trailing_flag_with_no_value() {
        let args = args_of(&["token", "--label"]);
        let parsed = parse_web_token_args(&args);
        assert_eq!(parsed.label, None);
    }
}
