//! Runtime de agente (T33.2): lo que cruza el IPC cuando un modelo trabaja
//! con herramientas del catálogo durante varios turnos.
//!
//! Un `AgentRun` no es un plan de un solo disparo (`Proposal`): es una
//! conversación en la que cada `tool_use` del modelo pasa por validación,
//! radio de impacto, aprobación, instantánea y ejecución confinada antes de
//! devolverle el resultado. Estos tipos son el contrato con la barra y el
//! CLI: pasos en vivo (`AgentStepEvent`) e informe final (`AgentReport`).

use serde::{Deserialize, Serialize};

/// Límites de un run. Todos se comprueban antes de cada turno; agotar
/// cualquiera termina el run limpiamente con un `AgentReport`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentBudget {
    /// Máximo de herramientas ejecutadas (cada `tool_use` cuenta una).
    pub max_steps: u32,
    /// Máximo de tokens de entrada+salida acumulados según el proveedor.
    pub max_tokens: u64,
    /// Tiempo máximo de pared para el run entero.
    pub max_seconds: u64,
}

impl Default for AgentBudget {
    fn default() -> Self {
        Self {
            max_steps: 20,
            max_tokens: 400_000,
            max_seconds: 600,
        }
    }
}

/// Qué pasó con una llamada a herramienta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStepOutcome {
    /// Ejecutada; `output` lleva lo que se devolvió al modelo (acotado).
    Executed,
    /// El catálogo la rechazó (capacidad fuera del toolset o argumentos
    /// inválidos); el rechazo se devolvió al modelo como error.
    Rejected,
    /// Requería confirmación y el usuario dijo que no; el run termina.
    Declined,
    /// Falló al ejecutarse; el error se devolvió al modelo.
    Failed,
    /// Herramienta terminal `finalizar`: el modelo dio la tarea por hecha.
    Finished,
}

/// Un paso del run, tal como lo ve la barra en vivo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentStepEvent {
    pub run_id: String,
    /// 1-based.
    pub step: u32,
    /// Nombre de la capacidad (o `finalizar`).
    pub tool: String,
    /// Argumentos resumidos en una línea (valores largos recortados).
    pub args_summary: String,
    pub outcome: AgentStepOutcome,
    /// Salida devuelta al modelo, recortada para la interfaz.
    pub output_preview: String,
    /// Tokens acumulados del run tras este paso (si el proveedor los informa).
    pub tokens_used: u64,
}

/// Por qué terminó un run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStopReason {
    /// El modelo llamó a `finalizar`.
    Finished,
    /// Se agotó `max_steps`, `max_tokens` o `max_seconds`.
    BudgetExhausted,
    /// El usuario rechazó un paso que requería confirmación.
    Declined,
    /// El modelo dejó de pedir herramientas sin finalizar (respuesta de texto).
    ModelStopped,
    /// Error del proveedor o del runtime.
    Error,
}

/// Informe final de un run: lo que el CLI imprime y la barra resume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentReport {
    pub run_id: String,
    pub goal: String,
    pub provider: String,
    pub model: String,
    pub stop_reason: AgentStopReason,
    /// Resumen que dio el modelo al finalizar (o su último texto).
    pub summary: String,
    pub steps: u32,
    pub tokens_used: u64,
    pub seconds: u64,
    /// Instantánea que cubre todos los ficheros escritos por el run; `undo`
    /// la restaura entera.
    pub snapshot_id: Option<String>,
    /// Rutas escritas, relativas al workspace.
    pub files_written: Vec<String>,
    /// Detalle del error si `stop_reason == Error`.
    pub error: Option<String>,
}
