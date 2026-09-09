//! Schema definitions and serialization contracts for antOS.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ============================================================================
// Tier, Steps and Execution Plans
// ============================================================================

// ------------------------------------------------------------ tier and plan

/// The order of variants IS the scale: Auto < Confirm < Grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Auto,
    Confirm,
    Grant,
}

/// The base of the scale. The default value being the most permissive level
/// is safe precisely because derivation only elevates privileges upward.
impl Default for Tier {
    fn default() -> Self {
        Tier::Auto
    }
}

impl Tier {
    pub fn label(self) -> &'static str {
        match self {
            Tier::Auto => "auto",
            Tier::Confirm => "confirm",
            Tier::Grant => "grant",
        }
    }
}

/// Atomic step in an execution plan requiring a specific capability.
///
/// # JSON Example
/// ```json
/// {
///   "capability": "git",
///   "args": { "action": "status" }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub capability: String,
    pub args: BTreeMap<String, String>,
}

/// Structured plan of action composed of discrete steps.
///
/// # JSON Example
/// ```json
/// {
///   "id": "plan-001",
///   "intent": "Inspect git repository status",
///   "planner": "antos-core",
///   "steps": [
///     { "capability": "git", "args": { "action": "status" } }
///   ]
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub intent: String,
    pub planner: String,
    pub steps: Vec<Step>,
}

// ------------------------------------------------------------------- diff

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Line {
    Info(String),
    Add(String),
    Del(String),
}

// ----------------------------------------------------------- interactive diff (T8.1)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineKind {
    Context,
    Addition,
    Deletion,
    HunkHeader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyntaxTokenType {
    Keyword,
    Type,
    StringLit,
    Comment,
    Number,
    Added,
    Deleted,
    Normal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxToken {
    pub text: String,
    pub token_type: SyntaxTokenType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_line_num: Option<usize>,
    pub new_line_num: Option<usize>,
    pub content: String,
    pub tokens: Vec<SyntaxToken>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffHunk {
    pub header: String,
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffFile {
    pub old_path: String,
    pub new_path: String,
    pub additions: usize,
    pub deletions: usize,
    pub hunks: Vec<DiffHunk>,
}

// ---------------------------------------------------------------- proposal

/// What is presented to a user before touching anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub plan: Plan,
    #[serde(alias = "cambios")]
    pub changes: Vec<Line>,
    #[serde(alias = "radio")]
    pub blast_radius: BlastRadius,
    #[serde(alias = "nivel")]
    pub tier: Tier,
    #[serde(alias = "razones")]
    pub reasons: Vec<String>,
    #[serde(alias = "recinto")]
    pub enclosure: Enclosure,
    /// If `true`, nothing will be executed: it is a dry run / preview.
    #[serde(alias = "seco")]
    pub dry_run: bool,
}

/// The declared effects, resolved to readable paths.
///
/// Resolved in the daemon and not the client on purpose: a client
/// should not need filesystem access to show a plan.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlastRadius {
    #[serde(alias = "escribe")]
    pub writes: Vec<String>,
    #[serde(alias = "borra")]
    pub deletes: Vec<String>,
    #[serde(alias = "lee")]
    pub reads: Vec<String>,
    #[serde(alias = "sistema")]
    pub system: Vec<String>,
    #[serde(alias = "red")]
    pub network: Vec<String>,
}

/// Execution sandbox enclosure parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Enclosure {
    #[serde(alias = "motor")]
    pub engine: String,
    #[serde(alias = "garantiza")]
    pub guarantees: String,
}

/// Outcome of executing a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub ok: bool,
    #[serde(alias = "mensaje")]
    pub message: String,
    #[serde(alias = "instantanea")]
    pub snapshot: Option<String>,
}
