//! Modular execution and change application dispatch.
#![allow(unused_imports, dead_code)]

pub mod fs;
pub mod git;
pub mod project;
pub mod sandbox;
pub mod service;

pub use fs::{collect_project_files, detect_project_language, scan_workspace_projects};
pub use git::{gitignore_template, init_project_git_repo};

use crate::capability::Capability;
use crate::ctx::Ctx;
use crate::plan::Step;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

mod change;
mod pending_changes;

pub use change::Change;
pub use pending_changes::PendingChanges;

pub fn changes_for(
    step: &Step,
    cap: &Capability,
    ctx: &Ctx,
    pending: &PendingChanges,
) -> Result<Vec<Change>> {
    let a = &step.args;
    if let Some(c) = fs::changes_for(cap.name.as_str(), a, ctx, pending)? {
        return Ok(c);
    }
    if let Some(c) = git::changes_for(cap.name.as_str(), a, ctx)? {
        return Ok(c);
    }
    if let Some(c) = service::changes_for(cap.name.as_str(), a, ctx)? {
        return Ok(c);
    }
    if let Some(c) = project::changes_for(cap.name.as_str(), a, ctx, pending)? {
        return Ok(c);
    }
    match cap.name.as_str() {
        "secret.grant" => {
            let secret = a
                .get("secret")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("debes especificar el secreto a conceder"))?;
            let minutes = a
                .get("minutes")
                .and_then(|m| m.parse::<i64>().ok())
                .unwrap_or(10);
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

        "secret.list" => Ok(vec![Change::SecretList {
            state_dir: ctx.state.clone(),
            grants_path: ctx.grants_path(),
        }]),

        "secret.set" => {
            let key = a
                .get("key")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("clave requerida"))?;
            let value = a
                .get("value")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("valor requerido"))?;
            Ok(vec![Change::SecretSet {
                key,
                value,
                state_dir: ctx.state.clone(),
            }])
        }

        "secret.read" => {
            let key = a
                .get("key")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("clave requerida"))?;
            Ok(vec![Change::SecretRead {
                key,
                state_dir: ctx.state.clone(),
                grants_path: ctx.grants_path(),
            }])
        }

        "spec.create_ticket" => {
            let ticket_id = a
                .get("ticket_id")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("ticket_id requerido"))?;
            let title = a
                .get("title")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("title requerido"))?;
            let description = a.get("description").cloned();
            let phase = a.get("phase").cloned();
            let target_ws = a
                .get("project")
                .map(|p| ctx.workspace.join(p))
                .unwrap_or_else(|| {
                    ctx.current_project
                        .clone()
                        .unwrap_or_else(|| ctx.workspace.clone())
                });
            Ok(vec![Change::TicketCreate {
                ticket_id,
                title,
                description,
                phase,
                workspace: target_ws,
            }])
        }

        "spec.update_ticket" => {
            let ticket_id = a
                .get("ticket_id")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("ticket_id requerido"))?;
            let status = a
                .get("status")
                .cloned()
                .unwrap_or_else(|| "completado".into());
            let target_ws = a
                .get("project")
                .map(|p| ctx.workspace.join(p))
                .unwrap_or_else(|| {
                    ctx.current_project
                        .clone()
                        .unwrap_or_else(|| ctx.workspace.clone())
                });
            Ok(vec![Change::TicketUpdateStatus {
                ticket_id,
                status,
                workspace: target_ws,
            }])
        }

        "spec.list_tickets" => {
            let filter = a.get("filter").cloned();
            let target_ws = a
                .get("project")
                .map(|p| ctx.workspace.join(p))
                .unwrap_or_else(|| {
                    ctx.current_project
                        .clone()
                        .unwrap_or_else(|| ctx.workspace.clone())
                });
            Ok(vec![Change::TicketList {
                workspace: target_ws,
                filter,
            }])
        }

        "memory.index" => Ok(vec![Change::MemoryIndex {
            workspace: ctx.workspace.clone(),
        }]),

        "memory.search" => {
            let query = a
                .get("query")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("query requerido"))?;
            let limit = a
                .get("limit")
                .and_then(|l| l.parse::<usize>().ok())
                .unwrap_or(5);
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

        "env.sync" => Ok(vec![Change::EnvProfileSync {
            workspace: ctx.workspace.clone(),
        }]),

        "env.profile_status" => Ok(vec![Change::EnvProfileStatus {
            workspace: ctx.workspace.clone(),
        }]),

        "quota.status" => Ok(vec![Change::QuotaStatus {
            workspace: ctx.workspace.clone(),
        }]),

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
            // T17.2: resolve project_path from the optional 'project' param.
            let project_path = a.get("project").map(|p| {
                let candidate = std::path::Path::new(p);
                if candidate.is_absolute() {
                    candidate.to_path_buf()
                } else {
                    ctx.workspace.join(p)
                }
            });
            Ok(vec![Change::UiDiffViewer {
                workspace: ctx.workspace.clone(),
                target,
                project_path,
            }])
        }

        "ui.terminal" => {
            let command = a.get("command").cloned();
            Ok(vec![Change::UiTerminal { command }])
        }

        "dev.workspace" => {
            let project = a.get("project").cloned();
            let action = a.get("action").cloned();
            Ok(vec![Change::DevWorkspace {
                workspace: ctx.workspace.clone(),
                project,
                action,
            }])
        }

        "notify.list" => Ok(vec![Change::NotifyList {
            workspace: ctx.workspace.clone(),
        }]),

        "notify.action" => {
            let id = a.get("id").cloned().unwrap_or_default();
            let action_str = a.get("action").map(String::as_str).unwrap_or("dismiss");
            let action = match action_str {
                "approve" | "aprobar" => antos_protocol::NotificationAction::Approve,
                "reject" | "rechazar" | "rollback" => antos_protocol::NotificationAction::Reject,
                "diff" | "view_diff" => antos_protocol::NotificationAction::ViewDiff,
                _ => antos_protocol::NotificationAction::Dismiss,
            };
            Ok(vec![Change::NotifyAction {
                workspace: ctx.workspace.clone(),
                notification_id: id,
                action,
            }])
        }

        "mesh.status" => Ok(vec![Change::MeshStatus {
            workspace: ctx.workspace.clone(),
        }]),

        "mesh.connect" => {
            let address = a
                .get("address")
                .cloned()
                .unwrap_or_else(|| "127.0.0.1:9042".into());
            Ok(vec![Change::MeshConnect {
                workspace: ctx.workspace.clone(),
                address,
            }])
        }

        "mesh.pair" => Ok(vec![Change::MeshPair {
            workspace: ctx.workspace.clone(),
        }]),

        "flow.swarm_status" => Ok(vec![Change::SwarmStatus {
            workspace: ctx.workspace.clone(),
        }]),

        "flow.dispatch_remote" => {
            let ticket_id = a.get("ticket_id").cloned().unwrap_or_else(|| "T1.1".into());
            let role_str = a.get("role").map(String::as_str).unwrap_or("coder");
            let role = match role_str {
                "arquitecto" | "architect" => antos_protocol::AgentRole::Architect,
                "qa" | "tester" => antos_protocol::AgentRole::QA,
                "auditor" => antos_protocol::AgentRole::Auditor,
                _ => antos_protocol::AgentRole::Coder,
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

        "vfs.validate_write" => {
            let file_path = a.get("file_path").cloned().unwrap_or_default();
            let content = a.get("content").cloned();
            Ok(vec![Change::VfsValidateWrite {
                workspace: ctx.workspace.clone(),
                file_path,
                content,
            }])
        }

        "vfs.guard_status" => Ok(vec![Change::VfsGuardStatus {
            workspace: ctx.workspace.clone(),
        }]),

        "ebpf.status" => Ok(vec![Change::EbpfStatus {
            workspace: ctx.workspace.clone(),
        }]),

        "ebpf.audit_log" => {
            let limit = a
                .get("limit")
                .and_then(|l| l.parse::<usize>().ok())
                .unwrap_or(20);
            let pid = a.get("pid").and_then(|p| p.parse::<u32>().ok());
            Ok(vec![Change::EbpfAuditLog {
                workspace: ctx.workspace.clone(),
                limit,
                pid,
            }])
        }

        "profile.run" => {
            let command = a
                .get("command")
                .cloned()
                .unwrap_or_else(|| "cargo test".into());
            Ok(vec![Change::ProfileRun {
                workspace: ctx.workspace.clone(),
                command,
            }])
        }

        "profile.analyze" => Ok(vec![Change::ProfileAnalyze {
            workspace: ctx.workspace.clone(),
        }]),

        "lsp.start" => {
            let mode = a.get("mode").cloned().unwrap_or_else(|| "stdio".into());
            Ok(vec![Change::LspStart {
                workspace: ctx.workspace.clone(),
                mode,
            }])
        }

        "lsp.status" => Ok(vec![Change::LspStatus {
            workspace: ctx.workspace.clone(),
        }]),

        "collab.session" => {
            let file = a
                .get("file")
                .cloned()
                .unwrap_or_else(|| "src/main.rs".into());
            let ticket = a.get("ticket").cloned();
            Ok(vec![Change::CollabSession {
                workspace: ctx.workspace.clone(),
                file,
                ticket,
            }])
        }

        "dap.attach" => {
            let command = a
                .get("command")
                .cloned()
                .unwrap_or_else(|| "cargo test".into());
            Ok(vec![Change::DapAttach {
                workspace: ctx.workspace.clone(),
                command,
            }])
        }

        "desktop.session" => {
            let action = a.get("action").cloned();
            Ok(vec![Change::DesktopSession {
                workspace: ctx.workspace.clone(),
                action,
            }])
        }

        "desktop.keys" => Ok(vec![Change::DesktopKeys {
            workspace: ctx.workspace.clone(),
        }]),

        "barra.status" => Ok(vec![Change::BarraStatus {
            workspace: ctx.workspace.clone(),
        }]),

        "barra.notify" => {
            let category = a
                .get("category")
                .cloned()
                .unwrap_or_else(|| "general".into());
            let message = a
                .get("message")
                .cloned()
                .unwrap_or_else(|| "Notificación de sistema".into());
            let urgent = a.get("urgent").map(|v| v == "true").unwrap_or(false);
            Ok(vec![Change::BarraNotify {
                workspace: ctx.workspace.clone(),
                category,
                message,
                urgent,
            }])
        }

        "boot.pipeline" => {
            let action = a.get("action").cloned().unwrap_or_else(|| "status".into());
            Ok(vec![Change::BootPipeline {
                workspace: ctx.workspace.clone(),
                action,
            }])
        }

        "plugin.list" => Ok(vec![Change::PluginList {
            workspace: ctx.workspace.clone(),
        }]),

        "plugin.run" => {
            let plugin = a.get("plugin").cloned().unwrap_or_default();
            let action = a.get("action").cloned().unwrap_or_else(|| "run".into());
            let mut params = std::collections::BTreeMap::new();
            if let Some(p_str) = a.get("params") {
                for pair in p_str.split(',') {
                    if let Some((k, v)) = pair.split_once('=') {
                        params.insert(k.trim().to_string(), v.trim().to_string());
                    }
                }
            }
            Ok(vec![Change::PluginRun {
                workspace: ctx.workspace.clone(),
                plugin,
                action,
                params,
            }])
        }

        "plugin.install" => {
            let p = a.get("path").cloned().unwrap_or_else(|| ".".into());
            Ok(vec![Change::PluginInstall {
                workspace: ctx.workspace.clone(),
                source_path: ctx.workspace.join(p),
            }])
        }

        "ui.screenshot" => {
            let target = a.get("target").cloned();
            let path = a.get("path").map(|p| ctx.workspace.join(p));
            Ok(vec![Change::UiScreenshot {
                workspace: ctx.workspace.clone(),
                target,
                path,
            }])
        }

        "ui.inspect_visual" => {
            let target = a.get("target").cloned().unwrap_or_else(|| "desktop".into());
            let criteria = a
                .get("criteria")
                .map(|c| c.split(';').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default();
            Ok(vec![Change::UiInspectVisual {
                workspace: ctx.workspace.clone(),
                target,
                criteria,
            }])
        }

        "disk.list" => Ok(vec![Change::DiskList {
            workspace: ctx.workspace.clone(),
        }]),

        "disk.inspect" => {
            let device = a
                .get("device")
                .cloned()
                .unwrap_or_else(|| "/dev/nvme0n1".into());
            Ok(vec![Change::DiskInspect {
                workspace: ctx.workspace.clone(),
                device,
            }])
        }

        "disk.partition" => {
            let device = a
                .get("device")
                .cloned()
                .unwrap_or_else(|| "/dev/nvme0n1".into());
            let clean = a.get("clean").map(|v| v == "true").unwrap_or(false);
            let dry_run = a.get("dry_run").map(|v| v == "true").unwrap_or(true);
            Ok(vec![Change::DiskPartition {
                workspace: ctx.workspace.clone(),
                device,
                clean,
                dry_run,
            }])
        }

        "install.prepare" => {
            let target_device = a
                .get("target_device")
                .cloned()
                .unwrap_or_else(|| "/dev/nvme0n1".into());
            let target_mount = a.get("target_mount").cloned();
            Ok(vec![Change::InstallPrepare {
                workspace: ctx.workspace.clone(),
                target_device,
                target_mount,
            }])
        }

        "install.deploy" => {
            let target_device = a
                .get("target_device")
                .cloned()
                .unwrap_or_else(|| "/dev/nvme0n1".into());
            let clean_install = a.get("clean").map(|v| v == "true").unwrap_or(false);
            let dry_run = a.get("dry_run").map(|v| v == "true").unwrap_or(true);
            let username = a.get("username").cloned().unwrap_or_else(|| "antos".into());
            let hostname = a
                .get("hostname")
                .cloned()
                .unwrap_or_else(|| "antos-box".into());
            let keymap = a.get("keymap").cloned().unwrap_or_else(|| "us".into());
            let system = a
                .get("system")
                .cloned()
                .unwrap_or_else(antos_protocol::default_system);
            let config = antos_protocol::InstallConfig {
                target_device,
                clean_install,
                target_mount: "/mnt/antos".into(),
                hostname,
                username,
                timezone: "UTC".into(),
                keymap,
                system,
                password_hash: a.get("password_hash").cloned(),
                dry_run,
            };
            Ok(vec![Change::InstallDeploy {
                workspace: ctx.workspace.clone(),
                config,
            }])
        }

        "bootloader.probe" => {
            let esp_path = a.get("esp_path").cloned();
            Ok(vec![Change::BootloaderProbe {
                workspace: ctx.workspace.clone(),
                esp_path,
            }])
        }

        "bootloader.install" => {
            let target_device = a
                .get("target_device")
                .cloned()
                .unwrap_or_else(|| "/dev/nvme0n1".into());
            let esp_mount = a.get("esp_path").cloned().unwrap_or_else(|| "/boot".into());
            let efi_partition = a
                .get("efi_partition")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(1);
            let timeout_seconds = a
                .get("timeout")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(5);
            let dry_run = a.get("dry_run").map(|v| v == "true").unwrap_or(true);
            // T36.1: por defecto la vía antOS Linux (solo sondeo); la
            // disposición bare-metal se pide explícitamente y necesita el
            // binario EFI real fuera de la simulación.
            let target = match a.get("target").map(String::as_str) {
                Some("bare-metal") | Some("bare_metal") => antos_protocol::BootTarget::BareMetal,
                _ => antos_protocol::BootTarget::NixOs,
            };
            let efi_binary = a.get("efi_binary").cloned();
            let config = antos_protocol::BootloaderConfig {
                esp_mount,
                target_device,
                efi_partition,
                default_os: "antos".into(),
                timeout_seconds,
                detected_os: Vec::new(),
                dry_run,
                target,
                efi_binary,
            };
            Ok(vec![Change::BootloaderInstall {
                workspace: ctx.workspace.clone(),
                config,
            }])
        }

        "microvm.spawn" => {
            let vm_id = a
                .get("vm_id")
                .cloned()
                .unwrap_or_else(|| format!("vm-{}", chrono::Local::now().format("%Y%m%d%H%M%S")));
            let vcpu_count = a
                .get("cpus")
                .and_then(|v| v.parse::<u8>().ok())
                .unwrap_or(2);
            let memory_mb = a
                .get("memory")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(512);
            let kernel_image = a
                .get("kernel")
                .cloned()
                .unwrap_or_else(|| "/boot/antos-vmlinuz".into());
            let config = antos_protocol::MicrovmConfig {
                vm_id,
                vcpu_count,
                memory_mb,
                kernel_image,
                initrd_image: None,
                overlay_disk: None,
                vsock_port: 5252,
                command: None,
            };
            Ok(vec![Change::MicrovmSpawn {
                state_dir: ctx.state.clone(),
                config,
            }])
        }

        "microvm.exec" => {
            let vm_id = a
                .get("vm_id")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("se requiere vm_id"))?;
            let command = a
                .get("command")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("se requiere command"))?;
            Ok(vec![Change::MicrovmExec {
                state_dir: ctx.state.clone(),
                vm_id,
                command,
            }])
        }

        "microvm.destroy" => {
            let vm_id = a
                .get("vm_id")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("se requiere vm_id"))?;
            Ok(vec![Change::MicrovmDestroy {
                state_dir: ctx.state.clone(),
                vm_id,
            }])
        }

        "pkg.install" => {
            let package = a
                .get("package")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("se requiere package"))?;
            let dry_run = a
                .get("dry_run")
                .and_then(|v| v.parse::<bool>().ok())
                .unwrap_or(false);
            Ok(vec![Change::PackageInstall {
                state_dir: ctx.state.clone(),
                package,
                dry_run,
            }])
        }

        "pkg.remove" => {
            let package = a
                .get("package")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("se requiere package"))?;
            Ok(vec![Change::PackageRemove {
                state_dir: ctx.state.clone(),
                package,
            }])
        }

        "pkg.rollback" => {
            let generation = a.get("generation").and_then(|v| v.parse::<u64>().ok());
            Ok(vec![Change::PackageRollback {
                state_dir: ctx.state.clone(),
                generation,
            }])
        }

        "pkg.list" => Ok(vec![Change::PackageList {
            state_dir: ctx.state.clone(),
        }]),

        "pkg.verify" => Ok(vec![Change::PackageVerify {
            state_dir: ctx.state.clone(),
        }]),

        "autopilot.start" => {
            let interval = a
                .get("interval")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(5);
            let auto_merge = a
                .get("auto_merge")
                .and_then(|v| v.parse::<bool>().ok())
                .unwrap_or(false);
            let config = antos_protocol::AutopilotConfig {
                enabled: true,
                poll_interval_secs: interval,
                watch_paths: Vec::new(),
                auto_merge,
                target_branch: "master".to_string(),
            };
            Ok(vec![Change::AutopilotStart {
                state_dir: ctx.state.clone(),
                workspace_dir: ctx.workspace.clone(),
                config,
            }])
        }

        "autopilot.stop" => Ok(vec![Change::AutopilotStop {
            state_dir: ctx.state.clone(),
            workspace_dir: ctx.workspace.clone(),
        }]),

        "autopilot.status" => Ok(vec![Change::AutopilotStatus {
            state_dir: ctx.state.clone(),
            workspace_dir: ctx.workspace.clone(),
        }]),

        "autopilot.scan" => Ok(vec![Change::AutopilotScan {
            state_dir: ctx.state.clone(),
            workspace_dir: ctx.workspace.clone(),
        }]),

        "autopilot.resolve" => {
            let incident_id = a
                .get("incident_id")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("se requiere incident_id"))?;
            let approve = a
                .get("approve")
                .and_then(|v| v.parse::<bool>().ok())
                .unwrap_or(true);
            Ok(vec![Change::AutopilotResolve {
                state_dir: ctx.state.clone(),
                workspace_dir: ctx.workspace.clone(),
                incident_id,
                approve,
            }])
        }

        "web.start" => {
            let bind = a.get("bind").cloned().unwrap_or_else(|| "127.0.0.1".into());
            let port = a
                .get("port")
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or(8088);
            let config = antos_protocol::WebConsoleConfig {
                bind_addr: bind,
                port,
                auth_required: true,
                ws_ping_interval_secs: 30,
            };
            Ok(vec![Change::WebStart {
                state_dir: ctx.state.clone(),
                workspace_dir: ctx.workspace.clone(),
                config,
            }])
        }

        "web.stop" => Ok(vec![Change::WebStop {
            state_dir: ctx.state.clone(),
        }]),

        "web.status" => Ok(vec![Change::WebStatus {
            state_dir: ctx.state.clone(),
        }]),

        "web.token" => {
            let label = a.get("label").cloned();
            let ttl = a.get("ttl").and_then(|v| v.parse::<u64>().ok());
            Ok(vec![Change::WebToken {
                state_dir: ctx.state.clone(),
                label,
                ttl,
            }])
        }

        "test.reproduce" => {
            let error_log = a
                .get("error")
                .or_else(|| a.get("error_log"))
                .cloned()
                .unwrap_or_default();
            let target_file = a.get("file").or_else(|| a.get("target_file")).cloned();
            Ok(vec![Change::TestReproduce {
                workspace: ctx.workspace.clone(),
                state_dir: ctx.state.clone(),
                error_log,
                target_file,
            }])
        }

        "test.gen" => {
            let target = a.get("target").cloned().unwrap_or_default();
            let suite_type = a
                .get("suite_type")
                .cloned()
                .unwrap_or_else(|| "unit".into());
            let cases = a
                .get("cases")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(3);
            Ok(vec![Change::TestGen {
                workspace: ctx.workspace.clone(),
                target,
                suite_type,
                cases,
            }])
        }

        "ci.run" => {
            let stage = a.get("stage").cloned();
            let fast = a.get("fast").map(|v| v == "true").unwrap_or(false);
            Ok(vec![Change::CiRun {
                workspace: ctx.workspace.clone(),
                state_dir: ctx.state.clone(),
                stage,
                fast,
            }])
        }

        "ci.status" => Ok(vec![Change::CiStatus {
            state_dir: ctx.state.clone(),
        }]),

        "git.hook" => {
            let action = a.get("action").cloned().unwrap_or_else(|| "status".into());
            Ok(vec![Change::GitHookManage {
                workspace: ctx.workspace.clone(),
                action,
            }])
        }

        "snapshot.create" => {
            let label = a.get("label").cloned();
            let author = a.get("author").cloned();
            Ok(vec![Change::SnapshotCreate {
                workspace: ctx.workspace.clone(),
                state_dir: ctx.state.clone(),
                label,
                author,
            }])
        }

        "snapshot.list" => Ok(vec![Change::SnapshotList {
            state_dir: ctx.state.clone(),
        }]),

        "snapshot.restore" => {
            let id_or_label = a.get("id").cloned().unwrap_or_else(|| "latest".into());
            let no_rescue = a.get("no_rescue").map(|v| v == "true").unwrap_or(false);
            Ok(vec![Change::SnapshotRestore {
                workspace: ctx.workspace.clone(),
                state_dir: ctx.state.clone(),
                id_or_label,
                create_rescue: !no_rescue,
            }])
        }

        "snapshot.delete" => {
            let id = a.get("id").cloned().unwrap_or_default();
            Ok(vec![Change::SnapshotDelete {
                state_dir: ctx.state.clone(),
                id,
            }])
        }

        "bench.run" => {
            let target = a.get("target").cloned();
            Ok(vec![Change::BenchRun {
                workspace: ctx.workspace.clone(),
                state_dir: ctx.state.clone(),
                target,
            }])
        }

        "bench.diff" => {
            let against_branch = a.get("against").cloned();
            let threshold_pct = a.get("threshold").and_then(|t| t.parse::<f64>().ok());
            Ok(vec![Change::BenchDiff {
                workspace: ctx.workspace.clone(),
                state_dir: ctx.state.clone(),
                against_branch,
                threshold_pct,
            }])
        }

        "bench.history" => Ok(vec![Change::BenchHistory {
            state_dir: ctx.state.clone(),
        }]),

        "issue.list" => Ok(vec![Change::IssueList {
            workspace: ctx.workspace.clone(),
            state_dir: ctx.state.clone(),
        }]),

        "issue.import" => {
            let id = a.get("id").cloned().unwrap_or_else(|| "42".into());
            Ok(vec![Change::IssueImport {
                workspace: ctx.workspace.clone(),
                state_dir: ctx.state.clone(),
                id,
            }])
        }

        "pr.create" => {
            let title = a.get("title").cloned();
            let base_branch = a.get("base").cloned();
            let draft = a.get("draft").map(|v| v == "true").unwrap_or(false);
            Ok(vec![Change::PrCreate {
                workspace: ctx.workspace.clone(),
                state_dir: ctx.state.clone(),
                title,
                base_branch,
                draft,
            }])
        }

        "pr.status" => {
            let number = a.get("number").and_then(|n| n.parse::<u64>().ok());
            Ok(vec![Change::PrStatus {
                state_dir: ctx.state.clone(),
                number,
            }])
        }

        "doc.arch" => {
            let kind = a.get("kind").cloned();
            Ok(vec![Change::DocArch {
                workspace: ctx.workspace.clone(),
                kind,
            }])
        }

        "doc.sync" => {
            let target_file = a.get("target").cloned();
            Ok(vec![Change::DocSync {
                workspace: ctx.workspace.clone(),
                target_file,
            }])
        }

        "doc.check" => {
            let target_file = a.get("target").cloned();
            Ok(vec![Change::DocCheck {
                workspace: ctx.workspace.clone(),
                target_file,
            }])
        }

        other => bail!("no hay implementación para la capacidad «{other}»"),
    }
}

pub fn apply(changes: &[Change]) -> Result<Vec<String>> {
    let mut output = Vec::new();
    for change in changes {
        if let Some(res) = fs::apply(change)? {
            if !res.is_empty() {
                output.push(res);
            }
            continue;
        }
        if let Some(res) = git::apply(change)? {
            if !res.is_empty() {
                output.push(res);
            }
            continue;
        }
        if let Some(res) = service::apply(change)? {
            if !res.is_empty() {
                output.push(res);
            }
            continue;
        }
        if let Some(res) = project::apply(change)? {
            if !res.is_empty() {
                output.push(res);
            }
            continue;
        }
        match change {
            Change::SecretGrant {
                secret,
                minutes,
                reason,
                grants_path,
            } => {
                let mut grants = crate::grants::Grants::load(grants_path)?;
                grants.grant_with_reason(secret, *minutes, reason.clone());
                grants.save(grants_path)?;
                let motivo = reason
                    .as_deref()
                    .map(|r| format!(" para «{r}»"))
                    .unwrap_or_default();
                output.push(format!(
                    "concesión temporal otorgada a «{secret}» por {minutes} minutos{motivo}"
                ));
            }
            Change::SecretRevoke {
                secret,
                grants_path,
            } => {
                let mut grants = crate::grants::Grants::load(grants_path)?;
                grants.revoke(secret);
                grants.save(grants_path)?;
                output.push(format!("concesión revocada: «{secret}»"));
            }
            Change::SecretList {
                state_dir,
                grants_path,
            } => {
                let list = crate::vault::list_secrets(state_dir)?;
                let grants = crate::grants::Grants::load(grants_path)?;
                let active = grants.list_active();
                let mut lines = Vec::new();
                lines.push(format!(
                    "secretos en bóveda: {} | concesiones activas: {}",
                    list.len(),
                    active.len()
                ));
                for s in list {
                    let granted = grants.is_granted("secret.read")
                        || grants.is_granted(&format!("secret.{}", s.key));
                    lines.push(format!(
                        "  - {} ({} bytes, concedido: {})",
                        s.key, s.length, granted
                    ));
                }
                output.push(lines.join("\n"));
            }
            Change::SecretSet {
                key,
                value,
                state_dir,
            } => {
                crate::vault::set_secret(state_dir, key, value)?;
                output.push(format!(
                    "secreto «{key}» guardado de forma segura en la bóveda de antOS"
                ));
            }
            Change::SecretRead {
                key,
                state_dir,
                grants_path,
            } => {
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
                output.push(format!(
                    "ticket «{}» creado exitosamente en {}",
                    ticket_id,
                    path.display()
                ));
            }
            Change::TicketUpdateStatus {
                ticket_id,
                status,
                workspace,
            } => {
                let st = match status.to_lowercase().as_str() {
                    "completado" | "done" | "hecho" => antos_protocol::TicketStatus::Completed,
                    "progreso" | "en_progreso" | "in_progress" => {
                        antos_protocol::TicketStatus::InProgress
                    }
                    "revision" | "revisión" | "review" => antos_protocol::TicketStatus::InReview,
                    _ => antos_protocol::TicketStatus::Pending,
                };
                let engine = crate::spec::SpecEngine::global();
                engine.update_ticket_status(workspace, ticket_id, st)?;
                output.push(format!(
                    "estado del ticket «{}» actualizado a {:?}",
                    ticket_id, st
                ));
            }
            Change::TicketList { workspace, filter } => {
                let engine = crate::spec::SpecEngine::global();
                let tickets = engine.list_tickets(workspace)?;
                let mut lines = Vec::new();
                lines.push(format!("tickets en el proyecto: {}", tickets.len()));
                for t in tickets {
                    if let Some(ref f) = filter {
                        let st_str = format!("{:?}", t.status).to_lowercase();
                        if !st_str.contains(&f.to_lowercase()) {
                            continue;
                        }
                    }
                    lines.push(format!(
                        "  - [{}] {} [{:?}] ({})",
                        t.id, t.title, t.status, t.phase
                    ));
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
            Change::MemorySearch {
                workspace,
                query,
                limit,
            } => {
                let db_path = crate::memory::MemoryEngine::default_db_path(workspace);
                let store = crate::memory::MemoryEngine::load(&db_path)?;
                let hits = crate::memory::MemoryEngine::search(&store, query, *limit);
                if hits.is_empty() {
                    output.push(format!("no se encontraron coincidencias para «{query}»"));
                } else {
                    let mut lines = Vec::new();
                    lines.push(format!(
                        "coincidencias semánticas para «{query}» ({}):",
                        hits.len()
                    ));
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
                        lines.push(format!(
                            "relaciones en el grafo para «{t}» ({}):",
                            related.len()
                        ));
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
                    Some(ref p) => crate::env::EnvProfile::from_str_loose(p)
                        .unwrap_or(crate::env::EnvProfile::Base),
                    None => crate::env::EnvEngine::detect_stack(workspace)
                        .unwrap_or(crate::env::EnvProfile::Base),
                };
                let summary = crate::env::EnvEngine::init_profile(
                    workspace,
                    prof,
                    *create_devbox,
                    *create_flake,
                )?;
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
                lines.push(format!(
                    "sincronización de entorno para {}",
                    workspace.display()
                ));
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
                        lines.push(format!(
                            "perfil activo: «{}» ({} paquetes)",
                            c.profile,
                            c.packages.len()
                        ));
                        let statuses = crate::env::EnvEngine::check_toolchains(workspace)?;
                        for s in statuses {
                            let mark = if s.available { "●" } else { "○" };
                            lines.push(format!(
                                "  {mark} {:<16} disponible: {}",
                                s.name, s.available
                            ));
                        }
                    }
                    None => {
                        let detected = crate::env::EnvEngine::detect_stack(workspace);
                        let det_str = detected.map(|d| d.as_str()).unwrap_or("no detectado");
                        lines.push(format!(
                            "sin perfil explícito configurado (stack detectado: {det_str})"
                        ));
                    }
                }
                output.push(lines.join("\n"));
            }
            Change::QuotaStatus { workspace } => {
                let q = crate::sandbox::quota::load_quota(workspace)?;
                let cgroup_supported = crate::sandbox::quota::CgroupV2Manager::is_available();
                let cgroup_str = if cgroup_supported {
                    "activo (cgroups v2)"
                } else {
                    "modo proceso/watchdog"
                };
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
            Change::UiDiffViewer {
                workspace,
                target,
                project_path,
            } => {
                // T17.2: prefer project_path (explicit project); fall back to workspace root.
                let diff_dir = project_path.as_deref().unwrap_or(workspace);
                let antos_root = crate::git::detect_antos_root();

                // Verify the diff_dir has a git repo that is NOT the antOS OS repo.
                let has_git =
                    crate::git::find_git_root_with_ceiling(diff_dir, antos_root.as_deref())
                        .is_some();

                if !has_git {
                    // Enumerate files in the project dir as an informational summary.
                    let file_list = collect_project_files(diff_dir, 30);
                    if file_list.is_empty() {
                        output.push(format!(
                            "project '{}' has no files and no Git repository",
                            diff_dir.display()
                        ));
                    } else {
                        output.push(format!(
                            "project '{}' is not a Git repository — detected {} file(s):\n  {}",
                            diff_dir.display(),
                            file_list.len(),
                            file_list.join("\n  ")
                        ));
                        output.push(format!(
                            "hint: run 'git init {}' or 'antos project init <name>' to start version control",
                            diff_dir.display()
                        ));
                    }
                } else {
                    // Build git diff with ceiling to enforce isolation.
                    let ceiling_val = antos_root
                        .as_deref()
                        .and_then(|r| r.parent())
                        .map(|p| p.display().to_string())
                        .unwrap_or_default();

                    let target_ref = target.as_deref().unwrap_or("HEAD");
                    let mut check_head = std::process::Command::new("git");
                    check_head
                        .current_dir(diff_dir)
                        .args(["rev-parse", "--verify", "HEAD"]);
                    if !ceiling_val.is_empty() {
                        check_head.env("GIT_CEILING_DIRECTORIES", &ceiling_val);
                    }
                    let has_commits = check_head
                        .output()
                        .map(|o| o.status.success())
                        .unwrap_or(false);

                    if !has_commits && target_ref == "HEAD" {
                        output.push(format!(
                            "project '{}' is a newly initialized Git repository (initial commit pending)",
                            diff_dir.display()
                        ));
                        return Ok(output);
                    }

                    let mut cmd = std::process::Command::new("git");
                    cmd.current_dir(diff_dir).args(["diff", target_ref]);
                    if !ceiling_val.is_empty() {
                        cmd.env("GIT_CEILING_DIRECTORIES", &ceiling_val);
                    }
                    let git_out = cmd.output();

                    match git_out {
                        Ok(o) if o.status.success() => {
                            let diff_text = String::from_utf8_lossy(&o.stdout);
                            let files =
                                crate::diff_view::DiffEngine::parse_unified_diff(&diff_text);
                            if files.is_empty() {
                                output.push(format!(
                                    "no differences against '{}' in project '{}'",
                                    target.as_deref().unwrap_or("HEAD"),
                                    diff_dir.display()
                                ));
                            } else {
                                output.push(crate::diff_view::DiffEngine::render_terminal(&files));
                            }
                        }
                        _ => {
                            output.push(format!(
                                "failed to run git diff in project '{}'",
                                diff_dir.display()
                            ));
                        }
                    }
                }
            }
            Change::UiTerminal { command } => {
                let mut session = crate::vte::TerminalSession::new("interactive");
                if let Some(ref cmd) = command {
                    let _ = session.execute_command(cmd);
                    output.push(format!(
                        "terminal: ejecutado «{cmd}» con código {:?}",
                        session.exit_code
                    ));
                } else {
                    output.push(format!(
                        "terminal interactivo listo con shell {}",
                        session.active_shell
                    ));
                }
            }
            Change::DevWorkspace {
                workspace,
                project,
                action,
            } => {
                if let Some("status") = action.as_deref() {
                    let status = crate::dev_tui::DevWorkspaceManager::get_status(
                        project.as_deref(),
                        workspace,
                    );
                    output.push(format!(
                        "dev workspace: editor={} term={}x{}",
                        status.editor_command, status.term_columns, status.term_rows
                    ));
                } else {
                    crate::dev_tui::DevWorkspaceManager::launch(
                        project.as_deref(),
                        workspace,
                        false,
                    )?;
                    output.push("dev workspace: blueprint renderizado exitosamente".into());
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
                        lines.push(format!(
                            "  {} [{}] {} — {}",
                            read_mark, n.id, n.title, n.body
                        ));
                    }
                    output.push(lines.join("\n"));
                }
            }
            Change::NotifyAction {
                workspace,
                notification_id,
                action,
            } => {
                let (ok, msg) = crate::notification::NotificationEngine::global().handle_action(
                    workspace,
                    notification_id,
                    *action,
                )?;
                if ok {
                    output.push(msg);
                } else {
                    output.push(format!("falló la acción sobre la notificación: {msg}"));
                }
            }
            Change::MeshStatus { workspace } => {
                let status = crate::mesh::MeshEngine::global().status(workspace)?;
                let mut lines = Vec::new();
                lines.push(format!(
                    "nodo local: {} ({}) en {}",
                    status.local_node.id, status.local_node.hostname, status.local_node.address
                ));
                lines.push(format!(
                    "  recursos: {} cores, {} MB RAM, VRAM: {:?}, modelos: {}",
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
                        lines.push(format!(
                            "  • {} [{}] {}ms latencia (modelos: {})",
                            p.hostname,
                            p.address,
                            p.latency_ms,
                            p.resources.available_models.join(", ")
                        ));
                    }
                }
                output.push(lines.join("\n"));
            }
            Change::MeshConnect { workspace, address } => {
                let peer = crate::mesh::MeshEngine::global().connect_peer(workspace, address)?;
                output.push(format!(
                    "conectado al peer {} [{}] con {}ms de latencia",
                    peer.id, peer.address, peer.latency_ms
                ));
            }
            Change::MeshPair { workspace } => {
                let token = crate::mesh::MeshEngine::global().generate_pairing_token(workspace)?;
                output.push(format!(
                    "token de emparejamiento generado: {} (nodo {})",
                    token.token, token.node_id
                ));
            }
            Change::SwarmStatus { workspace } => {
                let status = crate::distributed::SwarmEngine::global().status(workspace)?;
                let mut lines = Vec::new();
                lines.push(format!(
                    "antOS Swarm: {} nodos activos, {} tareas en curso",
                    status.nodes.len(),
                    status.total_tasks
                ));
                for n in status.nodes {
                    let loc_str = if n.is_local { "[Local]" } else { "[Remoto]" };
                    lines.push(format!(
                        "  • {} {} ({}) - {} cores, VRAM: {:?}",
                        n.hostname, loc_str, n.address, n.cpu_cores, n.vram_available_mb
                    ));
                    for t in n.running_tasks {
                        lines.push(format!(
                            "      └─ Tarea {}: rol {:?}, rama {}",
                            t.ticket_id, t.role, t.worktree_branch
                        ));
                    }
                }
                output.push(lines.join("\n"));
            }
            Change::SwarmDispatch {
                workspace,
                ticket_id,
                role,
                node,
            } => {
                let task = crate::distributed::SwarmEngine::global().dispatch_remote_role(
                    workspace,
                    ticket_id,
                    *role,
                    node.as_deref(),
                )?;
                output.push(format!(
                    "rol {:?} del ticket {} despachado al nodo {} (rama {})",
                    task.role, task.ticket_id, task.assigned_node_id, task.worktree_branch
                ));
            }
            Change::VfsQuery { workspace, path } => {
                let vpath = path.as_deref().unwrap_or("/antfs");
                if vpath == "/antfs"
                    || vpath.ends_with('/')
                    || vpath == "/antfs/symbols"
                    || vpath == "/antfs/git"
                    || vpath == "/antfs/symbols/structs"
                    || vpath == "/antfs/symbols/functions"
                {
                    let entries = crate::vfs::VfsEngine::global().list_dir(workspace, vpath)?;
                    let mut lines = Vec::new();
                    lines.push(format!("entradas en {vpath} ({}):", entries.len()));
                    for e in entries {
                        let mark = if e.is_dir { "📁" } else { "📄" };
                        lines.push(format!(
                            "  {mark} {:<24} ({}, {} bytes)",
                            e.name, e.node_type, e.size
                        ));
                    }
                    output.push(lines.join("\n"));
                } else {
                    let content = crate::vfs::VfsEngine::global().read_path(workspace, vpath)?;
                    output.push(content);
                }
            }
            Change::VfsMount {
                workspace,
                mount_point,
            } => {
                let mnt =
                    crate::vfs::VfsEngine::global().mount(workspace, mount_point.as_deref())?;
                output.push(format!(
                    "sistema de ficheros virtual /antfs montado en {}",
                    mnt.display()
                ));
            }
            Change::VfsUnmount {
                workspace,
                mount_point,
            } => {
                crate::vfs::VfsEngine::global().unmount(workspace, mount_point.as_deref())?;
                output.push("sistema de ficheros virtual /antfs desmontado correctamente".into());
            }
            Change::VfsValidateWrite {
                workspace,
                file_path,
                content,
            } => {
                let target_path = workspace.join(file_path);
                let text = match content {
                    Some(c) => c.clone(),
                    None => {
                        if target_path.exists() {
                            std::fs::read_to_string(&target_path)?
                        } else {
                            bail!(
                                "el archivo {} no existe en el workspace",
                                target_path.display()
                            );
                        }
                    }
                };
                let res =
                    crate::vfs_guard::VfsGuardEngine::global().validate_content(file_path, &text);
                let mut lines = Vec::new();
                if res.is_valid {
                    lines.push(format!(
                        "✓ archivo «{}» sintácticamente correcto (lenguaje: {}, {} líneas)",
                        file_path, res.language, res.line_count
                    ));
                } else {
                    lines.push(format!(
                        "✗ archivo «{}» contiene {} error(es) sintáctico(s) (lenguaje: {}):",
                        file_path,
                        res.errors.len(),
                        res.language
                    ));
                    for err in res.errors {
                        lines.push(format!(
                            "    • L{}:{}: {}",
                            err.line, err.column, err.message
                        ));
                    }
                }
                output.push(lines.join("\n"));
            }
            Change::VfsGuardStatus { .. } => {
                let status = crate::vfs_guard::VfsGuardEngine::global().status()?;
                let mut lines = Vec::new();
                let state_str = if status.enabled { "ACTIVO" } else { "INACTIVO" };
                lines.push(format!("antOS VFS Guard: [{state_str}]"));
                lines.push(format!(
                    "  • Escrituras interceptadas: {}",
                    status.total_intercepted
                ));
                lines.push(format!(
                    "  • Escrituras rechazadas:    {}",
                    status.total_rejected
                ));
                if !status.rejected_paths.is_empty() {
                    lines.push(format!(
                        "  • Ficheros protegidos contra corrupción: {}",
                        status.rejected_paths.join(", ")
                    ));
                }
                output.push(lines.join("\n"));
            }
            Change::EbpfStatus { .. } => {
                let status = crate::ebpf::EbpfSentinelEngine::global().status()?;
                let mut lines = Vec::new();
                // T31.14: el backend real es lo que decide si hay vigilancia
                // de kernel de verdad; `lsm_enabled` solo dice si el host la
                // ofrecería si algún backend la usara (ver crate::ebpf).
                let backend_badge = match status.backend {
                    antos_protocol::EbpfBackend::LinuxBpf => "Kernel LSM (BPF real activo)",
                    antos_protocol::EbpfBackend::Simulated => {
                        "Simulado (ring buffer en espacio de usuario, sin BPF real)"
                    }
                };
                lines.push(format!("antOS eBPF LSM Sentinel: {}", backend_badge));
                lines.push(format!(
                    "  • Sondas activas ({}): {}",
                    status.active_probes.len(),
                    status.active_probes.join(", ")
                ));
                lines.push(format!(
                    "  • Eventos capturados en ring buffer: {} / {}",
                    status.total_events_captured, status.ring_buffer_capacity
                ));
                lines.push(format!(
                    "  • Intentos de evasión bloqueados:   {}",
                    status.total_violations_blocked
                ));
                output.push(lines.join("\n"));
            }
            Change::EbpfAuditLog { limit, pid, .. } => {
                let engine = crate::ebpf::EbpfSentinelEngine::global();
                let events = match pid {
                    Some(p) => engine.trace_pid(*p),
                    None => engine.get_audit_log(*limit),
                };
                let mut lines = Vec::new();
                lines.push(format!(
                    "registro de auditoría eBPF ({} eventos):",
                    events.len()
                ));
                if events.is_empty() {
                    lines.push("  (sin eventos de seguridad registrados)".into());
                } else {
                    for ev in events {
                        let action_mark = match ev.action_taken {
                            antos_protocol::EbpfSecurityAction::Allowed => "✓ PERMITIDO",
                            antos_protocol::EbpfSecurityAction::Blocked => "⛔ BLOQUEADO",
                            antos_protocol::EbpfSecurityAction::Audited => "👁 AUDITADO",
                        };
                        lines.push(format!(
                            "  {} [{}] PID {}:{} ➔ {} ({:?})",
                            action_mark, ev.id, ev.pid, ev.comm, ev.target_resource, ev.hook
                        ));
                        if let Some(ref r) = ev.violation_reason {
                            lines.push(format!("      └─ Motivo: {r}"));
                        }
                    }
                }
                output.push(lines.join("\n"));
            }
            Change::ProfileRun { workspace, command } => {
                let report = crate::profiler::ProfilerEngine::global()
                    .run_and_profile(workspace, command)?;
                let mut lines = Vec::new();
                let peak_mb = report.peak_memory_bytes as f64 / (1024.0 * 1024.0);
                lines.push(format!(
                    "antOS Profiler: «{}» completado en {} ms (código {})",
                    report.command, report.duration_ms, report.exit_code
                ));
                lines.push(format!("  • CPU: {} ms usuario, {} ms sistema | Memoria pico: {:.2} MB RSS | Page faults: {}",
                    report.cpu_user_ms, report.cpu_sys_ms, peak_mb, report.page_faults
                ));
                if !report.hotspots.is_empty() {
                    lines.push("  • Puntos calientes identificados:".into());
                    for h in &report.hotspots {
                        lines.push(format!(
                            "      - {:<32} {:.1}% CPU, {:.1}% Mem ({} llamadas)",
                            h.name, h.percentage_cpu, h.percentage_memory, h.calls_or_samples
                        ));
                    }
                }
                if !report.suggestions.is_empty() {
                    lines.push("  • Recomendaciones de optimización para agentes:".into());
                    for s in &report.suggestions {
                        lines.push(format!(
                            "      ★ [{}] {}: {}",
                            s.potential_impact, s.title, s.description
                        ));
                    }
                }
                output.push(lines.join("\n"));
            }
            Change::ProfileAnalyze { workspace } => {
                let (hotspots, suggestions) =
                    crate::profiler::ProfilerEngine::global().analyze_aggregate(workspace);
                let mut lines = Vec::new();
                lines.push("antOS Profiler: Análisis Agregado de Rendimiento".into());
                if hotspots.is_empty() && suggestions.is_empty() {
                    lines.push("  (sin métricas registradas; ejecuta «antos profile run <cmd>» para generar datos)".into());
                } else {
                    if !hotspots.is_empty() {
                        lines.push("  • Top cuellos de botella (Hotspots):".into());
                        for h in &hotspots {
                            lines.push(format!(
                                "      - {:<32} {:.1}% CPU, {:.1}% Mem",
                                h.name, h.percentage_cpu, h.percentage_memory
                            ));
                        }
                    }
                    if !suggestions.is_empty() {
                        lines.push("  • Sugerencias técnicas para Coder / QA:".into());
                        for s in &suggestions {
                            lines.push(format!(
                                "      ★ [{}] {}: {}",
                                s.potential_impact, s.title, s.description
                            ));
                        }
                    }
                }
                output.push(lines.join("\n"));
            }
            Change::LspStart { workspace, mode } => {
                if mode == "stdio" {
                    output.push("antOS LSP: iniciando servidor sobre transporte stdio...".into());
                    crate::lsp::LspServer::global().run_stdio(workspace)?;
                } else {
                    output.push(format!("antOS LSP: modo «{mode}» configurado"));
                }
            }
            Change::LspStatus { workspace } => {
                let status = crate::lsp::LspServer::global().get_status(workspace);
                let mut lines = Vec::new();
                lines.push("antOS Unified Language Server Protocol (LSP)".into());
                lines.push(format!(
                    "  • Estado:                {}",
                    if status.running {
                        "Activo (En ejecución)"
                    } else {
                        "Listo (En espera de conexiones)"
                    }
                ));
                lines.push(format!("  • Transporte:            {}", status.transport));
                lines.push(format!(
                    "  • Clientes conectados:   {}",
                    status.connected_clients
                ));
                lines.push(format!(
                    "  • Espacio de trabajo:    {}",
                    status.active_workspace
                ));
                lines.push(format!(
                    "  • Símbolos indexados:    {}",
                    status.indexed_symbols_count
                ));
                lines.push(format!(
                    "  • Capacidades activas:   {}",
                    status.capabilities.join(", ")
                ));
                output.push(lines.join("\n"));
            }
            Change::CollabSession {
                workspace,
                file,
                ticket,
            } => {
                let status = crate::collab::CollabEngine::global().start_session(
                    workspace,
                    file,
                    ticket.clone(),
                )?;
                let mut lines = Vec::new();
                lines.push(format!(
                    "antOS Pair Programming · Sesión iniciada: {}",
                    status.session_id
                ));
                lines.push(format!("  • Archivo compartido:   {}", status.file_path));
                lines.push(format!(
                    "  • Colaboradores:        {}",
                    status.collaborators.join(", ")
                ));
                lines.push(format!(
                    "  • Longitud del buffer:  {} caracteres",
                    status.buffer_length
                ));
                if let Some(t) = status.active_ticket_id {
                    lines.push(format!("  • Ticket vinculado:     {t}"));
                }
                output.push(lines.join("\n"));
            }
            Change::DapAttach {
                workspace: _,
                command,
            } => {
                let mut dap = crate::collab::DapServer::new("dap-exec".into(), command.clone());
                let bp = dap.add_breakpoint("src/main.rs", 1);
                let mut lines = Vec::new();
                // T31.14: no hay ningún depurador adjunto de verdad — `command`
                // ni siquiera se ejecuta, ver DapServer/DapSessionStatus.
                lines.push(format!(
                    "antOS DAP Debugger (SIMULADO, no ejecuta «{command}»)"
                ));
                lines.push(format!("  • Estado:                {}", dap.state));
                lines.push(format!(
                    "  • Punto de interrupción: {}:{} (verificado: {})",
                    bp.file_path, bp.line, bp.verified
                ));
                lines.push(format!(
                    "  • Pila de llamadas:      {}",
                    dap.call_stack.join(" -> ")
                ));
                output.push(lines.join("\n"));
            }
            Change::DesktopSession { workspace, action } => {
                let _ = crate::desktop::DesktopManager::sync_configuration(workspace);
                if action.as_deref() == Some("start") {
                    let run_out = crate::desktop::DesktopManager::start_session(workspace, true)?;
                    output.push(run_out);
                } else {
                    let status = crate::desktop::DesktopManager::get_status();
                    let mut lines = Vec::new();
                    let st = if status.running {
                        "En ejecución"
                    } else {
                        "Inactivo / Headless"
                    };
                    lines.push(format!("antOS Desktop · Sesión Wayland [{st}]"));
                    lines.push(format!(
                        "  • Compositor:         {}",
                        status.compositor_name
                    ));
                    lines.push(format!(
                        "  • WAYLAND_DISPLAY:    {}",
                        status.wayland_display.as_deref().unwrap_or("ninguno")
                    ));
                    lines.push(format!(
                        "  • Clientes de capa:   {}",
                        status.active_clients_count
                    ));
                    lines.push(format!(
                        "  • Atajos globales:    {} registrados",
                        status.registered_hotkeys.len()
                    ));
                    output.push(lines.join("\n"));
                }
            }
            Change::DesktopKeys { .. } => {
                let hotkeys = crate::desktop::DesktopManager::get_hotkeys();
                let mut lines = Vec::new();
                lines.push("antOS Desktop · Atajos de Teclado Globales Registrados:".into());
                for hk in hotkeys {
                    lines.push(format!(
                        "  • {:<14} -> {:<22} ({})",
                        hk.key, hk.action, hk.description
                    ));
                }
                output.push(lines.join("\n"));
            }
            Change::BarraStatus { .. } => {
                let t = crate::barra::BarraManager::global().get_telemetry();
                let mut lines = Vec::new();
                let mb = t.profiler_rss_bytes as f64 / (1024.0 * 1024.0);
                lines.push("antOS Barra · Telemetría Consolidada de Escritorio:".into());
                lines.push(format!(
                    "  • eBPF LSM:              {}",
                    if t.ebpf_lsm_active {
                        "Activo"
                    } else {
                        "Auditoría"
                    }
                ));
                lines.push(format!(
                    "  • Bloqueos de seguridad:  {}",
                    t.ebpf_violations_count
                ));
                lines.push(format!(
                    "  • Consumo RSS / CPU:      {:.2} MB / {:.1}%",
                    mb, t.profiler_cpu_percent
                ));
                lines.push(format!(
                    "  • Sesión de Pair:        {}",
                    t.active_pair_session.as_deref().unwrap_or("ninguna")
                ));
                lines.push(format!("  • Nodos antMesh P2P:     {}", t.mesh_peers_count));
                lines.push(format!(
                    "  • Notificaciones activas: {}",
                    t.active_notifications_count
                ));
                output.push(lines.join("\n"));
            }
            Change::BarraNotify {
                category,
                message,
                urgent,
                ..
            } => {
                let alert = antos_protocol::BarraAlert {
                    category: category.clone(),
                    message: message.clone(),
                    urgent: *urgent,
                };
                crate::barra::BarraManager::global().emit_alert(alert)?;
                let badge = if *urgent { "URGENTE" } else { "INFO" };
                output.push(format!("✓ Alerta emitida hacia la barra de escritorio [{badge} - {category}]: {message}"));
            }
            Change::BootPipeline { workspace, action } => {
                let engine = crate::boot::BootEngine::global();
                match action.as_str() {
                    "build" => {
                        let img = engine.build(workspace)?;
                        output.push(format!(
                            "✓ Kernel compilado e imagen de disco creada:\n  {}",
                            img.display()
                        ));
                    }
                    "test" => {
                        let report = engine.test_boot(workspace)?;
                        output.push(report);
                    }
                    "qemu" => {
                        let _ = engine.build(workspace)?;
                        let report = engine.test_boot(workspace)?;
                        output.push(report);
                    }
                    _ => {
                        let st = engine.status(workspace);
                        let mut lines = Vec::new();
                        lines
                            .push("antOS Boot Pipeline · Estado de Artefactos y Emulación:".into());
                        lines.push(format!("  • Target:             {}", st.target_arch));
                        lines.push(format!(
                            "  • Kernel ELF:         {} ({})",
                            if st.kernel_elf_exists {
                                "Presente"
                            } else {
                                "No encontrado"
                            },
                            if st.kernel_elf_exists {
                                format!("{} KiB", st.kernel_elf_size_bytes / 1024)
                            } else {
                                "0 B".into()
                            }
                        ));
                        lines.push(format!(
                            "  • Imagen BIOS:        {} ({})",
                            if st.bios_image_exists {
                                "Presente"
                            } else {
                                "No encontrada"
                            },
                            if st.bios_image_exists {
                                format!(
                                    "{:.1} MB",
                                    st.bios_image_size_bytes as f64 / (1024.0 * 1024.0)
                                )
                            } else {
                                "0 B".into()
                            }
                        ));
                        lines.push(format!(
                            "  • Emulador QEMU:      {}",
                            if st.qemu_installed {
                                "Instalado (qemu-system-x86_64)"
                            } else {
                                "No encontrado"
                            }
                        ));
                        output.push(lines.join("\n"));
                    }
                }
            }
            Change::PluginList { workspace } => {
                let p_dir = crate::wasm::PluginManager::get_plugins_dir(workspace);
                let list = crate::wasm::PluginManager::list_plugins(&p_dir);
                if list.is_empty() {
                    output.push("antOS Plugins: No hay plugins WASM instalados.".into());
                } else {
                    let mut lines = Vec::new();
                    lines.push(format!("antOS Plugins ({} instalados):", list.len()));
                    for p in list {
                        let kb = p.wasm_size_bytes.div_ceil(1024);
                        lines.push(format!(
                            "  • {} v{} ({} KiB) - {} [acciones: {}]",
                            p.name,
                            p.version,
                            kb,
                            p.description,
                            p.capabilities.join(", ")
                        ));
                    }
                    output.push(lines.join("\n"));
                }
            }
            Change::PluginRun {
                workspace,
                plugin,
                action,
                params,
            } => {
                let p_dir = crate::wasm::PluginManager::get_plugins_dir(workspace);
                let res = crate::wasm::PluginManager::run_plugin(&p_dir, plugin, action, params);
                if res.success {
                    output.push(format!("✓ Plugin [{}:{}] ejecutado en sandbox WASM:\n  Salida: {}\n  Ciclos de instrucción: {}\n  Memoria asignada: {} KiB",
                        res.plugin, res.action, res.output, res.fuel_consumed, res.memory_allocated_bytes / 1024
                    ));
                } else {
                    let err = res.error.unwrap_or_else(|| "Error desconocido".into());
                    bail!("Fallo en plugin [{}:{}]: {}", res.plugin, res.action, err);
                }
            }
            Change::PluginInstall {
                workspace,
                source_path,
            } => {
                let p_dir = crate::wasm::PluginManager::get_plugins_dir(workspace);
                let installed = crate::wasm::PluginManager::install_plugin(&p_dir, source_path)?;
                output.push(format!(
                    "✓ Plugin «{}» v{} instalado con éxito en {}",
                    installed.name,
                    installed.version,
                    p_dir.display()
                ));
            }
            Change::UiScreenshot { target, path, .. } => {
                let engine = crate::vision::VisionEngine::global();
                let res = engine.capture_screen(target.as_deref(), path.as_deref())?;
                let saved = res.saved_path.unwrap_or_else(|| "en memoria".into());
                output.push(format!("✓ Captura de pantalla completada para «{}» ({}):\n  Dimensiones: {}x{} píxeles\n  Tamaño: {} KiB",
                    res.target, saved, res.width, res.height, res.size_bytes / 1024
                ));
            }
            Change::UiInspectVisual {
                target, criteria, ..
            } => {
                let engine = crate::vision::VisionEngine::global();
                let report = engine.inspect_visual(target, criteria, None)?;
                let mut lines = Vec::new();
                lines.push(format!(
                    "antOS Visual QA Report · Objetivo: «{}» [Resultado: {}]",
                    report.target,
                    if report.pass { "APROBADO" } else { "RECHAZADO" }
                ));
                lines.push(format!("  • Resumen: {}", report.summary));
                lines.push(format!(
                    "  • Resolución evaluada: {}x{} ({} KiB)",
                    report.image_width,
                    report.image_height,
                    report.image_size_bytes / 1024
                ));
                for f in report.findings {
                    let sev = match f.severity.as_str() {
                        "critical" => "CRÍTICO",
                        "warning" => "ADVERTENCIA",
                        _ => "INFO",
                    };
                    lines.push(format!("  • [{sev}] {}: {}", f.category, f.description));
                    if let Some(coords) = f.coordinates {
                        lines.push(format!("    Coordenadas: {coords}"));
                    }
                    lines.push(format!("    Recomendación: {}", f.recommendation));
                }
                output.push(lines.join("\n"));
            }
            Change::DiskList { .. } => {
                let disks = crate::installer::DiskManager::list_disks()?;
                let mut lines = Vec::new();
                lines.push(format!(
                    "antOS Almacenamiento · Unidades detectadas ({}):",
                    disks.len()
                ));
                for d in disks {
                    let gb = d.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                    lines.push(format!(
                        "  • {} ({:.1} GB, Bus: {}, Particiones: {})",
                        d.path,
                        gb,
                        d.bus_type,
                        d.partitions.len()
                    ));
                }
                output.push(lines.join("\n"));
            }
            Change::DiskInspect { device, .. } => {
                let disk = crate::installer::DiskManager::inspect_disk(device)?;
                match disk {
                    Some(d) => {
                        let gb = d.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                        let mut lines = Vec::new();
                        lines.push(format!("antOS Almacenamiento · Dispositivo {}:", d.path));
                        lines.push(format!(
                            "  • Modelo: {:.1} GB | Bus: {} | Tabla: {}",
                            gb, d.bus_type, d.partition_table
                        ));
                        for p in d.partitions {
                            let efi = if p.is_efi { " [EFI]" } else { "" };
                            lines.push(format!(
                                "    - {} ({} MB, {}){}",
                                p.name,
                                p.size_bytes / (1024 * 1024),
                                p.fs_type.as_deref().unwrap_or("none"),
                                efi
                            ));
                        }
                        output.push(lines.join("\n"));
                    }
                    None => {
                        output.push(format!("Dispositivo «{}» no encontrado", device));
                    }
                }
            }
            Change::DiskPartition {
                device,
                clean,
                dry_run,
                ..
            } => {
                let plan = crate::installer::DiskManager::plan_partitioning(device, *clean)?;
                let report =
                    crate::installer::DiskManager::apply_partitioning(device, &plan, *dry_run)?;
                output.push(report);
            }
            Change::InstallPrepare {
                target_device,
                target_mount,
                ..
            } => {
                let mut cfg = antos_protocol::InstallConfig {
                    target_device: target_device.clone(),
                    ..Default::default()
                };
                if let Some(ref m) = target_mount {
                    cfg.target_mount = m.clone();
                }
                let mount_dir = crate::installer::DeployEngine::prepare_target(&cfg)?;
                output.push(format!(
                    "✓ Entorno de instalación validado para {}: punto de montaje listo en {}",
                    target_device,
                    mount_dir.display()
                ));
            }
            Change::InstallDeploy { workspace, config } => {
                let report = crate::installer::DeployEngine::deploy_system(config, workspace)?;
                let mut lines = Vec::new();
                lines.push(format!("antOS Instalador · {}", report.summary));
                lines.push(format!("  • Modo:          {}", report.mode));
                lines.push(format!("  • Partición ESP: {}", report.efi_partition));
                lines.push(format!("  • Partición /:   {}", report.root_partition));
                lines.push(if report.simulated {
                    "  • Pasos (○ = simulado, nada escrito en el disco):".into()
                } else {
                    "  • Pasos ejecutados:".into()
                });
                for s in &report.steps {
                    lines.push(format!(
                        "    {} {}: {}",
                        if s.executed { "✓" } else { "○" },
                        s.name,
                        s.description
                    ));
                }
                output.push(lines.join("\n"));
            }
            Change::BootloaderProbe { esp_path, .. } => {
                let p = esp_path
                    .as_deref()
                    .map(Path::new)
                    .unwrap_or_else(|| Path::new("/boot"));
                let entries = crate::installer::BootloaderEngine::probe_operating_systems(p)?;
                let mut lines = Vec::new();
                lines.push(format!(
                    "antOS Bootloader · Sistemas Operativos Detectados ({}):",
                    entries.len()
                ));
                for (i, os) in entries.iter().enumerate() {
                    lines.push(format!(
                        "  [{}] {} (Tipo: {}, EFI: {})",
                        i + 1,
                        os.name,
                        os.os_type,
                        os.efi_path
                    ));
                }
                output.push(lines.join("\n"));
            }
            Change::BootloaderInstall { config, .. } => {
                let report = crate::installer::BootloaderEngine::install_bootloader(config)?;
                let mut lines = Vec::new();
                lines.push(format!("antOS Bootloader · {}", report.summary));
                lines.push(format!("  • Punto ESP:       {}", report.esp_path));
                lines.push(format!(
                    "  • Comando NVRAM:   {}",
                    report.efibootmgr_command
                ));
                lines.push("  • Entradas configuradas:".into());
                for e in &report.entries_configured {
                    lines.push(format!("    ✓ {}", e));
                }
                output.push(lines.join("\n"));
            }
            Change::MicrovmSpawn { state_dir, config } => {
                let instance = crate::vm::MicrovmManager::spawn_vm(state_dir, config)?;
                output.push(format!(
                    "antOS MicroVM · Instancia «{}» arrancada exitosamente:\n  • PID:          {}\n  • vCPUs:        {}\n  • Memoria:      {} MB\n  • Puerto vsock: {}\n  • Estado:       {}",
                    instance.id, instance.pid, instance.vcpus, instance.memory_mb, instance.vsock_port, instance.status
                ));
            }
            Change::MicrovmExec {
                state_dir,
                vm_id,
                command,
            } => {
                let res = crate::vm::MicrovmManager::exec_vm(state_dir, vm_id, command)?;
                output.push(format!(
                    "antOS MicroVM · Comando ejecutado en el anfitrión bajo el recinto local, asociado a la microVM registrada «{}» [Código: {}]:\n  • Salida:   {}\n  • Duración: {} ms",
                    res.vm_id, res.exit_code, res.stdout.trim(), res.duration_ms
                ));
            }
            Change::MicrovmDestroy { state_dir, vm_id } => {
                crate::vm::MicrovmManager::kill_vm(state_dir, vm_id)?;
                output.push(format!(
                    "antOS MicroVM · Instancia «{vm_id}» destruida y recursos liberados"
                ));
            }
            Change::HostShellExec { command } => {
                // Reached only inside the sandboxed executor process — see
                // the doc comment on this variant and on `vm::exec_policy`
                // (T31.4). Encoded as JSON so the caller on the other side
                // of `sandbox::run` (crate::vm::MicrovmManager::exec_vm) gets
                // back the exit code and both streams, not just one string.
                let outcome = crate::vm::run_host_shell_command(command);
                output.push(serde_json::to_string(&outcome)?);
            }
            Change::PackageInstall {
                state_dir,
                package,
                dry_run,
            } => {
                let rep = crate::pkg::PackageEngine::install(state_dir, package, *dry_run)?;
                let status_lbl = if rep.success { "OK" } else { "ERROR" };
                output.push(format!(
                    "antpkg [{status_lbl}]: {}\n  • Generación: {}\n  • Prefijo en store: {}",
                    rep.message, rep.generation, rep.store_path
                ));
            }
            Change::PackageRemove { state_dir, package } => {
                let rep = crate::pkg::PackageEngine::remove(state_dir, package)?;
                output.push(format!(
                    "antpkg: {}\n  • Nueva generación activa: {}",
                    rep.message, rep.generation
                ));
            }
            Change::PackageRollback {
                state_dir,
                generation,
            } => {
                let rep = crate::pkg::PackageEngine::rollback(state_dir, *generation)?;
                output.push(format!(
                    "antpkg: {}\n  • Generación restaurada: {}",
                    rep.message, rep.generation
                ));
            }
            Change::PackageList { state_dir } => {
                let pkgs = crate::pkg::PackageEngine::list(state_dir)?;
                if pkgs.is_empty() {
                    output.push(
                        "antpkg: No hay paquetes instalados en el perfil activo.".to_string(),
                    );
                } else {
                    let mut lines = vec![format!(
                        "antpkg · Paquetes instalados en perfil activo ({}):",
                        pkgs.len()
                    )];
                    for p in pkgs {
                        lines.push(format!(
                            "  • {} v{} (gen {}) [bin: {}]",
                            p.name,
                            p.version,
                            p.generation,
                            p.binaries.join(", ")
                        ));
                    }
                    output.push(lines.join("\n"));
                }
            }
            Change::PackageVerify { state_dir } => {
                let (all_valid, count, details) = crate::pkg::PackageEngine::verify(state_dir)?;
                let status_lbl = if all_valid {
                    "INTEGRIDAD CORRECTA"
                } else {
                    "ADVERTENCIAS DE INTEGRIDAD"
                };
                let mut lines = vec![format!(
                    "antpkg · {status_lbl} ({} paquetes comprobados):",
                    count
                )];
                lines.extend(details);
                output.push(lines.join("\n"));
            }
            Change::AutopilotStart {
                state_dir,
                workspace_dir,
                config,
            } => {
                let st = crate::autopilot::AutopilotEngine::start(
                    state_dir,
                    workspace_dir,
                    config.clone(),
                )?;
                output.push(format!("antOS Autopilot · Centinela iniciado (intervalo: {}s)\n  • Incidentes detectados: {}\n  • Espacio de trabajo: {}", st.poll_interval_secs, st.active_incidents_count, st.workspace_path));
            }
            Change::AutopilotStop {
                state_dir,
                workspace_dir,
            } => {
                let st = crate::autopilot::AutopilotEngine::stop(state_dir, workspace_dir)?;
                output.push(format!("antOS Autopilot · Centinela detenido (activo: {})\n  • Total incidentes resueltos: {}", st.active, st.resolved_incidents_count));
            }
            Change::AutopilotStatus {
                state_dir,
                workspace_dir,
            } => {
                let st = crate::autopilot::AutopilotEngine::status(state_dir, workspace_dir)?;
                let status_lbl = if st.active {
                    "ACTIVO (Vigilando)"
                } else {
                    "DETENIDO"
                };
                output.push(format!("antOS Autopilot · Estado: {status_lbl}\n  • Intervalo: {}s\n  • Incidentes activos: {}\n  • Incidentes resueltos: {}", st.poll_interval_secs, st.active_incidents_count, st.resolved_incidents_count));
            }
            Change::AutopilotScan {
                state_dir,
                workspace_dir,
            } => {
                let new_incs =
                    crate::autopilot::AutopilotEngine::scan_workspace(state_dir, workspace_dir)?;
                if new_incs.is_empty() {
                    output.push(
                        "antOS Autopilot · Escaneo finalizado: repositorio limpio sin incidentes"
                            .to_string(),
                    );
                } else {
                    let mut lines = vec![format!("antOS Autopilot · Escaneo finalizado: {} incidentes detectados con propuestas listas para aprobación:", new_incs.len())];
                    for inc in new_incs {
                        lines.push(format!(
                            "  • [{}] {} en «{}»: {}",
                            inc.id, inc.incident_type, inc.file_path, inc.error_message
                        ));
                    }
                    output.push(lines.join("\n"));
                }
            }
            Change::AutopilotResolve {
                state_dir,
                workspace_dir,
                incident_id,
                approve,
            } => {
                let inc = crate::autopilot::AutopilotEngine::resolve_incident(
                    state_dir,
                    workspace_dir,
                    incident_id,
                    *approve,
                )?;
                let action_lbl = if *approve {
                    "Aprobado y aplicado"
                } else {
                    "Descartado"
                };
                output.push(format!(
                    "antOS Autopilot · Incidente «{}» {action_lbl} exitosamente (estado: {})",
                    inc.id, inc.status
                ));
            }
            Change::WebStart {
                state_dir,
                workspace_dir,
                config,
            } => {
                let st = crate::web::WebEngine::start(state_dir, workspace_dir, config.clone())?;
                output.push(format!("antOS Web Console · Servidor iniciado en {}\n  • WebSocket Bridge: {}/ws/events\n  • Estado: {}", st.url, st.url, if st.running { "EN LÍNEA" } else { "DETENIDO" }));
            }
            Change::WebStop { state_dir } => {
                let st = crate::web::WebEngine::stop(state_dir)?;
                output.push(format!(
                    "antOS Web Console · Servidor detenido (activo: {})",
                    st.running
                ));
            }
            Change::WebStatus { state_dir } => {
                let st = crate::web::WebEngine::status(state_dir)?;
                let badge = if st.running {
                    "ACTIVO (En línea)"
                } else {
                    "DETENIDO"
                };
                output.push(format!("antOS Web Console · Estado: {badge}\n  • URL de Acceso:         {}\n  • Clientes Conectados:   {}\n  • Sesiones Activas:      {}", st.url, st.connected_clients, st.active_sessions_count));
            }
            Change::WebToken {
                state_dir,
                label,
                ttl,
            } => {
                let session =
                    crate::web::WebEngine::generate_token(state_dir, label.clone(), *ttl)?;
                output.push(format!("antOS Web Console · Token generado exitosamente:\n  • Token:     {}\n  • Expira en: {}s{}", session.token, session.expires_at.saturating_sub(session.created_at), session.client_label.as_ref().map(|l| format!("\n  • Cliente:   {l}")).unwrap_or_default()));
            }

            Change::TestReproduce {
                workspace,
                state_dir,
                error_log,
                target_file,
            } => {
                let report = crate::reproduce::TddEngine::run_reproduce_pipeline(
                    error_log,
                    target_file.as_deref(),
                    workspace,
                    state_dir,
                )?;
                output.push(format!(
                    "🧪 Pipeline de Reproducción TDD antOS · Bug [{}]\n  • Estado: {}\n  • Diagnóstico: {} ({:?})\n  • Archivo: {}:{}\n  • Test Generado: {}\n  • Verificación Final: {}\n  • Auditado: {}",
                    report.id,
                    report.phase.label(),
                    report.diagnostic.message,
                    report.diagnostic.language,
                    report.diagnostic.target_file.as_deref().unwrap_or("desconocido"),
                    report.diagnostic.target_line.unwrap_or(0),
                    report.test_file,
                    report.fix_summary.as_deref().unwrap_or("Pendiente"),
                    if report.audited { "Sí (Protegido contra regresiones)" } else { "No" },
                ));
            }
            Change::TestGen {
                workspace,
                target,
                suite_type,
                cases,
            } => {
                let report = crate::reproduce::TddEngine::generate_tests_for_target(
                    target, suite_type, *cases, workspace,
                )?;
                output.push(format!(
                    "⚡ Generador de Tests antOS · Suite [{}]\n  • Objetivo: {}\n  • Casos generados: {}\n  • Archivo de test: {}\n  • Lenguaje detectado: {:?}",
                    suite_type,
                    report.diagnostic.target_file.as_deref().unwrap_or(target),
                    cases,
                    report.test_file,
                    report.diagnostic.language,
                ));
            }
            Change::CiRun {
                workspace,
                state_dir,
                stage,
                fast,
            } => {
                let report = crate::ci::CiEngine::run_pipeline(
                    workspace,
                    state_dir,
                    stage.as_deref(),
                    *fast,
                )?;
                output.push(format!(
                    "⚙️ Matriz de CI Local antOS · Ejecución [{}]\n  • Resultado: {}\n  • Duración: {} ms\n  • Seguridad: {}\n  • Etapas ({}/{} exitosas)",
                    report.id,
                    if report.success { "✅ PASARON TODAS LAS ETAPAS" } else { "❌ FALLÓ EL PIPELINE" },
                    report.total_duration_ms,
                    if report.security_clean { "Limpio (sin secretos)" } else { "ALERTA: Fugas detectadas" },
                    report.stages.iter().filter(|s| s.status == antos_protocol::CiStageStatus::Passed).count(),
                    report.stages.len(),
                ));
            }
            Change::CiStatus { state_dir } => {
                let report = crate::ci::CiEngine::get_last_report(state_dir)?;
                if let Some(r) = report {
                    output.push(format!(
                        "antOS CI · Último Reporte Registrado:\n  • ID:       {}\n  • Estado:   {}\n  • Duración: {} ms\n  • Etapas:   {}",
                        r.id,
                        if r.success { "EXITOSO" } else { "FALLIDO" },
                        r.total_duration_ms,
                        r.stages.iter().map(|s| format!("{}: {:?}", s.name, s.status)).collect::<Vec<_>>().join(", ")
                    ));
                } else {
                    output.push("antOS CI · No hay reportes previos registrados.".into());
                }
            }
            Change::GitHookManage { workspace, action } => {
                let status = match action.as_str() {
                    "install" | "instalar" => crate::ci::CiEngine::install_git_hooks(workspace)?,
                    "uninstall" | "desinstalar" => {
                        crate::ci::CiEngine::uninstall_git_hooks(workspace)?
                    }
                    _ => crate::ci::CiEngine::query_git_hooks_status(workspace)?,
                };
                output.push(format!(
                    "🪝 antOS Git Hooks · Estado de Protección:\n  • Pre-commit: {}\n  • Pre-push:   {}\n  • Directorio: {}\n  • Guardias:   {}",
                    if status.pre_commit_installed { "INSTALADO" } else { "NO INSTALADO" },
                    if status.pre_push_installed { "INSTALADO" } else { "NO INSTALADO" },
                    status.hook_dir,
                    if status.active_guards.is_empty() { "ninguna".into() } else { status.active_guards.join(", ") },
                ));
            }
            Change::SnapshotCreate {
                workspace,
                state_dir,
                label,
                author,
            } => {
                let meta = crate::time_machine::TimeMachineEngine::create_snapshot(
                    workspace,
                    state_dir,
                    label.as_deref(),
                    author.as_deref(),
                )?;
                output.push(format!(
                    "📸 antOS Time Machine · Instantánea Creada [{}]\n  • Etiqueta:  {}\n  • Autor:     {}\n  • Archivos:  {} ({} KiB)\n  • Servicios: {}",
                    meta.id,
                    meta.label.as_deref().unwrap_or("—"),
                    meta.author,
                    meta.files_count,
                    meta.total_bytes / 1024,
                    if meta.services_included.is_empty() { "ninguno".into() } else { meta.services_included.join(", ") },
                ));
            }
            Change::SnapshotList { state_dir } => {
                let list = crate::time_machine::TimeMachineEngine::list_snapshots(state_dir)?;
                if list.is_empty() {
                    output.push("antOS Time Machine · No hay instantáneas registradas.".into());
                } else {
                    let mut lines = vec![format!(
                        "⏱️ antOS Time Machine · Total: {} instantáneas registradas:",
                        list.len()
                    )];
                    for s in &list {
                        lines.push(format!(
                            "  • [{}] {} ({} archivos, {} KiB)",
                            s.id,
                            s.label.as_deref().unwrap_or("—"),
                            s.files_count,
                            s.total_bytes / 1024
                        ));
                    }
                    output.push(lines.join("\n"));
                }
            }
            Change::SnapshotRestore {
                workspace,
                state_dir,
                id_or_label,
                create_rescue,
            } => {
                let res = crate::time_machine::TimeMachineEngine::restore_snapshot(
                    workspace,
                    state_dir,
                    id_or_label,
                    *create_rescue,
                )?;
                output.push(format!(
                    "⏪ antOS Time Machine · Instantánea [{}] Restaurada\n  • Rescate:   {}\n  • Archivos:  {} restaurados, {} eliminados\n  • Duración:  {} ms",
                    res.snapshot_id,
                    res.rescue_snapshot_id.as_deref().unwrap_or("ninguno"),
                    res.files_restored,
                    res.files_deleted,
                    res.duration_ms,
                ));
            }
            Change::SnapshotDelete { state_dir, id } => {
                let deleted =
                    crate::time_machine::TimeMachineEngine::delete_snapshot(state_dir, id)?;
                output.push(format!(
                    "🗑️ antOS Time Machine · Instantánea [{deleted}] eliminada con éxito."
                ));
            }
            Change::BenchRun {
                workspace,
                state_dir,
                target,
            } => {
                let report = crate::bench::BenchEngine::run_benchmark(
                    workspace,
                    state_dir,
                    target.as_deref(),
                )?;
                output.push(format!(
                    "⚡ antOS Bench · Suite [{}] Ejecutada\n  • Rama:      {}\n  • Métricas:  {} pruebas procesadas\n  • Duración:  {} ms",
                    report.id,
                    report.branch,
                    report.metrics.len(),
                    report.total_duration_ms,
                ));
            }
            Change::BenchDiff {
                workspace,
                state_dir,
                against_branch,
                threshold_pct,
            } => {
                let diff = crate::bench::BenchEngine::compare_benchmark(
                    workspace,
                    state_dir,
                    against_branch.as_deref(),
                    *threshold_pct,
                )?;
                output.push(format!(
                    "⚡ antOS Bench Diff · Comparación [{}]\n  • Base:      {} vs {}\n  • Regresión: {}\n  • Veredicto: {}",
                    diff.id,
                    diff.base_branch,
                    diff.target_branch,
                    if diff.has_regression { format!("SÍ (+{:.1}%)", diff.max_regression_pct) } else { "NO (rendimiento estable)".into() },
                    diff.auditor_verdict,
                ));
            }
            Change::BenchHistory { state_dir } => {
                let history = crate::bench::BenchEngine::load_history(state_dir);
                if history.is_empty() {
                    output.push("antOS Bench · No hay historial previo registrado.".into());
                } else {
                    let mut lines = vec![format!(
                        "📈 antOS Bench · Total: {} corridas registradas:",
                        history.len()
                    )];
                    for h in &history {
                        lines.push(format!(
                            "  • [{}] {} - {} ({} ms)",
                            h.id, h.branch, h.suite_name, h.total_duration_ms
                        ));
                    }
                    output.push(lines.join("\n"));
                }
            }
            Change::IssueList {
                workspace,
                state_dir,
            } => {
                let issues = crate::forge::ForgeEngine::list_issues(workspace, state_dir)?;
                if issues.is_empty() {
                    output.push(
                        "antOS Forge · No hay issues abiertos en el repositorio remoto.".into(),
                    );
                } else {
                    let mut lines = vec![format!(
                        "🐙 antOS Forge · Total: {} issues abiertos en origen:",
                        issues.len()
                    )];
                    for i in &issues {
                        lines.push(format!("  • #{:<4} {} (@{})", i.number, i.title, i.author));
                    }
                    output.push(lines.join("\n"));
                }
            }
            Change::IssueImport {
                workspace,
                state_dir,
                id,
            } => {
                let (ticket_id, path, title) =
                    crate::forge::ForgeEngine::import_issue(workspace, state_dir, id)?;
                output.push(format!(
                    "📥 antOS Forge · Issue Importado con Éxito\n  • Ticket ID: [{}]\n  • Título:    {}\n  • Fichero:   {}",
                    ticket_id,
                    title,
                    path.display(),
                ));
            }
            Change::PrCreate {
                workspace,
                state_dir,
                title,
                base_branch,
                draft,
            } => {
                let pr = crate::forge::ForgeEngine::create_pull_request(
                    workspace,
                    state_dir,
                    title.as_deref(),
                    base_branch.as_deref(),
                    *draft,
                )?;
                output.push(format!(
                    "🚀 antOS Forge · Pull Request #{}: {}\n  • Rama:      {} -> {}\n  • Modo:      {}\n  • URL:       {}",
                    pr.number,
                    pr.title,
                    pr.head_branch,
                    pr.base_branch,
                    if pr.draft { "Borrador" } else { "Listo" },
                    pr.url,
                ));
            }
            Change::PrStatus { state_dir, number } => {
                let status =
                    crate::forge::ForgeEngine::get_pull_request_status(state_dir, *number)?;
                output.push(format!(
                    "🔍 antOS Forge · Pull Request #{}: {}\n  • Estado:    {}\n  • Fusión:    {}\n  • CI Checks: {}\n  • URL:       {}",
                    status.number,
                    status.title,
                    status.state,
                    if status.mergeable { "Limpia" } else { "Conflictos" },
                    status.ci_status.as_deref().unwrap_or("—"),
                    status.url,
                ));
            }
            Change::DocArch { workspace, kind } => {
                let k = match kind.as_deref() {
                    Some("components") | Some("c4") | Some("comp") => {
                        antos_protocol::ArchDiagramKind::Components
                    }
                    Some("flow") | Some("ipc") => antos_protocol::ArchDiagramKind::IpcFlow,
                    Some("antflow") | Some("state") => antos_protocol::ArchDiagramKind::AntFlow,
                    _ => antos_protocol::ArchDiagramKind::Full,
                };
                let report = crate::doc_arch::DocArchEngine::generate_diagram(workspace, k);
                output.push(format!(
                    "📐 antOS Doc Arch · Diagrama [{}]\n  • Crates:       {}\n  • Módulos:      {}\n  • Capacidades:  {}\n\n```mermaid\n{}\n```",
                    report.kind.name(),
                    report.crates_count,
                    report.modules_count,
                    report.caps_count,
                    report.mermaid_content,
                ));
            }
            Change::DocSync {
                workspace,
                target_file,
            } => {
                let report =
                    crate::doc_arch::DocArchEngine::sync_docs(workspace, target_file.as_deref())?;
                output.push(format!(
                    "🔄 antOS Doc Sync · {}\n  • Escaneados:   {}\n  • Actualizados: {}",
                    report.message, report.files_scanned, report.files_updated,
                ));
            }
            Change::DocCheck {
                workspace,
                target_file,
            } => {
                let report =
                    crate::doc_arch::DocArchEngine::check_docs(workspace, target_file.as_deref())?;
                output.push(format!(
                    "🔍 antOS Doc Check · {}\n  • En sincronía: {}",
                    report.message, report.in_sync,
                ));
            }
            _ => {}
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_scaffold_emits_project_git_init() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = crate::capability::Catalog::load(&ctx.caps_dir).expect("catalog");
        let pending = PendingChanges::default();

        let mut args = BTreeMap::new();
        args.insert("name".into(), "auth-service".into());
        args.insert("language".into(), "rust".into());
        let step = Step {
            capability: "project.scaffold".into(),
            args,
        };
        let cap = catalog
            .get("project.scaffold")
            .expect("cap project.scaffold");
        let changes = changes_for(&step, cap, &ctx, &pending).expect("changes");

        let has_git_init = changes
            .iter()
            .any(|c| matches!(c, Change::ProjectGitInit { .. }));
        assert!(
            has_git_init,
            "project.scaffold must emit Change::ProjectGitInit"
        );
    }

    /// T35.1: con `framework` los ficheros salen del stack, se escribe
    /// `.antos/project.toml`, y un framework desconocido es un error que
    /// nombra los disponibles.
    #[test]
    fn project_scaffold_renders_the_framework_stack_and_manifest() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = crate::capability::Catalog::load(&ctx.caps_dir).expect("catalog");
        let pending = PendingChanges::default();
        let cap = catalog
            .get("project.scaffold")
            .expect("cap project.scaffold");

        let mut args = BTreeMap::new();
        args.insert("name".into(), "antostest".into());
        args.insert("language".into(), "typescript".into());
        args.insert("framework".into(), "nestjs".into());
        let step = Step {
            capability: "project.scaffold".into(),
            args,
        };
        let changes = changes_for(&step, cap, &ctx, &pending).expect("changes");
        let written: Vec<(String, String)> = changes
            .iter()
            .filter_map(|c| match c {
                Change::Write { path, content } => Some((
                    path.strip_prefix(ctx.workspace.join("antostest"))
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                    content.clone(),
                )),
                _ => None,
            })
            .collect();
        let paths: Vec<&str> = written.iter().map(|(p, _)| p.as_str()).collect();
        for expected in [
            "package.json",
            "src/main.ts",
            "src/app.module.ts",
            "src/app.controller.spec.ts",
            ".antos/project.toml",
        ] {
            assert!(paths.contains(&expected), "falta {expected}: {paths:?}");
        }
        let pkg = &written.iter().find(|(p, _)| p == "package.json").unwrap().1;
        assert!(pkg.contains("\"name\": \"antostest\""));
        assert!(pkg.contains("@nestjs/core"));
        let manifest = &written
            .iter()
            .find(|(p, _)| p == ".antos/project.toml")
            .unwrap()
            .1;
        let parsed: crate::stacks::ProjectManifest = toml::from_str(manifest).unwrap();
        assert_eq!(parsed.stack, "nestjs");
        assert_eq!(parsed.commands.test, vec!["npm", "test", "--silent"]);
        assert!(changes
            .iter()
            .any(|c| matches!(c, Change::ProjectGitInit { .. })));

        let mut bad = BTreeMap::new();
        bad.insert("name".into(), "x".into());
        bad.insert("language".into(), "typescript".into());
        bad.insert("framework".into(), "angular".into());
        let err = changes_for(
            &Step {
                capability: "project.scaffold".into(),
                args: bad,
            },
            cap,
            &ctx,
            &pending,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("angular") && msg.contains("nestjs"), "{msg}");
    }

    #[test]
    fn un_paso_ve_lo_que_decidio_el_anterior() {
        let mut pending = PendingChanges::default();
        let ruta = PathBuf::from("/ws/paquetes.toml");

        assert_eq!(pending.read(&ruta), None, "de partida, manda el disco");

        pending.apply(&Change::Write {
            path: ruta.clone(),
            content: "express".into(),
        });
        assert_eq!(
            pending.read(&ruta).as_deref(),
            Some("express"),
            "el paso siguiente debe ver lo que este escribió, no el disco"
        );
    }

    #[test]
    fn escribir_despues_de_borrar_parte_de_cero() {
        let mut pending = PendingChanges::default();
        let ruta = PathBuf::from("/ws/notas.txt");

        pending.apply(&Change::Delete { path: ruta.clone() });
        assert_eq!(
            pending.read(&ruta).as_deref(),
            Some(""),
            "un fichero borrado por un paso anterior está vacío, no como en el disco"
        );

        pending.apply(&Change::Write {
            path: ruta.clone(),
            content: "nuevo".into(),
        });
        assert_eq!(pending.read(&ruta).as_deref(), Some("nuevo"));
    }

    #[test]
    fn test_changes_for_capacidades_git() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = crate::capability::Catalog::load(&ctx.caps_dir).expect("catalog");
        let pending = PendingChanges::default();

        // 1. git.status
        let step_status = Step {
            capability: "git.status".into(),
            args: BTreeMap::new(),
        };
        let cap_status = catalog.get("git.status").expect("cap git.status");
        let changes_status =
            changes_for(&step_status, cap_status, &ctx, &pending).expect("changes");
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
        let cap_commit = catalog
            .get("git.commit_semantic")
            .expect("cap git.commit_semantic");
        let changes_commit =
            changes_for(&step_commit, cap_commit, &ctx, &pending).expect("changes");
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
        let cap_branch = catalog
            .get("git.smart_branch")
            .expect("cap git.smart_branch");
        let changes_branch =
            changes_for(&step_branch, cap_branch, &ctx, &pending).expect("changes");
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
        let cap_wt_create = catalog
            .get("git.worktree_create")
            .expect("cap git.worktree_create");
        let changes_wt_create =
            changes_for(&step_wt_create, cap_wt_create, &ctx, &pending).expect("changes");
        assert_eq!(changes_wt_create.len(), 1);
        match &changes_wt_create[0] {
            Change::GitWorktreeCreate {
                branch_name,
                target_path,
                ..
            } => {
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
        let cap_wt_clean = catalog
            .get("git.worktree_cleanup")
            .expect("cap git.worktree_cleanup");
        let changes_wt_clean =
            changes_for(&step_wt_clean, cap_wt_clean, &ctx, &pending).expect("changes");
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
        let cap_port_st = catalog
            .get("diag.port_status")
            .expect("cap diag.port_status");
        let changes_port_st =
            changes_for(&step_port_st, cap_port_st, &ctx, &pending).expect("changes");
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
        let changes_port_kill =
            changes_for(&step_port_kill, cap_port_kill, &ctx, &pending).expect("changes");
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
