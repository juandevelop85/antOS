//! Schema definitions and serialization contracts for antOS.

use serde::{Deserialize, Serialize};

// ============================================================================
// antFlow: Multi-Agent Orchestration, Team Tasks, and Distributed Swarm
// ============================================================================

// ----------------------------------------------------------- distributed swarm (T9.2)

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwarmTaskAssignment {
    pub task_id: String,
    pub ticket_id: String,
    pub role: AgentRole,
    pub assigned_node_id: String,
    pub target_model: Option<String>,
    pub worktree_branch: String,
    pub status: String,
    pub started_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwarmNodeStatus {
    pub node_id: String,
    pub hostname: String,
    pub address: String,
    pub is_local: bool,
    pub vram_available_mb: Option<u64>,
    pub cpu_cores: usize,
    pub running_tasks: Vec<SwarmTaskAssignment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwarmStatus {
    pub nodes: Vec<SwarmNodeStatus>,
    pub total_tasks: usize,
}

// ---------------------------------------------------- antFlow: multi-agent (T3.1)

/// Specialized agent role within the antFlow lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    #[serde(alias = "Arquitecto")]
    Architect,
    Coder,
    QA,
    Auditor,
    VisualQA,
}

#[allow(non_upper_case_globals)]
impl AgentRole {
    pub const Arquitecto: Self = Self::Architect;

    pub fn name(&self) -> &'static str {
        match self {
            AgentRole::Architect => "Architect",
            AgentRole::Coder => "Coder",
            AgentRole::QA => "QA / Tester",
            AgentRole::Auditor => "Security Auditor",
            AgentRole::VisualQA => "Visual QA",
        }
    }

    pub fn nombre(&self) -> &'static str {
        match self {
            AgentRole::Architect => "Arquitecto",
            AgentRole::Coder => "Coder",
            AgentRole::QA => "QA / Tester",
            AgentRole::Auditor => "Auditor de Seguridad",
            AgentRole::VisualQA => "QA Visual",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            AgentRole::Architect => "Technical planning, ticket breakdown and architecture design.",
            AgentRole::Coder => {
                "Modular implementation of changes and refactoring in the worktree."
            }
            AgentRole::QA => "Automated test suite generation and execution in sandbox.",
            AgentRole::Auditor => "Review of diffs, security, style and blast radius.",
            AgentRole::VisualQA => {
                "Multimodal inspection of GUI windows, screenshots and visual regression testing."
            }
        }
    }

    pub fn system_prompt(&self) -> &'static str {
        match self {
            AgentRole::Architect => {
                "You are the Architect Agent of antOS. Your goal is to break down technical tickets \
                 into atomic steps, validate dependencies, and design the architecture adhering \
                 to crate boundaries and zero unwraps in production."
            }
            AgentRole::Coder => {
                "You are the Coder Agent of antOS. Your goal is to implement changes in files \
                 within the assigned ephemeral worktree, maintaining robustness, idiomatic \
                 error handling, and project conventions."
            }
            AgentRole::QA => {
                "You are the QA Agent of antOS. Your goal is to build and run test suites \
                 inside the confined sandbox, detecting failures or regressions and reporting \
                 detailed error output for correction."
            }
            AgentRole::Auditor => {
                "You are the Auditor Agent of antOS. Your goal is to audit generated diffs, \
                 verify that the blast radius does not exceed limits, and ensure all \
                 acceptance criteria are met before merging."
            }
            AgentRole::VisualQA => {
                "You are the Visual QA Agent of antOS. Your goal is to inspect user interface \
                 screenshots, verify layout fidelity, color contrast, typography alignment, \
                 and detect visual glitches or errors in Wayland graphical applications."
            }
        }
    }

    #[deprecated(note = "use description")]
    pub fn descripcion(&self) -> &'static str {
        self.description()
    }

    #[deprecated(note = "use system_prompt")]
    pub fn prompt_sistema(&self) -> &'static str {
        self.system_prompt()
    }
}

/// Lifecycle state machine for an antFlow task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowState {
    #[serde(alias = "Pendiente")]
    Pending,
    #[serde(alias = "Planificando")]
    Planning,
    #[serde(alias = "Implementando")]
    Implementing,
    #[serde(alias = "VerificandoTests")]
    Testing,
    #[serde(alias = "RevisionAuditor")]
    Reviewing,
    #[serde(alias = "ListoParaAprobacion")]
    ReadyForApproval,
    #[serde(alias = "Fusionado")]
    Merged,
    #[serde(alias = "Fallido")]
    Failed,
}

#[allow(non_upper_case_globals)]
impl FlowState {
    pub const Pendiente: Self = Self::Pending;
    pub const Planificando: Self = Self::Planning;
    pub const Implementando: Self = Self::Implementing;
    pub const VerificandoTests: Self = Self::Testing;
    pub const RevisionAuditor: Self = Self::Reviewing;
    pub const ListoParaAprobacion: Self = Self::ReadyForApproval;
    pub const Fusionado: Self = Self::Merged;
    pub const Fallido: Self = Self::Failed;
    pub fn label(&self) -> &'static str {
        match self {
            FlowState::Pending => "Pending",
            FlowState::Planning => "Planning (Architect)",
            FlowState::Implementing => "Implementing (Coder)",
            FlowState::Testing => "Running Tests (QA)",
            FlowState::Reviewing => "Reviewing (Auditor)",
            FlowState::ReadyForApproval => "Ready for Approval",
            FlowState::Merged => "Merged",
            FlowState::Failed => "Failed",
        }
    }

    pub fn tag(&self) -> &'static str {
        match self {
            FlowState::Pending => "⏳ Pending",
            FlowState::Planning => "📐 Planning (Architect)",
            FlowState::Implementing => "💻 Implementing (Coder)",
            FlowState::Testing => "🧪 Testing (QA)",
            FlowState::Reviewing => "🛡️ Reviewing (Auditor)",
            FlowState::ReadyForApproval => "✨ Ready for Approval",
            FlowState::Merged => "✅ Merged",
            FlowState::Failed => "❌ Failed",
        }
    }

    pub fn active_role(&self) -> Option<AgentRole> {
        match self {
            FlowState::Planning => Some(AgentRole::Architect),
            FlowState::Implementing => Some(AgentRole::Coder),
            FlowState::Testing => Some(AgentRole::QA),
            FlowState::Reviewing => Some(AgentRole::Auditor),
            _ => None,
        }
    }
}

/// Record of a lifecycle state transition in antFlow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowTransition {
    #[serde(alias = "timestamp_segundos")]
    pub timestamp_seconds: u64,
    #[serde(alias = "estado_anterior")]
    pub old_state: FlowState,
    #[serde(alias = "estado_nuevo")]
    pub new_state: FlowState,
    #[serde(alias = "rol")]
    pub role: Option<AgentRole>,
    #[serde(alias = "detalle")]
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Active or historical task orchestrated by antFlow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowTask {
    pub id: String,
    pub ticket_id: String,
    #[serde(alias = "estado")]
    pub state: FlowState,
    #[serde(alias = "rol_actual")]
    pub current_role: Option<AgentRole>,
    pub worktree_path: Option<String>,
    pub branch_name: Option<String>,
    #[serde(alias = "reintentos_qa")]
    pub qa_retries: u32,
    #[serde(alias = "max_reintentos_qa")]
    pub max_qa_retries: u32,
    pub diff_preview: Option<String>,
    #[serde(alias = "resumen_auditoria")]
    pub audit_summary: Option<String>,
    #[serde(alias = "historial")]
    pub history: Vec<FlowTransition>,
}

/// Panel kind inside the integrated Dev TUI workspace (T20.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DevPanelKind {
    /// Main editor frame (Neovim).
    #[serde(alias = "Editor")]
    Editor,
    /// Agent monitor panel showing antFlow transitions and models.
    #[serde(alias = "AgentMonitor")]
    AgentMonitor,
    /// Syntax-highlighted diff and git status viewer.
    #[serde(alias = "DiffViewer")]
    DiffViewer,
    /// Interactive VTE terminal tray.
    #[serde(alias = "Terminal")]
    Terminal,
}

impl DevPanelKind {
    pub fn name(&self) -> &'static str {
        match self {
            DevPanelKind::Editor => "Editor (Neovim)",
            DevPanelKind::AgentMonitor => "antFlow Monitor",
            DevPanelKind::DiffViewer => "Visor de Diffs",
            DevPanelKind::Terminal => "Terminal VTE",
        }
    }
}

/// Geometric dimensions and configuration of Dev TUI panels (T20.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevPanelRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

/// Status and configuration of the Dev TUI workspace session (T20.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevWorkspaceStatus {
    pub active_project: Option<String>,
    pub active_panel: DevPanelKind,
    pub editor_command: String,
    pub side_panel_visible: bool,
    pub terminal_drawer_open: bool,
    pub term_columns: u16,
    pub term_rows: u16,
    pub editor_rect: DevPanelRect,
    pub agent_monitor_rect: DevPanelRect,
    pub diff_viewer_rect: DevPanelRect,
    pub terminal_rect: Option<DevPanelRect>,
    pub registered_hotkeys: Vec<String>,
}

/// Programming language or runtime detected from an error or stack trace (T20.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorLanguage {
    #[serde(alias = "Rust")]
    Rust,
    #[serde(alias = "Python")]
    Python,
    #[serde(alias = "JavaScript")]
    JavaScript,
    #[serde(alias = "Generic")]
    Generic,
}

impl ErrorLanguage {
    pub fn name(&self) -> &'static str {
        match self {
            ErrorLanguage::Rust => "Rust",
            ErrorLanguage::Python => "Python",
            ErrorLanguage::JavaScript => "JavaScript/TypeScript",
            ErrorLanguage::Generic => "Genérico",
        }
    }
}

/// Single stack frame location extracted from a backtrace (T20.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedStackFrame {
    pub file: String,
    pub line: Option<u32>,
    pub col: Option<u32>,
    pub function: Option<String>,
}

/// Structured diagnostic parsed from raw compiler errors, panics, or tracebacks (T20.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedErrorDiagnostic {
    pub language: ErrorLanguage,
    pub error_type: String,
    pub message: String,
    pub target_file: Option<String>,
    pub target_line: Option<u32>,
    pub target_function: Option<String>,
    pub frames: Vec<ParsedStackFrame>,
}

/// Verification phase in the autonomous TDD lifecycle (T20.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TddPhase {
    /// Test reproduced successfully and is failing (Red).
    #[serde(alias = "Red")]
    Red,
    /// Patch applied and test is now passing (Green).
    #[serde(alias = "Green")]
    Green,
    /// Auditor confirmed safety, diff isolation, and test suite pass (Verified).
    #[serde(alias = "Verified")]
    Verified,
}

impl TddPhase {
    pub fn label(&self) -> &'static str {
        match self {
            TddPhase::Red => "🔴 Red (Falla reproducible)",
            TddPhase::Green => "🟢 Green (Corrección validada)",
            TddPhase::Verified => "🛡️ Verified (Certificado por Auditor)",
        }
    }
}

/// Consolidated report of an autonomous bug reproduction run (T20.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TddRegressionReport {
    pub id: String,
    pub diagnostic: ParsedErrorDiagnostic,
    pub test_code: String,
    pub test_file: String,
    pub phase: TddPhase,
    pub fix_summary: Option<String>,
    pub audited: bool,
}
