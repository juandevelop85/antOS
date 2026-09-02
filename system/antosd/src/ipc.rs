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

pub use antos_protocolo::{Evento, Peticion};

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
            &Evento::Inicio {
                intencion: intencion.to_string(),
                planificador: planificador.to_string(),
            },
        )
    }

    fn nota(&mut self, texto: &str) -> Result<()> {
        enviar(self.escritura, &Evento::Nota(texto.to_string()))
    }

    fn propone(&mut self, propuesta: &Propuesta) -> Result<bool> {
        // No se puede serializar una referencia prestada dentro del enum sin
        // clonar, así que se reconstruye. Es una vez por plan.
        let copia: Propuesta = serde_json::from_str(&serde_json::to_string(propuesta)?)?;
        enviar(self.escritura, &Evento::Propuesta(Box::new(copia)))?;

        match recibir::<Peticion>(self.lectura)? {
            Some(Peticion::Aprobacion(decision)) => Ok(decision),
            // Un cliente que se va sin contestar no aprueba nada. El silencio
            // nunca es un sí.
            _ => Ok(false),
        }
    }

    fn salida(&mut self, texto: &str) -> Result<()> {
        enviar(self.escritura, &Evento::Salida(texto.to_string()))
    }

    fn resultado(&mut self, resultado: &Resultado) -> Result<()> {
        let copia: Resultado = serde_json::from_str(&serde_json::to_string(resultado)?)?;
        enviar(self.escritura, &Evento::Resultado(copia))
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

    let Some(peticion) = recibir::<Peticion>(&mut lectura)? else {
        return Ok(());
    };

    match peticion {
        Peticion::Intencion { texto, planificador, seco } => {
            let planner = crate::pick_planner_por_nombre(planificador.as_deref())?;
            let mut con = PorSocket {
                escritura: &mut escritura,
                lectura: &mut lectura,
            };
            if let Err(e) = sesion::intencion(ctx, catalog, &texto, &*planner, seco, &mut con) {
                enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
            }
        }
        Peticion::Aprobacion(_) => {
            enviar(
                &mut escritura,
                &Evento::Error("una aprobación sin propuesta previa".into()),
            )?;
        }
        Peticion::ConsultarEstadoGit { workspace_path } => {
            match crate::git::GitAnalyzer::global().consultar_estado(Path::new(&workspace_path)) {
                Ok(Some(status)) => {
                    enviar(&mut escritura, &Evento::EstadoGit(status))?;
                }
                Ok(None) => {
                    enviar(&mut escritura, &Evento::NoEsRepoGit)?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
                }
            }
        }
        Peticion::ListarTickets { workspace_path } => {
            match crate::spec::SpecEngine::global().listar_tickets(Path::new(&workspace_path)) {
                Ok(tickets) => {
                    enviar(&mut escritura, &Evento::ListaTickets(tickets))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
                }
            }
        }
        Peticion::ObtenerTicket { workspace_path, ticket_id } => {
            match crate::spec::SpecEngine::global().obtener_ticket(Path::new(&workspace_path), &ticket_id) {
                Ok(detalle) => {
                    enviar(&mut escritura, &Evento::DetalleTicket(detalle))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
                }
            }
        }
        Peticion::DiagnosticarPuertos { port } => {
            match crate::net::diagnosticar_puertos(port) {
                Ok(puertos) => {
                    enviar(&mut escritura, &Evento::EstadoPuertos(puertos))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
                }
            }
        }
        Peticion::IniciarFlow { workspace_path, ticket_id } => {
            match crate::flow::FlowEngine::global().iniciar_tarea(
                Path::new(&workspace_path),
                &ctx.state,
                &ticket_id,
            ) {
                Ok(task) => {
                    enviar(&mut escritura, &Evento::EstadoFlow(Some(task)))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
                }
            }
        }
        Peticion::ConsultarFlow { ticket_id } => {
            let task = crate::flow::FlowEngine::global().consultar_tarea(&ticket_id);
            enviar(&mut escritura, &Evento::EstadoFlow(task))?;
        }
        Peticion::ListarFlows { .. } => {
            let tasks = crate::flow::FlowEngine::global().listar_tareas();
            enviar(&mut escritura, &Evento::ListaFlows(tasks))?;
        }
        Peticion::AprobarFlow { ticket_id, decision } => {
            match crate::flow::FlowEngine::global().aprobar_tarea(&ticket_id, decision) {
                Ok(task) => {
                    enviar(&mut escritura, &Evento::EstadoFlow(Some(task)))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
                }
            }
        }
        Peticion::ConsultarDiff { workspace_path, target } => {
            let ws = Path::new(&workspace_path);
            let git_out = std::process::Command::new("git")
                .current_dir(ws)
                .args(&["diff", target.as_deref().unwrap_or("HEAD")])
                .output();

            match git_out {
                Ok(out) if out.status.success() => {
                    let diff_str = String::from_utf8_lossy(&out.stdout);
                    let files = crate::diff_view::DiffEngine::parse_unified_diff(&diff_str);
                    enviar(&mut escritura, &Evento::DiffEstructurado(files))?;
                }
                _ => {
                    enviar(&mut escritura, &Evento::DiffEstructurado(Vec::new()))?;
                }
            }
        }
        Peticion::ListarNotificaciones { workspace_path } => {
            let ws = Path::new(&workspace_path);
            let notifs = crate::notification::NotificationEngine::global().list(ws).unwrap_or_default();
            enviar(&mut escritura, &Evento::ListaNotificaciones(notifs))?;
        }
        Peticion::AccionNotificacion { workspace_path, notification_id, action } => {
            let ws = Path::new(&workspace_path);
            match crate::notification::NotificationEngine::global().handle_action(ws, &notification_id, action) {
                Ok((success, message)) => {
                    enviar(&mut escritura, &Evento::ResultadoNotificacion {
                        id: notification_id,
                        success,
                        message,
                    })?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::ResultadoNotificacion {
                        id: notification_id,
                        success: false,
                        message: format!("{e:#}"),
                    })?;
                }
            }
        }
        Peticion::ConsultarMesh { workspace_path } => {
            let ws = Path::new(&workspace_path);
            match crate::mesh::MeshEngine::global().status(ws) {
                Ok(status) => {
                    enviar(&mut escritura, &Evento::EstadoMesh(status))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
                }
            }
        }
        Peticion::ConectarPeer { workspace_path, address } => {
            let ws = Path::new(&workspace_path);
            match crate::mesh::MeshEngine::global().connect_peer(ws, &address) {
                Ok(peer) => {
                    enviar(&mut escritura, &Evento::ResultadoConexionPeer {
                        address: peer.address,
                        success: true,
                        message: format!("conectado con éxito al peer {} ({}ms)", peer.id, peer.latency_ms),
                    })?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::ResultadoConexionPeer {
                        address,
                        success: false,
                        message: format!("{e:#}"),
                    })?;
                }
            }
        }
        Peticion::GenerarTokenEmparejamiento { workspace_path } => {
            let ws = Path::new(&workspace_path);
            match crate::mesh::MeshEngine::global().generate_pairing_token(ws) {
                Ok(token) => {
                    enviar(&mut escritura, &Evento::TokenEmparejamientoGenerado(token))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
                }
            }
        }
        Peticion::ConsultarSwarm { workspace_path } => {
            let ws = Path::new(&workspace_path);
            match crate::distributed::SwarmEngine::global().status(ws) {
                Ok(status) => {
                    enviar(&mut escritura, &Evento::EstadoSwarm(status))?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
                }
            }
        }
        Peticion::DespacharRolRemoto { workspace_path, ticket_id, role, node_id } => {
            let ws = Path::new(&workspace_path);
            match crate::distributed::SwarmEngine::global().dispatch_remote_role(ws, &ticket_id, role, node_id.as_deref()) {
                Ok(task) => {
                    enviar(&mut escritura, &Evento::ResultadoDespachoSwarm {
                        ticket_id,
                        role,
                        assigned_node_id: task.assigned_node_id,
                        success: true,
                        message: format!("tarea {} despachada con éxito en nodo {}", task.task_id, task.status),
                    })?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::ResultadoDespachoSwarm {
                        ticket_id,
                        role,
                        assigned_node_id: node_id.unwrap_or_else(|| "unknown".into()),
                        success: false,
                        message: format!("{e:#}"),
                    })?;
                }
            }
        }
        Peticion::ConsultarVfs { workspace_path, virtual_path } => {
            let ws = Path::new(&workspace_path);
            let engine = crate::vfs::VfsEngine::global();
            if virtual_path.ends_with('/') || virtual_path == "/antfs" || virtual_path == "/antfs/symbols" || virtual_path.starts_with("/antfs/symbols/") && !virtual_path.split('/').skip(3).any(|p| !p.is_empty()) {
                match engine.list_dir(ws, &virtual_path) {
                    Ok(entries) => {
                        enviar(&mut escritura, &Evento::ListadoVfs { virtual_path, entries })?;
                    }
                    Err(e) => {
                        enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
                    }
                }
            } else {
                match engine.read_path(ws, &virtual_path) {
                    Ok(content) => {
                        enviar(&mut escritura, &Evento::ContenidoVfs { virtual_path, content })?;
                    }
                    Err(e) => {
                        enviar(&mut escritura, &Evento::Error(format!("{e:#}")))?;
                    }
                }
            }
        }
        Peticion::MontarVfs { workspace_path, mount_point } => {
            let ws = Path::new(&workspace_path);
            match crate::vfs::VfsEngine::global().mount(ws, mount_point.as_deref()) {
                Ok(path) => {
                    enviar(&mut escritura, &Evento::ResultadoVfs {
                        action: "mount".into(),
                        success: true,
                        message: format!("montado en {}", path.display()),
                    })?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::ResultadoVfs {
                        action: "mount".into(),
                        success: false,
                        message: format!("{e:#}"),
                    })?;
                }
            }
        }
        Peticion::DesmontarVfs { workspace_path, mount_point } => {
            let ws = Path::new(&workspace_path);
            match crate::vfs::VfsEngine::global().unmount(ws, mount_point.as_deref()) {
                Ok(_) => {
                    enviar(&mut escritura, &Evento::ResultadoVfs {
                        action: "unmount".into(),
                        success: true,
                        message: "desmontado correctamente".into(),
                    })?;
                }
                Err(e) => {
                    enviar(&mut escritura, &Evento::ResultadoVfs {
                        action: "unmount".into(),
                        success: false,
                        message: format!("{e:#}"),
                    })?;
                }
            }
        }
        Peticion::ValidarEscrituraVfs { file_path, content } => {
            let res = crate::vfs_guard::VfsGuardEngine::global().intercept_write(&file_path, &content)?;
            enviar(&mut escritura, &Evento::ResultadoValidacionVfs(res))?;
        }
        Peticion::ConsultarGuardVfs { .. } => {
            let status = crate::vfs_guard::VfsGuardEngine::global().status()?;
            enviar(&mut escritura, &Evento::EstadoGuardVfs(status))?;
        }
        Peticion::ConsultarEbpfStatus { .. } => {
            let status = crate::ebpf::EbpfSentinelEngine::global().status()?;
            enviar(&mut escritura, &Evento::EstadoEbpf(status))?;
        }
        Peticion::ConsultarEbpfAuditLog { limit, .. } => {
            let events = crate::ebpf::EbpfSentinelEngine::global().get_audit_log(limit);
            enviar(&mut escritura, &Evento::AuditLogEbpf(events))?;
        }
        Peticion::SimularViolacionEbpf { hook, target_resource, .. } => {
            let event = crate::ebpf::EbpfSentinelEngine::global().simulate_violation(hook, &target_resource);
            enviar(&mut escritura, &Evento::ResultadoEbpf {
                action: format!("{:?}", hook),
                success: true,
                message: format!("evento {} generado con éxito (acción: {:?})", event.id, event.action_taken),
            })?;
        }
        Peticion::EjecutarProfiler { workspace_path, command } => {
            let ws = std::path::PathBuf::from(workspace_path);
            let report = crate::profiler::ProfilerEngine::global().run_and_profile(&ws, &command)?;
            enviar(&mut escritura, &Evento::ReporteProfiler(report))?;
        }
        Peticion::ConsultarProfilerReportes { workspace_path, limit } => {
            let ws = std::path::PathBuf::from(workspace_path);
            let mut reports = crate::profiler::ProfilerEngine::global().load_reports(&ws);
            reports.truncate(limit);
            enviar(&mut escritura, &Evento::ListaReportesProfiler(reports))?;
        }
        Peticion::AnalizarProfilerHotspots { workspace_path } => {
            let ws = std::path::PathBuf::from(workspace_path);
            let (hotspots, suggestions) = crate::profiler::ProfilerEngine::global().analyze_aggregate(&ws);
            enviar(&mut escritura, &Evento::AnalisisProfiler { hotspots, suggestions })?;
        }
        Peticion::ConsultarLspStatus { workspace_path } => {
            let ws = std::path::PathBuf::from(workspace_path);
            let status = crate::lsp::LspServer::global().get_status(&ws);
            enviar(&mut escritura, &Evento::EstadoLsp(status))?;
        }
        Peticion::ObtenerLspConfig { editor, workspace_path } => {
            let ws = std::path::PathBuf::from(workspace_path);
            let (config_content, target_file) = crate::lsp::LspServer::global().generate_config(editor, &ws);
            enviar(&mut escritura, &Evento::ConfiguracionLsp {
                editor,
                config_content,
                target_file,
            })?;
        }
        Peticion::IniciarCollabSession { file_path, ticket_id, workspace_path } => {
            let ws = std::path::PathBuf::from(workspace_path);
            let status = crate::collab::CollabEngine::global().start_session(&ws, &file_path, ticket_id)?;
            enviar(&mut escritura, &Evento::EstadoCollabSession(status))?;
        }
        Peticion::ConsultarCollabStatus { session_id, .. } => {
            if let Some(status) = crate::collab::CollabEngine::global().get_session(&session_id) {
                enviar(&mut escritura, &Evento::EstadoCollabSession(status))?;
            } else {
                enviar(&mut escritura, &Evento::Error(format!("sesión «{session_id}» no encontrada")))?;
            }
        }
        Peticion::IniciarDapSession { command, .. } => {
            let mut dap = crate::collab::DapServer::new("dap-sess-001".into(), command);
            dap.add_breakpoint("src/main.rs", 1);
            enviar(&mut escritura, &Evento::EstadoDapSession(dap.to_status()))?;
        }
        Peticion::ConsultarDapStatus { session_id, .. } => {
            let dap = crate::collab::DapServer::new(session_id, "cargo test".into());
            enviar(&mut escritura, &Evento::EstadoDapSession(dap.to_status()))?;
        }
        Peticion::ConsultarDesktopStatus => {
            let status = crate::desktop::DesktopManager::get_status();
            enviar(&mut escritura, &Evento::EstadoDesktop(status))?;
        }
        Peticion::ListarDesktopHotkeys => {
            let hotkeys = crate::desktop::DesktopManager::get_hotkeys();
            enviar(&mut escritura, &Evento::ListaDesktopHotkeys(hotkeys))?;
        }
        Peticion::IniciarDesktopSession { .. } => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let _ = crate::desktop::DesktopManager::sync_configuration(&cwd);
            let status = crate::desktop::DesktopManager::get_status();
            enviar(&mut escritura, &Evento::EstadoDesktop(status))?;
        }
        Peticion::ConsultarBarraTelemetry => {
            let telemetry = crate::barra::BarraManager::global().get_telemetry();
            enviar(&mut escritura, &Evento::EstadoBarraTelemetry(telemetry))?;
        }
        Peticion::EmitirBarraAlert(alert) => {
            let res = crate::barra::BarraManager::global().emit_alert(alert.clone());
            if res.is_ok() {
                enviar(&mut escritura, &Evento::AlertaBarra(alert))?;
            } else {
                enviar(&mut escritura, &Evento::Error("Error al registrar alerta en la barra".into()))?;
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
        &Peticion::Intencion {
            texto: texto.to_string(),
            planificador: planificador.map(str::to_string),
            seco,
        },
    )?;

    let mut pantalla = terminal::Terminal::new(asumir_si);

    while let Some(evento) = recibir::<Evento>(&mut lectura)? {
        match evento {
            Evento::Inicio { intencion, planificador } => {
                pantalla.inicio(&intencion, &planificador)?
            }
            Evento::Nota(t) => pantalla.nota(&t)?,
            Evento::Propuesta(p) => {
                let decision = pantalla.propone(&p)?;
                enviar(&mut escritura, &Peticion::Aprobacion(decision))?;
            }
            Evento::Salida(t) => pantalla.salida(&t)?,
            Evento::Resultado(r) => pantalla.resultado(&r)?,
            Evento::EstadoGit(status) => {
                pantalla.nota(&format!("git branch: {:?}", status.rama))?;
            }
            Evento::NoEsRepoGit => {
                pantalla.nota("no es un repositorio Git")?;
            }
            Evento::ListaTickets(tickets) => {
                pantalla.nota(&format!("tickets disponibles: {}", tickets.len()))?;
            }
            Evento::DetalleTicket(detalle) => {
                if let Some(t) = detalle {
                    pantalla.nota(&format!("ticket {}: {}", t.id, t.titulo))?;
                }
            }
            Evento::EstadoPuertos(puertos) => {
                pantalla.nota(&format!("puertos en escucha: {}", puertos.len()))?;
            }
            Evento::EstadoFlow(task) => {
                if let Some(t) = task {
                    pantalla.nota(&format!("antFlow {}: {}", t.ticket_id, t.estado.etiqueta()))?;
                }
            }
            Evento::ListaFlows(tasks) => {
                pantalla.nota(&format!("tareas antFlow activas: {}", tasks.len()))?;
            }
            Evento::TransicionFlow { ticket_id, estado_nuevo, detalle, .. } => {
                pantalla.nota(&format!("[antFlow {ticket_id}] ➔ {}: {detalle}", estado_nuevo.etiqueta()))?;
            }
            Evento::DiffEstructurado(files) => {
                pantalla.nota(&format!("archivos con diff: {}", files.len()))?;
            }
            Evento::ListaNotificaciones(notifs) => {
                pantalla.nota(&format!("notificaciones recibidas: {}", notifs.len()))?;
            }
            Evento::ResultadoNotificacion { message, success, .. } => {
                if success {
                    pantalla.nota(&format!("✓ {message}"))?;
                } else {
                    pantalla.nota(&format!("✗ {message}"))?;
                }
            }
            Evento::EstadoMesh(status) => {
                pantalla.nota(&format!("antMesh local: {} (peers: {})", status.local_node.id, status.peers.len()))?;
            }
            Evento::TokenEmparejamientoGenerado(tok) => {
                pantalla.nota(&format!("token de emparejamiento generado: {}", tok.token))?;
            }
            Evento::ResultadoConexionPeer { address, success, message } => {
                if success {
                    pantalla.nota(&format!("✓ peer {address}: {message}"))?;
                } else {
                    pantalla.nota(&format!("✗ peer {address}: {message}"))?;
                }
            }
            Evento::EstadoSwarm(status) => {
                pantalla.nota(&format!("antOS Swarm: {} nodos ({} tareas activas)", status.nodes.len(), status.total_tasks))?;
            }
            Evento::ResultadoDespachoSwarm { ticket_id, role, assigned_node_id, success, message } => {
                if success {
                    pantalla.nota(&format!("✓ Swarm [{ticket_id}] rol {:?} ➔ {assigned_node_id}: {message}", role))?;
                } else {
                    pantalla.nota(&format!("✗ Swarm [{ticket_id}] rol {:?} ➔ {assigned_node_id}: {message}", role))?;
                }
            }
            Evento::ListadoVfs { virtual_path, entries } => {
                pantalla.nota(&format!("VFS {virtual_path}: {} entradas encontradas", entries.len()))?;
            }
            Evento::ContenidoVfs { virtual_path, content } => {
                pantalla.salida(&format!("{virtual_path}:\n{content}"))?;
            }
            Evento::ResultadoVfs { action, success, message } => {
                if success {
                    pantalla.nota(&format!("✓ VFS {action}: {message}"))?;
                } else {
                    pantalla.nota(&format!("✗ VFS {action}: {message}"))?;
                }
            }
            Evento::ResultadoValidacionVfs(res) => {
                if res.is_valid {
                    pantalla.nota(&format!("✓ VFS Guard: «{}» es sintácticamente válido ({} líneas)", res.file_path, res.line_count))?;
                } else {
                    pantalla.nota(&format!("✗ VFS Guard: «{}» tiene {} errores sintácticos", res.file_path, res.errors.len()))?;
                }
            }
            Evento::EstadoGuardVfs(status) => {
                pantalla.nota(&format!("VFS Guard: {} escrituras interceptadas ({} rechazadas)", status.total_intercepted, status.total_rejected))?;
            }
            Evento::EstadoEbpf(status) => {
                let lsm_badge = if status.lsm_enabled { "Kernel LSM Activo" } else { "Emulación Espacio Usuario" };
                pantalla.nota(&format!("eBPF Sentinel [{lsm_badge}]: {} sondas, {} eventos, {} bloqueos",
                    status.active_probes.len(), status.total_events_captured, status.total_violations_blocked
                ))?;
            }
            Evento::AuditLogEbpf(events) => {
                pantalla.nota(&format!("eBPF Audit: {} eventos capturados en el ring buffer", events.len()))?;
            }
            Evento::ResultadoEbpf { action, success, message } => {
                if success {
                    pantalla.nota(&format!("✓ eBPF {action}: {message}"))?;
                } else {
                    pantalla.nota(&format!("✗ eBPF {action}: {message}"))?;
                }
            }
            Evento::ReporteProfiler(report) => {
                let peak_mb = report.peak_memory_bytes as f64 / (1024.0 * 1024.0);
                pantalla.nota(&format!("✓ Profiler: «{}» en {} ms (Memoria pico: {:.2} MB RSS)", report.command, report.duration_ms, peak_mb))?;
            }
            Evento::ListaReportesProfiler(reports) => {
                pantalla.nota(&format!("Profiler: {} reportes históricos disponibles", reports.len()))?;
            }
            Evento::AnalisisProfiler { hotspots, suggestions } => {
                pantalla.nota(&format!("Profiler: {} hotspots y {} recomendaciones formuladas", hotspots.len(), suggestions.len()))?;
            }
            Evento::EstadoLsp(status) => {
                let state_str = if status.running { "Activo" } else { "En espera" };
                pantalla.nota(&format!("LSP: {state_str} ({}) con {} símbolos indexados", status.transport, status.indexed_symbols_count))?;
            }
            Evento::ConfiguracionLsp { editor, target_file, .. } => {
                pantalla.nota(&format!("LSP: configuración generada para {:?} ({target_file})", editor))?;
            }
            Evento::EstadoCollabSession(status) => {
                pantalla.nota(&format!("Pair: sesión {} en {} (colaboradores: {})", status.session_id, status.file_path, status.collaborators.len()))?;
            }
            Evento::EstadoDapSession(status) => {
                pantalla.nota(&format!("DAP: sesión {} en estado {} para «{}»", status.session_id, status.state, status.target_command))?;
            }
            Evento::ResultadoCollab { action, success, message } => {
                if success {
                    pantalla.nota(&format!("✓ Pair {action}: {message}"))?;
                } else {
                    pantalla.nota(&format!("✗ Pair {action}: {message}"))?;
                }
            }
            Evento::ResultadoDap { action, success, message } => {
                if success {
                    pantalla.nota(&format!("✓ DAP {action}: {message}"))?;
                } else {
                    pantalla.nota(&format!("✗ DAP {action}: {message}"))?;
                }
            }
            Evento::EstadoDesktop(status) => {
                let state_str = if status.running { "Activa" } else { "Detenida / Headless" };
                pantalla.nota(&format!("Escritorio antOS [{state_str}]: Compositor {} (Display: {:?})", status.compositor_name, status.wayland_display))?;
            }
            Evento::ListaDesktopHotkeys(keys) => {
                pantalla.nota(&format!("Escritorio antOS: {} atajos globales registrados", keys.len()))?;
            }
            Evento::EstadoBarraTelemetry(t) => {
                let mb = t.profiler_rss_bytes as f64 / (1024.0 * 1024.0);
                pantalla.nota(&format!("Barra antOS: eBPF: {}, Profiler: {:.1} MB ({:.1}%), Mesh: {} nodos, Notificaciones: {}",
                    if t.ebpf_lsm_active { "LSM Activo" } else { "Auditoría" },
                    mb, t.profiler_cpu_percent, t.mesh_peers_count, t.active_notifications_count
                ))?;
            }
            Evento::AlertaBarra(alert) => {
                let urg = if alert.urgent { "URGENTE" } else { "INFO" };
                pantalla.nota(&format!("Alerta en Barra [{urg} - {}]: {}", alert.category, alert.message))?;
            }
            Evento::Error(m) => bail!("{m}"),
        }
    }
    Ok(())
}
