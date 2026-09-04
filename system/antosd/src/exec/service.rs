//! Ephemeral nix services and port diagnostics/management.

use std::collections::BTreeMap;
use anyhow::Result;
use crate::ctx::Ctx;
use super::Change;

pub fn changes_for(
    cap: &str,
    a: &BTreeMap<String, String>,
    ctx: &Ctx,
) -> Result<Option<Vec<Change>>> {
    match cap {
        "diag.port_status" => {
            let port = a.get("port").and_then(|p| p.parse::<u16>().ok());
            Ok(Some(vec![Change::PortStatus { port }]))
        }

        "diag.port_kill" => {
            let port = a
                .get("port")
                .and_then(|p| p.parse::<u16>().ok())
                .ok_or_else(|| anyhow::anyhow!("debes especificar el puerto a liberar"))?;
            let force = a.get("force").map(|f| f == "true").unwrap_or(false);
            Ok(Some(vec![Change::PortKill { port, force }]))
        }

        "env.service_up" => {
            let service = a
                .get("service")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("debes especificar el nombre del servicio (ej. postgres, redis)"))?;
            let port = a.get("port").and_then(|p| p.parse::<u16>().ok());
            let db_name = a.get("db_name").cloned();
            Ok(Some(vec![Change::ServiceUp {
                service,
                port,
                db_name,
                state_dir: ctx.state.clone(),
                workspace: ctx.workspace.clone(),
            }]))
        }

        "env.service_down" => {
            let service = a
                .get("service")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("debes especificar el nombre del servicio a detener"))?;
            Ok(Some(vec![Change::ServiceDown {
                service,
                state_dir: ctx.state.clone(),
            }]))
        }

        "env.service_status" => {
            let service = a.get("service").cloned();
            Ok(Some(vec![Change::ServiceStatus {
                service,
                state_dir: ctx.state.clone(),
            }]))
        }

        _ => Ok(None),
    }
}

pub fn apply(change: &Change) -> Result<Option<String>> {
    match change {
        Change::PortStatus { port } => {
            let puertos = crate::net::diagnosticar_puertos(*port)?;
            if puertos.is_empty() {
                if let Some(p) = port {
                    Ok(Some(format!("puerto {p} está libre")))
                } else {
                    Ok(Some("no hay puertos de desarrollo en escucha".into()))
                }
            } else {
                let mut lineas = Vec::new();
                for p in puertos {
                    let dir_info = p
                        .working_dir
                        .as_deref()
                        .map(|d| format!(" (en {d})"))
                        .unwrap_or_default();
                    lineas.push(format!(
                        "puerto {:<5} | PID {:<6} | {:<15} | {}{dir_info}",
                        p.port, p.pid, p.process_name, p.command
                    ));
                }
                Ok(Some(lineas.join("\n")))
            }
        }
        Change::PortKill { port, force } => {
            let eliminados = crate::net::liberar_puerto(*port, *force)?;
            if eliminados.is_empty() {
                Ok(Some(format!("puerto {port} ya estaba libre")))
            } else {
                let pids: Vec<String> = eliminados
                    .iter()
                    .map(|p| format!("PID {} ({})", p.pid, p.process_name))
                    .collect();
                Ok(Some(format!("puerto {port} liberado terminando {}", pids.join(", "))))
            }
        }
        Change::ServiceUp {
            service,
            port,
            db_name,
            state_dir,
            workspace,
        } => {
            let info = crate::service::start_service(
                service,
                *port,
                db_name.as_deref(),
                state_dir,
                workspace,
            )?;
            Ok(Some(format!(
                "servicio «{}» arrancado en puerto {} | {}={}",
                info.name, info.port, info.env_var_key, info.env_var_value
            )))
        }
        Change::ServiceDown {
            service,
            state_dir,
        } => {
            crate::service::stop_service(service, state_dir)?;
            Ok(Some(format!("servicio «{service}» detenido y limpiado")))
        }
        Change::ServiceStatus {
            service,
            state_dir,
        } => {
            let services = crate::service::get_service_status(service.as_deref(), state_dir)?;
            if services.is_empty() {
                Ok(Some("no hay servicios efímeros aprovisionados".into()))
            } else {
                let mut lines = Vec::new();
                for s in services {
                    lines.push(format!(
                        "servicio {:<12} | puerto {:<5} | estado {:<8} | {}={}",
                        s.name, s.port, s.status, s.env_var_key, s.env_var_value
                    ));
                }
                Ok(Some(lines.join("\n")))
            }
        }
        _ => Ok(None),
    }
}
