//! Schema definitions and serialization contracts for antOS.

use serde::{Deserialize, Serialize};

// ============================================================================
// Git Introspection, Repository Status, and Forge Integrations
// ============================================================================

// ----------------------------------------------------- git introspection (T1.1)

/// Modification status of a tracked or untracked file in Git.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitFileStatus {
    Modified,
    Created,
    Deleted,
    Renamed,
    TypeChanged,
    Conflicted,
}

/// Granular change summary for a file inside the repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitFileDiffSummary {
    #[serde(alias = "ruta")]
    pub path: String,
    #[serde(alias = "lineas_anadidas")]
    pub added_lines: usize,
    #[serde(alias = "lineas_borradas")]
    pub deleted_lines: usize,
    #[serde(alias = "estado")]
    pub status: GitFileStatus,
}

/// Global status of a Git repository in the workspace.
///
/// # JSON Example
/// ```json
/// {
///   "branch": "main",
///   "head_commit": "a1b2c3d",
///   "ahead": 0,
///   "behind": 0,
///   "modified": [],
///   "staged": [],
///   "untracked": [],
///   "clean": true
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GitRepoStatus {
    /// Current active branch (e.g. `main`, `feature/x`) or `None` if detached HEAD.
    #[serde(alias = "rama")]
    pub branch: Option<String>,
    /// Short or full commit hash of HEAD.
    pub head_commit: Option<String>,
    /// Number of local commits ahead of remote upstream.
    #[serde(alias = "delante")]
    pub ahead: usize,
    /// Number of local commits behind remote upstream.
    #[serde(alias = "detras")]
    pub behind: usize,
    /// Files with unstaged modifications in working tree.
    #[serde(alias = "modificados")]
    pub modified: Vec<GitFileDiffSummary>,
    /// Files staged in the index.
    pub staged: Vec<GitFileDiffSummary>,
    /// Untracked files in the repository.
    #[serde(alias = "sin_seguimiento")]
    pub untracked: Vec<String>,
    /// Indicates if both working tree and index are clean.
    #[serde(alias = "limpio")]
    pub clean: bool,
}

impl GitRepoStatus {
    pub fn is_clean(&self) -> bool {
        self.modified.is_empty() && self.staged.is_empty() && self.untracked.is_empty()
    }
}

// --------------------------------------------------- git forge & issues / PRs (T21.2)

/// Type of collaborative Git hosting forge (T21.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForgeKind {
    GitHub,
    GitLab,
    Generic,
}

impl ForgeKind {
    pub fn name(&self) -> &'static str {
        match self {
            ForgeKind::GitHub => "GitHub",
            ForgeKind::GitLab => "GitLab",
            ForgeKind::Generic => "Git Forge",
        }
    }
}

/// Metadata identifying a remote repository origin (T21.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRepoInfo {
    pub host: String,
    pub owner: String,
    pub name: String,
    pub forge: ForgeKind,
    pub raw_url: String,
}

/// Structured issue imported from a remote forge (T21.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteIssue {
    pub id: u64,
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub author: String,
    pub labels: Vec<String>,
    pub url: String,
    pub created_at: String,
}

/// Pull Request / Merge Request descriptor (T21.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemotePullRequest {
    pub id: u64,
    pub number: u64,
    pub title: String,
    pub body: String,
    pub head_branch: String,
    pub base_branch: String,
    pub state: String,
    pub url: String,
    pub draft: bool,
}

/// Status and CI inspection of an existing Pull Request (T21.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestStatusReport {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub mergeable: bool,
    pub ci_status: Option<String>,
    pub url: String,
}
