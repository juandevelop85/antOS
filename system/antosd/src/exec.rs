//! Traduce pasos en cambios concretos, y aplica esos cambios.
//!
//! `changes_for` es la única fuente de verdad: la previsualización y la
//! ejecución llaman a la MISMA función. Si fueran dos caminos distintos, el
//! diff que apruebas y lo que ocurre podrían divergir — y ese es justo el
//! fallo que hace inaceptable un sistema gobernado por IA.

use crate::blast::expand;
use crate::capability::Capability;
use crate::ctx::Ctx;
use crate::plan::Step;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Serializable porque cruza la frontera de proceso: el broker decide los
/// cambios, el ejecutor confinado los aplica.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "lowercase")]
pub enum Change {
    Write { path: PathBuf, content: String },
    Mkdir { path: PathBuf },
    Delete { path: PathBuf },
    Read { path: PathBuf },
    GitStatus { repo_root: PathBuf },
    GitCommit { repo_root: PathBuf, commit_msg: String },
    GitBranch { repo_root: PathBuf, branch_name: String, base: Option<String> },
    GitWorktreeCreate {
        repo_root: PathBuf,
        target_path: PathBuf,
        branch_name: String,
        base: String,
    },
    GitWorktreeCleanup {
        repo_root: PathBuf,
        target_path: PathBuf,
        force: bool,
    },
    GitWorktreeMerge {
        repo_root: PathBuf,
        branch_name: String,
        target_branch: String,
        message: Option<String>,
    },
    PortStatus {
        port: Option<u16>,
    },
    PortKill {
        port: u16,
        force: bool,
    },
    ServiceUp {
        service: String,
        port: Option<u16>,
        db_name: Option<String>,
        state_dir: PathBuf,
        workspace: PathBuf,
    },
    ServiceDown {
        service: String,
        state_dir: PathBuf,
    },
    ServiceStatus {
        service: Option<String>,
        state_dir: PathBuf,
    },
    SecretGrant {
        secret: String,
        minutes: i64,
        reason: Option<String>,
        grants_path: PathBuf,
    },
    SecretRevoke {
        secret: String,
        grants_path: PathBuf,
    },
    SecretList {
        state_dir: PathBuf,
        grants_path: PathBuf,
    },
    SecretSet {
        key: String,
        value: String,
        state_dir: PathBuf,
    },
    SecretRead {
        key: String,
        state_dir: PathBuf,
        grants_path: PathBuf,
    },
    TicketCreate {
        ticket_id: String,
        title: String,
        description: Option<String>,
        phase: Option<String>,
        workspace: PathBuf,
    },
    TicketUpdateStatus {
        ticket_id: String,
        status: String,
        workspace: PathBuf,
    },
    TicketList {
        workspace: PathBuf,
        filter: Option<String>,
    },
    MemoryIndex {
        workspace: PathBuf,
    },
    MemorySearch {
        workspace: PathBuf,
        query: String,
        limit: usize,
    },
    MemoryGraph {
        workspace: PathBuf,
        target: Option<String>,
    },
    EnvProfileInit {
        workspace: PathBuf,
        profile: Option<String>,
        create_devbox: bool,
        create_flake: bool,
    },
    EnvProfileSync {
        workspace: PathBuf,
    },
    EnvProfileStatus {
        workspace: PathBuf,
    },
    QuotaStatus {
        workspace: PathBuf,
    },
    QuotaSet {
        workspace: PathBuf,
        quota: crate::sandbox::quota::ResourceQuota,
    },
    UiDiffViewer {
        workspace: PathBuf,
        target: Option<String>,
    },
    UiTerminal {
        command: Option<String>,
    },
    NotifyList {
        workspace: PathBuf,
    },
    NotifyAction {
        workspace: PathBuf,
        notification_id: String,
        action: antos_protocolo::NotificationAction,
    },
    MeshStatus {
        workspace: PathBuf,
    },
    MeshConnect {
        workspace: PathBuf,
        address: String,
    },
    MeshPair {
        workspace: PathBuf,
    },
    SwarmStatus {
        workspace: PathBuf,
    },
    SwarmDispatch {
        workspace: PathBuf,
        ticket_id: String,
        role: antos_protocolo::AgentRole,
        node: Option<String>,
    },
    VfsQuery {
        workspace: PathBuf,
        path: Option<String>,
    },
    VfsMount {
        workspace: PathBuf,
        mount_point: Option<String>,
    },
    VfsUnmount {
        workspace: PathBuf,
        mount_point: Option<String>,
    },
}

/// Lo que el plan ya ha decidido escribir, antes de haberlo escrito.
///
/// Sin esto, dos pasos que tocan el mismo fichero se pisan: cada uno lo lee
/// del disco tal y como estaba ANTES del plan, y al ejecutar gana el último.
/// Declarar tres dependencias dejaba una.
///
/// Y lo grave no era perder dos líneas: era que el diff aprobado dejaba de
/// describir el resultado. Que lo que ves sea lo que pasa es la propiedad
/// que sostiene todo lo demás.
#[derive(Default)]
pub struct Pendiente {
    escrituras: BTreeMap<PathBuf, String>,
    borrados: std::collections::BTreeSet<PathBuf>,
}

impl Pendiente {
    /// El contenido que tendrá el fichero cuando llegue este paso.
    /// `None` significa «pregúntale al disco».
    pub fn leer(&self, path: &Path) -> Option<String> {
        if self.borrados.contains(path) {
            return Some(String::new());
        }
        self.escrituras.get(path).cloned()
    }

    pub fn aplicar(&mut self, change: &Change) {
        match change {
            Change::Write { path, content } => {
                self.borrados.remove(path);
                self.escrituras.insert(path.clone(), content.clone());
            }
            Change::Delete { path } => {
                self.escrituras.remove(path);
                self.borrados.insert(path.clone());
            }
            Change::Mkdir { .. }
            | Change::Read { .. }
            | Change::GitStatus { .. }
            | Change::GitCommit { .. }
            | Change::GitBranch { .. }
            | Change::GitWorktreeCreate { .. }
            | Change::GitWorktreeCleanup { .. }
            | Change::GitWorktreeMerge { .. }
            | Change::PortStatus { .. }
            | Change::PortKill { .. }
            | Change::ServiceUp { .. }
            | Change::ServiceDown { .. }
            | Change::ServiceStatus { .. }
            | Change::SecretGrant { .. }
            | Change::SecretRevoke { .. }
            | Change::SecretList { .. }
            | Change::SecretSet { .. }
            | Change::SecretRead { .. }
            | Change::TicketCreate { .. }
            | Change::TicketUpdateStatus { .. }
            | Change::TicketList { .. }
            | Change::MemoryIndex { .. }
            | Change::MemorySearch { .. }
            | Change::MemoryGraph { .. }
            | Change::EnvProfileInit { .. }
            | Change::EnvProfileSync { .. }
            | Change::EnvProfileStatus { .. }
            | Change::QuotaStatus { .. }
            | Change::QuotaSet { .. }
            | Change::UiDiffViewer { .. }
            | Change::UiTerminal { .. }
            | Change::NotifyList { .. }
            | Change::NotifyAction { .. }
            | Change::MeshStatus { .. }
            | Change::MeshConnect { .. }
            | Change::MeshPair { .. }
            | Change::SwarmStatus { .. }
            | Change::SwarmDispatch { .. }
            | Change::VfsQuery { .. }
            | Change::VfsMount { .. }
            | Change::VfsUnmount { .. } => {}
        }
    }
}

/// Lee un fichero teniendo en cuenta lo que el plan ya ha decidido.
fn leer_con_pendiente(path: &Path, pendiente: &Pendiente) -> String {
    pendiente
        .leer(path)
        .unwrap_or_else(|| std::fs::read_to_string(path).unwrap_or_default())
}

pub fn changes_for(
    step: &Step,
    cap: &Capability,
    ctx: &Ctx,
    pendiente: &Pendiente,
) -> Result<Vec<Change>> {
    let a = &step.args;
    match cap.name.as_str() {
        "fs.read" => Ok(vec![Change::Read { path: abs(ctx, &a["path"]) }]),

        "fs.write" => Ok(vec![Change::Write {
            path: abs(ctx, &a["path"]),
            content: a["content"].clone(),
        }]),

        "fs.delete" => Ok(vec![Change::Delete { path: abs(ctx, &a["path"]) }]),

        "fs.mkdir" => Ok(vec![Change::Mkdir { path: abs(ctx, &a["path"]) }]),

        "project.scaffold" => {
            let root = ctx.workspace.join(&a["name"]);
            Ok(scaffold(&a["language"], &a["name"])
                .into_iter()
                .map(|(rel, content)| Change::Write { path: root.join(rel), content })
                .collect())
        }

        "pkg.declare" => {
            let proj = ctx.workspace.join(&a["project"]);
            let path = if proj.join("syso.packages.toml").exists() {
                proj.join("syso.packages.toml")
            } else {
                proj.join("antos.packages.toml")
            };
            let previo = leer_con_pendiente(&path, pendiente);
            Ok(vec![Change::Write {
                content: declare_package(&previo, &a["package"], &a["version"])?,
                path,
            }])
        }

        "system.declare" => {
            let path = if ctx.system_config.join("syso-paquetes.nix").exists() {
                ctx.system_config.join("syso-paquetes.nix")
            } else {
                ctx.system_config.join("antos-paquetes.nix")
            };
            let previo = leer_con_pendiente(&path, pendiente);
            Ok(vec![Change::Write {
                content: declare_system_package(&previo, &a["package"])?,
                path,
            }])
        }

        "git.status" => {
            let root = a.get("path").map(|p| abs(ctx, p)).unwrap_or_else(|| ctx.workspace.clone());
            Ok(vec![Change::GitStatus { repo_root: root }])
        }

        "git.commit_semantic" => {
            let root = a.get("path").map(|p| abs(ctx, p)).unwrap_or_else(|| ctx.workspace.clone());
            let tipo = a.get("type").cloned().unwrap_or_else(|| "feat".into());
            let msg = a.get("message").cloned().unwrap_or_default();
            let commit_msg = if let Some(scope) = a.get("scope").filter(|s| !s.is_empty()) {
                format!("{tipo}({scope}): {msg}")
            } else {
                format!("{tipo}: {msg}")
            };
            Ok(vec![Change::GitCommit { repo_root: root, commit_msg }])
        }

        "git.smart_branch" => {
            let root = a.get("path").map(|p| abs(ctx, p)).unwrap_or_else(|| ctx.workspace.clone());
            let name = a.get("name").cloned().unwrap_or_default();
            let base = a.get("base").cloned();
            let branch_name = if let Some(ticket) = a.get("ticket_id").filter(|t| !t.is_empty()) {
                format!("{}/{}", ticket.to_lowercase(), name)
            } else {
                name
            };
            Ok(vec![Change::GitBranch { repo_root: root, branch_name, base }])
        }

        "git.worktree_create" => {
            let root = a.get("path").map(|p| abs(ctx, p)).unwrap_or_else(|| ctx.workspace.clone());
            let ticket = a.get("ticket_id").cloned().unwrap_or_else(|| "task".into());
            let branch = a.get("branch").cloned().unwrap_or_else(|| format!("agent/{ticket}"));
            let base = a.get("base").cloned().unwrap_or_else(|| "HEAD".into());
            let target_path = ctx.state.join("worktrees").join(&ticket);
            Ok(vec![Change::GitWorktreeCreate {
                repo_root: root,
                target_path,
                branch_name: branch,
                base,
            }])
        }

        "git.worktree_cleanup" => {
            let root = a.get("path").map(|p| abs(ctx, p)).unwrap_or_else(|| ctx.workspace.clone());
            let ticket = a.get("ticket_id").cloned().unwrap_or_else(|| "task".into());
            let force = a.get("force").map(|f| f == "true").unwrap_or(false);
            let target_path = ctx.state.join("worktrees").join(&ticket);
            Ok(vec![Change::GitWorktreeCleanup {
                repo_root: root,
                target_path,
                force,
            }])
        }

        "git.worktree_merge" => {
            let root = a.get("path").map(|p| abs(ctx, p)).unwrap_or_else(|| ctx.workspace.clone());
            let ticket = a.get("ticket_id").cloned().unwrap_or_else(|| "task".into());
            let branch = format!("agent/{ticket}");
            let target = a.get("target").cloned().unwrap_or_else(|| "main".into());
            let message = a.get("message").cloned();
            Ok(vec![Change::GitWorktreeMerge {
                repo_root: root,
                branch_name: branch,
                target_branch: target,
                message,
            }])
        }

        "diag.port_status" => {
            let port = a.get("port").and_then(|p| p.parse::<u16>().ok());
            Ok(vec![Change::PortStatus { port }])
        }

        "diag.port_kill" => {
            let port = a
                .get("port")
                .and_then(|p| p.parse::<u16>().ok())
                .ok_or_else(|| anyhow::anyhow!("debes especificar el puerto a liberar"))?;
            let force = a.get("force").map(|f| f == "true").unwrap_or(false);
            Ok(vec![Change::PortKill { port, force }])
        }

        "env.service_up" => {
            let service = a
                .get("service")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("debes especificar el nombre del servicio (ej. postgres, redis)"))?;
            let port = a.get("port").and_then(|p| p.parse::<u16>().ok());
            let db_name = a.get("db_name").cloned();
            Ok(vec![Change::ServiceUp {
                service,
                port,
                db_name,
                state_dir: ctx.state.clone(),
                workspace: ctx.workspace.clone(),
            }])
        }

        "env.service_down" => {
            let service = a
                .get("service")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("debes especificar el nombre del servicio a detener"))?;
            Ok(vec![Change::ServiceDown {
                service,
                state_dir: ctx.state.clone(),
            }])
        }

        "env.service_status" => {
            let service = a.get("service").cloned();
            Ok(vec![Change::ServiceStatus {
                service,
                state_dir: ctx.state.clone(),
            }])
        }

        "secret.grant" => {
            let secret = a
                .get("secret")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("debes especificar el secreto a conceder"))?;
            let minutes = a.get("minutes").and_then(|m| m.parse::<i64>().ok()).unwrap_or(10);
            let reason = a.get("reason").cloned();
            Ok(vec![Change::SecretGrant {
                secret,
                minutes,
                reason,
                grants_path: ctx.grants_path(),
            }])
        }

        "secret.revoke" => {
            let secret = a
                .get("secret")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("debes especificar el secreto a revocar"))?;
            Ok(vec![Change::SecretRevoke {
                secret,
                grants_path: ctx.grants_path(),
            }])
        }

        "secret.list" => {
            Ok(vec![Change::SecretList {
                state_dir: ctx.state.clone(),
                grants_path: ctx.grants_path(),
            }])
        }

        "secret.set" => {
            let key = a.get("key").cloned().ok_or_else(|| anyhow::anyhow!("clave requerida"))?;
            let value = a.get("value").cloned().ok_or_else(|| anyhow::anyhow!("valor requerido"))?;
            Ok(vec![Change::SecretSet {
                key,
                value,
                state_dir: ctx.state.clone(),
            }])
        }

        "secret.read" => {
            let key = a.get("key").cloned().ok_or_else(|| anyhow::anyhow!("clave requerida"))?;
            Ok(vec![Change::SecretRead {
                key,
                state_dir: ctx.state.clone(),
                grants_path: ctx.grants_path(),
            }])
        }

        "spec.create_ticket" => {
            let ticket_id = a.get("ticket_id").cloned().ok_or_else(|| anyhow::anyhow!("ticket_id requerido"))?;
            let title = a.get("title").cloned().ok_or_else(|| anyhow::anyhow!("title requerido"))?;
            let description = a.get("description").cloned();
            let phase = a.get("phase").cloned();
            Ok(vec![Change::TicketCreate {
                ticket_id,
                title,
                description,
                phase,
                workspace: ctx.workspace.clone(),
            }])
        }

        "spec.update_ticket" => {
            let ticket_id = a.get("ticket_id").cloned().ok_or_else(|| anyhow::anyhow!("ticket_id requerido"))?;
            let status = a.get("status").cloned().unwrap_or_else(|| "completado".into());
            Ok(vec![Change::TicketUpdateStatus {
                ticket_id,
                status,
                workspace: ctx.workspace.clone(),
            }])
        }

        "spec.list_tickets" => {
            let filter = a.get("filter").cloned();
            Ok(vec![Change::TicketList {
                workspace: ctx.workspace.clone(),
                filter,
            }])
        }

        "memory.index" => {
            Ok(vec![Change::MemoryIndex {
                workspace: ctx.workspace.clone(),
            }])
        }

        "memory.search" => {
            let query = a.get("query").cloned().ok_or_else(|| anyhow::anyhow!("query requerido"))?;
            let limit = a.get("limit").and_then(|l| l.parse::<usize>().ok()).unwrap_or(5);
            Ok(vec![Change::MemorySearch {
                workspace: ctx.workspace.clone(),
                query,
                limit,
            }])
        }

        "memory.graph" => {
            let target = a.get("target").cloned();
            Ok(vec![Change::MemoryGraph {
                workspace: ctx.workspace.clone(),
                target,
            }])
        }

        "env.init" => {
            let profile = a.get("profile").cloned();
            let create_devbox = a.get("devbox").map(|s| s != "false").unwrap_or(true);
            let create_flake = a.get("flake").map(|s| s != "false").unwrap_or(true);
            Ok(vec![Change::EnvProfileInit {
                workspace: ctx.workspace.clone(),
                profile,
                create_devbox,
                create_flake,
            }])
        }

        "env.sync" => {
            Ok(vec![Change::EnvProfileSync {
                workspace: ctx.workspace.clone(),
            }])
        }

        "env.profile_status" => {
            Ok(vec![Change::EnvProfileStatus {
                workspace: ctx.workspace.clone(),
            }])
        }

        "quota.status" => {
            Ok(vec![Change::QuotaStatus {
                workspace: ctx.workspace.clone(),
            }])
        }

        "quota.set" => {
            let mut current = crate::sandbox::quota::load_quota(&ctx.workspace).unwrap_or_default();
            if let Some(t) = a.get("timeout").and_then(|v| v.parse::<u64>().ok()) {
                current.timeout_secs = t;
            }
            if let Some(m) = a.get("memory").and_then(|v| v.parse::<u64>().ok()) {
                current.max_memory_mb = m;
            }
            if let Some(c) = a.get("cpu").and_then(|v| v.parse::<u32>().ok()) {
                current.cpu_quota_percent = c;
            }
            if let Some(p) = a.get("pids").and_then(|v| v.parse::<u32>().ok()) {
                current.max_pids = p;
            }

            Ok(vec![Change::QuotaSet {
                workspace: ctx.workspace.clone(),
                quota: current,
            }])
        }

        "ui.diff_viewer" => {
            let target = a.get("target").cloned();
            Ok(vec![Change::UiDiffViewer {
                workspace: ctx.workspace.clone(),
                target,
            }])
        }

        "ui.terminal" => {
            let command = a.get("command").cloned();
            Ok(vec![Change::UiTerminal {
                command,
            }])
        }

        "notify.list" => {
            Ok(vec![Change::NotifyList {
                workspace: ctx.workspace.clone(),
            }])
        }

        "notify.action" => {
            let id = a.get("id").cloned().unwrap_or_default();
            let action_str = a.get("action").map(String::as_str).unwrap_or("dismiss");
            let action = match action_str {
                "approve" | "aprobar" => antos_protocolo::NotificationAction::Approve,
                "reject" | "rechazar" | "rollback" => antos_protocolo::NotificationAction::Reject,
                "diff" | "view_diff" => antos_protocolo::NotificationAction::ViewDiff,
                _ => antos_protocolo::NotificationAction::Dismiss,
            };
            Ok(vec![Change::NotifyAction {
                workspace: ctx.workspace.clone(),
                notification_id: id,
                action,
            }])
        }

        "mesh.status" => {
            Ok(vec![Change::MeshStatus {
                workspace: ctx.workspace.clone(),
            }])
        }

        "mesh.connect" => {
            let address = a.get("address").cloned().unwrap_or_else(|| "127.0.0.1:9042".into());
            Ok(vec![Change::MeshConnect {
                workspace: ctx.workspace.clone(),
                address,
            }])
        }

        "mesh.pair" => {
            Ok(vec![Change::MeshPair {
                workspace: ctx.workspace.clone(),
            }])
        }

        "flow.swarm_status" => {
            Ok(vec![Change::SwarmStatus {
                workspace: ctx.workspace.clone(),
            }])
        }

        "flow.dispatch_remote" => {
            let ticket_id = a.get("ticket_id").cloned().unwrap_or_else(|| "T1.1".into());
            let role_str = a.get("role").map(String::as_str).unwrap_or("coder");
            let role = match role_str {
                "arquitecto" | "architect" => antos_protocolo::AgentRole::Arquitecto,
                "qa" | "tester" => antos_protocolo::AgentRole::QA,
                "auditor" => antos_protocolo::AgentRole::Auditor,
                _ => antos_protocolo::AgentRole::Coder,
            };
            let node = a.get("node").cloned();
            Ok(vec![Change::SwarmDispatch {
                workspace: ctx.workspace.clone(),
                ticket_id,
                role,
                node,
            }])
        }

        "vfs.query" => {
            let path = a.get("path").cloned();
            Ok(vec![Change::VfsQuery {
                workspace: ctx.workspace.clone(),
                path,
            }])
        }

        "vfs.mount" => {
            let mount_point = a.get("mount_point").cloned();
            Ok(vec![Change::VfsMount {
                workspace: ctx.workspace.clone(),
                mount_point,
            }])
        }

        "vfs.unmount" => {
            let mount_point = a.get("mount_point").cloned();
            Ok(vec![Change::VfsUnmount {
                workspace: ctx.workspace.clone(),
                mount_point,
            }])
        }

        other => bail!("no hay implementación para la capacidad «{other}»"),
    }
}

pub fn apply(changes: &[Change]) -> Result<Vec<String>> {
    let mut output = Vec::new();
    for change in changes {
        match change {
            Change::Write { path, content } => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("creando el directorio {}", parent.display()))?;
                }
                std::fs::write(path, content)
                    .with_context(|| format!("escribiendo {}", path.display()))?;
            }
            Change::Mkdir { path } => {
                std::fs::create_dir_all(path)
                    .with_context(|| format!("creando el directorio {}", path.display()))?;
            }
            Change::Delete { path } => {
                if path.is_dir() {
                    std::fs::remove_dir_all(path)?;
                } else if path.exists() {
                    std::fs::remove_file(path)?;
                } else {
                    bail!("no existe: {}", path.display());
                }
            }
            Change::Read { path } => {
                output.push(std::fs::read_to_string(path)?);
            }
            Change::GitStatus { repo_root } => {
                if let Some(status) = crate::git::GitAnalyzer::global().consultar_estado(repo_root)? {
                    let lineas = vec![
                        format!("rama: {}", status.rama.unwrap_or_else(|| "HEAD desacoplado".into())),
                        format!("commits: +{} / -{}", status.delante, status.detras),
                        format!("modificados: {}", status.modificados.len()),
                        format!("staged: {}", status.staged.len()),
                        format!("sin seguimiento: {}", status.sin_seguimiento.len()),
                    ];
                    output.push(lineas.join("\n"));
                } else {
                    output.push("no es un repositorio Git".into());
                }
            }
            Change::GitCommit { repo_root, commit_msg } => {
                let add_out = std::process::Command::new("git")
                    .arg("-C")
                    .arg(repo_root)
                    .args(["add", "-A"])
                    .output()
                    .context("ejecutando git add")?;
                if !add_out.status.success() {
                    bail!("git add falló: {}", String::from_utf8_lossy(&add_out.stderr));
                }

                let commit_out = std::process::Command::new("git")
                    .arg("-C")
                    .arg(repo_root)
                    .args(["commit", "-m", commit_msg])
                    .output()
                    .context("ejecutando git commit")?;
                if !commit_out.status.success() {
                    let err = String::from_utf8_lossy(&commit_out.stderr);
                    let out_str = String::from_utf8_lossy(&commit_out.stdout);
                    if out_str.contains("nothing to commit") || err.contains("nothing to commit") {
                        output.push("nada que commitear (árbol limpio)".into());
                    } else {
                        bail!("git commit falló: {err}\n{out_str}");
                    }
                } else {
                    let resultado = String::from_utf8_lossy(&commit_out.stdout).trim().to_string();
                    output.push(format!("commit creado: {resultado}"));
                }
            }
            Change::GitBranch { repo_root, branch_name, base } => {
                let mut cmd = std::process::Command::new("git");
                cmd.arg("-C").arg(repo_root);
                if let Some(b) = base {
                    cmd.args(["checkout", "-B", branch_name, b]);
                } else {
                    cmd.args(["checkout", "-B", branch_name]);
                }
                let out = cmd.output().context("ejecutando git checkout")?;
                if !out.status.success() {
                    bail!("git checkout falló: {}", String::from_utf8_lossy(&out.stderr));
                }
                output.push(format!("rama activa: {branch_name}"));
            }
            Change::GitWorktreeCreate { repo_root, target_path, branch_name, base } => {
                crate::git::crear_worktree(repo_root, target_path, branch_name, base)?;
                output.push(format!(
                    "worktree creado en: {} (rama: {})",
                    target_path.display(),
                    branch_name
                ));
            }
            Change::GitWorktreeCleanup { repo_root, target_path, force } => {
                crate::git::eliminar_worktree(repo_root, target_path, *force)?;
                output.push(format!("worktree eliminado: {}", target_path.display()));
            }
            Change::GitWorktreeMerge { repo_root, branch_name, target_branch, message } => {
                let res = crate::git::merge_worktree(
                    repo_root,
                    branch_name,
                    target_branch,
                    message.as_deref(),
                )?;
                output.push(format!("merge completado: {res}"));
            }
            Change::PortStatus { port } => {
                let puertos = crate::net::diagnosticar_puertos(*port)?;
                if puertos.is_empty() {
                    if let Some(p) = port {
                        output.push(format!("puerto {p} está libre"));
                    } else {
                        output.push("no hay puertos de desarrollo en escucha".into());
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
                    output.push(lineas.join("\n"));
                }
            }
            Change::PortKill { port, force } => {
                let eliminados = crate::net::liberar_puerto(*port, *force)?;
                if eliminados.is_empty() {
                    output.push(format!("puerto {port} ya estaba libre"));
                } else {
                    let pids: Vec<String> = eliminados
                        .iter()
                        .map(|p| format!("PID {} ({})", p.pid, p.process_name))
                        .collect();
                    output.push(format!("puerto {port} liberado terminando {}", pids.join(", ")));
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
                output.push(format!(
                    "servicio «{}» arrancado en puerto {} | {}={}",
                    info.name, info.port, info.env_var_key, info.env_var_value
                ));
            }
            Change::ServiceDown {
                service,
                state_dir,
            } => {
                crate::service::stop_service(service, state_dir)?;
                output.push(format!("servicio «{service}» detenido y limpiado"));
            }
            Change::ServiceStatus {
                service,
                state_dir,
            } => {
                let services = crate::service::get_service_status(service.as_deref(), state_dir)?;
                if services.is_empty() {
                    output.push("no hay servicios efímeros aprovisionados".into());
                } else {
                    let mut lines = Vec::new();
                    for s in services {
                        lines.push(format!(
                            "servicio {:<12} | puerto {:<5} | estado {:<8} | {}={}",
                            s.name, s.port, s.status, s.env_var_key, s.env_var_value
                        ));
                    }
                    output.push(lines.join("\n"));
                }
            }
            Change::SecretGrant {
                secret,
                minutes,
                reason,
                grants_path,
            } => {
                let mut grants = crate::grants::Grants::load(grants_path)?;
                grants.grant_with_reason(secret, *minutes, reason.clone());
                grants.save(grants_path)?;
                let motivo = reason.as_deref().map(|r| format!(" para «{r}»")).unwrap_or_default();
                output.push(format!("concesión temporal otorgada a «{secret}» por {minutes} minutos{motivo}"));
            }
            Change::SecretRevoke { secret, grants_path } => {
                let mut grants = crate::grants::Grants::load(grants_path)?;
                grants.revoke(secret);
                grants.save(grants_path)?;
                output.push(format!("concesión revocada: «{secret}»"));
            }
            Change::SecretList { state_dir, grants_path } => {
                let list = crate::vault::list_secrets(state_dir)?;
                let grants = crate::grants::Grants::load(grants_path)?;
                let active = grants.list_active();
                let mut lines = Vec::new();
                lines.push(format!("secretos en bóveda: {} | concesiones activas: {}", list.len(), active.len()));
                for s in list {
                    let granted = grants.is_granted("secret.read") || grants.is_granted(&format!("secret.{}", s.key));
                    lines.push(format!("  - {} ({} bytes, concedido: {})", s.key, s.length, granted));
                }
                output.push(lines.join("\n"));
            }
            Change::SecretSet { key, value, state_dir } => {
                crate::vault::set_secret(state_dir, key, value)?;
                output.push(format!("secreto «{key}» guardado de forma segura en la bóveda de antOS"));
            }
            Change::SecretRead { key, state_dir, grants_path } => {
                let grants = crate::grants::Grants::load(grants_path)?;
                match crate::vault::get_secret(state_dir, key, &grants) {
                    Ok(Some(val)) => {
                        output.push(format!("secreto {key}={val}"));
                    }
                    Ok(None) => {
                        output.push(format!("secreto {key} no encontrado en la bóveda"));
                    }
                    Err(e) => {
                        output.push(format!("acceso bloqueado: {e}"));
                    }
                }
            }
            Change::TicketCreate {
                ticket_id,
                title,
                description,
                phase,
                workspace,
            } => {
                let engine = crate::spec::SpecEngine::global();
                let path = engine.create_ticket(
                    workspace,
                    ticket_id,
                    title,
                    description.as_deref(),
                    phase.as_deref(),
                )?;
                output.push(format!("ticket «{}» creado exitosamente en {}", ticket_id, path.display()));
            }
            Change::TicketUpdateStatus {
                ticket_id,
                status,
                workspace,
            } => {
                let st = match status.to_lowercase().as_str() {
                    "completado" | "done" | "hecho" => antos_protocolo::TicketStatus::Completado,
                    "progreso" | "en_progreso" | "in_progress" => antos_protocolo::TicketStatus::EnProgreso,
                    "revision" | "revisión" | "review" => antos_protocolo::TicketStatus::EnRevision,
                    _ => antos_protocolo::TicketStatus::Pendiente,
                };
                let engine = crate::spec::SpecEngine::global();
                engine.update_ticket_status(workspace, ticket_id, st)?;
                output.push(format!("estado del ticket «{}» actualizado a {:?}", ticket_id, st));
            }
            Change::TicketList { workspace, filter } => {
                let engine = crate::spec::SpecEngine::global();
                let tickets = engine.list_tickets(workspace)?;
                let mut lines = Vec::new();
                lines.push(format!("tickets en el proyecto: {}", tickets.len()));
                for t in tickets {
                    if let Some(ref f) = filter {
                        let st_str = format!("{:?}", t.estado).to_lowercase();
                        if !st_str.contains(&f.to_lowercase()) {
                            continue;
                        }
                    }
                    lines.push(format!("  - [{}] {} [{:?}] ({})", t.id, t.titulo, t.estado, t.fase));
                }
                output.push(lines.join("\n"));
            }
            Change::MemoryIndex { workspace } => {
                let db_path = crate::memory::MemoryEngine::default_db_path(workspace);
                let store = crate::memory::MemoryEngine::index_workspace(workspace)?;
                crate::memory::MemoryEngine::save(&store, &db_path)?;
                output.push(format!(
                    "memoria semántica actualizada: {} fragmentos y {} nodos de grafo indexados en {}",
                    store.chunks.len(),
                    store.graph.nodes.len(),
                    db_path.display()
                ));
            }
            Change::MemorySearch { workspace, query, limit } => {
                let db_path = crate::memory::MemoryEngine::default_db_path(workspace);
                let store = crate::memory::MemoryEngine::load(&db_path)?;
                let hits = crate::memory::MemoryEngine::search(&store, query, *limit);
                if hits.is_empty() {
                    output.push(format!("no se encontraron coincidencias para «{query}»"));
                } else {
                    let mut lines = Vec::new();
                    lines.push(format!("coincidencias semánticas para «{query}» ({}):", hits.len()));
                    for h in hits {
                        lines.push(format!(
                            "  - [{:.2}] {}:{} ({:?}) - {}",
                            h.score, h.path, h.line_start, h.kind, h.title
                        ));
                    }
                    output.push(lines.join("\n"));
                }
            }
            Change::MemoryGraph { workspace, target } => {
                let db_path = crate::memory::MemoryEngine::default_db_path(workspace);
                let store = crate::memory::MemoryEngine::load(&db_path)?;
                let mut lines = Vec::new();
                match target {
                    Some(t) => {
                        let related = store.graph.related_to(t);
                        lines.push(format!("relaciones en el grafo para «{t}» ({}):", related.len()));
                        for (node, edge) in related {
                            lines.push(format!("  - {:?} -> {} ({})", edge, node.label, node.kind));
                        }
                    }
                    None => {
                        lines.push(format!(
                            "grafo de contexto: {} nodos, {} aristas",
                            store.graph.nodes.len(),
                            store.graph.edges.len()
                        ));
                    }
                }
                output.push(lines.join("\n"));
            }
            Change::EnvProfileInit {
                workspace,
                profile,
                create_devbox,
                create_flake,
            } => {
                let prof = match profile {
                    Some(ref p) => crate::env::EnvProfile::from_str_loose(p).unwrap_or(crate::env::EnvProfile::Base),
                    None => crate::env::EnvEngine::detect_stack(workspace).unwrap_or(crate::env::EnvProfile::Base),
                };
                let summary = crate::env::EnvEngine::init_profile(workspace, prof, *create_devbox, *create_flake)?;
                let files_str = summary.created_files.join(", ");
                output.push(format!(
                    "perfil de entorno «{}» inicializado. Archivos creados: [{files_str}]. Paquetes: [{}]",
                    summary.profile,
                    summary.packages.join(", ")
                ));
            }
            Change::EnvProfileSync { workspace } => {
                let statuses = crate::env::EnvEngine::check_toolchains(workspace)?;
                let mut lines = Vec::new();
                lines.push(format!("sincronización de entorno para {}", workspace.display()));
                for s in statuses {
                    let mark = if s.available { "✓" } else { "✗" };
                    let loc = s.path.unwrap_or_else(|| "no instalado".into());
                    lines.push(format!("  [{mark}] {:<16} ({loc})", s.name));
                }
                output.push(lines.join("\n"));
            }
            Change::EnvProfileStatus { workspace } => {
                let cfg = crate::env::EnvEngine::load_config(workspace)?;
                let mut lines = Vec::new();
                match cfg {
                    Some(c) => {
                        lines.push(format!("perfil activo: «{}» ({} paquetes)", c.profile, c.packages.len()));
                        let statuses = crate::env::EnvEngine::check_toolchains(workspace)?;
                        for s in statuses {
                            let mark = if s.available { "●" } else { "○" };
                            lines.push(format!("  {mark} {:<16} disponible: {}", s.name, s.available));
                        }
                    }
                    None => {
                        let detected = crate::env::EnvEngine::detect_stack(workspace);
                        let det_str = detected.map(|d| d.as_str()).unwrap_or("no detectado");
                        lines.push(format!("sin perfil explícito configurado (stack detectado: {det_str})"));
                    }
                }
                output.push(lines.join("\n"));
            }
            Change::QuotaStatus { workspace } => {
                let q = crate::sandbox::quota::load_quota(workspace)?;
                let cgroup_supported = crate::sandbox::quota::CgroupV2Manager::is_available();
                let cgroup_str = if cgroup_supported { "activo (cgroups v2)" } else { "modo proceso/watchdog" };
                let mut lines = Vec::new();
                lines.push(format!("cuotas y límites de sandbox ({cgroup_str}):"));
                lines.push(format!("  • Timeout máximo:   {}s", q.timeout_secs));
                lines.push(format!("  • Memoria máxima:   {} MB", q.max_memory_mb));
                lines.push(format!("  • Cuota de CPU:     {}%", q.cpu_quota_percent));
                lines.push(format!("  • Límite de PIDs:   {} procesos", q.max_pids));
                output.push(lines.join("\n"));
            }
            Change::QuotaSet { workspace, quota } => {
                crate::sandbox::quota::save_quota(workspace, quota)?;
                output.push(format!(
                    "cuotas de sandbox actualizadas: timeout={}s, memoria={}MB, cpu={}%",
                    quota.timeout_secs, quota.max_memory_mb, quota.cpu_quota_percent
                ));
            }
            Change::UiDiffViewer { workspace, target } => {
                let git_out = std::process::Command::new("git")
                    .current_dir(workspace)
                    .args(&["diff", target.as_deref().unwrap_or("HEAD")])
                    .output();
                match git_out {
                    Ok(o) if o.status.success() => {
                        let diff_text = String::from_utf8_lossy(&o.stdout);
                        let files = crate::diff_view::DiffEngine::parse_unified_diff(&diff_text);
                        if files.is_empty() {
                            output.push("no hay diferencias registradas en el repositorio".into());
                        } else {
                            output.push(crate::diff_view::DiffEngine::render_terminal(&files));
                        }
                    }
                    _ => {
                        output.push("no se pudo generar el diff git para el objetivo especificado".into());
                    }
                }
            }
            Change::UiTerminal { command } => {
                let mut session = crate::vte::TerminalSession::new("interactive");
                if let Some(ref cmd) = command {
                    let _ = session.execute_command(cmd);
                    output.push(format!("terminal: ejecutado «{cmd}» con código {:?}", session.exit_code));
                } else {
                    output.push(format!("terminal interactivo listo con shell {}", session.active_shell));
                }
            }
            Change::NotifyList { workspace } => {
                let notifs = crate::notification::NotificationEngine::global().list(workspace)?;
                if notifs.is_empty() {
                    output.push("no hay notificaciones ni aprobaciones pendientes".into());
                } else {
                    let mut lines = Vec::new();
                    lines.push(format!("notificaciones pendientes ({}):", notifs.len()));
                    for n in notifs {
                        let read_mark = if n.read { " " } else { "●" };
                        lines.push(format!("  {} [{}] {} — {}", read_mark, n.id, n.title, n.body));
                    }
                    output.push(lines.join("\n"));
                }
            }
            Change::NotifyAction { workspace, notification_id, action } => {
                let (ok, msg) = crate::notification::NotificationEngine::global()
                    .handle_action(workspace, notification_id, *action)?;
                if ok {
                    output.push(msg);
                } else {
                    output.push(format!("falló la acción sobre la notificación: {msg}"));
                }
            }
            Change::MeshStatus { workspace } => {
                let status = crate::mesh::MeshEngine::global().status(workspace)?;
                let mut lines = Vec::new();
                lines.push(format!("nodo local: {} ({}) en {}", status.local_node.id, status.local_node.hostname, status.local_node.address));
                lines.push(format!("  recursos: {} cores, {} MB RAM, VRAM: {:?}, modelos: {}",
                    status.local_node.resources.cpu_cores,
                    status.local_node.resources.memory_mb,
                    status.local_node.resources.vram_mb,
                    status.local_node.resources.available_models.join(", ")
                ));
                if status.peers.is_empty() {
                    lines.push("no hay nodos peers conectados en la malla".into());
                } else {
                    lines.push(format!("peers conocidos ({}):", status.peers.len()));
                    for p in status.peers {
                        lines.push(format!("  • {} [{}] {}ms latencia (modelos: {})", p.hostname, p.address, p.latency_ms, p.resources.available_models.join(", ")));
                    }
                }
                output.push(lines.join("\n"));
            }
            Change::MeshConnect { workspace, address } => {
                let peer = crate::mesh::MeshEngine::global().connect_peer(workspace, address)?;
                output.push(format!("conectado al peer {} [{}] con {}ms de latencia", peer.id, peer.address, peer.latency_ms));
            }
            Change::MeshPair { workspace } => {
                let token = crate::mesh::MeshEngine::global().generate_pairing_token(workspace)?;
                output.push(format!("token de emparejamiento generado: {} (nodo {})", token.token, token.node_id));
            }
            Change::SwarmStatus { workspace } => {
                let status = crate::distributed::SwarmEngine::global().status(workspace)?;
                let mut lines = Vec::new();
                lines.push(format!("antOS Swarm: {} nodos activos, {} tareas en curso", status.nodes.len(), status.total_tasks));
                for n in status.nodes {
                    let loc_str = if n.is_local { "[Local]" } else { "[Remoto]" };
                    lines.push(format!("  • {} {} ({}) - {} cores, VRAM: {:?}", n.hostname, loc_str, n.address, n.cpu_cores, n.vram_available_mb));
                    for t in n.running_tasks {
                        lines.push(format!("      └─ Tarea {}: rol {:?}, rama {}", t.ticket_id, t.role, t.worktree_branch));
                    }
                }
                output.push(lines.join("\n"));
            }
            Change::SwarmDispatch { workspace, ticket_id, role, node } => {
                let task = crate::distributed::SwarmEngine::global()
                    .dispatch_remote_role(workspace, ticket_id, *role, node.as_deref())?;
                output.push(format!("rol {:?} del ticket {} despachado al nodo {} (rama {})",
                    task.role, task.ticket_id, task.assigned_node_id, task.worktree_branch
                ));
            }
            Change::VfsQuery { workspace, path } => {
                let vpath = path.as_deref().unwrap_or("/antfs");
                if vpath == "/antfs" || vpath.ends_with('/') || vpath == "/antfs/symbols" || vpath == "/antfs/git" || vpath == "/antfs/symbols/structs" || vpath == "/antfs/symbols/functions" {
                    let entries = crate::vfs::VfsEngine::global().list_dir(workspace, vpath)?;
                    let mut lines = Vec::new();
                    lines.push(format!("entradas en {vpath} ({}):", entries.len()));
                    for e in entries {
                        let mark = if e.is_dir { "📁" } else { "📄" };
                        lines.push(format!("  {mark} {:<24} ({}, {} bytes)", e.name, e.node_type, e.size));
                    }
                    output.push(lines.join("\n"));
                } else {
                    let content = crate::vfs::VfsEngine::global().read_path(workspace, vpath)?;
                    output.push(content);
                }
            }
            Change::VfsMount { workspace, mount_point } => {
                let mnt = crate::vfs::VfsEngine::global().mount(workspace, mount_point.as_deref())?;
                output.push(format!("sistema de ficheros virtual /antfs montado en {}", mnt.display()));
            }
            Change::VfsUnmount { workspace, mount_point } => {
                crate::vfs::VfsEngine::global().unmount(workspace, mount_point.as_deref())?;
                output.push("sistema de ficheros virtual /antfs desmontado correctamente".into());
            }
        }
    }
    Ok(output)
}

fn abs(ctx: &Ctx, raw: &str) -> PathBuf {
    let expanded = expand(raw, &BTreeMap::new(), &ctx.workspace);
    let p = PathBuf::from(expanded);
    if p.is_absolute() { p } else { ctx.workspace.join(p) }
}

/// Los ficheros que produce un proyecto nuevo, por lenguaje.
fn scaffold(language: &str, name: &str) -> Vec<(&'static str, String)> {
    match language {
        "rust" => vec![
            ("Cargo.toml", format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n"
            )),
            ("src/main.rs", format!(
                "fn main() {{\n    println!(\"{name} en marcha\");\n}}\n"
            )),
        ],
        "typescript" => vec![
            ("package.json", format!(
                "{{\n  \"name\": \"{name}\",\n  \"version\": \"0.1.0\",\n  \"type\": \"module\",\n  \"scripts\": {{\n    \"start\": \"node --experimental-strip-types src/index.ts\"\n  }}\n}}\n"
            )),
            ("tsconfig.json",
                "{\n  \"compilerOptions\": {\n    \"target\": \"es2022\",\n    \"module\": \"esnext\",\n    \"moduleResolution\": \"bundler\",\n    \"strict\": true\n  }\n}\n".to_string()),
            ("src/index.ts", format!("console.log(\"{name} en marcha\");\n")),
        ],
        "python" => vec![
            ("pyproject.toml", format!(
                "[project]\nname = \"{name}\"\nversion = \"0.1.0\"\nrequires-python = \">=3.11\"\ndependencies = []\n"
            )),
            ("main.py", format!("def main() -> None:\n    print(\"{name} en marcha\")\n\n\nif __name__ == \"__main__\":\n    main()\n")),
        ],
        _ => Vec::new(),
    }
}

const NIX_HEADER: &str = "\
# Paquetes del sistema, declarados por antOS.
#
# Esto NO instala nada: describe qué debe tener la máquina. Aplicarlo es un
# paso aparte, explícito y tuyo:
#
#     sudo nixos-rebuild switch
#
# Editarlo a mano es correcto: antOS respeta lo que encuentre aquí.
{ pkgs, ... }:
{
  environment.systemPackages = with pkgs; [
";

const NIX_FOOTER: &str = "  ];\n}\n";

/// Devuelve el fichero Nix COMPLETO tras añadir el paquete.
///
/// Se lee lo que hay y se vuelve a escribir entero, en vez de aplicar un
/// parche. Es lo que permite fotografiarlo, previsualizarlo y revertirlo con
/// el mismo código que cualquier otro fichero.
fn declare_system_package(previo: &str, package: &str) -> Result<String> {
    let mut packages: BTreeMap<String, ()> = BTreeMap::new();

    {
        let existing = previo;
        // Un análisis por líneas basta porque este fichero lo genera antOS.
        // Si alguien lo reescribe con Nix de verdad, lo peor que pasa es que
        // no reconozcamos sus paquetes — y eso se ve en el diff antes de
        // aprobar nada.
        let mut inside = false;
        for line in existing.lines() {
            let trimmed = line.trim();
            if trimmed.ends_with('[') {
                inside = true;
                continue;
            }
            if trimmed.starts_with(']') {
                inside = false;
                continue;
            }
            if inside && !trimmed.is_empty() && !trimmed.starts_with('#') {
                packages.insert(trimmed.to_string(), ());
            }
        }
    }
    packages.insert(package.to_string(), ());

    let cuerpo: String = packages
        .keys()
        .map(|name| format!("    {name}\n"))
        .collect();
    Ok(format!("{NIX_HEADER}{cuerpo}{NIX_FOOTER}"))
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PackagesFile {
    #[serde(default)]
    packages: BTreeMap<String, String>,
}

const PACKAGES_HEADER: &str = "\
# Dependencias declaradas por antOS.
#
# Declarar no es instalar: este fichero dice qué necesita el proyecto, y la
# instalación es un paso aparte y explícito. Así lo que apruebas es un diff
# legible, y deshacerlo es volver a la declaración anterior.
";

/// Devuelve el contenido COMPLETO que tendría el fichero de declaraciones
/// tras añadir el paquete. Devolver el fichero entero (y no un parche) es lo
/// que permite fotografiarlo y previsualizarlo con el mismo código.
fn declare_package(previo: &str, package: &str, version: &str) -> Result<String> {
    let mut file: PackagesFile = toml::from_str(previo).unwrap_or_default();
    file.packages.insert(package.to_string(), version.to_string());
    Ok(format!("{PACKAGES_HEADER}\n{}", toml::to_string(&file)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_paso_ve_lo_que_decidio_el_anterior() {
        let mut pendiente = Pendiente::default();
        let ruta = PathBuf::from("/ws/paquetes.toml");

        assert_eq!(pendiente.leer(&ruta), None, "de partida, manda el disco");

        pendiente.aplicar(&Change::Write {
            path: ruta.clone(),
            content: "express".into(),
        });
        assert_eq!(
            pendiente.leer(&ruta).as_deref(),
            Some("express"),
            "el paso siguiente debe ver lo que este escribió, no el disco"
        );
    }

    #[test]
    fn escribir_despues_de_borrar_parte_de_cero() {
        let mut pendiente = Pendiente::default();
        let ruta = PathBuf::from("/ws/notas.txt");

        pendiente.aplicar(&Change::Delete { path: ruta.clone() });
        assert_eq!(
            pendiente.leer(&ruta).as_deref(),
            Some(""),
            "un fichero borrado por un paso anterior está vacío, no como en el disco"
        );

        pendiente.aplicar(&Change::Write {
            path: ruta.clone(),
            content: "nuevo".into(),
        });
        assert_eq!(pendiente.leer(&ruta).as_deref(), Some("nuevo"));
    }

    #[test]
    fn test_changes_for_capacidades_git() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = crate::capability::Catalog::load(&ctx.caps_dir).expect("catalog");
        let pendiente = Pendiente::default();

        // 1. git.status
        let step_status = Step {
            capability: "git.status".into(),
            args: BTreeMap::new(),
        };
        let cap_status = catalog.get("git.status").expect("cap git.status");
        let changes_status = changes_for(&step_status, cap_status, &ctx, &pendiente).expect("changes");
        assert_eq!(changes_status.len(), 1);
        match &changes_status[0] {
            Change::GitStatus { repo_root } => assert_eq!(repo_root, &ctx.workspace),
            _ => panic!("debe ser GitStatus"),
        }

        // 2. git.commit_semantic
        let mut args_commit = BTreeMap::new();
        args_commit.insert("type".into(), "feat".into());
        args_commit.insert("scope".into(), "auth".into());
        args_commit.insert("message".into(), "soporte de tokens JWT".into());
        let step_commit = Step {
            capability: "git.commit_semantic".into(),
            args: args_commit,
        };
        let cap_commit = catalog.get("git.commit_semantic").expect("cap git.commit_semantic");
        let changes_commit = changes_for(&step_commit, cap_commit, &ctx, &pendiente).expect("changes");
        assert_eq!(changes_commit.len(), 1);
        match &changes_commit[0] {
            Change::GitCommit { commit_msg, .. } => {
                assert_eq!(commit_msg, "feat(auth): soporte de tokens JWT");
            }
            _ => panic!("debe ser GitCommit"),
        }

        // 3. git.smart_branch
        let mut args_branch = BTreeMap::new();
        args_branch.insert("name".into(), "login-oauth".into());
        args_branch.insert("ticket_id".into(), "T2.1".into());
        let step_branch = Step {
            capability: "git.smart_branch".into(),
            args: args_branch,
        };
        let cap_branch = catalog.get("git.smart_branch").expect("cap git.smart_branch");
        let changes_branch = changes_for(&step_branch, cap_branch, &ctx, &pendiente).expect("changes");
        assert_eq!(changes_branch.len(), 1);
        match &changes_branch[0] {
            Change::GitBranch { branch_name, .. } => {
                assert_eq!(branch_name, "t2.1/login-oauth");
            }
            _ => panic!("debe ser GitBranch"),
        }

        // 4. git.worktree_create
        let mut args_wt_create = BTreeMap::new();
        args_wt_create.insert("ticket_id".into(), "T2.2".into());
        let step_wt_create = Step {
            capability: "git.worktree_create".into(),
            args: args_wt_create,
        };
        let cap_wt_create = catalog.get("git.worktree_create").expect("cap git.worktree_create");
        let changes_wt_create = changes_for(&step_wt_create, cap_wt_create, &ctx, &pendiente).expect("changes");
        assert_eq!(changes_wt_create.len(), 1);
        match &changes_wt_create[0] {
            Change::GitWorktreeCreate { branch_name, target_path, .. } => {
                assert_eq!(branch_name, "agent/T2.2");
                assert_eq!(target_path, &ctx.state.join("worktrees/T2.2"));
            }
            _ => panic!("debe ser GitWorktreeCreate"),
        }

        // 5. git.worktree_cleanup
        let mut args_wt_clean = BTreeMap::new();
        args_wt_clean.insert("ticket_id".into(), "T2.2".into());
        let step_wt_clean = Step {
            capability: "git.worktree_cleanup".into(),
            args: args_wt_clean,
        };
        let cap_wt_clean = catalog.get("git.worktree_cleanup").expect("cap git.worktree_cleanup");
        let changes_wt_clean = changes_for(&step_wt_clean, cap_wt_clean, &ctx, &pendiente).expect("changes");
        assert_eq!(changes_wt_clean.len(), 1);
        match &changes_wt_clean[0] {
            Change::GitWorktreeCleanup { target_path, .. } => {
                assert_eq!(target_path, &ctx.state.join("worktrees/T2.2"));
            }
            _ => panic!("debe ser GitWorktreeCleanup"),
        }

        // 6. diag.port_status
        let mut args_port_st = BTreeMap::new();
        args_port_st.insert("port".into(), "3000".into());
        let step_port_st = Step {
            capability: "diag.port_status".into(),
            args: args_port_st,
        };
        let cap_port_st = catalog.get("diag.port_status").expect("cap diag.port_status");
        let changes_port_st = changes_for(&step_port_st, cap_port_st, &ctx, &pendiente).expect("changes");
        assert_eq!(changes_port_st.len(), 1);
        match &changes_port_st[0] {
            Change::PortStatus { port } => assert_eq!(*port, Some(3000)),
            _ => panic!("debe ser PortStatus"),
        }

        // 7. diag.port_kill
        let mut args_port_kill = BTreeMap::new();
        args_port_kill.insert("port".into(), "8080".into());
        args_port_kill.insert("force".into(), "true".into());
        let step_port_kill = Step {
            capability: "diag.port_kill".into(),
            args: args_port_kill,
        };
        let cap_port_kill = catalog.get("diag.port_kill").expect("cap diag.port_kill");
        let changes_port_kill = changes_for(&step_port_kill, cap_port_kill, &ctx, &pendiente).expect("changes");
        assert_eq!(changes_port_kill.len(), 1);
        match &changes_port_kill[0] {
            Change::PortKill { port, force } => {
                assert_eq!(*port, 8080);
                assert!(*force);
            }
            _ => panic!("debe ser PortKill"),
        }
    }
}
