//! Schema definitions and serialization contracts for antOS.

use serde::{Deserialize, Serialize};

// ============================================================================
// Specification Engine, Technical Tickets, and Acceptance Criteria
// ============================================================================

// ------------------------------------------------- spec engine and tickets (T1.3)

/// Status of a specification or development ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    #[serde(alias = "Pendiente")]
    Pending,
    #[serde(alias = "EnProgreso")]
    InProgress,
    #[serde(alias = "EnRevision")]
    InReview,
    #[serde(alias = "Completado")]
    Completed,
}

#[allow(non_upper_case_globals)]
impl TicketStatus {
    pub const Pendiente: Self = Self::Pending;
    pub const EnProgreso: Self = Self::InProgress;
    pub const EnRevision: Self = Self::InReview;
    pub const Completado: Self = Self::Completed;

    pub fn label(&self) -> &'static str {
        match self {
            TicketStatus::Pending => "Pending",
            TicketStatus::InProgress => "In Progress",
            TicketStatus::InReview => "In Review",
            TicketStatus::Completed => "Completed",
        }
    }

    pub fn tag(&self) -> &'static str {
        match self {
            TicketStatus::Pending => "⏳ Pending",
            TicketStatus::InProgress => "🔄 In Progress",
            TicketStatus::InReview => "🔍 In Review",
            TicketStatus::Completed => "✅ Completed",
        }
    }
}

/// Summary of a ticket for listings and Kanban boards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketSummary {
    pub id: String,
    #[serde(alias = "fase")]
    pub phase: String,
    #[serde(alias = "titulo")]
    pub title: String,
    #[serde(alias = "estado")]
    pub status: TicketStatus,
    #[serde(alias = "ruta_archivo")]
    pub file_path: String,
}

/// Full details of a ticket parsed from Markdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketDetail {
    pub id: String,
    #[serde(alias = "fase")]
    pub phase: String,
    #[serde(alias = "titulo")]
    pub title: String,
    #[serde(alias = "estado")]
    pub status: TicketStatus,
    #[serde(alias = "ruta_archivo")]
    pub file_path: String,
    #[serde(alias = "descripcion")]
    pub description: String,
    #[serde(alias = "alcance_tecnico")]
    pub technical_scope: Vec<String>,
    #[serde(alias = "criterios_aceptacion")]
    pub acceptance_criteria: Vec<String>,
}

/// Diagnostic info for a listening TCP port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortDiagnosticInfo {
    pub port: u16,
    pub pid: u32,
    pub process_name: String,
    pub command: String,
    pub working_dir: Option<String>,
}
