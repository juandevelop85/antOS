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
//! El socket vive en el directorio de estado con permisos 0600. Esa es hoy
//! toda la autorización: quien pueda abrir el fichero puede pedir cosas. Es
//! suficiente para un solo usuario en su máquina y claramente insuficiente
//! para cualquier otra cosa.

use crate::capability::Catalog;
use crate::ctx::Ctx;
use crate::protocolo::{Interlocutor, Propuesta, Resultado};
use crate::{sesion, terminal};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

pub fn ruta_socket(ctx: &Ctx) -> PathBuf {
    ctx.state.join("antos.sock")
}

// ------------------------------------------------------------- el protocolo

#[allow(unused_imports)]
pub use antos_protocol::{Event, Evento, Request};
#[allow(unused_imports)]
pub use antos_protocol::Peticion;

/// Una línea de JSON por mensaje. Sin marco binario ni longitudes: se puede
/// leer con `nc` y depurar mirándolo, que a esta escala vale más que los
/// bytes que ahorraría.
fn enviar<T: Serialize>(destino: &mut impl Write, mensaje: &T) -> Result<()> {
    writeln!(destino, "{}", serde_json::to_string(mensaje)?)?;
    destino.flush()?;
    Ok(())
}

fn recibir<T: for<'a> Deserialize<'a>>(origen: &mut impl BufRead) -> Result<Option<T>> {
    let mut linea = String::new();
    if origen.read_line(&mut linea)? == 0 {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(linea.trim())?))
}

// ------------------------------------------------------------- lado servidor

/// El interlocutor del demonio: en vez de pintar, escribe por el socket; y
/// para preguntar, espera una respuesta por él.
struct PorSocket<'a> {
    escritura: &'a mut UnixStream,
    lectura: &'a mut BufReader<UnixStream>,
}

impl Interlocutor for PorSocket<'_> {
    fn inicio(&mut self, intencion: &str, planificador: &str) -> Result<()> {
        enviar(
            self.escritura,
            &Event::Start {
                intent: intencion.to_string(),
                planner: planificador.to_string(),
            },
        )
    }

    fn nota(&mut self, texto: &str) -> Result<()> {
        enviar(self.escritura, &Event::Note(texto.to_string()))
    }

    fn propone(&mut self, propuesta: &Propuesta) -> Result<bool> {
        enviar(
            self.escritura,
            &Event::Proposal(Box::new(propuesta.clone())),
        )?;

        match recibir::<Request>(self.lectura)? {
            Some(Request::Approval(decision)) => Ok(decision),
            // Un cliente que se va sin contestar no aprueba nada. El silencio
            // nunca es un sí.
            _ => Ok(false),
        }
    }

    fn salida(&mut self, texto: &str) -> Result<()> {
        enviar(self.escritura, &Event::Output(texto.to_string()))
    }

    fn resultado(&mut self, resultado: &Resultado) -> Result<()> {
        let copia: Resultado = serde_json::from_str(&serde_json::to_string(resultado)?)?;
        enviar(self.escritura, &Event::Result(copia))
    }
}

pub fn servir(ctx: &Ctx, catalog: &Catalog) -> Result<()> {
    let ruta = ruta_socket(ctx);
    // Un socket huérfano de una ejecución anterior impediría escuchar.
    let _ = std::fs::remove_file(&ruta);

    let escucha = UnixListener::bind(&ruta)
        .with_context(|| format!("no pude escuchar en {}", ruta.display()))?;
    std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o600))?;

    println!(
        "{} {}",
        terminal::paint("syso · demonio escuchando en", terminal::BOLD),
        terminal::paint(&ruta.display().to_string(), terminal::DIM)
    );

    for conexion in escucha.incoming() {
        let flujo = match conexion {
            Ok(f) => f,
            Err(e) => {
                eprintln!("conexión rechazada: {e}");
                continue;
            }
        };
        // Se atiende una conexión cada vez, a propósito. Dos intenciones
        // mutando el mismo espacio de trabajo a la vez producirían diffs que
        // ya no describen el resultado — el mismo fallo que se arregló
        // calculando los pasos en orden, pero entre procesos.
        if let Err(e) = atender(ctx, catalog, flujo) {
            eprintln!("sesión terminada con error: {e:#}");
        }
    }
    Ok(())
}

fn atender(ctx: &Ctx, catalog: &Catalog, flujo: UnixStream) -> Result<()> {
    let mut lectura = BufReader::new(flujo.try_clone()?);
    let mut escritura = flujo;

    let Some(peticion) = recibir::<Request>(&mut lectura)? else {
        return Ok(());
    };

    match peticion {
        Request::Intent { text, planner, dry_run } => {
            let planner_instance = crate::pick_planner_por_nombre(planner.as_deref())?;
            let mut con = PorSocket {
                escritura: &mut escritura,
                lectura: &mut lectura,
            };
            if let Err(e) = sesion::intencion(ctx, catalog, &text, &*planner_instance, dry_run, &mut con) {
                enviar(&mut escritura, &Event::Error(format!("{e:#}")))?;
            }
        }
        Request::Approval(_) => {
            enviar(
                &mut escritura,
                &Event::Error("una aprobación sin propuesta previa".into()),
            )?;
        }
        Request::QueryGitStatus { workspace_path } => {
            match crate::git::GitAnalyzer::global().consultar_estado(Path::new(&workspace_path)) {
                Ok(Some(status)) => {
                    enviar(&mut escritura, &Event::GitStatus(status))?;
                }
                Ok(None) => {
                    enviar(&mut escritura, &Event::NotGitRepo)?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Event::Error(format!("{e:#}")))?;
                }
            }
        }
        Request::ListTickets { workspace_path } => {
            match crate::spec::SpecEngine::global().listar_tickets(Path::new(&workspace_path)) {
                Ok(tickets) => {
                    enviar(&mut escritura, &Event::TicketList(tickets))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Event::Error(format!("{e:#}")))?;
                }
            }
        }
        Request::GetTicket { workspace_path, ticket_id } => {
            match crate::spec::SpecEngine::global().obtener_ticket(Path::new(&workspace_path), &ticket_id) {
                Ok(detalle) => {
                    enviar(&mut escritura, &Event::TicketDetail(detalle))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Event::Error(format!("{e:#}")))?;
                }
            }
        }
        Request::DiagnosePorts { port } => {
            match crate::net::diagnosticar_puertos(port) {
                Ok(puertos) => {
                    enviar(&mut escritura, &Event::PortsStatus(puertos))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Event::Error(format!("{e:#}")))?;
                }
            }
        }
        Request::StartFlow { workspace_path, ticket_id } => {
            match crate::flow::FlowEngine::global().iniciar_tarea(
                Path::new(&workspace_path),
                &ctx.state,
                &ticket_id,
            ) {
                Ok(task) => {
                    enviar(&mut escritura, &Event::FlowStatus(Some(task)))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Event::Error(format!("{e:#}")))?;
                }
            }
        }
        Request::QueryFlow { ticket_id } => {
            let task = crate::flow::FlowEngine::global().consultar_tarea(&ticket_id);
            enviar(&mut escritura, &Event::FlowStatus(task))?;
        }
        Request::ListFlows { .. } => {
            let tasks = crate::flow::FlowEngine::global().listar_tareas();
            enviar(&mut escritura, &Event::FlowList(tasks))?;
        }
        Request::ApproveFlow { ticket_id, decision } => {
            match crate::flow::FlowEngine::global().aprobar_tarea(&ticket_id, decision) {
                Ok(task) => {
                    enviar(&mut escritura, &Event::FlowStatus(Some(task)))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Event::Error(format!("{e:#}")))?;
                }
            }
        }
        Request::QueryDiff { workspace_path, target } => {
            let ws = Path::new(&workspace_path);
            let git_out = std::process::Command::new("git")
                .current_dir(ws)
                .args(&["diff", target.as_deref().unwrap_or("HEAD")])
                .output();

            match git_out {
                Ok(out) if out.status.success() => {
                    let diff_str = String::from_utf8_lossy(&out.stdout);
                    let files = crate::diff_view::DiffEngine::parse_unified_diff(&diff_str);
                    enviar(&mut escritura, &Event::StructuredDiff(files))?;
                }
                _ => {
                    enviar(&mut escritura, &Event::StructuredDiff(Vec::new()))?;
                }
            }
        }
        Request::ListNotifications { workspace_path } => {
            let ws = Path::new(&workspace_path);
            let notifs = crate::notification::NotificationEngine::global().list(ws).unwrap_or_default();
            enviar(&mut escritura, &Event::NotificationList(notifs))?;
        }
        Request::HandleNotificationAction { workspace_path, notification_id, action } => {
            let ws = Path::new(&workspace_path);
            match crate::notification::NotificationEngine::global().handle_action(ws, &notification_id, action) {
                Ok((success, message)) => {
                    enviar(&mut escritura, &Event::NotificationResult {
                        id: notification_id,
                        success,
                        message,
                    })?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Event::NotificationResult {
                        id: notification_id,
                        success: false,
                        message: format!("{e:#}"),
                    })?;
                }
            }
        }
        Request::QueryMesh { workspace_path } => {
            let ws = Path::new(&workspace_path);
            match crate::mesh::MeshEngine::global().status(ws) {
                Ok(status) => {
                    enviar(&mut escritura, &Event::MeshStatus(status))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Event::Error(format!("{e:#}")))?;
                }
            }
        }
        Request::ConnectPeer { workspace_path, address } => {
            let ws = Path::new(&workspace_path);
            match crate::mesh::MeshEngine::global().connect_peer(ws, &address) {
                Ok(peer) => {
                    enviar(&mut escritura, &Event::PeerConnectionResult {
                        address: peer.address,
                        success: true,
                        message: format!("conectado con éxito al peer {} ({}ms)", peer.id, peer.latency_ms),
                    })?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Event::PeerConnectionResult {
                        address,
                        success: false,
                        message: format!("{e:#}"),
                    })?;
                }
            }
        }
        Request::GeneratePairingToken { workspace_path } => {
            let ws = Path::new(&workspace_path);
            match crate::mesh::MeshEngine::global().generate_pairing_token(ws) {
                Ok(token) => {
                    enviar(&mut escritura, &Event::PairingTokenGenerated(token))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Event::Error(format!("{e:#}")))?;
                }
            }
        }
        Request::QuerySwarm { workspace_path } => {
            let ws = Path::new(&workspace_path);
            match crate::distributed::SwarmEngine::global().status(ws) {
                Ok(status) => {
                    enviar(&mut escritura, &Event::SwarmStatus(status))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Event::Error(format!("{e:#}")))?;
                }
            }
        }
        Request::DispatchRemoteRole { workspace_path, ticket_id, role, node_id } => {
            let ws = Path::new(&workspace_path);
            match crate::distributed::SwarmEngine::global().dispatch_remote_role(ws, &ticket_id, role, node_id.as_deref()) {
                Ok(task) => {
                    enviar(&mut escritura, &Event::SwarmDispatchResult {
                        ticket_id,
                        role,
                        assigned_node_id: task.assigned_node_id,
                        success: true,
                        message: format!("tarea {} despachada con éxito en nodo {}", task.task_id, task.status),
                    })?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Event::SwarmDispatchResult {
                        ticket_id,
                        role,
                        assigned_node_id: node_id.unwrap_or_else(|| "unknown".into()),
                        success: false,
                        message: format!("{e:#}"),
                    })?;
                }
            }
        }
        Request::QueryVfs { workspace_path, virtual_path } => {
            let ws = Path::new(&workspace_path);
            let engine = crate::vfs::VfsEngine::global();
            if virtual_path.ends_with('/') || virtual_path == "/antfs" || virtual_path == "/antfs/symbols" || virtual_path.starts_with("/antfs/symbols/") && !virtual_path.split('/').skip(3).any(|p| !p.is_empty()) {
                match engine.list_dir(ws, &virtual_path) {
                    Ok(entries) => {
                        enviar(&mut escritura, &Event::VfsList { virtual_path, entries })?;
                    }
                    Err(e) => {
                        enviar(&mut escritura, &Event::Error(format!("{e:#}")))?;
                    }
                }
            } else {
                match engine.read_path(ws, &virtual_path) {
                    Ok(content) => {
                        enviar(&mut escritura, &Event::VfsContent { virtual_path, content })?;
                    }
                    Err(e) => {
                        enviar(&mut escritura, &Event::Error(format!("{e:#}")))?;
                    }
                }
            }
        }
        Request::MountVfs { workspace_path, mount_point } => {
            let ws = Path::new(&workspace_path);
            match crate::vfs::VfsEngine::global().mount(ws, mount_point.as_deref()) {
                Ok(path) => {
                    enviar(&mut escritura, &Event::VfsResult {
                        action: "mount".into(),
                        success: true,
                        message: format!("montado en {}", path.display()),
                    })?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Event::VfsResult {
                        action: "mount".into(),
                        success: false,
                        message: format!("{e:#}"),
                    })?;
                }
            }
        }
        Request::UnmountVfs { workspace_path, mount_point } => {
            let ws = Path::new(&workspace_path);
            match crate::vfs::VfsEngine::global().unmount(ws, mount_point.as_deref()) {
                Ok(_) => {
                    enviar(&mut escritura, &Event::VfsResult {
                        action: "unmount".into(),
                        success: true,
                        message: "desmontado correctamente".into(),
                    })?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Event::VfsResult {
                        action: "unmount".into(),
                        success: false,
                        message: format!("{e:#}"),
                    })?;
                }
            }
        }
        Request::ValidateVfsWrite { file_path, content } => {
            let res = crate::vfs_guard::VfsGuardEngine::global().intercept_write(&file_path, &content)?;
            enviar(&mut escritura, &Event::VfsValidationResult(res))?;
        }
        Request::QueryVfsGuard { .. } => {
            let status = crate::vfs_guard::VfsGuardEngine::global().status()?;
            enviar(&mut escritura, &Event::VfsGuardStatus(status))?;
        }
        Request::QueryEbpfStatus { .. } => {
            let status = crate::ebpf::EbpfSentinelEngine::global().status()?;
            enviar(&mut escritura, &Event::EbpfStatus(status))?;
        }
        Request::QueryEbpfAuditLog { limit, .. } => {
            let events = crate::ebpf::EbpfSentinelEngine::global().get_audit_log(limit);
            enviar(&mut escritura, &Event::EbpfAuditLog(events))?;
        }
        Request::SimulateEbpfViolation { hook, target_resource, .. } => {
            let event = crate::ebpf::EbpfSentinelEngine::global().simulate_violation(hook, &target_resource);
            enviar(&mut escritura, &Event::EbpfResult {
                action: format!("{:?}", hook),
                success: true,
                message: format!("evento {} generado con éxito (acción: {:?})", event.id, event.action_taken),
            })?;
        }
        Request::RunProfiler { workspace_path, command } => {
            let ws = std::path::PathBuf::from(workspace_path);
            let report = crate::profiler::ProfilerEngine::global().run_and_profile(&ws, &command)?;
            enviar(&mut escritura, &Event::ProfilerReport(report))?;
        }
        Request::QueryProfilerReports { workspace_path, limit } => {
            let ws = std::path::PathBuf::from(workspace_path);
            let mut reports = crate::profiler::ProfilerEngine::global().load_reports(&ws);
            reports.truncate(limit);
            enviar(&mut escritura, &Event::ProfilerReportList(reports))?;
        }
        Request::AnalyzeProfilerHotspots { workspace_path } => {
            let ws = std::path::PathBuf::from(workspace_path);
            let (hotspots, suggestions) = crate::profiler::ProfilerEngine::global().analyze_aggregate(&ws);
            enviar(&mut escritura, &Event::ProfilerAnalysis { hotspots, suggestions })?;
        }
        Request::QueryLspStatus { workspace_path } => {
            let ws = std::path::PathBuf::from(workspace_path);
            let status = crate::lsp::LspServer::global().get_status(&ws);
            enviar(&mut escritura, &Event::LspStatus(status))?;
        }
        Request::GetLspConfig { editor, workspace_path } => {
            let ws = std::path::PathBuf::from(workspace_path);
            let (config_content, target_file) = crate::lsp::LspServer::global().generate_config(editor, &ws);
            enviar(&mut escritura, &Event::LspConfiguration {
                editor,
                config_content,
                target_file,
            })?;
        }
        Request::StartCollabSession { file_path, ticket_id, workspace_path } => {
            let ws = std::path::PathBuf::from(workspace_path);
            let status = crate::collab::CollabEngine::global().start_session(&ws, &file_path, ticket_id)?;
            enviar(&mut escritura, &Event::CollabSessionStatus(status))?;
        }
        Request::QueryCollabStatus { session_id, .. } => {
            if let Some(status) = crate::collab::CollabEngine::global().get_session(&session_id) {
                enviar(&mut escritura, &Event::CollabSessionStatus(status))?;
            } else {
                enviar(&mut escritura, &Event::Error(format!("sesión «{session_id}» no encontrada")))?;
            }
        }
        Request::StartDapSession { command, .. } => {
            let mut dap = crate::collab::DapServer::new("dap-sess-001".into(), command);
            dap.add_breakpoint("src/main.rs", 1);
            enviar(&mut escritura, &Event::DapSessionStatus(dap.to_status()))?;
        }
        Request::QueryDapStatus { session_id, .. } => {
            let dap = crate::collab::DapServer::new(session_id, "cargo test".into());
            enviar(&mut escritura, &Event::DapSessionStatus(dap.to_status()))?;
        }
        Request::QueryDesktopStatus => {
            let status = crate::desktop::DesktopManager::get_status();
            enviar(&mut escritura, &Event::DesktopStatus(status))?;
        }
        Request::ListDesktopHotkeys => {
            let hotkeys = crate::desktop::DesktopManager::get_hotkeys();
            enviar(&mut escritura, &Event::DesktopHotkeysList(hotkeys))?;
        }
        Request::StartDesktopSession { .. } => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let _ = crate::desktop::DesktopManager::sync_configuration(&cwd);
            let status = crate::desktop::DesktopManager::get_status();
            enviar(&mut escritura, &Event::DesktopStatus(status))?;
        }
        Request::QueryBarraTelemetry => {
            let telemetry = crate::barra::BarraManager::global().get_telemetry();
            enviar(&mut escritura, &Event::BarraTelemetryStatus(telemetry))?;
        }
        Request::EmitBarraAlert(alert) => {
            let res = crate::barra::BarraManager::global().emit_alert(alert.clone());
            if res.is_ok() {
                enviar(&mut escritura, &Event::BarraAlert(alert))?;
            } else {
                enviar(&mut escritura, &Event::Error("Error al registrar alerta en la barra".into()))?;
            }
        }
        Request::QueryBootStatus => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let status = crate::boot::BootEngine::global().status(&cwd);
            enviar(&mut escritura, &Event::BootStatus(status))?;
        }
        Request::RunBootPipeline { action, .. } => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let engine = crate::boot::BootEngine::global();
            match action.as_str() {
                "test" => match engine.test_boot(&cwd) {
                    Ok(out) => enviar(&mut escritura, &Event::BootResult { action, output: out, success: true })?,
                    Err(e) => enviar(&mut escritura, &Event::BootResult { action, output: e.to_string(), success: false })?,
                },
                "build" => match engine.build(&cwd) {
                    Ok(p) => enviar(&mut escritura, &Event::BootResult { action, output: format!("Imagen de disco generada: {}", p.display()), success: true })?,
                    Err(e) => enviar(&mut escritura, &Event::BootResult { action, output: e.to_string(), success: false })?,
                },
                _ => {
                    let st = engine.status(&cwd);
                    enviar(&mut escritura, &Event::BootStatus(st))?;
                }
            }
        }
        Request::ListPlugins => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let plugins_dir = crate::wasm::PluginManager::get_plugins_dir(&cwd);
            let summaries = crate::wasm::PluginManager::list_plugins(&plugins_dir);
            enviar(&mut escritura, &Event::PluginList(summaries))?;
        }
        Request::RunPlugin { plugin_name, action, params } => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let plugins_dir = crate::wasm::PluginManager::get_plugins_dir(&cwd);
            let res = crate::wasm::PluginManager::run_plugin(&plugins_dir, &plugin_name, &action, &params);
            enviar(&mut escritura, &Event::PluginResult(res))?;
        }
        Request::InstallPlugin { source_path } => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let plugins_dir = crate::wasm::PluginManager::get_plugins_dir(&cwd);
            let src = std::path::PathBuf::from(&source_path);
            match crate::wasm::PluginManager::install_plugin(&plugins_dir, &src) {
                Ok(summary) => enviar(&mut escritura, &Event::PluginList(vec![summary]))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::CaptureScreen { target, save_path } => {
            let p_opt = save_path.map(std::path::PathBuf::from);
            match crate::vision::VisionEngine::global().capture_screen(target.as_deref(), p_opt.as_deref()) {
                Ok(res) => enviar(&mut escritura, &Event::ScreenshotResult(res))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::InspectVisualQa { target, criteria } => {
            match crate::vision::VisionEngine::global().inspect_visual(&target, &criteria, None) {
                Ok(rep) => enviar(&mut escritura, &Event::VisualQAReport(rep))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::ListDisks => {
            match crate::installer::DiskManager::list_disks() {
                Ok(disks) => enviar(&mut escritura, &Event::DiskList(disks))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::InspectDisk { device } => {
            match crate::installer::DiskManager::inspect_disk(&device) {
                Ok(opt) => enviar(&mut escritura, &Event::DiskDetail(opt))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::PartitionDisk { device, clean_install, dry_run } => {
            match crate::installer::DiskManager::plan_partitioning(&device, clean_install) {
                Ok(plan) => {
                    if !dry_run {
                        let _ = crate::installer::DiskManager::apply_partitioning(&device, &plan, false);
                    }
                    enviar(&mut escritura, &Event::PartitionPlan(plan))?;
                }
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::InstallSystem(config) => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            match crate::installer::DeployEngine::deploy_system(&config, &cwd) {
                Ok(report) => enviar(&mut escritura, &Event::InstallReport(report))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::ProbeOperatingSystems { esp_mount } => {
            let esp = esp_mount.as_deref().map(std::path::Path::new).unwrap_or_else(|| std::path::Path::new("/boot/efi"));
            match crate::installer::BootloaderEngine::probe_operating_systems(esp) {
                Ok(entries) => enviar(&mut escritura, &Event::DetectedOperatingSystems(entries))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::InstallBootloader(config) => {
            match crate::installer::BootloaderEngine::install_bootloader(&config) {
                Ok(report) => enviar(&mut escritura, &Event::BootloaderReport(report))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::SpawnMicrovm(config) => {
            match crate::vm::MicrovmManager::spawn_vm(&ctx.state, &config) {
                Ok(instance) => enviar(&mut escritura, &Event::MicrovmList(vec![instance]))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::ExecMicrovm { vm_id, command } => {
            match crate::vm::MicrovmManager::exec_vm(&ctx.state, &vm_id, &command) {
                Ok(result) => enviar(&mut escritura, &Event::MicrovmResult(result))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::DestroyMicrovm { vm_id } => {
            match crate::vm::MicrovmManager::kill_vm(&ctx.state, &vm_id) {
                Ok(_) => enviar(&mut escritura, &Event::Note(format!("MicroVM «{vm_id}» destruida")) )?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::ListMicrovms => {
            match crate::vm::MicrovmManager::list_vms(&ctx.state) {
                Ok(list) => enviar(&mut escritura, &Event::MicrovmList(list))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::QueryMicrovmStatus => {
            match crate::vm::MicrovmManager::get_status(&ctx.state) {
                Ok(status) => enviar(&mut escritura, &Event::MicrovmStatus(status))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::InstallPackage { recipe_path_or_name, dry_run } => {
            match crate::pkg::PackageEngine::install(&ctx.state, &recipe_path_or_name, dry_run) {
                Ok(rep) => enviar(&mut escritura, &Event::PackageInstallReport(rep))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::RemovePackage { package_name } => {
            match crate::pkg::PackageEngine::remove(&ctx.state, &package_name) {
                Ok(rep) => enviar(&mut escritura, &Event::PackageInstallReport(rep))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::ListPackages => {
            match crate::pkg::PackageEngine::list(&ctx.state) {
                Ok(list) => enviar(&mut escritura, &Event::PackageList(list))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::RollbackPackage { target_generation } => {
            match crate::pkg::PackageEngine::rollback(&ctx.state, target_generation) {
                Ok(rep) => enviar(&mut escritura, &Event::PackageInstallReport(rep))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::VerifyPackages => {
            match crate::pkg::PackageEngine::verify(&ctx.state) {
                Ok((all_valid, verified_packages, details)) => {
                    enviar(&mut escritura, &Event::PackageVerificationResult { all_valid, verified_packages, details })?
                }
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::QueryPackageStoreStatus => {
            match crate::pkg::PackageEngine::status(&ctx.state) {
                Ok(st) => enviar(&mut escritura, &Event::PackageStoreStatus(st))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::StartAutopilot(config) => {
            match crate::autopilot::AutopilotEngine::start(&ctx.state, &ctx.workspace, config) {
                Ok(st) => enviar(&mut escritura, &Event::AutopilotStatus(st))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::StopAutopilot => {
            match crate::autopilot::AutopilotEngine::stop(&ctx.state, &ctx.workspace) {
                Ok(st) => enviar(&mut escritura, &Event::AutopilotStatus(st))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::GetAutopilotStatus => {
            match crate::autopilot::AutopilotEngine::status(&ctx.state, &ctx.workspace) {
                Ok(st) => enviar(&mut escritura, &Event::AutopilotStatus(st))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::ListAutopilotIncidents => {
            match crate::autopilot::AutopilotEngine::list_incidents(&ctx.state) {
                Ok(list) => enviar(&mut escritura, &Event::AutopilotIncidentsList(list))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::ScanAutopilot => {
            match crate::autopilot::AutopilotEngine::scan_workspace(&ctx.state, &ctx.workspace) {
                Ok(list) => enviar(&mut escritura, &Event::AutopilotIncidentsList(list))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::ResolveAutopilotIncident { incident_id, approve_and_merge } => {
            match crate::autopilot::AutopilotEngine::resolve_incident(&ctx.state, &ctx.workspace, &incident_id, approve_and_merge) {
                Ok(inc) => enviar(&mut escritura, &Event::AutopilotAlert(inc))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::StartWebConsole(config) => {
            match crate::web::WebEngine::start(&ctx.state, &ctx.workspace, config) {
                Ok(st) => enviar(&mut escritura, &Event::WebConsoleStatus(st))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::StopWebConsole => {
            match crate::web::WebEngine::stop(&ctx.state) {
                Ok(st) => enviar(&mut escritura, &Event::WebConsoleStatus(st))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::GetWebConsoleStatus => {
            match crate::web::WebEngine::status(&ctx.state) {
                Ok(st) => enviar(&mut escritura, &Event::WebConsoleStatus(st))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
        Request::GenerateWebToken { client_label, ttl_secs } => {
            match crate::web::WebEngine::generate_token(&ctx.state, client_label, ttl_secs) {
                Ok(session) => enviar(&mut escritura, &Event::WebTokenGenerated(session))?,
                Err(e) => enviar(&mut escritura, &Event::Error(e.to_string()))?,
            }
        }
    }
    Ok(())
}

// -------------------------------------------------------------- lado cliente

pub fn hay_demonio(ctx: &Ctx) -> bool {
    let ruta = ruta_socket(ctx);
    ruta.exists() && UnixStream::connect(&ruta).is_ok()
}

/// Manda una intención al demonio y dibuja lo que conteste.
///
/// Fíjate en que el dibujado es EL MISMO `Terminal` que usa el modo local:
/// no hay dos maneras de enseñar un plan, y por eso no pueden divergir.
pub fn intencion_remota(
    ruta: &Path,
    texto: &str,
    planificador: Option<&str>,
    seco: bool,
    asumir_si: bool,
) -> Result<()> {
    let flujo = UnixStream::connect(ruta)
        .with_context(|| format!("no pude conectar con el demonio en {}", ruta.display()))?;
    let mut lectura = BufReader::new(flujo.try_clone()?);
    let mut escritura = flujo;

    enviar(
        &mut escritura,
        &Request::Intent {
            text: texto.to_string(),
            planner: planificador.map(str::to_string),
            dry_run: seco,
        },
    )?;

    let mut pantalla = terminal::Terminal::new(asumir_si);

    while let Some(evento) = recibir::<Event>(&mut lectura)? {
        match evento {
            Event::Start { intent, planner } => {
                pantalla.inicio(&intent, &planner)?
            }
            Event::Note(t) => pantalla.nota(&t)?,
            Event::Proposal(p) => {
                let decision = pantalla.propone(&p)?;
                enviar(&mut escritura, &Request::Approval(decision))?;
            }
            Event::Output(t) => pantalla.salida(&t)?,
            Event::Result(r) => pantalla.resultado(&r)?,
            Event::GitStatus(status) => {
                pantalla.nota(&format!("git branch: {:?}", status.branch))?;
            }
            Event::NotGitRepo => {
                pantalla.nota("no es un repositorio Git")?;
            }
            Event::TicketList(tickets) => {
                pantalla.nota(&format!("tickets disponibles: {}", tickets.len()))?;
            }
            Event::TicketDetail(detalle) => {
                if let Some(t) = detalle {
                    pantalla.nota(&format!("ticket {}: {}", t.id, t.title))?;
                }
            }
            Event::PortsStatus(puertos) => {
                pantalla.nota(&format!("puertos en escucha: {}", puertos.len()))?;
            }
            Event::FlowStatus(task) => {
                if let Some(t) = task {
                    pantalla.nota(&format!("antFlow {}: {}", t.ticket_id, t.state.label()))?;
                }
            }
            Event::FlowList(tasks) => {
                pantalla.nota(&format!("tareas antFlow activas: {}", tasks.len()))?;
            }
            Event::FlowTransition { ticket_id, new_state, detail, .. } => {
                pantalla.nota(&format!("[antFlow {ticket_id}] ➔ {}: {detail}", new_state.label()))?;
            }
            Event::StructuredDiff(files) => {
                pantalla.nota(&format!("archivos con diff: {}", files.len()))?;
            }
            Event::NotificationList(notifs) => {
                pantalla.nota(&format!("notificaciones recibidas: {}", notifs.len()))?;
            }
            Event::NotificationResult { message, success, .. } => {
                if success {
                    pantalla.nota(&format!("✓ {message}"))?;
                } else {
                    pantalla.nota(&format!("✗ {message}"))?;
                }
            }
            Event::MeshStatus(status) => {
                pantalla.nota(&format!("antMesh local: {} (peers: {})", status.local_node.id, status.peers.len()))?;
            }
            Event::PairingTokenGenerated(tok) => {
                pantalla.nota(&format!("token de emparejamiento generado: {}", tok.token))?;
            }
            Event::PeerConnectionResult { address, success, message } => {
                if success {
                    pantalla.nota(&format!("✓ peer {address}: {message}"))?;
                } else {
                    pantalla.nota(&format!("✗ peer {address}: {message}"))?;
                }
            }
            Event::SwarmStatus(status) => {
                pantalla.nota(&format!("antOS Swarm: {} nodos ({} tareas activas)", status.nodes.len(), status.total_tasks))?;
            }
            Event::SwarmDispatchResult { ticket_id, role, assigned_node_id, success, message } => {
                if success {
                    pantalla.nota(&format!("✓ Swarm [{ticket_id}] rol {:?} ➔ {assigned_node_id}: {message}", role))?;
                } else {
                    pantalla.nota(&format!("✗ Swarm [{ticket_id}] rol {:?} ➔ {assigned_node_id}: {message}", role))?;
                }
            }
            Event::VfsList { virtual_path, entries } => {
                pantalla.nota(&format!("VFS {virtual_path}: {} entradas encontradas", entries.len()))?;
            }
            Event::VfsContent { virtual_path, content } => {
                pantalla.salida(&format!("{virtual_path}:\n{content}"))?;
            }
            Event::VfsResult { action, success, message } => {
                if success {
                    pantalla.nota(&format!("✓ VFS {action}: {message}"))?;
                } else {
                    pantalla.nota(&format!("✗ VFS {action}: {message}"))?;
                }
            }
            Event::VfsValidationResult(res) => {
                if res.is_valid {
                    pantalla.nota(&format!("✓ VFS Guard: «{}» es sintácticamente válido ({} líneas)", res.file_path, res.line_count))?;
                } else {
                    pantalla.nota(&format!("✗ VFS Guard: «{}» tiene {} errores sintácticos", res.file_path, res.errors.len()))?;
                }
            }
            Event::VfsGuardStatus(status) => {
                pantalla.nota(&format!("VFS Guard: {} escrituras interceptadas ({} rechazadas)", status.total_intercepted, status.total_rejected))?;
            }
            Event::EbpfStatus(status) => {
                let lsm_badge = if status.lsm_enabled { "Kernel LSM Activo" } else { "Emulación Espacio Usuario" };
                pantalla.nota(&format!("eBPF Sentinel [{lsm_badge}]: {} sondas, {} eventos, {} bloqueos",
                    status.active_probes.len(), status.total_events_captured, status.total_violations_blocked
                ))?;
            }
            Event::EbpfAuditLog(events) => {
                pantalla.nota(&format!("eBPF Audit: {} eventos capturados en el ring buffer", events.len()))?;
            }
            Event::EbpfResult { action, success, message } => {
                if success {
                    pantalla.nota(&format!("✓ eBPF {action}: {message}"))?;
                } else {
                    pantalla.nota(&format!("✗ eBPF {action}: {message}"))?;
                }
            }
            Event::ProfilerReport(report) => {
                let peak_mb = report.peak_memory_bytes as f64 / (1024.0 * 1024.0);
                pantalla.nota(&format!("✓ Profiler: «{}» en {} ms (Memoria pico: {:.2} MB RSS)", report.command, report.duration_ms, peak_mb))?;
            }
            Event::ProfilerReportList(reports) => {
                pantalla.nota(&format!("Profiler: {} reportes históricos disponibles", reports.len()))?;
            }
            Event::ProfilerAnalysis { hotspots, suggestions } => {
                pantalla.nota(&format!("Profiler: {} hotspots y {} recomendaciones formuladas", hotspots.len(), suggestions.len()))?;
            }
            Event::LspStatus(status) => {
                let state_str = if status.running { "Activo" } else { "En espera" };
                pantalla.nota(&format!("LSP: {state_str} ({}) con {} símbolos indexados", status.transport, status.indexed_symbols_count))?;
            }
            Event::LspConfiguration { editor, target_file, .. } => {
                pantalla.nota(&format!("LSP: configuración generada para {:?} ({target_file})", editor))?;
            }
            Event::CollabSessionStatus(status) => {
                pantalla.nota(&format!("Pair: sesión {} en {} (colaboradores: {})", status.session_id, status.file_path, status.collaborators.len()))?;
            }
            Event::DapSessionStatus(status) => {
                pantalla.nota(&format!("DAP: sesión {} en estado {} para «{}»", status.session_id, status.state, status.target_command))?;
            }
            Event::CollabResult { action, success, message } => {
                if success {
                    pantalla.nota(&format!("✓ Pair {action}: {message}"))?;
                } else {
                    pantalla.nota(&format!("✗ Pair {action}: {message}"))?;
                }
            }
            Event::DapResult { action, success, message } => {
                if success {
                    pantalla.nota(&format!("✓ DAP {action}: {message}"))?;
                } else {
                    pantalla.nota(&format!("✗ DAP {action}: {message}"))?;
                }
            }
            Event::DesktopStatus(status) => {
                let state_str = if status.running { "Activa" } else { "Detenida / Headless" };
                pantalla.nota(&format!("Escritorio antOS [{state_str}]: Compositor {} (Display: {:?})", status.compositor_name, status.wayland_display))?;
            }
            Event::DesktopHotkeysList(keys) => {
                pantalla.nota(&format!("Escritorio antOS: {} atajos globales registrados", keys.len()))?;
            }
            Event::BarraTelemetryStatus(t) => {
                let mb = t.profiler_rss_bytes as f64 / (1024.0 * 1024.0);
                pantalla.nota(&format!("Barra antOS: eBPF: {}, Profiler: {:.1} MB ({:.1}%), Mesh: {} nodos, Notificaciones: {}",
                    if t.ebpf_lsm_active { "LSM Activo" } else { "Auditoría" },
                    mb, t.profiler_cpu_percent, t.mesh_peers_count, t.active_notifications_count
                ))?;
            }
            Event::BarraAlert(alert) => {
                let urg = if alert.urgent { "URGENTE" } else { "INFO" };
                pantalla.nota(&format!("Alerta en Barra [{urg} - {}]: {}", alert.category, alert.message))?;
            }
            Event::BootStatus(st) => {
                let kb = st.kernel_elf_size_bytes / 1024;
                let mb = st.bios_image_size_bytes / (1024 * 1024);
                pantalla.nota(&format!("antOS Boot: Kernel ELF: {} KiB, BIOS IMG: {} MB, QEMU: {}",
                    kb, mb, if st.qemu_installed { "instalado" } else { "no disponible" }
                ))?;
            }
            Event::BootResult { action, output, success } => {
                let status = if success { "OK" } else { "ERROR" };
                pantalla.nota(&format!("antOS Boot [{action} - {status}]: {output}"))?;
            }
            Event::PluginList(list) => {
                if list.is_empty() {
                    pantalla.nota("No hay plugins WASM instalados en antOS.")?;
                } else {
                    pantalla.nota(&format!("antOS Plugins ({} activos):", list.len()))?;
                    for p in list {
                        pantalla.nota(&format!("  • {} v{} - {} (acciones: {})",
                            p.name, p.version, p.description, p.capabilities.join(", ")
                        ))?;
                    }
                }
            }
            Event::PluginResult(res) => {
                if res.success {
                    pantalla.nota(&format!("✓ Plugin [{}:{}] ejecutado con éxito ({} ciclos, {} KiB memoria):\n{}",
                        res.plugin, res.action, res.fuel_consumed, res.memory_allocated_bytes / 1024, res.output
                    ))?;
                } else {
                    let err = res.error.unwrap_or_else(|| "Error desconocido".into());
                    bail!("Fallo en plugin [{}:{}]: {}", res.plugin, res.action, err);
                }
            }
            Event::ScreenshotResult(cap) => {
                let ruta = cap.saved_path.unwrap_or_else(|| "en memoria".into());
                pantalla.nota(&format!("✓ Captura de pantalla «{}» ({}) [{}x{}, {} KiB]",
                    cap.target, ruta, cap.width, cap.height, cap.size_bytes / 1024
                ))?;
            }
            Event::VisualQAReport(rep) => {
                let status = if rep.pass { "APROBADO" } else { "RECHAZADO" };
                pantalla.nota(&format!("antOS Visual QA [{}] · {}:", rep.target, status))?;
                pantalla.nota(&format!("  {}", rep.summary))?;
                for f in rep.findings {
                    let sev = match f.severity.as_str() {
                        "critical" => "CRÍTICO",
                        "warning" => "ADVERTENCIA",
                        _ => "INFO",
                    };
                    pantalla.nota(&format!("  • [{sev}] {}: {}", f.category, f.description))?;
                    if let Some(coords) = f.coordinates {
                        pantalla.nota(&format!("    Coordenadas: {coords}"))?;
                    }
                    pantalla.nota(&format!("    Recomendación: {}", f.recommendation))?;
                }
            }
            Event::DiskList(disks) => {
                pantalla.nota(&format!("Dispositivos de almacenamiento detectados ({}):", disks.len()))?;
                for d in disks {
                    let gb = d.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                    pantalla.nota(&format!("  • {} ({:.1} GB, Bus: {}, Particiones: {})", d.path, gb, d.bus_type, d.partitions.len()))?;
                }
            }
            Event::DiskDetail(opt) => {
                if let Some(d) = opt {
                    let gb = d.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                    pantalla.nota(&format!("Dispositivo {}: {:.1} GB, Bus: {}, Tabla: {}", d.path, gb, d.bus_type, d.partition_table))?;
                } else {
                    pantalla.nota("Dispositivo no encontrado")?;
                }
            }
            Event::PartitionPlan(plan) => {
                pantalla.nota(&format!("Plan de particionado GPT para {}: ESP: {} MB, Raíz: {} MB",
                    plan.target_device,
                    plan.efi_partition_bytes / (1024 * 1024),
                    plan.root_partition_bytes / (1024 * 1024)
                ))?;
            }
            Event::InstallReport(rep) => {
                pantalla.nota(&format!("antOS Instalador · {}", rep.summary))?;
                pantalla.nota(&format!("  • Modo:          {}", rep.mode))?;
                pantalla.nota(&format!("  • Partición ESP: {}", rep.efi_partition))?;
                pantalla.nota(&format!("  • Partición /:   {}", rep.root_partition))?;
                for s in rep.steps {
                    pantalla.nota(&format!("  ✓ {}: {}", s.name, s.description))?;
                }
            }
            Event::DetectedOperatingSystems(entries) => {
                pantalla.nota(&format!("antOS Bootloader · Sistemas Operativos Detectados ({}):", entries.len()))?;
                for (i, os) in entries.iter().enumerate() {
                    pantalla.nota(&format!("  [{}] {} (Tipo: {}, EFI: {})", i + 1, os.name, os.os_type, os.efi_path))?;
                }
            }
            Event::BootloaderReport(rep) => {
                pantalla.nota(&format!("antOS Bootloader · {}", rep.summary))?;
                pantalla.nota(&format!("  • Punto ESP:       {}", rep.esp_path))?;
                pantalla.nota(&format!("  • Comando NVRAM:   {}", rep.efibootmgr_command))?;
                for e in &rep.entries_configured {
                    pantalla.nota(&format!("  ✓ {}", e))?;
                }
            }
            Event::MicrovmStatus(st) => {
                pantalla.nota(&format!("antOS MicroVM · Hipervisor: {} [KVM: {}]", st.hypervisor_engine, if st.kvm_available { "Sí" } else { "No" }))?;
                pantalla.nota(&format!("  • VMs activas:       {}", st.active_vms_count))?;
                pantalla.nota(&format!("  • Memoria asignada:  {} MB", st.total_memory_allocated_mb))?;
                pantalla.nota(&format!("  • Kernel:            {}", st.kernel_version))?;
            }
            Event::MicrovmList(vms) => {
                pantalla.nota(&format!("antOS MicroVM · Instancias activas ({}):", vms.len()))?;
                for v in vms {
                    pantalla.nota(&format!("  • [{}] PID {}, {} vCPUs, {} MB, vsock {}", v.id, v.pid, v.vcpus, v.memory_mb, v.vsock_port))?;
                }
            }
            Event::MicrovmResult(res) => {
                pantalla.nota(&format!("antOS MicroVM · Comando ejecutado en «{}» [Código: {}]:", res.vm_id, res.exit_code))?;
                if !res.stdout.is_empty() {
                    pantalla.nota(&format!("  {}", res.stdout.trim()))?;
                }
            }
            Event::PackageInstallReport(rep) => {
                let status_label = if rep.success { "OK" } else { "ERROR" };
                pantalla.nota(&format!("antpkg [{status_label}]: {}", rep.message))?;
                if !rep.binaries_linked.is_empty() {
                    pantalla.nota(&format!("  • Binarios enlazados: {}", rep.binaries_linked.join(", ")))?;
                }
                if !rep.store_path.is_empty() {
                    pantalla.nota(&format!("  • Prefijo en almacén: {}", rep.store_path))?;
                }
            }
            Event::PackageList(pkgs) => {
                if pkgs.is_empty() {
                    pantalla.nota("antpkg: No hay paquetes instalados en el perfil activo.")?;
                } else {
                    pantalla.nota(&format!("antpkg · Paquetes en perfil activo ({}):", pkgs.len()))?;
                    for p in pkgs {
                        let kb = p.installed_size_bytes / 1024;
                        pantalla.nota(&format!("  • {} v{} ({} KiB, gen {}) [bin: {}]",
                            p.name, p.version, kb, p.generation, p.binaries.join(", ")
                        ))?;
                    }
                }
            }
            Event::PackageStoreStatus(st) => {
                let mb = st.total_store_bytes as f64 / (1024.0 * 1024.0);
                pantalla.nota(&format!("antpkg Store: {:.2} MB en almacén, {} paquetes, gen activa: {} ({} generaciones)",
                    mb, st.total_packages, st.current_generation, st.generations_count
                ))?;
                pantalla.nota(&format!("  • Ruta de almacén: {}", st.store_path))?;
                pantalla.nota(&format!("  • Perfil actual:   {}", st.current_profile_path))?;
            }
            Event::PackageGenerationsList(gens) => {
                pantalla.nota(&format!("antpkg · Generaciones de perfil ({}):", gens.len()))?;
                for g in gens {
                    let active_mark = if g.active { " (activa)" } else { "" };
                    pantalla.nota(&format!("  • Gen {}{}: {} paquetes [{}]",
                        g.generation, active_mark, g.packages.len(), g.packages.join(", ")
                    ))?;
                }
            }
            Event::PackageVerificationResult { all_valid, verified_packages, details } => {
                let status_lbl = if all_valid { "INTEGRIDAD VERIFICADA" } else { "ADVERTENCIAS DE INTEGRIDAD" };
                pantalla.nota(&format!("antpkg Verificación · {} ({} paquetes comprobados):", status_lbl, verified_packages))?;
                for d in details {
                    pantalla.nota(&format!("  {d}"))?;
                }
            }
            Event::AutopilotStatus(st) => {
                let active_badge = if st.active { "ACTIVO (Vigilando)" } else { "DETENIDO" };
                pantalla.nota(&format!("antOS Autopilot · Estado: {active_badge}"))?;
                pantalla.nota(&format!("  • Espacio de trabajo: {}", st.workspace_path))?;
                pantalla.nota(&format!("  • Intervalo sondeo:   {}s", st.poll_interval_secs))?;
                pantalla.nota(&format!("  • Incidentes activos: {}", st.active_incidents_count))?;
                pantalla.nota(&format!("  • Total resueltos:    {}", st.resolved_incidents_count))?;
                if let Some(ts) = st.last_scan_timestamp {
                    pantalla.nota(&format!("  • Último escaneo:     {ts}"))?;
                }
            }
            Event::AutopilotIncidentsList(list) => {
                if list.is_empty() {
                    pantalla.nota("antOS Autopilot: No hay incidencias activas en el repositorio.")?;
                } else {
                    pantalla.nota(&format!("antOS Autopilot · Incidencias Registradas ({}):", list.len()))?;
                    for inc in list {
                        pantalla.nota(&format!("  • [{}] {} en «{}» [{}] — {}",
                            inc.id, inc.incident_type, inc.file_path, inc.status, inc.error_message
                        ))?;
                    }
                }
            }
            Event::AutopilotAlert(inc) => {
                pantalla.nota(&format!("antOS Autopilot · Alerta de Incidencia [{}] en «{}»:", inc.id, inc.file_path))?;
                pantalla.nota(&format!("  • Error:  {}", inc.error_message))?;
                pantalla.nota(&format!("  • Estado: {}", inc.status))?;
                if let Some(ref prop) = inc.fix_proposal {
                    pantalla.nota(&format!("  • Solución: {} (Rama: {})", prop.title, prop.branch))?;
                    if !prop.diff.is_empty() {
                        pantalla.nota(&format!("  • Diff:\n{}", prop.diff))?;
                    }
                }
            }
            Event::WebConsoleStatus(st) => {
                let status_badge = if st.running { "ACTIVO (En línea)" } else { "DETENIDO" };
                pantalla.nota(&format!("antOS Web Console · Estado: {status_badge}"))?;
                pantalla.nota(&format!("  • URL de Acceso:         {}", st.url))?;
                pantalla.nota(&format!("  • Clientes Conectados:   {}", st.connected_clients))?;
                pantalla.nota(&format!("  • Sesiones Activas:      {}", st.active_sessions_count))?;
            }
            Event::WebTokenGenerated(session) => {
                pantalla.nota("antOS Web Console · Token de Autenticación Criptográfico:")?;
                pantalla.nota(&format!("  • Token:     {}", session.token))?;
                pantalla.nota(&format!("  • Expira en: {}s", session.expires_at.saturating_sub(session.created_at)))?;
                if let Some(lbl) = session.client_label {
                    pantalla.nota(&format!("  • Cliente:   {lbl}"))?;
                }
            }
            Event::Error(m) => bail!("{m}"),
        }
    }
    Ok(())
}
