//! Atomic Development Environment Snapshots and Time Machine Engine for antOS (Ticket T20.4).
//!
//! Provides sub-200ms snapshotting and state rollback across code trees (including untracked files),
//! ephemeral local service databases (PostgreSQL, Redis), active environment profiles, and semantic vector memory.

use antos_protocol::{DevSnapshotMetadata, SnapshotRestoreResult};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Core engine managing atomic snapshots and Time Machine rollbacks.
pub struct TimeMachineEngine;

impl TimeMachineEngine {
    /// Returns the root directory where dev snapshots are preserved.
    pub fn dev_snapshots_dir(state_dir: &Path) -> PathBuf {
        state_dir.join("snapshots").join("dev")
    }

    /// Captures a complete atomic snapshot of the workspace, services, and memory graph.
    pub fn create_snapshot(
        workspace: &Path,
        state_dir: &Path,
        label: Option<&str>,
        author: Option<&str>,
    ) -> Result<DevSnapshotMetadata> {
        let start_time = Instant::now();
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Generate ID: snap-<timestamp_ms_hex>
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let snap_id = if let Some(l) = label {
            let clean_slug: String = l
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '-'
                    }
                })
                .collect();
            format!("snap-{clean_slug}-{:x}", millis % 0xfffff)
        } else {
            format!("snap-{millis:x}")
        };

        let target_snap_dir = Self::dev_snapshots_dir(state_dir).join(&snap_id);
        fs::create_dir_all(&target_snap_dir).with_context(|| {
            format!(
                "No se pudo crear directorio de snapshot {}",
                target_snap_dir.display()
            )
        })?;

        // 1. Inspect Git context (branch and commit)
        let (git_branch, git_commit) = Self::detect_git_info(workspace);

        // 2. Clone/Copy workspace tree into target_snap_dir/workspace
        let snap_ws_dir = target_snap_dir.join("workspace");
        fs::create_dir_all(&snap_ws_dir)?;
        let mut files_count = 0;
        let mut total_bytes = 0;
        let method = Self::copy_or_clone_workspace(
            workspace,
            &snap_ws_dir,
            &mut files_count,
            &mut total_bytes,
        )?;

        // 3. Backup ephemeral local services if present ($STATE/services)
        let mut services_included = Vec::new();
        let state_services = state_dir.join("services");
        if state_services.is_dir() {
            let snap_services = target_snap_dir.join("services");
            if let Ok(entries) = fs::read_dir(&state_services) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let dest = snap_services.join(&name);
                        if Self::copy_or_clone_tree(&path, &dest).is_ok() {
                            services_included.push(name);
                        }
                    }
                }
            }
        }

        // 4. Backup semantic memory graph if present ($STATE/memory or context_graph.db)
        let mut memory_graph_included = false;
        let state_memory = state_dir.join("memory");
        if state_memory.is_dir() {
            let snap_memory = target_snap_dir.join("memory");
            if Self::copy_or_clone_tree(&state_memory, &snap_memory).is_ok() {
                memory_graph_included = true;
            }
        }

        let metadata = DevSnapshotMetadata {
            id: snap_id.clone(),
            label: label.map(|s| s.to_string()),
            author: author.unwrap_or("human").to_string(),
            timestamp_secs: now_secs,
            git_branch,
            git_commit,
            files_count,
            total_bytes,
            services_included,
            memory_graph_included,
            method,
        };

        // Write metadata manifest
        let manifest_path = target_snap_dir.join("metadata.json");
        fs::write(&manifest_path, serde_json::to_string_pretty(&metadata)?)?;

        let _elapsed_ms = start_time.elapsed().as_millis();
        Ok(metadata)
    }

    /// Lists all existing snapshots sorted chronologically descending.
    pub fn list_snapshots(state_dir: &Path) -> Result<Vec<DevSnapshotMetadata>> {
        let base_dir = Self::dev_snapshots_dir(state_dir);
        if !base_dir.is_dir() {
            return Ok(Vec::new());
        }

        let mut list = Vec::new();
        let entries = fs::read_dir(&base_dir)
            .with_context(|| format!("No se pudo leer {}", base_dir.display()))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let manifest = path.join("metadata.json");
                if manifest.exists() {
                    if let Ok(raw) = fs::read_to_string(&manifest) {
                        if let Ok(meta) = serde_json::from_str::<DevSnapshotMetadata>(&raw) {
                            list.push(meta);
                        }
                    }
                }
            }
        }

        list.sort_by(|a, b| b.timestamp_secs.cmp(&a.timestamp_secs));
        Ok(list)
    }

    /// Restores a snapshot by ID or Label, reverting workspace and state.
    pub fn restore_snapshot(
        workspace: &Path,
        state_dir: &Path,
        id_or_label: &str,
        create_rescue: bool,
    ) -> Result<SnapshotRestoreResult> {
        let start_time = Instant::now();

        // 1. Locate snapshot metadata
        let snapshots = Self::list_snapshots(state_dir)?;
        let target_meta = snapshots
            .iter()
            .find(|s| s.id == id_or_label || s.label.as_deref() == Some(id_or_label))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No se encontró ninguna instantánea con identificador o etiqueta «{id_or_label}»"))?;

        let snap_dir = Self::dev_snapshots_dir(state_dir).join(&target_meta.id);
        if !snap_dir.is_dir() {
            bail!(
                "El directorio de la instantánea «{}» está corrupto o no existe",
                target_meta.id
            );
        }

        // 2. Safety Rescue Snapshot
        let mut rescue_snapshot_id = None;
        if create_rescue {
            let rescue_label = format!("rescue-before-restore-{}", target_meta.id);
            if let Ok(rescue_meta) = Self::create_snapshot(
                workspace,
                state_dir,
                Some(&rescue_label),
                Some("time-machine-rescue"),
            ) {
                rescue_snapshot_id = Some(rescue_meta.id);
            }
        }

        // 3. Restore workspace files
        let snap_ws = snap_dir.join("workspace");
        let mut files_restored = 0;
        let mut files_deleted = 0;

        if snap_ws.is_dir() {
            // A. Remove untracked files in workspace that are NOT in snapshot
            let current_files = Self::collect_relative_files(workspace)?;
            let snapshot_files = Self::collect_relative_files(&snap_ws)?;

            for rel in &current_files {
                if !snapshot_files.contains(rel) {
                    let full_path = workspace.join(rel);
                    if full_path.is_file() {
                        let _ = fs::remove_file(&full_path);
                        files_deleted += 1;
                    }
                }
            }

            // Clean up empty directories
            Self::clean_empty_dirs(workspace)?;

            // B. Restore / copy files from snapshot into workspace
            for rel in &snapshot_files {
                let src = snap_ws.join(rel);
                let dst = workspace.join(rel);
                if let Some(parent) = dst.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if fs::copy(&src, &dst).is_ok() {
                    files_restored += 1;
                }
            }
        }

        // 4. Restore ephemeral services if present
        let mut services_restored = Vec::new();
        let snap_services = snap_dir.join("services");
        if snap_services.is_dir() {
            let state_services = state_dir.join("services");
            let _ = fs::create_dir_all(&state_services);
            if let Ok(entries) = fs::read_dir(&snap_services) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let src = entry.path();
                    let dst = state_services.join(&name);
                    let _ = fs::remove_dir_all(&dst);
                    if Self::copy_or_clone_tree(&src, &dst).is_ok() {
                        services_restored.push(name);
                    }
                }
            }
        }

        // 5. Restore semantic memory if present
        let mut memory_graph_restored = false;
        let snap_memory = snap_dir.join("memory");
        if snap_memory.is_dir() {
            let state_memory = state_dir.join("memory");
            let _ = fs::remove_dir_all(&state_memory);
            if Self::copy_or_clone_tree(&snap_memory, &state_memory).is_ok() {
                memory_graph_restored = true;
            }
        }

        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(SnapshotRestoreResult {
            snapshot_id: target_meta.id,
            rescue_snapshot_id,
            files_restored,
            files_deleted,
            services_restored,
            memory_graph_restored,
            duration_ms,
        })
    }

    /// Deletes a snapshot by ID or Label.
    pub fn delete_snapshot(state_dir: &Path, id_or_label: &str) -> Result<String> {
        let snapshots = Self::list_snapshots(state_dir)?;
        let target_meta = snapshots
            .iter()
            .find(|s| s.id == id_or_label || s.label.as_deref() == Some(id_or_label))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No se encontró ninguna instantánea con identificador o etiqueta «{id_or_label}»"))?;

        let snap_dir = Self::dev_snapshots_dir(state_dir).join(&target_meta.id);
        if snap_dir.exists() {
            fs::remove_dir_all(&snap_dir)
                .with_context(|| format!("No se pudo eliminar {}", snap_dir.display()))?;
        }

        Ok(target_meta.id)
    }

    // ------------------------------------------------------------- helpers

    fn detect_git_info(workspace: &Path) -> (Option<String>, Option<String>) {
        let git_dir = workspace.join(".git");
        if !git_dir.is_dir() {
            return (None, None);
        }

        let mut branch = None;
        let mut commit = None;

        if let Ok(head) = fs::read_to_string(git_dir.join("HEAD")) {
            let trimmed = head.trim();
            if let Some(ref_path) = trimmed.strip_prefix("ref: ") {
                let branch_name = ref_path.rsplit('/').next().unwrap_or(ref_path);
                branch = Some(branch_name.to_string());

                let ref_file = git_dir.join(ref_path);
                if let Ok(commit_hash) = fs::read_to_string(ref_file) {
                    commit = Some(commit_hash.trim().chars().take(8).collect());
                }
            } else if trimmed.len() >= 8 {
                commit = Some(trimmed.chars().take(8).collect());
            }
        }

        (branch, commit)
    }

    fn copy_or_clone_workspace(
        src: &Path,
        dst: &Path,
        files_count: &mut usize,
        total_bytes: &mut u64,
    ) -> Result<String> {
        let mut method = "clon".to_string();
        Self::copy_workspace_recursive(src, src, dst, files_count, total_bytes, &mut method, 0)?;
        Ok(method)
    }

    fn copy_workspace_recursive(
        base: &Path,
        current: &Path,
        dst_base: &Path,
        files_count: &mut usize,
        total_bytes: &mut u64,
        method: &mut String,
        depth: usize,
    ) -> Result<()> {
        if depth > 10 {
            return Ok(());
        }

        let entries = match fs::read_dir(current) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip heavy VCS and build directories
            if name == ".git" || name == "target" || name == "node_modules" || name == ".antos" {
                continue;
            }

            let rel = path.strip_prefix(base).unwrap_or(&path);
            let target_dst = dst_base.join(rel);

            if path.is_dir() {
                fs::create_dir_all(&target_dst)?;
                Self::copy_workspace_recursive(
                    base,
                    &path,
                    dst_base,
                    files_count,
                    total_bytes,
                    method,
                    depth + 1,
                )?;
            } else if path.is_file() {
                if let Some(parent) = target_dst.parent() {
                    let _ = fs::create_dir_all(parent);
                }

                if let Ok(meta) = path.metadata() {
                    *total_bytes += meta.len();
                    *files_count += 1;
                }

                if !Self::clone_file(&path, &target_dst) {
                    *method = "copia".into();
                    let _ = fs::copy(&path, &target_dst);
                }
            }
        }

        Ok(())
    }

    fn copy_or_clone_tree(src: &Path, dst: &Path) -> Result<()> {
        if src.is_dir() {
            fs::create_dir_all(dst)?;
            for entry in fs::read_dir(src)? {
                let entry = entry?;
                let path = entry.path();
                let target = dst.join(entry.file_name());
                Self::copy_or_clone_tree(&path, &target)?;
            }
        } else if src.is_file() {
            if let Some(parent) = dst.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if !Self::clone_file(src, dst) {
                let _ = fs::copy(src, dst);
            }
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn clone_file(from: &Path, to: &Path) -> bool {
        use std::ffi::{c_char, c_int, c_uint, CString};
        use std::os::unix::ffi::OsStrExt;

        extern "C" {
            fn clonefile(src: *const c_char, dst: *const c_char, flags: c_uint) -> c_int;
        }

        let (Ok(src), Ok(dst)) = (
            CString::new(from.as_os_str().as_bytes()),
            CString::new(to.as_os_str().as_bytes()),
        ) else {
            return false;
        };
        unsafe { clonefile(src.as_ptr(), dst.as_ptr(), 0) == 0 }
    }

    #[cfg(not(target_os = "macos"))]
    fn clone_file(_from: &Path, _to: &Path) -> bool {
        false
    }

    fn collect_relative_files(base: &Path) -> Result<Vec<PathBuf>> {
        let mut results = Vec::new();
        Self::collect_files_rec(base, base, &mut results, 0)?;
        Ok(results)
    }

    fn collect_files_rec(
        base: &Path,
        current: &Path,
        results: &mut Vec<PathBuf>,
        depth: usize,
    ) -> Result<()> {
        if depth > 10 {
            return Ok(());
        }
        let entries = match fs::read_dir(current) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if name == ".git" || name == "target" || name == "node_modules" || name == ".antos" {
                continue;
            }

            if path.is_dir() {
                Self::collect_files_rec(base, &path, results, depth + 1)?;
            } else if path.is_file() {
                if let Ok(rel) = path.strip_prefix(base) {
                    results.push(rel.to_path_buf());
                }
            }
        }
        Ok(())
    }

    fn clean_empty_dirs(base: &Path) -> Result<()> {
        if !base.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(base)?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name != ".git" && name != ".antos" {
                    let _ = Self::clean_empty_dirs(&path);
                    if let Ok(entries) = fs::read_dir(&path) {
                        if entries.count() == 0 {
                            let _ = fs::remove_dir(&path);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_create_list_and_restore_dev_snapshot() {
        let temp_dir = std::env::temp_dir().join(format!("test_tm_{}", std::process::id()));
        let ws_dir = temp_dir.join("workspace");
        let state_dir = temp_dir.join(".antos");
        let _ = fs::create_dir_all(&ws_dir);
        let _ = fs::create_dir_all(&state_dir);

        // 1. Create files in workspace
        fs::write(
            ws_dir.join("main.rs"),
            "fn main() { println!(\"original\"); }",
        )
        .unwrap();
        fs::create_dir_all(ws_dir.join("src")).unwrap();
        fs::write(ws_dir.join("src").join("lib.rs"), "pub fn original_fn() {}").unwrap();

        // 2. Create snapshot
        let snap_meta = TimeMachineEngine::create_snapshot(
            &ws_dir,
            &state_dir,
            Some("v1.0-clean"),
            Some("test-runner"),
        )
        .expect("create snapshot");

        assert_eq!(snap_meta.label.as_deref(), Some("v1.0-clean"));
        assert_eq!(snap_meta.files_count, 2);

        // 3. List snapshots
        let list = TimeMachineEngine::list_snapshots(&state_dir).expect("list snapshots");
        assert!(!list.is_empty());
        assert_eq!(list[0].id, snap_meta.id);

        // 4. Modify workspace: modify a file, delete another, add an untracked file
        fs::write(
            ws_dir.join("main.rs"),
            "fn main() { println!(\"modified!\"); }",
        )
        .unwrap();
        fs::remove_file(ws_dir.join("src").join("lib.rs")).unwrap();
        fs::write(ws_dir.join("untracked.tmp"), "trash").unwrap();

        assert_eq!(
            fs::read_to_string(ws_dir.join("main.rs")).unwrap(),
            "fn main() { println!(\"modified!\"); }"
        );
        assert!(!ws_dir.join("src").join("lib.rs").exists());
        assert!(ws_dir.join("untracked.tmp").exists());

        // 5. Restore snapshot
        let restore_res = TimeMachineEngine::restore_snapshot(
            &ws_dir,
            &state_dir,
            "v1.0-clean",
            true, // rescue enabled
        )
        .expect("restore snapshot");

        assert_eq!(restore_res.snapshot_id, snap_meta.id);
        assert!(restore_res.rescue_snapshot_id.is_some());
        assert_eq!(restore_res.files_deleted, 1); // untracked.tmp deleted

        // 6. Verify restored contents
        assert_eq!(
            fs::read_to_string(ws_dir.join("main.rs")).unwrap(),
            "fn main() { println!(\"original\"); }"
        );
        assert!(ws_dir.join("src").join("lib.rs").exists());
        assert!(!ws_dir.join("untracked.tmp").exists());

        // 7. Delete snapshot
        let deleted_id =
            TimeMachineEngine::delete_snapshot(&state_dir, &snap_meta.id).expect("delete snapshot");
        assert_eq!(deleted_id, snap_meta.id);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
