//! Schema definitions and serialization contracts for antOS.

use serde::{Deserialize, Serialize};

// ============================================================================
// Developer Environment: LSP, DAP, Dev TUI, CI, Benchmarks and Snapshots
// ============================================================================

// ---------------------------------------------------------------- LSP (T12.1)

/// Type of editor or developer client compatible with the antOS LSP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LspEditorKind {
    VsCode,
    Neovim,
    Helix,
    Emacs,
    Generic,
}

impl LspEditorKind {
    pub fn name(&self) -> &'static str {
        match self {
            LspEditorKind::VsCode => "VS Code",
            LspEditorKind::Neovim => "Neovim",
            LspEditorKind::Helix => "Helix",
            LspEditorKind::Emacs => "Emacs",
            LspEditorKind::Generic => "Generic LSP Client",
        }
    }

    pub fn config_filename(&self) -> &'static str {
        match self {
            LspEditorKind::VsCode => ".vscode/settings.json",
            LspEditorKind::Neovim => "init.lua / nvim-lspconfig",
            LspEditorKind::Helix => "~/.config/helix/languages.toml",
            LspEditorKind::Emacs => ".dir-locals.el / init.el",
            LspEditorKind::Generic => "lsp-client.json",
        }
    }
}

/// Status of the embedded antOS LSP server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspServerStatus {
    pub running: bool,
    pub transport: String,
    pub socket_path: Option<String>,
    pub connected_clients: usize,
    pub active_workspace: String,
    pub indexed_symbols_count: usize,
    pub capabilities: Vec<String>,
}

// ---------------------------------------------------- Collab & DAP (T12.2)

/// Virtual cursor position in a collaborative co-editing session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollabCursor {
    pub client_id: String,
    pub line: usize,
    pub character: usize,
    pub ghost_text: Option<String>,
}

/// Status of a real-time collaborative editing session (CRDT).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollabSessionStatus {
    pub session_id: String,
    pub file_path: String,
    pub collaborators: Vec<String>,
    pub cursors: Vec<CollabCursor>,
    pub buffer_length: usize,
    pub active_ticket_id: Option<String>,
}

/// Breakpoint in an isolated DAP debugging session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapBreakpoint {
    pub id: usize,
    pub file_path: String,
    pub line: usize,
    pub verified: bool,
}

/// Inspected variable during debugging.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapVariable {
    pub name: String,
    pub value: String,
    pub type_name: String,
}

/// Status of a sandbox-isolated DAP debugging session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DapSessionStatus {
    pub session_id: String,
    pub target_command: String,
    pub state: String,
    pub breakpoints: Vec<DapBreakpoint>,
    pub current_line: Option<usize>,
    pub call_stack: Vec<String>,
    pub variables: Vec<DapVariable>,
}



// ----------------------------------------------------------- local ci & git hooks (T20.3)

/// Execution status of a single CI pipeline stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CiStageStatus {
    #[serde(alias = "Pending")]
    Pending,
    #[serde(alias = "Running")]
    Running,
    #[serde(alias = "Passed")]
    Passed,
    #[serde(alias = "Failed")]
    Failed,
    #[serde(alias = "Skipped")]
    Skipped,
}

impl CiStageStatus {
    pub fn label(&self) -> &'static str {
        match self {
            CiStageStatus::Pending => "⏳ Pendiente",
            CiStageStatus::Running => "⚙️ En ejecución",
            CiStageStatus::Passed => "✅ Pasó",
            CiStageStatus::Failed => "❌ Falló",
            CiStageStatus::Skipped => "⏭️ Omitido",
        }
    }
}

/// Result of an individual stage in the CI matrix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiStageResult {
    pub name: String,
    pub command: String,
    pub status: CiStageStatus,
    pub duration_ms: u64,
    pub output_snippet: String,
    pub exit_code: Option<i32>,
}

/// Consolidated report of a full local CI pipeline run (T20.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiReport {
    pub id: String,
    pub success: bool,
    pub stages: Vec<CiStageResult>,
    pub total_duration_ms: u64,
    pub security_clean: bool,
    pub secrets_found: Vec<String>,
    pub timestamp_secs: u64,
}

/// Status of antOS Git pre-commit and pre-push hooks (T20.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHookStatus {
    pub pre_commit_installed: bool,
    pub pre_push_installed: bool,
    pub hook_dir: String,
    pub active_guards: Vec<String>,
}

// ----------------------------------------------------------- dev snapshots & time machine (T20.4)

/// Metadata describing a full workspace & state atomic snapshot (T20.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevSnapshotMetadata {
    pub id: String,
    pub label: Option<String>,
    pub author: String,
    pub timestamp_secs: u64,
    pub git_branch: Option<String>,
    pub git_commit: Option<String>,
    pub files_count: usize,
    pub total_bytes: u64,
    pub services_included: Vec<String>,
    pub memory_graph_included: bool,
    pub method: String,
}

/// Result of a snapshot restore operation (T20.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotRestoreResult {
    pub snapshot_id: String,
    pub rescue_snapshot_id: Option<String>,
    pub files_restored: usize,
    pub files_deleted: usize,
    pub services_restored: Vec<String>,
    pub memory_graph_restored: bool,
    pub duration_ms: u64,
}

// --------------------------------------------------- continuous benchmarking & perf diff (T21.1)

/// A single benchmark measurement for a function, endpoint or test case (T21.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkMetric {
    pub name: String,
    pub mean_ns: u64,
    pub min_ns: u64,
    pub max_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
    pub peak_rss_bytes: u64,
    pub ops_per_sec: f64,
}

/// Report of a full benchmark suite execution (T21.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkRunReport {
    pub id: String,
    pub timestamp_secs: u64,
    pub branch: String,
    pub commit: Option<String>,
    pub suite_name: String,
    pub metrics: Vec<BenchmarkMetric>,
    pub total_duration_ms: u64,
}

/// Comparative metric between a baseline and target benchmark run (T21.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkComparisonMetric {
    pub name: String,
    pub base_mean_ns: u64,
    pub target_mean_ns: u64,
    pub delta_pct: f64,
    pub base_rss_bytes: u64,
    pub target_rss_bytes: u64,
    pub rss_delta_pct: f64,
    pub is_regression: bool,
    pub severity: String,
}

/// Comprehensive diff report comparing performance between branches/worktrees (T21.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkDiffReport {
    pub id: String,
    pub timestamp_secs: u64,
    pub base_branch: String,
    pub target_branch: String,
    pub comparisons: Vec<BenchmarkComparisonMetric>,
    pub has_regression: bool,
    pub max_regression_pct: f64,
    pub auditor_verdict: String,
}



// ------------------------------------ live architecture & mermaid docs (T21.3)

/// Diagram type for living architecture documentation (T21.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchDiagramKind {
    Components,
    IpcFlow,
    AntFlow,
    Full,
}

impl ArchDiagramKind {
    pub fn name(&self) -> &'static str {
        match self {
            ArchDiagramKind::Components => "C4 Componentes / Topología del Workspace",
            ArchDiagramKind::IpcFlow => "Flujo de Datos y Contratos IPC",
            ArchDiagramKind::AntFlow => "Ciclo de Vida Multi-Agente antFlow",
            ArchDiagramKind::Full => "Arquitectura Completa y Diagramas Vivos",
        }
    }
}

/// Generated living architecture report (T21.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchDiagramReport {
    pub kind: ArchDiagramKind,
    pub mermaid_content: String,
    pub crates_count: usize,
    pub modules_count: usize,
    pub caps_count: usize,
    pub generated_at_secs: u64,
}

/// Synchronization result of living architecture documentation in markdown files (T21.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocSyncReport {
    pub files_scanned: usize,
    pub files_updated: usize,
    pub in_sync: bool,
    pub updated_paths: Vec<String>,
    pub message: String,
}

