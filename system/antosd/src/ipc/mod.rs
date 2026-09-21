//! El demonio y su cliente.
//!
//! `antosd` es el demonio central del sistema operativo antOS.
//!
//! ## Por qué ahora
//!
//! Un escritorio necesita el mismo recorrido movido por otra interfaz. Sin
//! esta separación habría que duplicarlo, y duplicar un recorrido es duplicar
//! sus garantías — la puerta de confirmación, el radio de impacto, la
//! instantánea. Dos copias de una garantía acaban siendo una.
//!
//! ## Qué NO cambia
//!
//! El demonio decide el nivel de permiso y comprueba las concesiones. Un
//! cliente solo puede contestar sí o no a una propuesta: no puede fabricar un
//! nivel más bajo ni saltarse la puerta, porque no es él quien la pone.
//!
//! ## Quién puede conectarse
//!
//! El socket vive en el directorio de estado con permisos 0600, sin ventana
//! transitoria más permisiva: se crea bajo una `umask` restrictiva propia y
//! solo después se restaura la del proceso (T31.8). Esa es hoy toda la
//! autorización: quien pueda abrir el fichero puede pedir cosas. Es
//! suficiente para un solo usuario en su máquina y claramente insuficiente
//! para cualquier otra cosa.
//!
//! ## Servidor concurrente y desacoplamiento de mutación (T32.3)
//!
//! `serve` despacha cada conexión en su propio hilo de trabajo. A diferencia
//! del diseño serial original (T31.8), las consultas de solo lectura y telemetría
//! (`QueryGitStatus`, `ListTickets`, `GetTicket`, `QueryBarraTelemetry`, etc.)
//! no sufren bloqueo alguno cuando un cliente mantiene una sesión interactiva abierta.
//!
//! Para prevenir que dos intenciones simultáneas modifiquen el mismo espacio
//! de trabajo a la vez produciendo diffs inconsistentes, la ejecución de
//! `Request::Intent` se sincroniza mediante `WORKSPACE_MUTATION_LOCK`, encolando
//! ordenadamente las mutaciones sin bloquear al resto de clientes.

#![allow(unused_imports, dead_code)]

use crate::capability::Catalog;
use crate::ctx::Ctx;
use crate::protocol::{ExecutionResult, Proposal, SessionHandler};
use crate::{session, terminal};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

mod transport;

use transport::*;
pub use transport::{serve, socket_path};

/// Cerrojo exclusivo para mutaciones de espacio de trabajo / sesiones de intención (T32.3).
/// Evita que dos intenciones simultáneas modifiquen el workspace y produzcan diffs inconsistentes,
/// permitiendo a la vez que todas las consultas de solo lectura y telemetría se ejecuten
/// concurrentemente sin bloqueo.
pub static WORKSPACE_MUTATION_LOCK: Mutex<()> = Mutex::new(());

fn handle_connection(ctx: &Ctx, catalog: &Catalog, stream: UnixStream) -> Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;

    let request = match receive::<Request>(&mut reader)? {
        Received::Message(r) => r,
        Received::Eof => return Ok(()),
        Received::TooLarge => {
            send(
                &mut writer,
                &Event::Error(format!(
                    "mensaje IPC excede el límite de {MAX_MESSAGE_BYTES} bytes"
                )),
            )?;
            return Ok(());
        }
    };

    // A real request just arrived: this is a live, actively-communicating
    // session, not an idle or abandoned connection. Lift the read timeout
    // for what may be a long, human-interactive approval wait (T31.8) — see
    // the module doc comment. The write timeout stays in effect for the
    // rest of the connection.
    let _ = writer.set_read_timeout(None);

    match request {
        Request::Intent {
            text,
            planner,
            dry_run,
        } => {
            // T32.3: Serializa exclusivamente las mutaciones de workspace entre intenciones,
            // permitiendo que consultas de lectura y telemetría sigan respondiendo en paralelo.
            let _mutation_guard = WORKSPACE_MUTATION_LOCK
                .lock()
                .unwrap_or_else(|p| p.into_inner());

            let planner_instance = crate::pick_planner(Some(ctx), planner.as_deref())?;
            let mut handler = SocketHandler {
                writer: &mut writer,
                reader: &mut reader,
            };
            if let Err(e) = session::intent_session(
                ctx,
                catalog,
                &text,
                &*planner_instance,
                dry_run,
                &mut handler,
            ) {
                send(&mut writer, &Event::Error(format!("{e:#}")))?;
            }
        }
        Request::AgentRun {
            goal,
            provider,
            toolset,
            budget,
            dry_run,
        } => {
            // Un run de agente muta el workspace durante varios turnos: se
            // serializa igual que una intención (T32.3).
            let _mutation_guard = WORKSPACE_MUTATION_LOCK
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            let mut handler = SocketHandler {
                writer: &mut writer,
                reader: &mut reader,
            };
            let mut cfg = crate::agent::RunConfig::new(goal);
            if let Some(t) = toolset {
                cfg.toolset = t;
                cfg.toolset_compact = None;
            }
            if let Some(b) = budget {
                cfg.budget = b;
            }
            cfg.dry_run = dry_run;
            let outcome = crate::agent::providers::resolve(&ctx.state, provider.as_deref())
                .and_then(|mut p| crate::agent::run(ctx, catalog, &mut *p, &cfg, &mut handler));
            if let Err(e) = outcome {
                send(&mut writer, &Event::Error(format!("{e:#}")))?;
            }
        }
        Request::AgentStop => {
            // Fuera de un run no hay nada que detener (T33.4).
            send(
                &mut writer,
                &Event::Note("no hay ningún run de agente activo".into()),
            )?;
        }
        Request::Approval(_) => {
            send(
                &mut writer,
                &Event::Error("approval without prior proposal".into()),
            )?;
        }
        Request::QueryGitStatus { workspace_path } => {
            match crate::git::GitAnalyzer::global().consultar_estado(Path::new(&workspace_path)) {
                Ok(Some(status)) => {
                    send(&mut writer, &Event::GitStatus(status))?;
                }
                Ok(None) => {
                    send(&mut writer, &Event::NotGitRepo)?;
                }
                Err(e) => {
                    send(&mut writer, &Event::Error(format!("{e:#}")))?;
                }
            }
        }
        Request::ListTickets { workspace_path } => {
            match crate::spec::SpecEngine::global().list_tickets(Path::new(&workspace_path)) {
                Ok(tickets) => {
                    send(&mut writer, &Event::TicketList(tickets))?;
                }
                Err(e) => {
                    send(&mut writer, &Event::Error(format!("{e:#}")))?;
                }
            }
        }
        Request::GetTicket {
            workspace_path,
            ticket_id,
        } => {
            match crate::spec::SpecEngine::global()
                .get_ticket(Path::new(&workspace_path), &ticket_id)
            {
                Ok(detalle) => {
                    send(&mut writer, &Event::TicketDetail(detalle))?;
                }
                Err(e) => {
                    send(&mut writer, &Event::Error(format!("{e:#}")))?;
                }
            }
        }
        Request::DiagnosePorts { port } => match crate::net::diagnose_ports(port) {
            Ok(puertos) => {
                send(&mut writer, &Event::PortsStatus(puertos))?;
            }
            Err(e) => {
                send(&mut writer, &Event::Error(format!("{e:#}")))?;
            }
        },
        Request::StartFlow {
            workspace_path,
            ticket_id,
        } => {
            // T33.4: con proveedores configurados, el despacho desde la barra
            // corre el pipeline REAL (T33.3) por esta misma conexión: pasos
            // (`AgentStep`), aprobaciones (`Proposal`/`Approval`) e informes
            // (`AgentDone`) llegan a quien despachó. Sin proveedor, se arranca
            // la tarea simulada de T3.1 y se dice.
            let ws = Path::new(&workspace_path);
            let llm_config = crate::llm::LlmConfig::load_from_state(&ctx.state);
            let coder_spec = llm_config.get_role_model("coder");
            let has_provider = crate::agent::providers::resolve(&ctx.state, Some(&coder_spec))
                .is_ok()
                || std::env::var_os("ANTOS_AGENT_FAKE_SCRIPT").is_some();
            if has_provider {
                let _mutation_guard = WORKSPACE_MUTATION_LOCK
                    .lock()
                    .unwrap_or_else(|p| p.into_inner());
                let flow_ctx = Ctx {
                    workspace: ws.to_path_buf(),
                    current_project: None,
                    ..ctx.clone()
                };
                let state = ctx.state.clone();
                let fake = std::env::var_os("ANTOS_AGENT_FAKE_SCRIPT").is_some();
                let mut providers = move |role: antos_protocol::AgentRole| {
                    if fake {
                        return crate::agent::providers::resolve(&state, Some("fake"));
                    }
                    let key = match role {
                        antos_protocol::AgentRole::Architect => "architect",
                        antos_protocol::AgentRole::Coder => "coder",
                        antos_protocol::AgentRole::Auditor => "auditor",
                        _ => "qa",
                    };
                    let spec = llm_config.get_role_model(key);
                    crate::agent::providers::resolve_for_role(&state, Some(&spec), Some(key))
                };
                let mut handler = SocketHandler {
                    writer: &mut writer,
                    reader: &mut reader,
                };
                match crate::flow::FlowEngine::global().run_agent_pipeline(
                    &flow_ctx,
                    catalog,
                    &ticket_id,
                    &mut providers,
                    &mut handler,
                ) {
                    Ok(task) => send(&mut writer, &Event::FlowStatus(Some(task)))?,
                    Err(e) => send(&mut writer, &Event::Error(format!("{e:#}")))?,
                }
            } else {
                send(
                    &mut writer,
                    &Event::Note(
                        "sin proveedor de modelo configurado: se arranca la tarea SIMULADA (T33.1); \
                         configura uno con `antos llm use <proveedor>` y `antos agent config`"
                            .into(),
                    ),
                )?;
                match crate::flow::FlowEngine::global().start_task(ws, &ctx.state, &ticket_id) {
                    Ok(task) => send(&mut writer, &Event::FlowStatus(Some(task)))?,
                    Err(e) => send(&mut writer, &Event::Error(format!("{e:#}")))?,
                }
            }
        }
        Request::QueryFlow { ticket_id } => {
            let task = crate::flow::FlowEngine::global().get_task(&ticket_id);
            send(&mut writer, &Event::FlowStatus(task))?;
        }
        Request::ListFlows { .. } => {
            let tasks = crate::flow::FlowEngine::global().list_tasks();
            send(&mut writer, &Event::FlowList(tasks))?;
        }
        Request::ApproveFlow {
            ticket_id,
            decision,
        } => match crate::flow::FlowEngine::global().approve_task(&ticket_id, decision) {
            Ok(task) => {
                send(&mut writer, &Event::FlowStatus(Some(task)))?;
            }
            Err(e) => {
                send(&mut writer, &Event::Error(format!("{e:#}")))?;
            }
        },
        Request::QueryDiff {
            workspace_path,
            target,
            project_path,
        } => {
            let workspace = Path::new(&workspace_path);
            let antos_root = crate::git::detect_antos_root();

            // T17.2: resolve the directory to diff.
            // If project_path is provided, prefer it; otherwise fall back to workspace.
            let diff_dir_owned: std::path::PathBuf;
            let diff_dir: &Path = if let Some(ref pp) = project_path {
                let candidate = Path::new(pp.as_str());
                if candidate.is_absolute() {
                    diff_dir_owned = candidate.to_path_buf();
                } else {
                    diff_dir_owned = workspace.join(pp.as_str());
                }
                &diff_dir_owned
            } else {
                workspace
            };

            // Verify the diff_dir has a .git that is NOT the antOS OS repo (T17.1 ceiling).
            let has_git =
                crate::git::find_git_root_with_ceiling(diff_dir, antos_root.as_deref()).is_some();

            if !has_git {
                // No git repo in the project: return empty diff and an informational event.
                send(&mut writer, &Event::StructuredDiff(Vec::new()))?;
            } else {
                let ceiling_val = antos_root
                    .as_deref()
                    .and_then(|r| r.parent())
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();

                let mut cmd = std::process::Command::new("git");
                cmd.current_dir(diff_dir)
                    .args(["diff", target.as_deref().unwrap_or("HEAD")]);
                if !ceiling_val.is_empty() {
                    cmd.env("GIT_CEILING_DIRECTORIES", &ceiling_val);
                }
                let git_out = cmd.output();

                match git_out {
                    Ok(out) if out.status.success() => {
                        let diff_str = String::from_utf8_lossy(&out.stdout);
                        let files = crate::diff_view::DiffEngine::parse_unified_diff(&diff_str);
                        send(&mut writer, &Event::StructuredDiff(files))?;
                    }
                    _ => {
                        send(&mut writer, &Event::StructuredDiff(Vec::new()))?;
                    }
                }
            }
        }
        Request::ListNotifications { workspace_path } => {
            let ws = Path::new(&workspace_path);
            let notifs = crate::notification::NotificationEngine::global()
                .list(ws)
                .unwrap_or_default();
            send(&mut writer, &Event::NotificationList(notifs))?;
        }
        Request::HandleNotificationAction {
            workspace_path,
            notification_id,
            action,
        } => {
            let ws = Path::new(&workspace_path);
            match crate::notification::NotificationEngine::global().handle_action(
                ws,
                &notification_id,
                action,
            ) {
                Ok((success, message)) => {
                    send(
                        &mut writer,
                        &Event::NotificationResult {
                            id: notification_id,
                            success,
                            message,
                        },
                    )?;
                }
                Err(e) => {
                    send(
                        &mut writer,
                        &Event::NotificationResult {
                            id: notification_id,
                            success: false,
                            message: format!("{e:#}"),
                        },
                    )?;
                }
            }
        }
        Request::QueryMesh { workspace_path } => {
            let ws = Path::new(&workspace_path);
            match crate::mesh::MeshEngine::global().status(ws) {
                Ok(status) => {
                    send(&mut writer, &Event::MeshStatus(status))?;
                }
                Err(e) => {
                    send(&mut writer, &Event::Error(format!("{e:#}")))?;
                }
            }
        }
        Request::ConnectPeer {
            workspace_path,
            address,
        } => {
            let ws = Path::new(&workspace_path);
            match crate::mesh::MeshEngine::global().connect_peer(ws, &address) {
                Ok(peer) => {
                    send(
                        &mut writer,
                        &Event::PeerConnectionResult {
                            address: peer.address,
                            success: true,
                            message: format!(
                                "conectado con éxito al peer {} ({}ms)",
                                peer.id, peer.latency_ms
                            ),
                        },
                    )?;
                }
                Err(e) => {
                    send(
                        &mut writer,
                        &Event::PeerConnectionResult {
                            address,
                            success: false,
                            message: format!("{e:#}"),
                        },
                    )?;
                }
            }
        }
        Request::GeneratePairingToken { workspace_path } => {
            let ws = Path::new(&workspace_path);
            match crate::mesh::MeshEngine::global().generate_pairing_token(ws) {
                Ok(token) => {
                    send(&mut writer, &Event::PairingTokenGenerated(token))?;
                }
                Err(e) => {
                    send(&mut writer, &Event::Error(format!("{e:#}")))?;
                }
            }
        }
        Request::QuerySwarm { workspace_path } => {
            let ws = Path::new(&workspace_path);
            match crate::distributed::SwarmEngine::global().status(ws) {
                Ok(status) => {
                    send(&mut writer, &Event::SwarmStatus(status))?;
                }
                Err(e) => {
                    send(&mut writer, &Event::Error(format!("{e:#}")))?;
                }
            }
        }
        Request::DispatchRemoteRole {
            workspace_path,
            ticket_id,
            role,
            node_id,
        } => {
            let ws = Path::new(&workspace_path);
            match crate::distributed::SwarmEngine::global().dispatch_remote_role(
                ws,
                &ticket_id,
                role,
                node_id.as_deref(),
            ) {
                Ok(task) => {
                    send(
                        &mut writer,
                        &Event::SwarmDispatchResult {
                            ticket_id,
                            role,
                            assigned_node_id: task.assigned_node_id,
                            success: true,
                            message: format!(
                                "tarea {} despachada con éxito en nodo {}",
                                task.task_id, task.status
                            ),
                        },
                    )?;
                }
                Err(e) => {
                    send(
                        &mut writer,
                        &Event::SwarmDispatchResult {
                            ticket_id,
                            role,
                            assigned_node_id: node_id.unwrap_or_else(|| "unknown".into()),
                            success: false,
                            message: format!("{e:#}"),
                        },
                    )?;
                }
            }
        }
        Request::QueryVfs {
            workspace_path,
            virtual_path,
        } => {
            let ws = Path::new(&workspace_path);
            let engine = crate::vfs::VfsEngine::global();
            if virtual_path.ends_with('/')
                || virtual_path == "/antfs"
                || virtual_path == "/antfs/symbols"
                || virtual_path.starts_with("/antfs/symbols/")
                    && !virtual_path.split('/').skip(3).any(|p| !p.is_empty())
            {
                match engine.list_dir(ws, &virtual_path) {
                    Ok(entries) => {
                        send(
                            &mut writer,
                            &Event::VfsList {
                                virtual_path,
                                entries,
                            },
                        )?;
                    }
                    Err(e) => {
                        send(&mut writer, &Event::Error(format!("{e:#}")))?;
                    }
                }
            } else {
                match engine.read_path(ws, &virtual_path) {
                    Ok(content) => {
                        send(
                            &mut writer,
                            &Event::VfsContent {
                                virtual_path,
                                content,
                            },
                        )?;
                    }
                    Err(e) => {
                        send(&mut writer, &Event::Error(format!("{e:#}")))?;
                    }
                }
            }
        }
        Request::MountVfs {
            workspace_path,
            mount_point,
        } => {
            let ws = Path::new(&workspace_path);
            match crate::vfs::VfsEngine::global().mount(ws, mount_point.as_deref()) {
                Ok(path) => {
                    send(
                        &mut writer,
                        &Event::VfsResult {
                            action: "mount".into(),
                            success: true,
                            message: format!("montado en {}", path.display()),
                        },
                    )?;
                }
                Err(e) => {
                    send(
                        &mut writer,
                        &Event::VfsResult {
                            action: "mount".into(),
                            success: false,
                            message: format!("{e:#}"),
                        },
                    )?;
                }
            }
        }
        Request::UnmountVfs {
            workspace_path,
            mount_point,
        } => {
            let ws = Path::new(&workspace_path);
            match crate::vfs::VfsEngine::global().unmount(ws, mount_point.as_deref()) {
                Ok(_) => {
                    send(
                        &mut writer,
                        &Event::VfsResult {
                            action: "unmount".into(),
                            success: true,
                            message: "desmontado correctamente".into(),
                        },
                    )?;
                }
                Err(e) => {
                    send(
                        &mut writer,
                        &Event::VfsResult {
                            action: "unmount".into(),
                            success: false,
                            message: format!("{e:#}"),
                        },
                    )?;
                }
            }
        }
        Request::ValidateVfsWrite { file_path, content } => {
            let res =
                crate::vfs_guard::VfsGuardEngine::global().intercept_write(&file_path, &content)?;
            send(&mut writer, &Event::VfsValidationResult(res))?;
        }
        Request::QueryVfsGuard { .. } => {
            let status = crate::vfs_guard::VfsGuardEngine::global().status()?;
            send(&mut writer, &Event::VfsGuardStatus(status))?;
        }
        Request::QueryEbpfStatus { .. } => {
            let status = crate::ebpf::EbpfSentinelEngine::global().status()?;
            send(&mut writer, &Event::EbpfStatus(status))?;
        }
        Request::QueryEbpfAuditLog { limit, .. } => {
            let events = crate::ebpf::EbpfSentinelEngine::global().get_audit_log(limit);
            send(&mut writer, &Event::EbpfAuditLog(events))?;
        }
        Request::SimulateEbpfViolation {
            hook,
            target_resource,
            ..
        } => {
            let event = crate::ebpf::EbpfSentinelEngine::global()
                .simulate_violation(hook, &target_resource);
            send(
                &mut writer,
                &Event::EbpfResult {
                    action: format!("{:?}", hook),
                    success: true,
                    message: format!(
                        "evento {} generado con éxito (acción: {:?})",
                        event.id, event.action_taken
                    ),
                },
            )?;
        }
        Request::RunProfiler {
            workspace_path,
            command,
        } => {
            let ws = std::path::PathBuf::from(workspace_path);
            let report =
                crate::profiler::ProfilerEngine::global().run_and_profile(&ws, &command)?;
            send(&mut writer, &Event::ProfilerReport(report))?;
        }
        Request::QueryProfilerReports {
            workspace_path,
            limit,
        } => {
            let ws = std::path::PathBuf::from(workspace_path);
            let mut reports = crate::profiler::ProfilerEngine::global().load_reports(&ws);
            reports.truncate(limit);
            send(&mut writer, &Event::ProfilerReportList(reports))?;
        }
        Request::AnalyzeProfilerHotspots { workspace_path } => {
            let ws = std::path::PathBuf::from(workspace_path);
            let (hotspots, suggestions) =
                crate::profiler::ProfilerEngine::global().analyze_aggregate(&ws);
            send(
                &mut writer,
                &Event::ProfilerAnalysis {
                    hotspots,
                    suggestions,
                },
            )?;
        }
        Request::QueryLspStatus { workspace_path } => {
            let ws = std::path::PathBuf::from(workspace_path);
            let status = crate::lsp::LspServer::global().get_status(&ws);
            send(&mut writer, &Event::LspStatus(status))?;
        }
        Request::GetLspConfig {
            editor,
            workspace_path,
        } => {
            let ws = std::path::PathBuf::from(workspace_path);
            let (config_content, target_file) =
                crate::lsp::LspServer::global().generate_config(editor, &ws);
            send(
                &mut writer,
                &Event::LspConfiguration {
                    editor,
                    config_content,
                    target_file,
                },
            )?;
        }
        Request::StartCollabSession {
            file_path,
            ticket_id,
            workspace_path,
        } => {
            let ws = std::path::PathBuf::from(workspace_path);
            let status =
                crate::collab::CollabEngine::global().start_session(&ws, &file_path, ticket_id)?;
            send(&mut writer, &Event::CollabSessionStatus(status))?;
        }
        Request::QueryCollabStatus { session_id, .. } => {
            if let Some(status) = crate::collab::CollabEngine::global().get_session(&session_id) {
                send(&mut writer, &Event::CollabSessionStatus(status))?;
            } else {
                send(
                    &mut writer,
                    &Event::Error(format!("sesión «{session_id}» no encontrada")),
                )?;
            }
        }
        Request::StartDapSession { command, .. } => {
            let mut dap = crate::collab::DapServer::new("dap-sess-001".into(), command);
            dap.add_breakpoint("src/main.rs", 1);
            send(&mut writer, &Event::DapSessionStatus(dap.to_status()))?;
        }
        Request::QueryDapStatus { session_id, .. } => {
            let dap = crate::collab::DapServer::new(session_id, "cargo test".into());
            send(&mut writer, &Event::DapSessionStatus(dap.to_status()))?;
        }
        Request::QueryDesktopStatus => {
            let status = crate::desktop::DesktopManager::get_status();
            send(&mut writer, &Event::DesktopStatus(status))?;
        }
        Request::ListDesktopHotkeys => {
            let hotkeys = crate::desktop::DesktopManager::get_hotkeys();
            send(&mut writer, &Event::DesktopHotkeysList(hotkeys))?;
        }
        Request::StartDesktopSession { .. } => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let _ = crate::desktop::DesktopManager::sync_configuration(&cwd);
            let status = crate::desktop::DesktopManager::get_status();
            send(&mut writer, &Event::DesktopStatus(status))?;
        }
        Request::QueryBarraTelemetry => {
            let telemetry = crate::barra::BarraManager::global().get_telemetry();
            send(&mut writer, &Event::BarraTelemetryStatus(telemetry))?;
        }
        Request::EmitBarraAlert(alert) => {
            let res = crate::barra::BarraManager::global().emit_alert(alert.clone());
            if res.is_ok() {
                send(&mut writer, &Event::BarraAlert(alert))?;
            } else {
                send(
                    &mut writer,
                    &Event::Error("Error al registrar alerta en la barra".into()),
                )?;
            }
        }
        Request::QueryBootStatus => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let status = crate::boot::BootEngine::global().status(&cwd);
            send(&mut writer, &Event::BootStatus(status))?;
        }
        Request::RunBootPipeline { action, .. } => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let engine = crate::boot::BootEngine::global();
            match action.as_str() {
                "test" => match engine.test_boot(&cwd) {
                    Ok(out) => send(
                        &mut writer,
                        &Event::BootResult {
                            action,
                            output: out,
                            success: true,
                        },
                    )?,
                    Err(e) => send(
                        &mut writer,
                        &Event::BootResult {
                            action,
                            output: e.to_string(),
                            success: false,
                        },
                    )?,
                },
                "build" => match engine.build(&cwd) {
                    Ok(p) => send(
                        &mut writer,
                        &Event::BootResult {
                            action,
                            output: format!("Imagen de disco generada: {}", p.display()),
                            success: true,
                        },
                    )?,
                    Err(e) => send(
                        &mut writer,
                        &Event::BootResult {
                            action,
                            output: e.to_string(),
                            success: false,
                        },
                    )?,
                },
                _ => {
                    let st = engine.status(&cwd);
                    send(&mut writer, &Event::BootStatus(st))?;
                }
            }
        }
        Request::ListPlugins => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let plugins_dir = crate::wasm::PluginManager::get_plugins_dir(&cwd);
            let summaries = crate::wasm::PluginManager::list_plugins(&plugins_dir);
            send(&mut writer, &Event::PluginList(summaries))?;
        }
        Request::RunPlugin {
            plugin_name,
            action,
            params,
        } => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let plugins_dir = crate::wasm::PluginManager::get_plugins_dir(&cwd);
            let res = crate::wasm::PluginManager::run_plugin(
                &plugins_dir,
                &plugin_name,
                &action,
                &params,
            );
            send(&mut writer, &Event::PluginResult(res))?;
        }
        Request::InstallPlugin { source_path } => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let plugins_dir = crate::wasm::PluginManager::get_plugins_dir(&cwd);
            let src = std::path::PathBuf::from(&source_path);
            match crate::wasm::PluginManager::install_plugin(&plugins_dir, &src) {
                Ok(summary) => send(&mut writer, &Event::PluginList(vec![summary]))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::CaptureScreen { target, save_path } => {
            let p_opt = save_path.map(std::path::PathBuf::from);
            match crate::vision::VisionEngine::global()
                .capture_screen(target.as_deref(), p_opt.as_deref())
            {
                Ok(res) => send(&mut writer, &Event::ScreenshotResult(res))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::InspectVisualQa { target, criteria } => {
            match crate::vision::VisionEngine::global().inspect_visual(&target, &criteria, None) {
                Ok(rep) => send(&mut writer, &Event::VisualQAReport(rep))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::ListDisks => match crate::installer::DiskManager::list_disks() {
            Ok(disks) => send(&mut writer, &Event::DiskList(disks))?,
            Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
        },
        Request::InspectDisk { device } => {
            match crate::installer::DiskManager::inspect_disk(&device) {
                Ok(opt) => send(&mut writer, &Event::DiskDetail(opt))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::PartitionDisk {
            device,
            clean_install,
            dry_run,
        } => match crate::installer::DiskManager::plan_partitioning(&device, clean_install) {
            Ok(plan) => {
                if !dry_run {
                    let _ =
                        crate::installer::DiskManager::apply_partitioning(&device, &plan, false);
                }
                send(&mut writer, &Event::PartitionPlan(plan))?;
            }
            Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
        },
        Request::InstallSystem(config) => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            match crate::installer::DeployEngine::deploy_system(&config, &cwd) {
                Ok(report) => send(&mut writer, &Event::InstallReport(report))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::ProbeOperatingSystems { esp_mount } => {
            let esp = esp_mount
                .as_deref()
                .map(std::path::Path::new)
                .unwrap_or_else(|| std::path::Path::new("/boot/efi"));
            match crate::installer::BootloaderEngine::probe_operating_systems(esp) {
                Ok(entries) => send(&mut writer, &Event::DetectedOperatingSystems(entries))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::InstallBootloader(config) => {
            match crate::installer::BootloaderEngine::install_bootloader(&config) {
                Ok(report) => send(&mut writer, &Event::BootloaderReport(report))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::SpawnMicrovm(config) => {
            match crate::vm::MicrovmManager::spawn_vm(&ctx.state, &config) {
                Ok(instance) => send(&mut writer, &Event::MicrovmList(vec![instance]))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::ExecMicrovm { vm_id, command } => {
            match crate::vm::MicrovmManager::exec_vm(&ctx.state, &vm_id, &command) {
                Ok(result) => send(&mut writer, &Event::MicrovmResult(result))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::DestroyMicrovm { vm_id } => {
            match crate::vm::MicrovmManager::kill_vm(&ctx.state, &vm_id) {
                Ok(_) => send(
                    &mut writer,
                    &Event::Note(format!("MicroVM «{vm_id}» destruida")),
                )?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::ListMicrovms => match crate::vm::MicrovmManager::list_vms(&ctx.state) {
            Ok(list) => send(&mut writer, &Event::MicrovmList(list))?,
            Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
        },
        Request::QueryMicrovmStatus => match crate::vm::MicrovmManager::get_status(&ctx.state) {
            Ok(status) => send(&mut writer, &Event::MicrovmStatus(status))?,
            Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
        },
        Request::InstallPackage {
            recipe_path_or_name,
            dry_run,
        } => match crate::pkg::PackageEngine::install(&ctx.state, &recipe_path_or_name, dry_run) {
            Ok(rep) => send(&mut writer, &Event::PackageInstallReport(rep))?,
            Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
        },
        Request::RemovePackage { package_name } => {
            match crate::pkg::PackageEngine::remove(&ctx.state, &package_name) {
                Ok(rep) => send(&mut writer, &Event::PackageInstallReport(rep))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::ListPackages => match crate::pkg::PackageEngine::list(&ctx.state) {
            Ok(list) => send(&mut writer, &Event::PackageList(list))?,
            Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
        },
        Request::RollbackPackage { target_generation } => {
            match crate::pkg::PackageEngine::rollback(&ctx.state, target_generation) {
                Ok(rep) => send(&mut writer, &Event::PackageInstallReport(rep))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::VerifyPackages => match crate::pkg::PackageEngine::verify(&ctx.state) {
            Ok((all_valid, verified_packages, details)) => send(
                &mut writer,
                &Event::PackageVerificationResult {
                    all_valid,
                    verified_packages,
                    details,
                },
            )?,
            Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
        },
        Request::QueryPackageStoreStatus => match crate::pkg::PackageEngine::status(&ctx.state) {
            Ok(st) => send(&mut writer, &Event::PackageStoreStatus(st))?,
            Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
        },
        Request::ListDesktopApps => {
            match crate::pkg::PackageEngine::list_desktop_apps(&ctx.state) {
                Ok(apps) => send(&mut writer, &Event::DesktopAppList(apps))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::ValidateDesktopEntry { content } => {
            let report = crate::pkg::PackageEngine::validate_desktop_entry(&content);
            send(&mut writer, &Event::DesktopValidationReport(report))?;
        }
        Request::SearchPackages { query } => {
            match crate::pkg::PackageEngine::search_catalog(&query) {
                Ok(results) => send(&mut writer, &Event::PackageSearchResults(results))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::GetPackageInfo { recipe } => {
            match crate::pkg::PackageEngine::resolve_manifest(&recipe) {
                Ok(info) => send(&mut writer, &Event::PackageInfo(info))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::ListApps { source } => {
            match crate::apps::AppEngine::list_apps(&ctx.state, source) {
                Ok(apps) => send(&mut writer, &Event::AppList(apps))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::SearchApps { query } => {
            match crate::apps::AppEngine::search_apps(&ctx.state, &query) {
                Ok(results) => send(&mut writer, &Event::AppSearchResults(results))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::InstallApp { id, source } => {
            match crate::apps::AppEngine::install_app(&ctx.state, &id, source, |_| {}) {
                Ok(res) => send(&mut writer, &Event::AppActionResult(res))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::UninstallApp { id } => {
            match crate::apps::AppEngine::uninstall_app(&ctx.state, &id) {
                Ok(res) => send(&mut writer, &Event::AppActionResult(res))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::LaunchApp {
            id,
            workspace,
            args,
        } => {
            let ws = workspace
                .as_deref()
                .map(std::path::Path::new)
                .unwrap_or(&ctx.workspace);
            match crate::apps::AppEngine::launch_app(&ctx.state, &id, Some(ws), &args) {
                Ok(res) => send(&mut writer, &Event::AppLaunchResult(res))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::StartAutopilot(config) => {
            match crate::autopilot::AutopilotEngine::start(&ctx.state, &ctx.workspace, config) {
                Ok(st) => send(&mut writer, &Event::AutopilotStatus(st))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::StopAutopilot => {
            match crate::autopilot::AutopilotEngine::stop(&ctx.state, &ctx.workspace) {
                Ok(st) => send(&mut writer, &Event::AutopilotStatus(st))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::GetAutopilotStatus => {
            match crate::autopilot::AutopilotEngine::status(&ctx.state, &ctx.workspace) {
                Ok(st) => send(&mut writer, &Event::AutopilotStatus(st))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::ListAutopilotIncidents => {
            match crate::autopilot::AutopilotEngine::list_incidents(&ctx.state) {
                Ok(list) => send(&mut writer, &Event::AutopilotIncidentsList(list))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::ScanAutopilot => {
            match crate::autopilot::AutopilotEngine::scan_workspace(&ctx.state, &ctx.workspace) {
                Ok(list) => send(&mut writer, &Event::AutopilotIncidentsList(list))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::ResolveAutopilotIncident {
            incident_id,
            approve_and_merge,
        } => {
            match crate::autopilot::AutopilotEngine::resolve_incident(
                &ctx.state,
                &ctx.workspace,
                &incident_id,
                approve_and_merge,
            ) {
                Ok(inc) => send(&mut writer, &Event::AutopilotAlert(inc))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::StartWebConsole(config) => {
            match crate::web::WebEngine::start(&ctx.state, &ctx.workspace, config) {
                Ok(st) => send(&mut writer, &Event::WebConsoleStatus(st))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::StopWebConsole => match crate::web::WebEngine::stop(&ctx.state) {
            Ok(st) => send(&mut writer, &Event::WebConsoleStatus(st))?,
            Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
        },
        Request::GetWebConsoleStatus => match crate::web::WebEngine::status(&ctx.state) {
            Ok(st) => send(&mut writer, &Event::WebConsoleStatus(st))?,
            Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
        },
        Request::GenerateWebToken {
            client_label,
            ttl_secs,
        } => match crate::web::WebEngine::generate_token(&ctx.state, client_label, ttl_secs) {
            Ok(session) => send(&mut writer, &Event::WebTokenGenerated(session))?,
            Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
        },
        Request::GetDevWorkspaceStatus { project } => {
            let status =
                crate::dev_tui::DevWorkspaceManager::get_status(project.as_deref(), &ctx.workspace);
            send(&mut writer, &Event::DevWorkspaceStatus(status))?;
        }
        Request::ReproduceBug {
            error_text,
            target_file,
        } => {
            match crate::reproduce::TddEngine::run_reproduce_pipeline(
                &error_text,
                target_file.as_deref(),
                &ctx.workspace,
                &ctx.state,
            ) {
                Ok(report) => send(&mut writer, &Event::TddReport(report))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::GenerateTest { target } => {
            match crate::reproduce::TddEngine::generate_tests_for_target(
                &target,
                "unit",
                3,
                &ctx.workspace,
            ) {
                Ok(report) => send(&mut writer, &Event::TddReport(report))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::RunCi { stage, fast } => {
            match crate::ci::CiEngine::run_pipeline(
                &ctx.workspace,
                &ctx.state,
                stage.as_deref(),
                fast,
            ) {
                Ok(report) => send(&mut writer, &Event::CiReport(report))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::GetCiStatus => match crate::ci::CiEngine::get_last_report(&ctx.state) {
            Ok(Some(report)) => send(&mut writer, &Event::CiReport(report))?,
            Ok(None) => send(
                &mut writer,
                &Event::Note("No hay reportes de CI previos registrados".into()),
            )?,
            Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
        },
        Request::ManageGitHooks { action } => {
            let res = match action.as_str() {
                "install" => crate::ci::CiEngine::install_git_hooks(&ctx.workspace),
                "uninstall" => crate::ci::CiEngine::uninstall_git_hooks(&ctx.workspace),
                _ => crate::ci::CiEngine::query_git_hooks_status(&ctx.workspace),
            };
            match res {
                Ok(status) => send(&mut writer, &Event::GitHooksStatus(status))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::CreateSnapshot { label, author } => {
            match crate::time_machine::TimeMachineEngine::create_snapshot(
                &ctx.workspace,
                &ctx.state,
                label.as_deref(),
                author.as_deref(),
            ) {
                Ok(meta) => send(&mut writer, &Event::SnapshotCreated(meta))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::ListSnapshots => {
            match crate::time_machine::TimeMachineEngine::list_snapshots(&ctx.state) {
                Ok(list) => send(&mut writer, &Event::SnapshotsList(list))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::RestoreSnapshot {
            id_or_label,
            create_rescue,
        } => {
            match crate::time_machine::TimeMachineEngine::restore_snapshot(
                &ctx.workspace,
                &ctx.state,
                &id_or_label,
                create_rescue,
            ) {
                Ok(res) => send(&mut writer, &Event::SnapshotRestored(res))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::DeleteSnapshot { id } => {
            match crate::time_machine::TimeMachineEngine::delete_snapshot(&ctx.state, &id) {
                Ok(deleted_id) => send(&mut writer, &Event::SnapshotDeleted { id: deleted_id })?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::RunBenchmark { target } => {
            match crate::bench::BenchEngine::run_benchmark(
                &ctx.workspace,
                &ctx.state,
                target.as_deref(),
            ) {
                Ok(report) => send(&mut writer, &Event::BenchmarkReport(report))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::CompareBenchmark {
            against_branch,
            threshold_pct,
        } => {
            let threshold = threshold_pct.map(|t| t as f64);
            match crate::bench::BenchEngine::compare_benchmark(
                &ctx.workspace,
                &ctx.state,
                against_branch.as_deref(),
                threshold,
            ) {
                Ok(diff) => send(&mut writer, &Event::BenchmarkDiff(diff))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::GetBenchmarkHistory => {
            let list = crate::bench::BenchEngine::load_history(&ctx.state);
            send(&mut writer, &Event::BenchmarkHistory(list))?;
        }
        Request::ListRemoteIssues => {
            match crate::forge::ForgeEngine::list_issues(&ctx.workspace, &ctx.state) {
                Ok(issues) => send(&mut writer, &Event::RemoteIssuesList(issues))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::ImportRemoteIssue { id_or_url } => {
            match crate::forge::ForgeEngine::import_issue(&ctx.workspace, &ctx.state, &id_or_url) {
                Ok((ticket_id, path, title)) => send(
                    &mut writer,
                    &Event::RemoteIssueImported {
                        ticket_id,
                        path: path.display().to_string(),
                        title,
                    },
                )?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::CreatePullRequest {
            title,
            base_branch,
            draft,
        } => {
            match crate::forge::ForgeEngine::create_pull_request(
                &ctx.workspace,
                &ctx.state,
                title.as_deref(),
                base_branch.as_deref(),
                draft,
            ) {
                Ok(pr) => send(&mut writer, &Event::PullRequestCreated(pr))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::GetPullRequestStatus { number } => {
            match crate::forge::ForgeEngine::get_pull_request_status(&ctx.state, number) {
                Ok(status) => send(&mut writer, &Event::PullRequestStatus(status))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::GenerateArchDiagram { kind } => {
            let k = match kind.as_deref() {
                Some("components") | Some("c4") | Some("comp") => {
                    antos_protocol::ArchDiagramKind::Components
                }
                Some("flow") | Some("ipc") => antos_protocol::ArchDiagramKind::IpcFlow,
                Some("antflow") | Some("state") => antos_protocol::ArchDiagramKind::AntFlow,
                _ => antos_protocol::ArchDiagramKind::Full,
            };
            let report = crate::doc_arch::DocArchEngine::generate_diagram(&ctx.workspace, k);
            send(&mut writer, &Event::ArchDiagram(report))?;
        }
        Request::SyncArchDocs { target_file } => {
            match crate::doc_arch::DocArchEngine::sync_docs(&ctx.workspace, target_file.as_deref())
            {
                Ok(report) => send(&mut writer, &Event::DocSync(report))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
        Request::CheckArchDocs { target_file } => {
            match crate::doc_arch::DocArchEngine::check_docs(&ctx.workspace, target_file.as_deref())
            {
                Ok(report) => send(&mut writer, &Event::DocSync(report))?,
                Err(e) => send(&mut writer, &Event::Error(e.to_string()))?,
            }
        }
    }
    Ok(())
}

// -------------------------------------------------------------- lado cliente

pub fn daemon_is_running(ctx: &Ctx) -> bool {
    let path = socket_path(ctx);
    path.exists() && UnixStream::connect(&path).is_ok()
}

/// `antos ping` (T36.2): la comprobación mínima de que el demonio está
/// vivo y habla el protocolo de la barra — conecta al socket, envía
/// `QueryGitStatus` sobre el workspace (lo mismo que hace `antos-barra` al
/// arrancar) y devuelve el nombre del primer evento recibido
/// (`GitStatus` o `NotGitRepo`). Lo usa el smoke de instalación desde el
/// sistema instalado, sin `socat` ni `python`.
pub fn ping(ctx: &Ctx) -> Result<String> {
    let path = socket_path(ctx);
    let stream = UnixStream::connect(&path)
        .with_context(|| format!("no hay demonio escuchando en {}", path.display()))?;
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    send(
        &mut writer,
        &Request::QueryGitStatus {
            workspace_path: ctx.workspace.to_string_lossy().into_owned(),
        },
    )?;
    let _ = writer.shutdown(std::net::Shutdown::Write);
    let mut first = String::new();
    let read = reader
        .read_line(&mut first)
        .context("leyendo la respuesta del demonio")?;
    if read == 0 {
        bail!("el demonio cerró la conexión sin responder a QueryGitStatus");
    }
    let event: Event = serde_json::from_str(first.trim())
        .with_context(|| format!("respuesta que la barra no entendería: {}", first.trim()))?;
    Ok(match event {
        Event::GitStatus(_) => "GitStatus".to_string(),
        Event::NotGitRepo => "NotGitRepo".to_string(),
        other => format!("{other:?}").chars().take(40).collect(),
    })
}

/// Sends an intent to the daemon and renders the response.
///
/// Note that rendering uses the SAME `Terminal` as local mode:
/// there is no second way to show a plan, so they cannot diverge.
/// Cliente de un run de agente (T33.2): `antos agent do …`. Misma mecánica
/// que `remote_intent`: eventos hasta `AgentDone`, aprobaciones por terminal.
pub fn remote_agent_run(
    socket: &Path,
    goal: &str,
    provider: Option<&str>,
    toolset: Option<Vec<String>>,
    budget: Option<antos_protocol::AgentBudget>,
    dry_run: bool,
    assume_yes: bool,
) -> Result<Option<antos_protocol::AgentReport>> {
    let stream = UnixStream::connect(socket)
        .with_context(|| format!("could not connect to daemon at {}", socket.display()))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;

    send(
        &mut writer,
        &Request::AgentRun {
            goal: goal.to_string(),
            provider: provider.map(str::to_string),
            toolset,
            budget,
            dry_run,
        },
    )?;

    let mut term = terminal::Terminal::new(assume_yes);
    let mut report = None;
    while let Received::Message(event) = receive::<Event>(&mut reader)? {
        match event {
            Event::Note(t) => term.on_note(&t)?,
            Event::Proposal(p) => {
                let decision = term.on_proposal(&p)?;
                send(&mut writer, &Request::Approval(decision))?;
            }
            Event::AgentStep(step) => term.on_note(&format_agent_step(&step))?,
            Event::AgentDone(r) => {
                term.on_note(&format_agent_report(&r))?;
                report = Some(*r);
                break;
            }
            Event::Error(e) => bail!("{e}"),
            other => term.on_note(&format!("{other:?}"))?,
        }
    }
    Ok(report)
}

pub fn format_agent_step(step: &antos_protocol::AgentStepEvent) -> String {
    use antos_protocol::AgentStepOutcome as O;
    let mark = match step.outcome {
        O::Executed => "✓",
        O::Rejected => "✗ rechazada",
        O::Declined => "✗ no aprobada",
        O::Failed => "✗ falló",
        O::Finished => "■ finalizar",
    };
    let preview = if step.output_preview.is_empty() {
        String::new()
    } else {
        format!(" → {}", step.output_preview)
    };
    format!(
        "[agente · paso {}] {} {} {}{} ({} tokens)",
        step.step, mark, step.tool, step.args_summary, preview, step.tokens_used
    )
}

pub fn format_agent_report(r: &antos_protocol::AgentReport) -> String {
    let mut out = format!(
        "[agente · fin] {:?} · {} pasos · {} tokens · {} s · {}:{}\n  {}",
        r.stop_reason, r.steps, r.tokens_used, r.seconds, r.provider, r.model, r.summary
    );
    if !r.files_written.is_empty() {
        out.push_str(&format!("\n  escritos: {}", r.files_written.join(", ")));
    }
    if let Some(s) = &r.snapshot_id {
        out.push_str(&format!(
            "\n  instantánea {s} · `antos undo` deshace el run entero"
        ));
    }
    if let Some(e) = &r.error {
        out.push_str(&format!("\n  error: {e}"));
    }
    if let Some(ctx) = r.context_window {
        out.push_str(&format!("\n  contexto: {ctx} tokens"));
        if let Some(n) = &r.context_note {
            out.push_str(&format!(" ({n})"));
        }
    }
    out
}

pub fn remote_intent(
    socket: &Path,
    text: &str,
    planner: Option<&str>,
    dry_run: bool,
    assume_yes: bool,
) -> Result<()> {
    let stream = UnixStream::connect(socket)
        .with_context(|| format!("could not connect to daemon at {}", socket.display()))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;

    send(
        &mut writer,
        &Request::Intent {
            text: text.to_string(),
            planner: planner.map(str::to_string),
            dry_run,
        },
    )?;

    let mut term = terminal::Terminal::new(assume_yes);

    // A `TooLarge` event would mean the daemon itself sent something
    // implausible; treated the same as a clean end of stream rather than
    // failing the whole session over it (T31.8).
    while let Received::Message(event) = receive::<Event>(&mut reader)? {
        match event {
            Event::Start { intent, planner } => term.on_start(&intent, &planner)?,
            Event::Note(t) => term.on_note(&t)?,
            Event::Proposal(p) => {
                let decision = term.on_proposal(&p)?;
                send(&mut writer, &Request::Approval(decision))?;
            }
            Event::Output(t) => term.on_output(&t)?,
            Event::Result(r) => term.on_result(&r)?,
            Event::AgentStep(step) => term.on_note(&format_agent_step(&step))?,
            Event::AgentDone(report) => term.on_note(&format_agent_report(&report))?,
            Event::GitStatus(status) => {
                term.on_note(&format!("git branch: {:?}", status.branch))?;
            }
            Event::NotGitRepo => {
                term.on_note("no es un repositorio Git")?;
            }
            Event::TicketList(tickets) => {
                term.on_note(&format!("tickets disponibles: {}", tickets.len()))?;
            }
            Event::TicketDetail(detalle) => {
                if let Some(t) = detalle {
                    term.on_note(&format!("ticket {}: {}", t.id, t.title))?;
                }
            }
            Event::PortsStatus(puertos) => {
                term.on_note(&format!("puertos en escucha: {}", puertos.len()))?;
            }
            Event::FlowStatus(task) => {
                if let Some(t) = task {
                    term.on_note(&format!("antFlow {}: {}", t.ticket_id, t.state.label()))?;
                }
            }
            Event::FlowList(tasks) => {
                term.on_note(&format!("tareas antFlow activas: {}", tasks.len()))?;
            }
            Event::FlowTransition {
                ticket_id,
                new_state,
                detail,
                ..
            } => {
                term.on_note(&format!(
                    "[antFlow {ticket_id}] ➔ {}: {detail}",
                    new_state.label()
                ))?;
            }
            Event::StructuredDiff(files) => {
                term.on_note(&format!("archivos con diff: {}", files.len()))?;
            }
            Event::NotificationList(notifs) => {
                term.on_note(&format!("notificaciones recibidas: {}", notifs.len()))?;
            }
            Event::NotificationResult {
                message, success, ..
            } => {
                if success {
                    term.on_note(&format!("✓ {message}"))?;
                } else {
                    term.on_note(&format!("✗ {message}"))?;
                }
            }
            Event::MeshStatus(status) => {
                term.on_note(&format!(
                    "antMesh local: {} (peers: {})",
                    status.local_node.id,
                    status.peers.len()
                ))?;
            }
            Event::PairingTokenGenerated(tok) => {
                term.on_note(&format!("token de emparejamiento generado: {}", tok.token))?;
            }
            Event::PeerConnectionResult {
                address,
                success,
                message,
            } => {
                if success {
                    term.on_note(&format!("✓ peer {address}: {message}"))?;
                } else {
                    term.on_note(&format!("✗ peer {address}: {message}"))?;
                }
            }
            Event::SwarmStatus(status) => {
                term.on_note(&format!(
                    "antOS Swarm: {} nodos ({} tareas activas)",
                    status.nodes.len(),
                    status.total_tasks
                ))?;
            }
            Event::SwarmDispatchResult {
                ticket_id,
                role,
                assigned_node_id,
                success,
                message,
            } => {
                if success {
                    term.on_note(&format!(
                        "✓ Swarm [{ticket_id}] rol {:?} ➔ {assigned_node_id}: {message}",
                        role
                    ))?;
                } else {
                    term.on_note(&format!(
                        "✗ Swarm [{ticket_id}] rol {:?} ➔ {assigned_node_id}: {message}",
                        role
                    ))?;
                }
            }
            Event::VfsList {
                virtual_path,
                entries,
            } => {
                term.on_note(&format!(
                    "VFS {virtual_path}: {} entradas encontradas",
                    entries.len()
                ))?;
            }
            Event::VfsContent {
                virtual_path,
                content,
            } => {
                term.on_output(&format!("{virtual_path}:\n{content}"))?;
            }
            Event::VfsResult {
                action,
                success,
                message,
            } => {
                if success {
                    term.on_note(&format!("✓ VFS {action}: {message}"))?;
                } else {
                    term.on_note(&format!("✗ VFS {action}: {message}"))?;
                }
            }
            Event::VfsValidationResult(res) => {
                if res.is_valid {
                    term.on_note(&format!(
                        "✓ VFS Guard: «{}» es sintácticamente válido ({} líneas)",
                        res.file_path, res.line_count
                    ))?;
                } else {
                    term.on_note(&format!(
                        "✗ VFS Guard: «{}» tiene {} errores sintácticos",
                        res.file_path,
                        res.errors.len()
                    ))?;
                }
            }
            Event::VfsGuardStatus(status) => {
                term.on_note(&format!(
                    "VFS Guard: {} escrituras interceptadas ({} rechazadas)",
                    status.total_intercepted, status.total_rejected
                ))?;
            }
            Event::EbpfStatus(status) => {
                let lsm_badge = if status.lsm_enabled {
                    "Kernel LSM Activo"
                } else {
                    "Emulación Espacio Usuario"
                };
                term.on_note(&format!(
                    "eBPF Sentinel [{lsm_badge}]: {} sondas, {} eventos, {} bloqueos",
                    status.active_probes.len(),
                    status.total_events_captured,
                    status.total_violations_blocked
                ))?;
            }
            Event::EbpfAuditLog(events) => {
                term.on_note(&format!(
                    "eBPF Audit: {} eventos capturados en el ring buffer",
                    events.len()
                ))?;
            }
            Event::EbpfResult {
                action,
                success,
                message,
            } => {
                if success {
                    term.on_note(&format!("✓ eBPF {action}: {message}"))?;
                } else {
                    term.on_note(&format!("✗ eBPF {action}: {message}"))?;
                }
            }
            Event::ProfilerReport(report) => {
                let peak_mb = report.peak_memory_bytes as f64 / (1024.0 * 1024.0);
                term.on_note(&format!(
                    "✓ Profiler: «{}» en {} ms (Memoria pico: {:.2} MB RSS)",
                    report.command, report.duration_ms, peak_mb
                ))?;
            }
            Event::ProfilerReportList(reports) => {
                term.on_note(&format!(
                    "Profiler: {} reportes históricos disponibles",
                    reports.len()
                ))?;
            }
            Event::ProfilerAnalysis {
                hotspots,
                suggestions,
            } => {
                term.on_note(&format!(
                    "Profiler: {} hotspots y {} recomendaciones formuladas",
                    hotspots.len(),
                    suggestions.len()
                ))?;
            }
            Event::LspStatus(status) => {
                let state_str = if status.running {
                    "Activo"
                } else {
                    "En espera"
                };
                term.on_note(&format!(
                    "LSP: {state_str} ({}) con {} símbolos indexados",
                    status.transport, status.indexed_symbols_count
                ))?;
            }
            Event::LspConfiguration {
                editor,
                target_file,
                ..
            } => {
                term.on_note(&format!(
                    "LSP: configuración generada para {:?} ({target_file})",
                    editor
                ))?;
            }
            Event::CollabSessionStatus(status) => {
                term.on_note(&format!(
                    "Pair: sesión {} en {} (colaboradores: {})",
                    status.session_id,
                    status.file_path,
                    status.collaborators.len()
                ))?;
            }
            Event::DapSessionStatus(status) => {
                term.on_note(&format!(
                    "DAP: sesión {} en estado {} para «{}»",
                    status.session_id, status.state, status.target_command
                ))?;
            }
            Event::CollabResult {
                action,
                success,
                message,
            } => {
                if success {
                    term.on_note(&format!("✓ Pair {action}: {message}"))?;
                } else {
                    term.on_note(&format!("✗ Pair {action}: {message}"))?;
                }
            }
            Event::DapResult {
                action,
                success,
                message,
            } => {
                if success {
                    term.on_note(&format!("✓ DAP {action}: {message}"))?;
                } else {
                    term.on_note(&format!("✗ DAP {action}: {message}"))?;
                }
            }
            Event::DesktopStatus(status) => {
                let state_str = if status.running {
                    "Activa"
                } else {
                    "Detenida / Headless"
                };
                term.on_note(&format!(
                    "Escritorio antOS [{state_str}]: Compositor {} (Display: {:?})",
                    status.compositor_name, status.wayland_display
                ))?;
            }
            Event::DesktopHotkeysList(keys) => {
                term.on_note(&format!(
                    "Escritorio antOS: {} atajos globales registrados",
                    keys.len()
                ))?;
            }
            Event::BarraTelemetryStatus(t) => {
                let mb = t.profiler_rss_bytes as f64 / (1024.0 * 1024.0);
                term.on_note(&format!("Barra antOS: eBPF: {}, Profiler: {:.1} MB ({:.1}%), Mesh: {} nodos, Notificaciones: {}",
                    if t.ebpf_lsm_active { "LSM Activo" } else { "Auditoría" },
                    mb, t.profiler_cpu_percent, t.mesh_peers_count, t.active_notifications_count
                ))?;
            }
            Event::BarraAlert(alert) => {
                let urg = if alert.urgent { "URGENTE" } else { "INFO" };
                term.on_note(&format!(
                    "Alerta en Barra [{urg} - {}]: {}",
                    alert.category, alert.message
                ))?;
            }
            Event::BootStatus(st) => {
                let kb = st.kernel_elf_size_bytes / 1024;
                let mb = st.bios_image_size_bytes / (1024 * 1024);
                term.on_note(&format!(
                    "antOS Boot: Kernel ELF: {} KiB, BIOS IMG: {} MB, QEMU: {}",
                    kb,
                    mb,
                    if st.qemu_installed {
                        "instalado"
                    } else {
                        "no disponible"
                    }
                ))?;
            }
            Event::BootResult {
                action,
                output,
                success,
            } => {
                let status = if success { "OK" } else { "ERROR" };
                term.on_note(&format!("antOS Boot [{action} - {status}]: {output}"))?;
            }
            Event::PluginList(list) => {
                if list.is_empty() {
                    term.on_note("No hay plugins WASM instalados en antOS.")?;
                } else {
                    term.on_note(&format!("antOS Plugins ({} activos):", list.len()))?;
                    for p in list {
                        term.on_note(&format!(
                            "  • {} v{} - {} (acciones: {})",
                            p.name,
                            p.version,
                            p.description,
                            p.capabilities.join(", ")
                        ))?;
                    }
                }
            }
            Event::PluginResult(res) => {
                if res.success {
                    term.on_note(&format!(
                        "✓ Plugin [{}:{}] ejecutado con éxito ({} ciclos, {} KiB memoria):\n{}",
                        res.plugin,
                        res.action,
                        res.fuel_consumed,
                        res.memory_allocated_bytes / 1024,
                        res.output
                    ))?;
                } else {
                    let err = res.error.unwrap_or_else(|| "Error desconocido".into());
                    bail!("Fallo en plugin [{}:{}]: {}", res.plugin, res.action, err);
                }
            }
            Event::ScreenshotResult(cap) => {
                let path = cap.saved_path.unwrap_or_else(|| "en memoria".into());
                term.on_note(&format!(
                    "✓ Captura de pantalla «{}» ({}) [{}x{}, {} KiB]",
                    cap.target,
                    path,
                    cap.width,
                    cap.height,
                    cap.size_bytes / 1024
                ))?;
            }
            Event::VisualQAReport(rep) => {
                let status = if rep.pass { "APROBADO" } else { "RECHAZADO" };
                term.on_note(&format!("antOS Visual QA [{}] · {}:", rep.target, status))?;
                term.on_note(&format!("  {}", rep.summary))?;
                for f in rep.findings {
                    let sev = match f.severity.as_str() {
                        "critical" => "CRÍTICO",
                        "warning" => "ADVERTENCIA",
                        _ => "INFO",
                    };
                    term.on_note(&format!("  • [{sev}] {}: {}", f.category, f.description))?;
                    if let Some(coords) = f.coordinates {
                        term.on_note(&format!("    Coordenadas: {coords}"))?;
                    }
                    term.on_note(&format!("    Recomendación: {}", f.recommendation))?;
                }
            }
            Event::DiskList(disks) => {
                term.on_note(&format!(
                    "Dispositivos de almacenamiento detectados ({}):",
                    disks.len()
                ))?;
                for d in disks {
                    let gb = d.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                    term.on_note(&format!(
                        "  • {} ({:.1} GB, Bus: {}, Particiones: {})",
                        d.path,
                        gb,
                        d.bus_type,
                        d.partitions.len()
                    ))?;
                }
            }
            Event::DiskDetail(opt) => {
                if let Some(d) = opt {
                    let gb = d.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                    term.on_note(&format!(
                        "Dispositivo {}: {:.1} GB, Bus: {}, Tabla: {}",
                        d.path, gb, d.bus_type, d.partition_table
                    ))?;
                } else {
                    term.on_note("Dispositivo no encontrado")?;
                }
            }
            Event::PartitionPlan(plan) => {
                term.on_note(&format!(
                    "Plan de particionado GPT para {}: ESP: {} MB, Raíz: {} MB",
                    plan.target_device,
                    plan.efi_partition_bytes / (1024 * 1024),
                    plan.root_partition_bytes / (1024 * 1024)
                ))?;
            }
            Event::InstallReport(rep) => {
                term.on_note(&format!("antOS Instalador · {}", rep.summary))?;
                term.on_note(&format!("  • Modo:          {}", rep.mode))?;
                term.on_note(&format!("  • Partición ESP: {}", rep.efi_partition))?;
                term.on_note(&format!("  • Partición /:   {}", rep.root_partition))?;
                if rep.simulated {
                    term.on_note("  • Simulación: nada escrito en el disco (○ = paso simulado)")?;
                }
                for s in rep.steps {
                    term.on_note(&format!(
                        "  {} {}: {}",
                        if s.executed { "✓" } else { "○" },
                        s.name,
                        s.description
                    ))?;
                }
            }
            Event::DetectedOperatingSystems(entries) => {
                term.on_note(&format!(
                    "antOS Bootloader · Sistemas Operativos Detectados ({}):",
                    entries.len()
                ))?;
                for (i, os) in entries.iter().enumerate() {
                    term.on_note(&format!(
                        "  [{}] {} (Tipo: {}, EFI: {})",
                        i + 1,
                        os.name,
                        os.os_type,
                        os.efi_path
                    ))?;
                }
            }
            Event::BootloaderReport(rep) => {
                term.on_note(&format!("antOS Bootloader · {}", rep.summary))?;
                term.on_note(&format!("  • Punto ESP:       {}", rep.esp_path))?;
                term.on_note(&format!("  • Comando NVRAM:   {}", rep.efibootmgr_command))?;
                for e in &rep.entries_configured {
                    term.on_note(&format!("  ✓ {}", e))?;
                }
            }
            Event::MicrovmStatus(st) => {
                term.on_note(&format!(
                    "antOS MicroVM · Hipervisor: {} [KVM: {}]",
                    st.hypervisor_engine,
                    if st.kvm_available { "Sí" } else { "No" }
                ))?;
                term.on_note(&format!("  • VMs activas:       {}", st.active_vms_count))?;
                term.on_note(&format!(
                    "  • Memoria asignada:  {} MB",
                    st.total_memory_allocated_mb
                ))?;
                term.on_note(&format!("  • Kernel:            {}", st.kernel_version))?;
            }
            Event::MicrovmList(vms) => {
                term.on_note(&format!(
                    "antOS MicroVM · Instancias activas ({}):",
                    vms.len()
                ))?;
                for v in vms {
                    term.on_note(&format!(
                        "  • [{}] PID {}, {} vCPUs, {} MB, vsock {}",
                        v.id, v.pid, v.vcpus, v.memory_mb, v.vsock_port
                    ))?;
                }
            }
            Event::MicrovmResult(res) => {
                term.on_note(&format!(
                    "antOS MicroVM · Comando ejecutado en «{}» [Código: {}]:",
                    res.vm_id, res.exit_code
                ))?;
                if !res.stdout.is_empty() {
                    term.on_note(&format!("  {}", res.stdout.trim()))?;
                }
            }
            Event::PackageInstallReport(rep) => {
                let status_label = if rep.success { "OK" } else { "ERROR" };
                term.on_note(&format!("antpkg [{status_label}]: {}", rep.message))?;
                if !rep.binaries_linked.is_empty() {
                    term.on_note(&format!(
                        "  • Binarios enlazados: {}",
                        rep.binaries_linked.join(", ")
                    ))?;
                }
                if !rep.store_path.is_empty() {
                    term.on_note(&format!("  • Prefijo en almacén: {}", rep.store_path))?;
                }
            }
            Event::PackageList(pkgs) => {
                if pkgs.is_empty() {
                    term.on_note("antpkg: No hay paquetes instalados en el perfil activo.")?;
                } else {
                    term.on_note(&format!(
                        "antpkg · Paquetes en perfil activo ({}):",
                        pkgs.len()
                    ))?;
                    for p in pkgs {
                        let kb = p.installed_size_bytes / 1024;
                        term.on_note(&format!(
                            "  • {} v{} ({} KiB, gen {}) [bin: {}]",
                            p.name,
                            p.version,
                            kb,
                            p.generation,
                            p.binaries.join(", ")
                        ))?;
                    }
                }
            }
            Event::PackageStoreStatus(st) => {
                let mb = st.total_store_bytes as f64 / (1024.0 * 1024.0);
                term.on_note(&format!("antpkg Store: {:.2} MB en almacén, {} paquetes, gen activa: {} ({} generaciones)",
                    mb, st.total_packages, st.current_generation, st.generations_count
                ))?;
                term.on_note(&format!("  • Ruta de almacén: {}", st.store_path))?;
                term.on_note(&format!("  • Perfil actual:   {}", st.current_profile_path))?;
            }
            Event::PackageGenerationsList(gens) => {
                term.on_note(&format!(
                    "antpkg · Generaciones de perfil ({}):",
                    gens.len()
                ))?;
                for g in gens {
                    let active_mark = if g.active { " (activa)" } else { "" };
                    term.on_note(&format!(
                        "  • Gen {}{}: {} paquetes [{}]",
                        g.generation,
                        active_mark,
                        g.packages.len(),
                        g.packages.join(", ")
                    ))?;
                }
            }
            Event::PackageVerificationResult {
                all_valid,
                verified_packages,
                details,
            } => {
                let status_lbl = if all_valid {
                    "INTEGRIDAD VERIFICADA"
                } else {
                    "ADVERTENCIAS DE INTEGRIDAD"
                };
                term.on_note(&format!(
                    "antpkg Verificación · {} ({} paquetes comprobados):",
                    status_lbl, verified_packages
                ))?;
                for d in details {
                    term.on_note(&format!("  {d}"))?;
                }
            }
            Event::DesktopAppList(apps) => {
                if apps.is_empty() {
                    term.on_note("antpkg: No hay aplicaciones de escritorio registradas.")?;
                } else {
                    term.on_note(&format!(
                        "antpkg · Aplicaciones de escritorio ({}):",
                        apps.len()
                    ))?;
                    for a in apps {
                        term.on_note(&format!(
                            "  • {} ({}) [exec: {}]",
                            a.name, a.package_name, a.exec
                        ))?;
                    }
                }
            }
            Event::DesktopValidationReport(rep) => {
                if rep.valid {
                    term.on_note(
                        "antpkg: El archivo .desktop es VÁLIDO según especificación Freedesktop.",
                    )?;
                } else {
                    term.on_note("antpkg: El archivo .desktop contiene ERRORES:")?;
                    for err in &rep.errors {
                        term.on_note(&format!("  - [ERROR] {err}"))?;
                    }
                }
                for warn in &rep.warnings {
                    term.on_note(&format!("  - [WARN] {warn}"))?;
                }
            }
            Event::PackageSearchResults(results) => {
                if results.is_empty() {
                    term.on_note("antpkg Catálogo: No se encontraron paquetes coincidentes.")?;
                } else {
                    term.on_note(&format!(
                        "📦 antpkg Catálogo · Paquetes encontrados ({}):",
                        results.len()
                    ))?;
                    for p in results {
                        let app_type_str = match p.app_type {
                            PackageAppType::Gui => "GUI",
                            PackageAppType::Cli => "CLI",
                        };
                        let cat_str = if let Some(ref d) = p.desktop_entry {
                            format!(" [{}]", d.categories.join(", "))
                        } else {
                            String::new()
                        };
                        term.on_note(&format!(
                            "  • {} v{} ({}){} - {}",
                            p.name, p.version, app_type_str, cat_str, p.description
                        ))?;
                    }
                }
            }
            Event::PackageInfo(info) => {
                term.on_note(&format!(
                    "📦 antpkg Receta: {} v{}",
                    info.name, info.version
                ))?;
                term.on_note(&format!("  • Descripción: {}", info.description))?;
                if let Some(ref home) = info.homepage {
                    term.on_note(&format!("  • Homepage:    {}", home))?;
                }
                if let Some(ref lic) = info.license {
                    term.on_note(&format!("  • Licencia:    {}", lic))?;
                }
                let app_type_str = match info.app_type {
                    PackageAppType::Gui => "GUI (Wayland/X11)",
                    PackageAppType::Cli => "CLI (Terminal)",
                };
                term.on_note(&format!("  • Tipo de App: {}", app_type_str))?;
                if !info.binaries.is_empty() {
                    term.on_note(&format!("  • Binarios:    {}", info.binaries.join(", ")))?;
                }
                if let Some(ref src) = info.source_url {
                    term.on_note(&format!("  • Origen:      {}", src))?;
                }
                if let Some(ref sha) = info.sha256 {
                    term.on_note(&format!("  • SHA-256:     {}", sha))?;
                }
                if !info.dependencies.is_empty() {
                    term.on_note(&format!(
                        "  • Deps:        {}",
                        info.dependencies.join(", ")
                    ))?;
                }
                if let Some(ref d) = info.desktop_entry {
                    term.on_note("  • Entrada de escritorio:")?;
                    term.on_note(&format!("      Nombre:     {}", d.name))?;
                    if let Some(ref g) = d.generic_name {
                        term.on_note(&format!("      Genérico:   {}", g))?;
                    }
                    term.on_note(&format!("      Ejecutable: {}", d.exec))?;
                    if let Some(ref ic) = d.icon {
                        term.on_note(&format!("      Icono:      {}", ic))?;
                    }
                    if !d.categories.is_empty() {
                        term.on_note(&format!("      Categorías: {}", d.categories.join("; ")))?;
                    }
                    if !d.mime_types.is_empty() {
                        term.on_note(&format!("      MIME:       {}", d.mime_types.join("; ")))?;
                    }
                }
            }
            Event::AppList(apps) => {
                if apps.is_empty() {
                    term.on_note("antOS Apps: No hay aplicaciones instaladas.")?;
                } else {
                    term.on_note(&format!(
                        "📦 antOS Apps · Aplicaciones Registradas ({}):",
                        apps.len()
                    ))?;
                    for a in apps {
                        term.on_note(&format!(
                            "  • {:<32} [{}] v{} {}",
                            a.id,
                            a.source.as_str(),
                            a.version,
                            a.name
                        ))?;
                    }
                }
            }
            Event::AppSearchResults(results) => {
                if results.is_empty() {
                    term.on_note("antOS Apps: No se encontraron aplicaciones.")?;
                } else {
                    term.on_note(&format!(
                        "🔍 antOS Apps · Catálogo ({}) resultados:",
                        results.len()
                    ))?;
                    for r in results {
                        let status = if r.installed {
                            "[Instalada]"
                        } else {
                            "[Disponible]"
                        };
                        term.on_note(&format!(
                            "  • {:<32} [{}] {} v{} - {}",
                            r.id,
                            r.source.as_str(),
                            status,
                            r.version,
                            r.name
                        ))?;
                    }
                }
            }
            Event::AppProgress(p) => {
                term.on_note(&format!("  [{:>3.0}%] {}", p.percentage, p.status))?;
            }
            Event::AppActionResult(res) => {
                let badge = if res.success { "✅" } else { "❌" };
                term.on_note(&format!(
                    "{badge} antOS Apps · [{}] {}",
                    res.action, res.message
                ))?;
            }
            Event::AppLaunchResult(res) => {
                let badge = if res.success { "🚀" } else { "❌" };
                term.on_note(&format!("{badge} antOS Apps · {}", res.message))?;
            }
            Event::AutopilotStatus(st) => {
                let active_badge = if st.active {
                    "ACTIVO (Vigilando)"
                } else {
                    "DETENIDO"
                };
                term.on_note(&format!("antOS Autopilot · Estado: {active_badge}"))?;
                term.on_note(&format!("  • Espacio de trabajo: {}", st.workspace_path))?;
                term.on_note(&format!(
                    "  • Intervalo sondeo:   {}s",
                    st.poll_interval_secs
                ))?;
                term.on_note(&format!(
                    "  • Incidentes activos: {}",
                    st.active_incidents_count
                ))?;
                term.on_note(&format!(
                    "  • Total resueltos:    {}",
                    st.resolved_incidents_count
                ))?;
                if let Some(ts) = st.last_scan_timestamp {
                    term.on_note(&format!("  • Último escaneo:     {ts}"))?;
                }
            }
            Event::AutopilotIncidentsList(list) => {
                if list.is_empty() {
                    term.on_note("antOS Autopilot: No hay incidencias activas en el repositorio.")?;
                } else {
                    term.on_note(&format!(
                        "antOS Autopilot · Incidencias Registradas ({}):",
                        list.len()
                    ))?;
                    for inc in list {
                        term.on_note(&format!(
                            "  • [{}] {} en «{}» [{}] — {}",
                            inc.id, inc.incident_type, inc.file_path, inc.status, inc.error_message
                        ))?;
                    }
                }
            }
            Event::AutopilotAlert(inc) => {
                term.on_note(&format!(
                    "antOS Autopilot · Alerta de Incidencia [{}] en «{}»:",
                    inc.id, inc.file_path
                ))?;
                term.on_note(&format!("  • Error:  {}", inc.error_message))?;
                term.on_note(&format!("  • Estado: {}", inc.status))?;
                if let Some(ref prop) = inc.fix_proposal {
                    term.on_note(&format!(
                        "  • Solución: {} (Rama: {})",
                        prop.title, prop.branch
                    ))?;
                    if !prop.diff.is_empty() {
                        term.on_note(&format!("  • Diff:\n{}", prop.diff))?;
                    }
                }
            }
            Event::WebConsoleStatus(st) => {
                let status_badge = if st.running {
                    "ACTIVO (En línea)"
                } else {
                    "DETENIDO"
                };
                term.on_note(&format!("antOS Web Console · Estado: {status_badge}"))?;
                term.on_note(&format!("  • URL de Acceso:         {}", st.url))?;
                term.on_note(&format!(
                    "  • Clientes Conectados:   {}",
                    st.connected_clients
                ))?;
                term.on_note(&format!(
                    "  • Sesiones Activas:      {}",
                    st.active_sessions_count
                ))?;
            }
            Event::WebTokenGenerated(session) => {
                term.on_note("antOS Web Console · Token de Autenticación Criptográfico:")?;
                term.on_note(&format!("  • Token:     {}", session.token))?;
                term.on_note(&format!(
                    "  • Expira en: {}s",
                    session.expires_at.saturating_sub(session.created_at)
                ))?;
                if let Some(lbl) = session.client_label {
                    term.on_note(&format!("  • Cliente:   {lbl}"))?;
                }
            }
            Event::DevWorkspaceStatus(status) => {
                term.on_note("antOS · Espacio de Trabajo Integrado Dev TUI (T20.1):")?;
                term.on_note(&format!(
                    "  • Proyecto Activo:   {}",
                    status.active_project.as_deref().unwrap_or("ninguno")
                ))?;
                term.on_note(&format!("  • Editor:            {}", status.editor_command))?;
                term.on_note(&format!(
                    "  • Dimensiones:       {}x{}",
                    status.term_columns, status.term_rows
                ))?;
            }
            Event::TddReport(report) => {
                term.on_note(&format!(
                    "🧪 antOS TDD Engine · Reporte de Reproducción [{}]",
                    report.id
                ))?;
                term.on_note(&format!("  • Estado:            {}", report.phase.label()))?;
                term.on_note(&format!(
                    "  • Lenguaje:          {:?}",
                    report.diagnostic.language
                ))?;
                term.on_note(&format!(
                    "  • Tipo de Error:     {}",
                    report.diagnostic.error_type
                ))?;
                term.on_note(&format!(
                    "  • Mensaje:           {}",
                    report.diagnostic.message
                ))?;
                if let Some(ref f) = report.diagnostic.target_file {
                    term.on_note(&format!(
                        "  • Archivo Objetivo:  {}:{}",
                        f,
                        report.diagnostic.target_line.unwrap_or(0)
                    ))?;
                }
                if let Some(ref fn_name) = report.diagnostic.target_function {
                    term.on_note(&format!("  • Función:           {}", fn_name))?;
                }
                term.on_note(&format!("  • Test Generado:     {}", report.test_file))?;
                if let Some(ref fix) = report.fix_summary {
                    term.on_note(&format!("  • Corrección:        {}", fix))?;
                }
                term.on_note(&format!(
                    "  • Auditado:          {}",
                    if report.audited {
                        "Sí (Protegido contra regresiones)"
                    } else {
                        "No"
                    }
                ))?;
            }
            Event::CiReport(report) => {
                term.on_note(&format!(
                    "⚙️ antOS CI · Reporte de Ejecución [{}]",
                    report.id
                ))?;
                term.on_note(&format!(
                    "  • Estado General:    {}",
                    if report.success { "PASÓ" } else { "FALLÓ" }
                ))?;
                term.on_note(&format!(
                    "  • Duración Total:    {} ms",
                    report.total_duration_ms
                ))?;
                term.on_note(&format!(
                    "  • Seguridad:         {}",
                    if report.security_clean {
                        "Limpio (sin secretos)"
                    } else {
                        "Secretos detectados"
                    }
                ))?;
                for s in &report.stages {
                    term.on_note(&format!(
                        "    - {:<12} {:<15} ({} ms) {}",
                        s.name,
                        s.status.label(),
                        s.duration_ms,
                        s.command
                    ))?;
                }
            }
            Event::GitHooksStatus(status) => {
                term.on_note("🪝 antOS Git Hooks · Estado de Protección:")?;
                term.on_note(&format!(
                    "  • Pre-commit: {}",
                    if status.pre_commit_installed {
                        "Instalado"
                    } else {
                        "No instalado"
                    }
                ))?;
                term.on_note(&format!(
                    "  • Pre-push:   {}",
                    if status.pre_push_installed {
                        "Instalado"
                    } else {
                        "No instalado"
                    }
                ))?;
                term.on_note(&format!("  • Directorio: {}", status.hook_dir))?;
            }
            Event::SnapshotCreated(meta) => {
                term.on_note(&format!(
                    "📸 antOS Time Machine · Instantánea [{}] creada ({} archivos, {} KiB)",
                    meta.id,
                    meta.files_count,
                    meta.total_bytes / 1024
                ))?;
            }
            Event::SnapshotsList(list) => {
                term.on_note(&format!(
                    "⏱️ antOS Time Machine · {} instantánea(s) registradas:",
                    list.len()
                ))?;
                for s in list {
                    term.on_note(&format!(
                        "  • [{}] {} ({} archivos, {} KiB)",
                        s.id,
                        s.label.as_deref().unwrap_or("—"),
                        s.files_count,
                        s.total_bytes / 1024
                    ))?;
                }
            }
            Event::SnapshotRestored(res) => {
                term.on_note(&format!("⏪ antOS Time Machine · Instantánea [{}] restaurada en {} ms ({} archivos actualizados)", res.snapshot_id, res.duration_ms, res.files_restored))?;
            }
            Event::SnapshotDeleted { id } => {
                term.on_note(&format!(
                    "🗑️ antOS Time Machine · Instantánea [{id}] eliminada."
                ))?;
            }
            Event::BenchmarkReport(report) => {
                term.on_note(&format!(
                    "⚡ antOS Bench · Suite [{}] ejecutada en {} ms ({} métricas)",
                    report.id,
                    report.total_duration_ms,
                    report.metrics.len()
                ))?;
                for m in &report.metrics {
                    term.on_note(&format!(
                        "  • {:<24} media: {} ns, p95: {} ns, {:.0} ops/s",
                        m.name, m.mean_ns, m.p95_ns, m.ops_per_sec
                    ))?;
                }
            }
            Event::BenchmarkDiff(diff) => {
                term.on_note(&format!(
                    "⚡ antOS Bench Diff · Base: {} vs Objetivo: {}",
                    diff.base_branch, diff.target_branch
                ))?;
                for c in &diff.comparisons {
                    term.on_note(&format!(
                        "  • {:<24} delta: {:+.2}% [{}]",
                        c.name,
                        c.delta_pct,
                        if c.is_regression {
                            "REGRESIÓN"
                        } else {
                            "ÓPTIMO"
                        }
                    ))?;
                }
                term.on_note(&format!("  Veredicto Auditor: {}", diff.auditor_verdict))?;
            }
            Event::BenchmarkHistory(list) => {
                term.on_note(&format!(
                    "📈 antOS Bench · {} corrida(s) en historial:",
                    list.len()
                ))?;
                for h in list {
                    term.on_note(&format!(
                        "  • [{}] {} - {} ({} ms)",
                        h.id, h.branch, h.suite_name, h.total_duration_ms
                    ))?;
                }
            }
            Event::RemoteIssuesList(issues) => {
                term.on_note(&format!(
                    "🐙 antOS Forge · {} issues abiertos en origen:",
                    issues.len()
                ))?;
                for i in issues {
                    term.on_note(&format!("  • #{:<4} {} (@{})", i.number, i.title, i.author))?;
                }
            }
            Event::RemoteIssueImported {
                ticket_id,
                path,
                title,
            } => {
                term.on_note(&format!(
                    "📥 antOS Forge · Issue importado como [{ticket_id}]: {title} ({path})"
                ))?;
            }
            Event::PullRequestCreated(pr) => {
                term.on_note(&format!(
                    "🚀 antOS Forge · Pull Request #{}: {} ({})",
                    pr.number, pr.title, pr.url
                ))?;
            }
            Event::PullRequestStatus(status) => {
                term.on_note(&format!(
                    "🔍 antOS Forge · Pull Request #{}: {} [Estado: {}]",
                    status.number, status.title, status.state
                ))?;
            }
            Event::ArchDiagram(report) => {
                term.on_note(&format!(
                    "📐 antOS Doc Arch · {} ({} crates, {} módulos, {} caps)",
                    report.kind.name(),
                    report.crates_count,
                    report.modules_count,
                    report.caps_count
                ))?;
                term.on_note(&format!("```mermaid\n{}\n```", report.mermaid_content))?;
            }
            Event::DocSync(report) => {
                term.on_note(&format!("🔄 antOS Doc Sync · {}", report.message))?;
                for p in &report.updated_paths {
                    term.on_note(&format!("  • Archivo: {p}"))?;
                }
            }
            Event::Error(m) => bail!("{m}"),
        }
    }
    Ok(())
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::time::Instant;

    /// A `Ctx` usable for these tests: `caps_dir` comes from a real
    /// `Ctx::discover()` (this test binary runs from inside the antOS
    /// tree), `workspace`/`state` point at an isolated temp directory —
    /// none of these tests exercise a code path that needs either to
    /// contain anything.
    fn test_ctx() -> (Ctx, std::path::PathBuf) {
        let discovered = Ctx::discover().expect("Ctx::discover must succeed inside the antOS tree");
        let temp = std::env::temp_dir().join(format!("antos_test_ipc_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        let ctx = Ctx {
            workspace: temp.clone(),
            state: temp.clone(),
            ..discovered
        };
        (ctx, temp)
    }

    fn test_catalog(ctx: &Ctx) -> Catalog {
        Catalog::load(&ctx.caps_dir).expect("the real capability catalogue must load")
    }

    #[test]
    fn test_oversized_message_gets_a_protocol_error_not_a_hang() {
        let (ctx, temp) = test_ctx();
        let catalog = test_catalog(&ctx);

        let (server_stream, mut client_stream) = UnixStream::pair().unwrap();
        client_stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        let handle = std::thread::spawn(move || handle_connection(&ctx, &catalog, server_stream));

        // Comfortably over MAX_MESSAGE_BYTES, no newline anywhere in it —
        // the exact "client never sends \n" scenario from the ticket. Only
        // `MAX_MESSAGE_BYTES + 1` of this is ever actively read by
        // `receive`'s capped `take`; the rest is well within a Unix domain
        // socket's kernel buffer, so this write does not itself block.
        let garbage = vec![b'x'; (MAX_MESSAGE_BYTES as usize) + 4096];
        client_stream.write_all(&garbage).unwrap();

        let mut reader = BufReader::new(client_stream);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .expect("must receive a response instead of hanging or being disconnected without one");
        let event: Event = serde_json::from_str(line.trim()).expect("response must be valid JSON");
        assert!(
            matches!(event, Event::Error(_)),
            "expected a protocol error, got: {event:?}"
        );

        handle.join().unwrap().ok();
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_second_connection_is_served_after_an_idle_first_connection_times_out() {
        let (ctx, temp) = test_ctx();
        let catalog = test_catalog(&ctx);

        // Connection A: idle, never sends anything — the abandoned
        // connection from the ticket. A short timeout stands in for the
        // real `IPC_READ_TIMEOUT` so this test is fast and deterministic;
        // the mechanism under test is the same `set_read_timeout` call
        // `serve` makes in production.
        let (server_a, client_a) = UnixStream::pair().unwrap();
        server_a
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();

        let start = Instant::now();
        let result_a = handle_connection(&ctx, &catalog, server_a);
        let elapsed = start.elapsed();
        assert!(
            result_a.is_err(),
            "an idle connection past its read timeout must surface as an error"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "must give up within its configured timeout, took {elapsed:?}"
        );
        drop(client_a); // kept alive until here on purpose, so the read above genuinely times out rather than seeing an immediate EOF.

        // Connection B: served right after, exactly as `serve`'s serial
        // accept loop would do once connection A releases control — must
        // not have been starved by A's idleness.
        let (server_b, mut client_b) = UnixStream::pair().unwrap();
        server_b
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        send(&mut client_b, &Request::Approval(false)).unwrap();

        handle_connection(&ctx, &catalog, server_b).expect("connection B must be served normally");

        let mut reader = BufReader::new(client_b);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let event: Event = serde_json::from_str(line.trim()).unwrap();
        assert!(
            matches!(event, Event::Error(_)),
            "expected the usual 'approval without prior proposal' error, got: {event:?}"
        );

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[cfg(unix)]
    #[test]
    fn test_socket_is_never_group_or_world_accessible_even_transiently() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("antos_test_ipc_bind_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sock_path = dir.join("test.sock");

        // SAFETY: temporarily loosens the process umask to prove the
        // socket doesn't rely on an already-strict ambient umask — it must
        // be denied to group/other from the instant it exists, not merely
        // chmod'd afterward under a lucky umask. Restored unconditionally.
        unsafe {
            let previous = libc::umask(0o000);
            let result = std::panic::catch_unwind(|| {
                // The umask-wrapped bind step alone, *not* the follow-up
                // explicit chmod in `bind_socket` — this is what actually
                // proves there is no window between the two.
                let _listener = bind_socket_with_restrictive_umask(&sock_path).unwrap();
                let mode = std::fs::metadata(&sock_path).unwrap().permissions().mode();
                assert_eq!(
                    mode & 0o077,
                    0,
                    "the socket must deny group/other access from the instant it exists, even under a permissive process umask"
                );
            });
            libc::umask(previous);
            result.unwrap();
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn test_bind_socket_final_permissions_are_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir =
            std::env::temp_dir().join(format!("antos_test_ipc_bind_final_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sock_path = dir.join("test.sock");

        let _listener = bind_socket(&sock_path).unwrap();
        let mode = std::fs::metadata(&sock_path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_read_only_queries_bypass_active_mutation_lock() {
        let (ctx, temp) = test_ctx();
        let catalog = test_catalog(&ctx);

        // Simulamos que una sesión interactiva de Request::Intent tiene retenido el cerrojo de mutación.
        let mutation_lock = WORKSPACE_MUTATION_LOCK.lock().unwrap();

        let (server_stream, mut client_stream) = UnixStream::pair().unwrap();
        client_stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();

        send(
            &mut client_stream,
            &Request::DiagnosePorts { port: Some(59999) },
        )
        .unwrap();

        let start = Instant::now();
        let handle = std::thread::spawn(move || handle_connection(&ctx, &catalog, server_stream));

        let mut reader = BufReader::new(client_stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let elapsed = start.elapsed();

        let event: Event = serde_json::from_str(line.trim()).unwrap();
        assert!(
            matches!(event, Event::PortsStatus(_)),
            "consulta de solo lectura debe responder exitosamente mientras hay una intención en curso, obtuvo: {event:?}"
        );
        assert!(
            elapsed < Duration::from_millis(500),
            "consulta de solo lectura debe responder de inmediato sin esperar al cerrojo de mutación, tardó: {elapsed:?}"
        );

        handle.join().unwrap().unwrap();
        drop(mutation_lock);
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_concurrent_intents_are_serialized_cleanly() {
        let (ctx, temp) = test_ctx();
        let catalog = test_catalog(&ctx);

        // Bloqueamos manualmente el cerrojo de mutación
        let guard = WORKSPACE_MUTATION_LOCK.lock().unwrap();

        let (server_stream, mut client_stream) = UnixStream::pair().unwrap();
        client_stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();

        // Enviamos una petición de Intent sintética con dry_run
        send(
            &mut client_stream,
            &Request::Intent {
                text: "listar archivos".into(),
                planner: Some("local".into()),
                dry_run: true,
            },
        )
        .unwrap();

        let handle = std::thread::spawn(move || handle_connection(&ctx, &catalog, server_stream));

        // Damos tiempo para que el hilo intente adquirir el cerrojo
        std::thread::sleep(Duration::from_millis(50));

        // Liberamos el cerrojo: ahora la intención puede proceder
        drop(guard);

        let res = handle.join().unwrap();
        assert!(res.is_ok());

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_socket_handler_on_result_without_double_serialization() {
        let (mut server, client) = UnixStream::pair().unwrap();
        let mut server_reader = BufReader::new(server.try_clone().unwrap());

        let mut handler = SocketHandler {
            writer: &mut server,
            reader: &mut server_reader,
        };

        let exec_res = ExecutionResult {
            ok: true,
            message: "operación completada".into(),
            snapshot: None,
        };

        handler
            .on_result(&exec_res)
            .expect("on_result debe emitir sin error");

        let mut client_reader = BufReader::new(client);
        let mut line = String::new();
        client_reader.read_line(&mut line).unwrap();

        let event: Event = serde_json::from_str(line.trim()).unwrap();
        match event {
            Event::Result(res) => {
                assert!(res.ok);
                assert_eq!(res.message, "operación completada");
            }
            other => panic!("se esperaba Event::Result, se obtuvo: {other:?}"),
        }
    }
}
