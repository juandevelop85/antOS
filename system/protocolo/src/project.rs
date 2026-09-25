//! Projects of the workspace and which one antOS is operating on (T38.1).
//!
//! Until this type existed, "active project" meant three different things
//! depending on who was asked: the CLI resolved it from `cwd`, an
//! environment variable and a file written by `antos use`; the Dev TUI used
//! a different variable and a different directory; and the bar simply sent
//! its own working directory. None of them reached the daemon's request
//! handlers, so `antos use` governed nothing that the desktop did.
//!
//! This module is the single answer to "what am I operating on", and it
//! travels over IPC so that every client — terminal or bar — shares it.

use serde::{Deserialize, Serialize};

/// Where the active project selection comes from.
///
/// The origin is part of the answer, not a detail: the confusion this type
/// fixes was precisely that nobody could tell *why* antOS was operating
/// where it was operating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectOrigin {
    /// The request named a project explicitly; it wins over everything.
    Request,
    /// The `ANTOS_PROJECT` environment variable of the daemon.
    Environment,
    /// The persistent selection written by `antos use` (or `UseProject`).
    Selection,
    /// No project: the scope is the whole workspace.
    None,
}

impl ProjectOrigin {
    /// Human-readable reason, in Spanish, for the CLI and the bar.
    pub fn reason_es(&self) -> &'static str {
        match self {
            ProjectOrigin::Request => "pedido explícitamente en esta petición",
            ProjectOrigin::Environment => "variable de entorno ANTOS_PROJECT",
            ProjectOrigin::Selection => "selección persistente (`antos use`)",
            ProjectOrigin::None => "sin proyecto activo: ámbito del workspace",
        }
    }
}

/// A project inside `workspace/`, as the system sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSummary {
    /// Directory name inside `workspace/`; the identifier used by `antos use`.
    pub name: String,
    /// Absolute path.
    pub path: String,
    /// Language or stack detected from the manifest or the files.
    pub language: String,
    /// Current git branch, or `None` when it is not a repository (or the
    /// HEAD is detached).
    pub branch: Option<String>,
    /// `true` when the working tree has changes.
    pub dirty: bool,
    /// `true` for the project that is active right now.
    pub is_active: bool,
}

/// What antOS is operating on, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectStatus {
    /// The active project, or `None` when operating on the whole workspace.
    pub active: Option<ProjectSummary>,
    /// Where that answer comes from.
    pub origin: ProjectOrigin,
    /// The workspace the daemon serves; the root every project hangs from.
    pub workspace: String,
}

impl ProjectStatus {
    /// The scope with no project selected.
    pub fn workspace_scope(workspace: impl Into<String>) -> Self {
        ProjectStatus {
            active: None,
            origin: ProjectOrigin::None,
            workspace: workspace.into(),
        }
    }

    /// Name of the active project, or `None`.
    pub fn active_name(&self) -> Option<&str> {
        self.active.as_ref().map(|p| p.name.as_str())
    }
}
