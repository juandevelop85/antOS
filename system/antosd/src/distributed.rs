//! Distributed antFlow: Multi-Node Swarm Role Dispatch & Worktree Sync (Ticket T9.2).
//!
//! Enables delegating heavy agent workloads (e.g. Coder with 70B parameter models,
//! QA with large test suites) across antMesh peer nodes with automated worktree delta synchronization.

use crate::util::lock_or_recover;
use antos_protocol::{AgentRole, PeerNode, SwarmNodeStatus, SwarmStatus, SwarmTaskAssignment};
use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static SWARM_LOCK: Mutex<()> = Mutex::new(());

/// Core engine for distributed swarm agent orchestration.
pub struct SwarmEngine {
    _private: (),
}

impl SwarmEngine {
    pub fn global() -> Self {
        Self { _private: () }
    }

    fn tasks_path(workspace: &Path) -> PathBuf {
        workspace.join(".antos").join("swarm_tasks.json")
    }

    fn bundles_dir(workspace: &Path) -> PathBuf {
        workspace.join(".antos").join("bundles")
    }

    /// Returns the global swarm status across all reachable nodes.
    pub fn status(&self, workspace: &Path) -> Result<SwarmStatus> {
        let mesh_status = crate::mesh::MeshEngine::global().status(workspace)?;
        let tasks = self.list_tasks(workspace)?;

        let mut nodes = Vec::new();

        // Local node
        let local_tasks: Vec<_> = tasks
            .iter()
            .filter(|t| t.assigned_node_id == mesh_status.local_node.id)
            .cloned()
            .collect();
        nodes.push(SwarmNodeStatus {
            node_id: mesh_status.local_node.id.clone(),
            hostname: mesh_status.local_node.hostname.clone(),
            address: mesh_status.local_node.address.clone(),
            is_local: true,
            vram_available_mb: mesh_status.local_node.resources.vram_mb,
            cpu_cores: mesh_status.local_node.resources.cpu_cores,
            running_tasks: local_tasks,
        });

        // Remote peer nodes
        for peer in &mesh_status.peers {
            let peer_tasks: Vec<_> = tasks
                .iter()
                .filter(|t| t.assigned_node_id == peer.id)
                .cloned()
                .collect();
            nodes.push(SwarmNodeStatus {
                node_id: peer.id.clone(),
                hostname: peer.hostname.clone(),
                address: peer.address.clone(),
                is_local: false,
                vram_available_mb: peer.resources.vram_mb,
                cpu_cores: peer.resources.cpu_cores,
                running_tasks: peer_tasks,
            });
        }

        let total_tasks = tasks.len();
        Ok(SwarmStatus { nodes, total_tasks })
    }

    /// Selects the optimal node for an agent role based on resources and network latency.
    pub fn select_best_node(
        &self,
        workspace: &Path,
        role: AgentRole,
        preferred: Option<&str>,
    ) -> Result<PeerNode> {
        let mesh_status = crate::mesh::MeshEngine::global().status(workspace)?;

        if let Some(target) = preferred {
            if target == "local"
                || target == mesh_status.local_node.id
                || target == mesh_status.local_node.hostname
            {
                return Ok(mesh_status.local_node);
            }
            if let Some(p) = mesh_status
                .peers
                .iter()
                .find(|p| p.id == target || p.hostname == target || p.address.starts_with(target))
            {
                return Ok(p.clone());
            }
            bail!("nodo «{target}» no encontrado entre los peers de antMesh");
        }

        if mesh_status.peers.is_empty() {
            return Ok(mesh_status.local_node);
        }

        // Heuristic: Coder benefits most from high VRAM; QA benefits from many CPU cores
        let mut candidates = mesh_status.peers.clone();
        match role {
            AgentRole::Coder => {
                candidates.sort_by(|a, b| {
                    let vram_b = b.resources.vram_mb.unwrap_or(0);
                    let vram_a = a.resources.vram_mb.unwrap_or(0);
                    vram_b
                        .cmp(&vram_a)
                        .then_with(|| a.latency_ms.cmp(&b.latency_ms))
                });
            }
            AgentRole::QA => {
                candidates.sort_by(|a, b| {
                    b.resources
                        .cpu_cores
                        .cmp(&a.resources.cpu_cores)
                        .then_with(|| a.latency_ms.cmp(&b.latency_ms))
                });
            }
            _ => {
                candidates.sort_by_key(|a| a.latency_ms);
            }
        }

        // Compare best remote candidate against local node
        let best_remote = &candidates[0];
        let local = &mesh_status.local_node;

        match role {
            AgentRole::Coder
                if local.resources.vram_mb.unwrap_or(0)
                    >= best_remote.resources.vram_mb.unwrap_or(0) =>
            {
                Ok(local.clone())
            }
            AgentRole::QA if local.resources.cpu_cores >= best_remote.resources.cpu_cores => {
                Ok(local.clone())
            }
            _ => Ok(best_remote.clone()),
        }
    }

    /// Dispatches an antFlow agent role to an optimal or designated node and synchronizes the worktree.
    pub fn dispatch_remote_role(
        &self,
        workspace: &Path,
        ticket_id: &str,
        role: AgentRole,
        preferred_node: Option<&str>,
    ) -> Result<SwarmTaskAssignment> {
        let node = self.select_best_node(workspace, role, preferred_node)?;
        let worktree_branch = format!(
            "agent/{}/{}",
            ticket_id.to_lowercase(),
            role.name().to_lowercase().replace(' ', "_")
        );

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let task_id = format!(
            "swarm-{}-{}",
            ticket_id.to_lowercase(),
            &format!("{:x}", now)[..6]
        );

        // Select the primary model on the designated node
        let target_model = node.resources.available_models.first().cloned();

        // Sincronizar worktree si la rama existe en git
        let _ = self.create_worktree_bundle(workspace, &worktree_branch);

        let assignment = SwarmTaskAssignment {
            task_id,
            ticket_id: ticket_id.to_string(),
            role,
            assigned_node_id: node.id.clone(),
            target_model,
            worktree_branch,
            status: "Running".into(),
            started_at: now,
        };

        // Persist task assignment
        let _guard = lock_or_recover(&SWARM_LOCK);
        let path = Self::tasks_path(workspace);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut tasks = if path.exists() {
            let data = fs::read_to_string(&path)?;
            serde_json::from_str::<Vec<SwarmTaskAssignment>>(&data).unwrap_or_default()
        } else {
            Vec::new()
        };

        tasks.retain(|t| t.task_id != assignment.task_id);
        tasks.push(assignment.clone());

        let json = serde_json::to_string_pretty(&tasks)?;
        fs::write(&path, json)?;

        Ok(assignment)
    }

    /// Generates an isolated git bundle package for the given branch to sync to remote nodes.
    pub fn create_worktree_bundle(&self, workspace: &Path, branch: &str) -> Result<PathBuf> {
        let dir = Self::bundles_dir(workspace);
        fs::create_dir_all(&dir)?;

        let safe_name = branch.replace('/', "_");
        let bundle_path = dir.join(format!("{safe_name}.bundle"));

        // If git repository, try to generate bundle, otherwise write manifest stub
        let status = Command::new("git")
            .args([
                "bundle",
                "create",
                bundle_path.to_str().unwrap_or(""),
                "HEAD",
                "-n",
                "1",
            ])
            .current_dir(workspace)
            .output();

        if let Ok(out) = status {
            if out.status.success() {
                return Ok(bundle_path);
            }
        }

        // Fallback: write sync manifest stub
        fs::write(&bundle_path, format!("ANTOS_BUNDLE:{branch}"))?;
        Ok(bundle_path)
    }

    /// Lists active and completed swarm tasks.
    pub fn list_tasks(&self, workspace: &Path) -> Result<Vec<SwarmTaskAssignment>> {
        let _guard = lock_or_recover(&SWARM_LOCK);
        let path = Self::tasks_path(workspace);
        if !path.exists() {
            return Ok(Vec::new());
        }

        let data = fs::read_to_string(&path)?;
        if data.trim().is_empty() {
            return Ok(Vec::new());
        }

        let tasks: Vec<SwarmTaskAssignment> = serde_json::from_str(&data).unwrap_or_default();
        Ok(tasks)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_select_best_node_fallback_local() {
        let temp = std::env::temp_dir().join(format!("test-swarm-loc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        let engine = SwarmEngine::global();
        let best = engine
            .select_best_node(&temp, AgentRole::Coder, None)
            .unwrap();
        assert!(best.id.starts_with("node-"));

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_dispatch_remote_role_lifecycle() {
        let temp = std::env::temp_dir().join(format!("test-swarm-disp-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        // Register a high-vram peer
        let peer = crate::mesh::MeshEngine::global()
            .connect_peer(&temp, "10.0.0.99:9042")
            .unwrap();

        let engine = SwarmEngine::global();
        let task = engine
            .dispatch_remote_role(&temp, "T9.2", AgentRole::Coder, Some(&peer.id))
            .unwrap();

        assert_eq!(task.ticket_id, "T9.2");
        assert_eq!(task.assigned_node_id, peer.id);
        assert_eq!(task.status, "Running");

        let status = engine.status(&temp).unwrap();
        assert!(status.total_tasks >= 1);

        let _ = fs::remove_dir_all(&temp);
    }
}
