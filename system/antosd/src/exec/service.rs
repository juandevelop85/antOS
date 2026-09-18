//! Servicios efímeros (`env.service_*`) y diagnóstico de puertos.
//!
//! Los servicios los arranca `crate::service` de verdad (T34.1): cuando esta
//! función corre dentro del ejecutor confinado, el proceso del servicio
//! hereda el recinto y sobrevive al ejecutor porque se separa de sesión.

use super::Change;
use crate::ctx::Ctx;
use anyhow::Result;
use std::collections::BTreeMap;

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
            let service = a.get("service").cloned().ok_or_else(|| {
                anyhow::anyhow!("debes especificar el nombre del servicio (ej. postgres, redis)")
            })?;
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
            let service = a.get("service").cloned().ok_or_else(|| {
                anyhow::anyhow!("debes especificar el nombre del servicio a detener")
            })?;
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

fn services_had_unknown_health(lines: &[String]) -> bool {
    lines.iter().any(|l| l.contains("| salud ?"))
}

pub fn apply(change: &Change) -> Result<Option<String>> {
    match change {
        Change::PortStatus { port } => {
            let ports = crate::net::diagnose_ports(*port)?;
            if ports.is_empty() {
                if let Some(p) = port {
                    Ok(Some(format!("puerto {p} está libre")))
                } else {
                    Ok(Some("no hay ports de desarrollo en escucha".into()))
                }
            } else {
                let mut lines = Vec::new();
                for p in ports {
                    let dir_info = p
                        .working_dir
                        .as_deref()
                        .map(|d| format!(" (en {d})"))
                        .unwrap_or_default();
                    lines.push(format!(
                        "puerto {:<5} | PID {:<6} | {:<15} | {}{dir_info}",
                        p.port, p.pid, p.process_name, p.command
                    ));
                }
                Ok(Some(lines.join("\n")))
            }
        }
        Change::PortKill { port, force } => {
            let removed = crate::net::kill_port(*port, *force)?;
            if removed.is_empty() {
                Ok(Some(format!("puerto {port} ya estaba libre")))
            } else {
                let pids: Vec<String> = removed
                    .iter()
                    .map(|p| format!("PID {} ({})", p.pid, p.process_name))
                    .collect();
                Ok(Some(format!(
                    "puerto {port} liberado terminando {}",
                    pids.join(", ")
                )))
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
            let verb = match info.backend {
                crate::service::ServiceBackend::External => "adoptado (ya escuchaba)",
                _ => "arrancado",
            };
            let pid = info.pid.map(|p| format!(" PID {p} ·")).unwrap_or_default();
            Ok(Some(format!(
                "servicio «{}» {verb} en 127.0.0.1:{} ({}) ·{pid} sano | {}={}",
                info.name,
                info.port,
                info.backend.label(),
                info.env_var_key,
                info.env_var_value
            )))
        }
        Change::ServiceDown { service, state_dir } => {
            use crate::service::StopOutcome;
            let msg = match crate::service::stop_service(service, state_dir)? {
                StopOutcome::Terminated { pid, forced: false } => {
                    format!("servicio «{service}» detenido (PID {pid}); los datos se conservan")
                }
                StopOutcome::Terminated { pid, forced: true } => format!(
                    "servicio «{service}» detenido con SIGKILL (PID {pid} no atendió SIGTERM); los datos se conservan"
                ),
                StopOutcome::AlreadyGone => {
                    format!("servicio «{service}» ya no estaba corriendo; registro actualizado")
                }
                StopOutcome::ExternalUnregistered => format!(
                    "servicio «{service}» era externo: antOS retira el registro y no toca el proceso"
                ),
            };
            Ok(Some(msg))
        }
        Change::ServiceStatus { service, state_dir } => {
            let services = crate::service::get_service_status(service.as_deref(), state_dir)?;
            if services.is_empty() {
                Ok(Some("no hay servicios efímeros registrados".into()))
            } else {
                let mut lines = Vec::new();
                for s in services {
                    let pid = s.pid.map(|p| p.to_string()).unwrap_or_else(|| "-".into());
                    lines.push(format!(
                        "servicio {:<12} | puerto {:<5} | {:<9} | {:<8} | salud {:<3} | PID {:<6} | {}={}",
                        s.name,
                        s.port,
                        s.status,
                        s.backend.label(),
                        s.health.label(),
                        pid,
                        s.env_var_key,
                        s.env_var_value
                    ));
                }
                if services_had_unknown_health(&lines) {
                    lines.push(
                        "(salud «?»: la sonda de loopback no está disponible en este recinto; \
                         el estado se basa solo en el PID)"
                            .into(),
                    );
                }
                Ok(Some(lines.join("\n")))
            }
        }
        _ => Ok(None),
    }
}
