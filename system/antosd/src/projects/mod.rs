//! The active project of the workspace: one resolution for everyone (T38.1).
//!
//! ## Why this module exists
//!
//! `antos use` used to write `state/active_project` and nothing else read
//! it except the CLI. The daemon resolved its own project once, when it
//! started ([`crate::ctx::Ctx`] is cloned into an `Arc` and shared by every
//! connection), so a selection made afterwards never arrived; and no IPC
//! handler consulted it anyway. The bar, for its part, sent its own working
//! directory as the scope. Three answers to the same question.
//!
//! Everything that needs to know "which project" now comes through here.
//!
//! ## Precedence, and why the daemon ignores `cwd`
//!
//! 1. The project named in the request — explicit always wins.
//! 2. `ANTOS_PROJECT` in the daemon's environment.
//! 3. The persistent selection in `state/active_project` (`antos use`).
//! 4. None: the scope is the whole workspace.
//!
//! The CLI additionally infers the project from the current directory, and
//! that is right for a command typed inside `workspace/api/src`. For the
//! daemon it would be wrong: its working directory is wherever systemd
//! started it, and it serves clients that live somewhere else entirely.
//!
//! ## Read on every call
//!
//! The selection is read from disk each time, not cached. That is the whole
//! point: `antos use` in a terminal has to reach a daemon that is already
//! running, without restarting it.

use antos_protocol::{ProjectOrigin, ProjectStatus, ProjectSummary};
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// File under `state/` holding the persistent selection. Same file that
/// `antos use` has always written: the point is that both agree.
pub const SELECTION_FILE: &str = "active_project";

/// Values that mean "no project" in the selection file.
const CLEARED: [&str; 3] = ["", "none", "system"];

/// Path of the selection file inside `state`.
pub fn selection_path(state: &Path) -> PathBuf {
    state.join(SELECTION_FILE)
}

/// The persisted selection, or `None` when there is none (or it was cleared).
pub fn read_selection(state: &Path) -> Option<String> {
    let content = std::fs::read_to_string(selection_path(state)).ok()?;
    let name = content.trim().to_string();
    if CLEARED.contains(&name.as_str()) {
        return None;
    }
    Some(name)
}

/// Persists the selection. `None` clears it.
pub fn write_selection(state: &Path, name: Option<&str>) -> Result<()> {
    let path = selection_path(state);
    match name {
        Some(name) => {
            std::fs::create_dir_all(state)?;
            std::fs::write(path, name.trim())?;
        }
        None => {
            if path.exists() {
                std::fs::remove_file(path)?;
            }
        }
    }
    Ok(())
}

/// Rejects a project name that is not a plain directory name.
///
/// The name arrives over IPC and is joined to the workspace, so `..`, an
/// absolute path or a separator would step outside the enclosure. This is
/// the check that keeps a project name from becoming a path traversal.
pub fn validate_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        bail!("el nombre del proyecto está vacío");
    }
    if trimmed == "." || trimmed == ".." {
        bail!("«{trimmed}» no es un nombre de proyecto");
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains('\0') {
        bail!(
            "el nombre de un proyecto es el de su directorio en workspace/, sin rutas: «{trimmed}»"
        );
    }
    Ok(())
}

/// Builds the summary of one project directory.
pub fn summarize(path: &Path, is_active: bool) -> ProjectSummary {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());
    let language = crate::exec::fs::detect_project_language(path);
    let (branch, dirty) = match crate::git::GitAnalyzer::global().consultar_estado(path) {
        Ok(Some(status)) => (
            status.branch.clone(),
            !status.modified.is_empty()
                || !status.staged.is_empty()
                || !status.untracked.is_empty(),
        ),
        _ => (None, false),
    };
    ProjectSummary {
        name,
        path: path.display().to_string(),
        language,
        branch,
        dirty,
        is_active,
    }
}

/// Every project of `workspace/`, with the active one marked.
pub fn list(workspace: &Path, state: &Path) -> Vec<ProjectSummary> {
    let active = resolve(workspace, state, None)
        .active
        .map(|p| p.name)
        .unwrap_or_default();
    crate::exec::fs::scan_workspace_projects(workspace)
        .into_iter()
        .map(|path| {
            let is_active = path
                .file_name()
                .map(|n| n.to_string_lossy() == active.as_str())
                .unwrap_or(false);
            summarize(&path, is_active)
        })
        .collect()
}

/// Resolves what antOS operates on, following the precedence above.
///
/// A name that does not exist under `workspace/` is not an error here: it
/// degrades to the workspace scope. Refusing would leave the daemon unable
/// to answer anything at all just because a stale selection points at a
/// project that was deleted.
pub fn resolve(workspace: &Path, state: &Path, requested: Option<&str>) -> ProjectStatus {
    let workspace_str = workspace.display().to_string();

    let candidates: [(Option<String>, ProjectOrigin); 3] = [
        (requested.map(str::to_string), ProjectOrigin::Request),
        (
            std::env::var("ANTOS_PROJECT")
                .ok()
                .filter(|v| !v.is_empty()),
            ProjectOrigin::Environment,
        ),
        (read_selection(state), ProjectOrigin::Selection),
    ];

    for (candidate, origin) in candidates {
        let Some(name) = candidate else { continue };
        if validate_name(&name).is_err() {
            continue;
        }
        let path = workspace.join(name.trim());
        if path.is_dir() && path != workspace {
            return ProjectStatus {
                active: Some(summarize(&path, true)),
                origin,
                workspace: workspace_str,
            };
        }
    }

    ProjectStatus::workspace_scope(workspace_str)
}

/// The directory a request should operate on: the active project, or the
/// workspace when there is none.
pub fn scope_dir(workspace: &Path, state: &Path, requested: Option<&str>) -> PathBuf {
    resolve(workspace, state, requested)
        .active
        .map(|p| PathBuf::from(p.path))
        .unwrap_or_else(|| workspace.to_path_buf())
}

/// Applies a selection: validates, checks that it exists, and persists it.
pub fn select(workspace: &Path, state: &Path, name: Option<&str>) -> Result<ProjectStatus> {
    match name {
        Some(name) => {
            validate_name(name)?;
            let path = workspace.join(name.trim());
            if !path.is_dir() {
                bail!(
                    "el proyecto «{}» no existe en {}",
                    name.trim(),
                    workspace.display()
                );
            }
            write_selection(state, Some(name))?;
        }
        None => write_selection(state, None)?,
    }
    Ok(resolve(workspace, state, None))
}

#[cfg(test)]
mod tests;
