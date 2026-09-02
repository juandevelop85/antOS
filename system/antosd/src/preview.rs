//! Renderiza el diff que se le enseña al usuario antes de ejecutar.

use crate::ctx::Ctx;
use crate::exec::Change;

pub use antos_protocolo::Line;

const MAX_DIFF_LINES: usize = 16;

pub fn render(ctx: &Ctx, changes: &[Change]) -> Vec<Line> {
    let mut out = Vec::new();
    // El diff de un paso se compara con lo que dejó el paso anterior, no con
    // el disco de partida. Si no, tres escrituras al mismo fichero se
    // mostrarían las tres como si partieran de cero.
    let mut pendiente = crate::exec::Pendiente::default();

    for change in changes {
        match change {
            Change::Read { path } => {
                out.push(Line::Info(format!("lee       {}", ctx.display(path))));
            }
            Change::Mkdir { path } => {
                out.push(Line::Info(format!("crea dir  {}", ctx.display(path))));
            }
            Change::Delete { path } => {
                let detail = if path.is_dir() {
                    format!(" ({} elementos)", count_entries(path))
                } else {
                    String::new()
                };
                out.push(Line::Info(format!("borra     {}{detail}", ctx.display(path))));
                out.push(Line::Del(format!("  {}", ctx.display(path))));
            }
            Change::Write { path, content } => {
                let ya_previsto = pendiente.leer(path);
                let existia = ya_previsto.is_some() || path.exists();
                let old = ya_previsto
                    .unwrap_or_else(|| std::fs::read_to_string(path).unwrap_or_default());
                let verb = if existia { "modifica" } else { "crea    " };
                out.push(Line::Info(format!("{verb}  {}", ctx.display(path))));
                for (marker, text) in diff(&old, content) {
                    match marker {
                        '-' => out.push(Line::Del(format!("  {text}"))),
                        _ => out.push(Line::Add(format!("  {text}"))),
                    }
                }
            }
            Change::GitStatus { repo_root } => {
                out.push(Line::Info(format!("consulta estado git en {}", ctx.display(repo_root))));
            }
            Change::GitCommit { repo_root, commit_msg } => {
                out.push(Line::Info(format!("crea commit en {}", ctx.display(repo_root))));
                out.push(Line::Add(format!("  + {commit_msg}")));
            }
            Change::GitBranch { repo_root, branch_name, base } => {
                let base_info = base.as_deref().map(|b| format!(" (base: {b})")).unwrap_or_default();
                out.push(Line::Info(format!(
                    "crea/cambia a rama {branch_name}{base_info} en {}",
                    ctx.display(repo_root)
                )));
            }
            Change::GitWorktreeCreate { target_path, branch_name, base, .. } => {
                out.push(Line::Info(format!(
                    "crea worktree efímero en {} (rama: {branch_name}, base: {base})",
                    ctx.display(target_path)
                )));
            }
            Change::GitWorktreeCleanup { target_path, force, .. } => {
                let force_info = if *force { " (forzado)" } else { "" };
                out.push(Line::Info(format!(
                    "elimina worktree efímero en {}{force_info}",
                    ctx.display(target_path)
                )));
                out.push(Line::Del(format!("  {}", ctx.display(target_path))));
            }
            Change::GitWorktreeMerge { branch_name, target_branch, message, .. } => {
                let msg_info = message.as_deref().map(|m| format!(" «{m}»")).unwrap_or_default();
                out.push(Line::Info(format!(
                    "fusiona rama {branch_name} a {target_branch}{msg_info}"
                )));
            }
            Change::PortStatus { port } => {
                let p_info = port.map(|p| format!(" {p}")).unwrap_or_default();
                out.push(Line::Info(format!("diagnostica puertos TCP{p_info}")));
            }
            Change::PortKill { port, force } => {
                let force_info = if *force { " (SIGKILL forzado)" } else { " (SIGTERM)" };
                out.push(Line::Info(format!(
                    "termina procesos ocupando el puerto {port}{force_info}"
                )));
                out.push(Line::Del(format!("  liberar puerto :{port}")));
            }
            Change::ServiceUp { service, port, db_name, .. } => {
                let port_str = port.map(|p| format!(" en puerto {p}")).unwrap_or_default();
                let db_str = db_name.as_deref().map(|d| format!(" (db: {d})")).unwrap_or_default();
                out.push(Line::Info(format!(
                    "aprovisiona y arranca servicio {service}{port_str}{db_str}"
                )));
                out.push(Line::Add(format!("  + iniciar demonio de {service} en background")));
                out.push(Line::Add(format!("  + inyectar variable de conexión en .env")));
            }
            Change::ServiceDown { service, .. } => {
                out.push(Line::Info(format!("detiene y limpia servicio {service}")));
                out.push(Line::Del(format!("  - detener proceso de {service}")));
            }
            Change::ServiceStatus { service, .. } => {
                let svc_info = service.as_deref().map(|s| format!(" ({s})")).unwrap_or_default();
                out.push(Line::Info(format!("consulta estado de servicios locales{svc_info}")));
            }
            Change::SecretGrant { secret, minutes, reason, .. } => {
                let r_str = reason.as_deref().map(|r| format!(" (motivo: «{r}»)")).unwrap_or_default();
                out.push(Line::Info(format!("concede acceso temporal a {secret} por {minutes} min{r_str}")));
                out.push(Line::Add(format!("  + habilitar permiso {secret} en $STATE/grants.json")));
            }
            Change::SecretRevoke { secret, .. } => {
                out.push(Line::Info(format!("revoca acceso a {secret}")));
                out.push(Line::Del(format!("  - revocar permiso {secret} en $STATE/grants.json")));
            }
            Change::SecretList { .. } => {
                out.push(Line::Info("lista secretos y concesiones activas de la bóveda".into()));
            }
            Change::SecretSet { key, .. } => {
                out.push(Line::Info(format!("almacena clave {key} en bóveda de secretos")));
                out.push(Line::Add(format!("  + guardar {key} en $STATE/vault.json (0600)")));
            }
            Change::SecretRead { key, .. } => {
                out.push(Line::Info(format!("lee secreto {key} de la bóveda")));
            }
            Change::TicketCreate { ticket_id, title, phase, .. } => {
                let f_str = phase.as_deref().map(|f| format!(" [{f}]")).unwrap_or_default();
                out.push(Line::Info(format!("crea especificación / ticket {ticket_id}: «{title}»{f_str}")));
                out.push(Line::Add(format!("  + crear archivo docs/tickets/{ticket_id}-*.md")));
                out.push(Line::Add(format!("  + actualizar índice maestro docs/tickets/README.md")));
            }
            Change::TicketUpdateStatus { ticket_id, status, .. } => {
                out.push(Line::Info(format!("actualiza estado del ticket {ticket_id} a «{status}»")));
            }
            Change::TicketList { .. } => {
                out.push(Line::Info("lista tickets y catálogo de especificaciones del proyecto".into()));
            }
            Change::MemoryIndex { .. } => {
                out.push(Line::Info("indexa código y especificaciones en la memoria semántica vectorial".into()));
                out.push(Line::Add("  + generar vectores y grafo en .antos/memory.json".into()));
            }
            Change::MemorySearch { query, .. } => {
                out.push(Line::Info(format!("búsqueda semántica por similitud coseno para «{query}»")));
            }
            Change::MemoryGraph { target, .. } => {
                let t_str = target.as_deref().unwrap_or("raíz");
                out.push(Line::Info(format!("explora el grafo de dependencias y contexto para «{t_str}»")));
            }
            Change::EnvProfileInit { profile, create_devbox, create_flake, .. } => {
                let p = profile.as_deref().unwrap_or("auto");
                out.push(Line::Info(format!("inicializa perfil declarativo de desarrollo: «{p}»")));
                out.push(Line::Add("  + generar .antos/env.toml".into()));
                if *create_devbox {
                    out.push(Line::Add("  + generar devbox.json".into()));
                }
                if *create_flake {
                    out.push(Line::Add("  + generar flake.nix".into()));
                }
            }
            Change::EnvProfileSync { .. } => {
                out.push(Line::Info("sincroniza y verifica toolchains declaradas en el workspace".into()));
            }
            Change::EnvProfileStatus { .. } => {
                out.push(Line::Info("consulta el estado del perfil de entorno del proyecto".into()));
            }
            Change::QuotaStatus { .. } => {
                out.push(Line::Info("consulta cuotas y límites de recursos para sandboxes".into()));
            }
            Change::QuotaSet { quota, .. } => {
                out.push(Line::Info(format!(
                    "establece límites de sandbox: {}s timeout, {}MB memoria, {}% CPU",
                    quota.timeout_secs, quota.max_memory_mb, quota.cpu_quota_percent
                )));
                out.push(Line::Add("  + actualizar configuración en .antos/quota.toml".into()));
            }
            Change::UiDiffViewer { target, .. } => {
                let t_str = target.as_deref().unwrap_or("HEAD");
                out.push(Line::Info(format!("abre el visor interactivo de diffs para «{t_str}»")));
            }
            Change::UiTerminal { command } => {
                let cmd_str = command.as_deref().unwrap_or("shell");
                out.push(Line::Info(format!("abre la consola terminal interactiva VTE: {cmd_str}")));
            }
            Change::NotifyList { .. } => {
                out.push(Line::Info("consulta la bandeja de notificaciones y aprobaciones de agentes".into()));
            }
            Change::NotifyAction { notification_id, action, .. } => {
                let act_str = match action {
                    antos_protocolo::NotificationAction::Approve => "aprobar y fusionar",
                    antos_protocolo::NotificationAction::Reject => "rechazar y rollback",
                    antos_protocolo::NotificationAction::Dismiss => "descartar",
                    antos_protocolo::NotificationAction::ViewDiff => "inspeccionar diff",
                };
                out.push(Line::Info(format!("ejecuta «{act_str}» sobre la notificación «{notification_id}»")));
            }
            Change::MeshStatus { .. } => {
                out.push(Line::Info("consulta el estado del nodo y los peers en la malla P2P antMesh".into()));
            }
            Change::MeshConnect { address, .. } => {
                out.push(Line::Info(format!("conecta al nodo peer remoto «{address}» mediante QUIC")));
            }
            Change::MeshPair { .. } => {
                out.push(Line::Info("genera un token criptográfico de emparejamiento con 15m de expiración".into()));
            }
            Change::SwarmStatus { .. } => {
                out.push(Line::Info("consulta la matriz de distribución de agentes y tareas en el Swarm".into()));
            }
            Change::SwarmDispatch { ticket_id, role, node, .. } => {
                let n_str = node.as_deref().unwrap_or("auto");
                out.push(Line::Info(format!("despacha el rol «{:?}» del ticket {ticket_id} al nodo «{n_str}»", role)));
            }
            Change::VfsQuery { path, .. } => {
                let p = path.as_deref().unwrap_or("/antfs");
                out.push(Line::Info(format!("consulta la ruta semántica «{p}» en el sistema virtual /antfs")));
            }
            Change::VfsMount { mount_point, .. } => {
                let m = mount_point.as_deref().unwrap_or(".antos/mnt/antfs");
                out.push(Line::Info(format!("monta la jerarquía de símbolos y diffs de /antfs en «{m}»")));
            }
            Change::VfsUnmount { mount_point, .. } => {
                let m = mount_point.as_deref().unwrap_or(".antos/mnt/antfs");
                out.push(Line::Info(format!("desmonta y limpia el punto de montaje «{m}»")));
            }
        }
        pendiente.aplicar(change);
    }
    out
}

fn count_entries(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir).map(|d| d.count()).unwrap_or(0)
}

/// Diff por líneas recortando prefijo y sufijo comunes.
///
/// No es un algoritmo de diff completo, pero para el caso real — reescribir
/// un fichero pequeño — muestra exactamente lo que cambia sin dependencias.
fn diff(old: &str, new: &str) -> Vec<(char, String)> {
    let o: Vec<&str> = old.lines().collect();
    let n: Vec<&str> = new.lines().collect();

    let mut start = 0;
    while start < o.len() && start < n.len() && o[start] == n[start] {
        start += 1;
    }
    let max_end = (o.len() - start).min(n.len() - start);
    let mut end = 0;
    while end < max_end && o[o.len() - 1 - end] == n[n.len() - 1 - end] {
        end += 1;
    }

    let removed = &o[start..o.len() - end];
    let added = &n[start..n.len() - end];

    let mut lines: Vec<(char, String)> = Vec::new();
    for l in removed {
        lines.push(('-', l.to_string()));
    }
    for l in added {
        lines.push(('+', l.to_string()));
    }

    if lines.len() > MAX_DIFF_LINES {
        let hidden = lines.len() - MAX_DIFF_LINES;
        lines.truncate(MAX_DIFF_LINES);
        lines.push((' ', format!("… y {hidden} línea(s) más")));
    }
    lines
}
