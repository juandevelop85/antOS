//! System path context: workspace location, state directory, and capability catalogue.
//!
//! ## Contextual discovery (T17.1)
//!
//! `Ctx::discover` now ascends the directory tree from `current_dir` to locate
//! the antOS installation root (identified by the presence of `system/capabilities`).
//! This allows commands to be run from inside project subdirectories such as
//! `workspace/api-service` without losing track of the capability catalogue or the
//! system root.
//!
//! It also detects the active project by checking whether the current directory
//! resides under `workspace/` and, if so, records the first path component below
//! the workspace root as `current_project`.

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

pub struct Ctx {
    /// The sole location where capabilities are allowed to touch files.
    pub workspace: PathBuf,
    /// Runtime state: snapshots, journal, grants.
    pub state: PathBuf,
    /// Directory containing capability manifests.
    pub caps_dir: PathBuf,
    /// The declarative system configuration root.
    ///
    /// This is the SECOND root that antOS recognises, and is not just another
    /// workspace: changes here affect the whole machine, so they always require
    /// an explicit grant.
    pub system_config: PathBuf,
    /// The antOS installation root detected by ascending from `current_dir`.
    ///
    /// Set to `None` only when running outside the antOS tree (unusual).
    pub antos_root: Option<PathBuf>,
    /// The active developer project inside `workspace/`, inferred from `cwd`.
    ///
    /// For example, if `cwd` is `workspace/api-service/src`, this will be
    /// `workspace/api-service`. `None` when not inside any project.
    pub current_project: Option<PathBuf>,
}

impl Ctx {
    pub fn discover() -> Result<Self> {
        // ── Step 1: locate the antOS installation root ──────────────────────
        // Ascend from cwd looking for the `system/capabilities` marker directory.
        let antos_root = find_antos_root_from_cwd();

        // ── Step 2: resolve workspace ────────────────────────────────────────
        let workspace = match std::env::var_os("ANTOS_WORKSPACE")
            .or_else(|| std::env::var_os("SYSO_WORKSPACE"))
        {
            Some(v) => PathBuf::from(v),
            None => {
                // Prefer the workspace relative to the detected antOS root;
                // fall back to a relative path (legacy behaviour).
                antos_root
                    .as_deref()
                    .map(|r| r.join("workspace"))
                    .unwrap_or_else(|| PathBuf::from("workspace"))
            }
        };
        std::fs::create_dir_all(&workspace)?;
        // Canonicalisation is mandatory: containment checks compare path prefixes,
        // and "workspace" vs "/Users/.../workspace" would not match otherwise.
        let workspace = workspace.canonicalize()?;

        // ── Step 3: resolve state dir ────────────────────────────────────────
        let state = match std::env::var_os("ANTOS_STATE")
            .or_else(|| std::env::var_os("SYSO_STATE"))
        {
            Some(v) => PathBuf::from(v),
            None => {
                // Prefer state dir relative to antOS root when known.
                let base = antos_root.as_deref().unwrap_or(Path::new("."));
                if base.join(".antos").is_dir() {
                    base.join(".antos")
                } else if base.join(".syso").is_dir() {
                    base.join(".syso")
                } else {
                    base.join(".antos")
                }
            }
        };
        std::fs::create_dir_all(&state)?;
        let state = state.canonicalize()?;

        // ── Step 4: locate the capability catalogue ──────────────────────────
        let caps_dir = match std::env::var_os("ANTOS_CAPABILITIES")
            .or_else(|| std::env::var_os("SYSO_CAPABILITIES"))
        {
            Some(v) => PathBuf::from(v),
            None => {
                // Build candidate list anchored at the antOS root (preferred),
                // then the legacy relative paths as fallbacks.
                let mut candidates: Vec<PathBuf> = Vec::new();
                if let Some(ref root) = antos_root {
                    candidates.push(root.join("system").join("capabilities"));
                }
                candidates.push(PathBuf::from("system/capabilities"));
                candidates.push(PathBuf::from("capabilities"));
                candidates.push(PathBuf::from("../capabilities"));

                candidates
                    .into_iter()
                    .find(|p| p.is_dir())
                    .ok_or_else(|| {
                        anyhow!(
                            "capability catalogue not found; set ANTOS_CAPABILITIES or run from inside the antOS tree"
                        )
                    })?
            }
        };

        // ── Step 5: system configuration root ───────────────────────────────
        let system_config = match std::env::var_os("ANTOS_SYSTEM_CONFIG")
            .or_else(|| std::env::var_os("SYSO_SYSTEM_CONFIG"))
        {
            Some(v) => PathBuf::from(v),
            None => {
                let nixos = PathBuf::from("/etc/nixos");
                // Outside NixOS there is no declarative configuration to govern;
                // use a placeholder inside the state directory.
                if nixos.is_dir() {
                    nixos
                } else {
                    state.join("etc-nixos")
                }
            }
        };
        std::fs::create_dir_all(&system_config)?;
        let system_config = system_config.canonicalize()?;

        // ── Step 6: detect active project ───────────────────────────────────
        let current_project = detect_current_project(&workspace, &state);

        Ok(Ctx {
            workspace,
            state,
            caps_dir,
            system_config,
            antos_root,
            current_project,
        })
    }

    pub fn snapshots_dir(&self) -> PathBuf { self.state.join("snapshots") }
    pub fn journal_path(&self) -> PathBuf { self.state.join("journal.jsonl") }
    pub fn grants_path(&self) -> PathBuf { self.state.join("grants.json") }

    /// Renders a path relative to the workspace for cleaner terminal output.
    pub fn display<'a>(&self, p: &'a Path) -> String {
        p.strip_prefix(&self.workspace)
            .map(|r| r.display().to_string())
            .unwrap_or_else(|_| p.display().to_string())
    }
}

// ─────────────────────────────────────────────────────────────── T17.1 ──────

/// Ascends from `cwd` looking for the antOS installation root.
///
/// The root is identified by the presence of a `system/capabilities` directory.
/// Returns `None` when called from an unrelated directory.
fn find_antos_root_from_cwd() -> Option<PathBuf> {
    let start = std::env::current_dir().ok()?;
    let mut candidate = start.as_path();
    loop {
        if candidate.join("system").join("capabilities").is_dir() {
            return Some(candidate.to_path_buf());
        }
        match candidate.parent() {
            Some(p) => candidate = p,
            None => return None,
        }
    }
}

/// Resolves the currently active project inside `workspace/`.
///
/// Precedence order:
/// 1. The project directory containing `cwd` (if cwd is inside `workspace/<project>`).
/// 2. The `ANTOS_PROJECT` environment variable (if pointing to an existing directory in `workspace/`).
/// 3. The persistent selection in `state/active_project` (set via `antos use <project>`).
/// 4. `None` (running at workspace root or globally).
fn detect_current_project(workspace: &Path, state: &Path) -> Option<PathBuf> {
    if let Some(cwd_proj) = detect_project_from_cwd(workspace) {
        return Some(cwd_proj);
    }

    if let Some(env_proj) = std::env::var_os("ANTOS_PROJECT") {
        let p = workspace.join(env_proj);
        if p.is_dir() && p != workspace {
            return Some(p);
        }
    }

    let active_file = state.join("active_project");
    if let Ok(content) = std::fs::read_to_string(&active_file) {
        let name = content.trim();
        if !name.is_empty() && name != "none" && name != "system" {
            let p = workspace.join(name);
            if p.is_dir() && p != workspace {
                return Some(p);
            }
        }
    }

    None
}

/// Returns the first-level project directory under `workspace/` that contains
/// the current working directory, if any.
///
/// For example:
/// - `cwd = workspace/api-service/src` → `Some(workspace/api-service)`
/// - `cwd = workspace`                 → `None` (at the root, not inside a project)
/// - `cwd = /tmp/other`                → `None`
fn detect_project_from_cwd(workspace: &Path) -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let cwd_canon = cwd.canonicalize().ok().unwrap_or(cwd);

    // Strip the workspace prefix to get the relative path inside workspace/.
    let relative = cwd_canon.strip_prefix(workspace).ok()?;

    // The first component is the project name (e.g. "api-service").
    let project_name = relative.components().next()?;
    let project_path = workspace.join(project_name);

    if project_path.is_dir() && project_path != workspace {
        Some(project_path)
    } else {
        None
    }
}

// ───────────────────────────────────────────────────────────────────── tests ──

#[cfg(test)]
mod tests {
    use super::*;

    /// T17.1 — detect_current_project must return None for directories that do
    /// not reside inside the workspace and when no active project is set.
    #[test]
    fn test_detect_current_project_outside_workspace() {
        let workspace = PathBuf::from("/tmp/antos_fake_workspace");
        let state = PathBuf::from("/tmp/antos_fake_state");
        let result = detect_current_project(&workspace, &state);
        assert!(
            result.is_none(),
            "project detection should return None when cwd is outside workspace"
        );
    }

    #[test]
    fn test_detect_current_project_with_persistent_state() {
        let unique = format!("antos_test_proj_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
        let root = std::env::temp_dir().join(unique);
        let workspace = root.join("workspace");
        let state = root.join("state");
        let proj = workspace.join("web-api");
        std::fs::create_dir_all(&proj).expect("create proj");
        std::fs::create_dir_all(&state).expect("create state");

        // Sin active_project debe ser None
        assert!(detect_current_project(&workspace, &state).is_none());

        // Con active_project fijado
        std::fs::write(state.join("active_project"), "web-api").expect("write active_project");
        let detected = detect_current_project(&workspace, &state);
        assert_eq!(detected, Some(proj));

        // Con active_project borrado o "none"
        std::fs::write(state.join("active_project"), "none").expect("write none");
        assert!(detect_current_project(&workspace, &state).is_none());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_detect_current_project_with_env_var() {
        let unique = format!("antos_test_proj_env_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
        let root = std::env::temp_dir().join(unique);
        let workspace = root.join("workspace");
        let state = root.join("state");
        let proj = workspace.join("micro-svc");
        std::fs::create_dir_all(&proj).expect("create proj");
        std::fs::create_dir_all(&state).expect("create state");

        std::env::set_var("ANTOS_PROJECT", "micro-svc");
        let detected = detect_current_project(&workspace, &state);
        assert_eq!(detected, Some(proj));
        std::env::remove_var("ANTOS_PROJECT");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// T17.1 — find_antos_root_from_cwd must find the antOS root when invoked
    /// from within the antOS repository (which is the case for cargo test).
    #[test]
    fn test_find_antos_root_from_cwd_finds_root() {
        let root = find_antos_root_from_cwd();
        assert!(
            root.is_some(),
            "should detect antOS root from within the repository tree"
        );
        let root = root.unwrap();
        assert!(
            root.join("system").join("capabilities").is_dir(),
            "detected root must contain system/capabilities"
        );
    }

    /// T17.1 — Ctx::discover resolves the capability catalogue even when
    /// invoked from a directory that does not directly contain system/capabilities.
    #[test]
    fn test_ctx_discover_resolves_caps_dir() {
        let ctx = Ctx::discover();
        assert!(
            ctx.is_ok(),
            "Ctx::discover must succeed from within antOS tree: {:?}",
            ctx.err()
        );
        let ctx = ctx.unwrap();
        assert!(
            ctx.caps_dir.is_dir(),
            "caps_dir must point to an existing directory; got {}",
            ctx.caps_dir.display()
        );
    }

    /// T17.1 — antos_root field must be populated when running inside the repo.
    #[test]
    fn test_ctx_antos_root_is_populated() {
        let ctx = Ctx::discover().expect("discover");
        assert!(
            ctx.antos_root.is_some(),
            "antos_root must be set when running from within antOS tree"
        );
    }
}
