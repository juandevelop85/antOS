//! El contrato entre el demonio de antOS y sus clientes.
//!
//! Vivía dentro de `antosd` mientras el único cliente era su propio terminal.
//! Sale a un crate aparte en cuanto aparece un segundo cliente —la barra de
//! intención— porque la alternativa sería que cada uno tuviera su copia de
//! estos tipos. Un protocolo duplicado es un protocolo que diverge.
//!
//! Aquí NO hay lógica: ni se decide un nivel de permiso, ni se calcula un
//! diff, ni se valida nada. Eso vive en el demonio, y es deliberado — un
//! cliente que pudiera calcular su propio nivel podría elegirlo.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ------------------------------------------------------------ nivel y plan

/// El orden de las variantes ES la escala: Auto < Confirm < Grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Auto,
    Confirm,
    Grant,
}

/// El suelo de la escala. Que el valor por defecto sea el nivel MÁS permisivo
/// es seguro precisamente porque la derivación solo sabe subir.
impl Default for Tier {
    fn default() -> Self {
        Tier::Auto
    }
}

impl Tier {
    pub fn label(self) -> &'static str {
        match self {
            Tier::Auto => "automático",
            Tier::Confirm => "confirmación",
            Tier::Grant => "concesión",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub capability: String,
    pub args: BTreeMap<String, String>,
}

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

// ----------------------------------------------------------- diff interactivo (T8.1)

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

// ----------------------------------------------------------- notificaciones (T8.2)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    TaskFinished,
    ApprovalRequired,
    QAFailed,
    SecurityAlert,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationAction {
    Approve,
    Reject,
    ViewDiff,
    Dismiss,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationItem {
    pub id: String,
    pub ticket_id: String,
    pub title: String,
    pub body: String,
    pub kind: NotificationKind,
    pub created_at: u64,
    pub read: bool,
    pub actions: Vec<NotificationAction>,
}

// ----------------------------------------------------------- red p2p / antMesh (T9.1)

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeResources {
    pub cpu_cores: usize,
    pub memory_mb: u64,
    pub vram_mb: Option<u64>,
    pub available_models: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerNode {
    pub id: String,
    pub hostname: String,
    pub address: String,
    pub latency_ms: u64,
    pub connected: bool,
    pub resources: NodeResources,
    pub last_seen_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingToken {
    pub token: String,
    pub node_id: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshStatus {
    pub local_node: PeerNode,
    pub peers: Vec<PeerNode>,
}

// -------------------------------------------------------------- propuesta

/// Lo que se le enseña a alguien antes de tocar nada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Propuesta {
    pub plan: Plan,
    pub cambios: Vec<Line>,
    pub radio: Radio,
    pub nivel: Tier,
    pub razones: Vec<String>,
    pub recinto: Recinto,
    /// Si es `true`, no se ejecutará pase lo que pase: solo se está mirando.
    pub seco: bool,
}

/// Los efectos DECLARADOS, ya resueltos a texto.
///
/// Se resuelven en el demonio y no en el cliente a propósito: un cliente no
/// debería necesitar acceso al sistema de ficheros para enseñar un plan.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Radio {
    pub escribe: Vec<String>,
    pub borra: Vec<String>,
    pub lee: Vec<String>,
    pub sistema: Vec<String>,
    pub red: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recinto {
    pub motor: String,
    pub garantiza: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resultado {
    pub ok: bool,
    pub mensaje: String,
    pub instantanea: Option<String>,
}

// ------------------------------------------------- introspección git (T1.1)

/// Estado de modificación de un archivo rastreado o no rastreado en Git.
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

/// Resumen granular de cambios en un archivo dentro del repositorio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitFileDiffSummary {
    pub ruta: String,
    pub lineas_anadidas: usize,
    pub lineas_borradas: usize,
    pub estado: GitFileStatus,
}

/// Estado global de un repositorio Git en el espacio de trabajo.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GitRepoStatus {
    /// Rama activa actual (ej. `main`, `feature/x`) o `None` si es HEAD desacoplado.
    pub rama: Option<String>,
    /// Hash abreviado o completo del commit HEAD.
    pub head_commit: Option<String>,
    /// Cantidad de commits locales por delante del upstream remoto.
    pub delante: usize,
    /// Cantidad de commits locales por detrás del upstream remoto.
    pub detras: usize,
    /// Archivos con modificaciones en el árbol de trabajo (no staged).
    pub modificados: Vec<GitFileDiffSummary>,
    /// Archivos preparados en el índice (staged).
    pub staged: Vec<GitFileDiffSummary>,
    /// Archivos no rastreados en el repositorio.
    pub sin_seguimiento: Vec<String>,
    /// Indica si el árbol de trabajo y el índice están totalmente limpios.
    pub limpio: bool,
}

impl GitRepoStatus {
    pub fn is_clean(&self) -> bool {
        self.modificados.is_empty() && self.staged.is_empty() && self.sin_seguimiento.is_empty()
    }

    /// Helper para verificar si el estado no tiene modificaciones ni archivos pendientes.
    pub fn es_limpio(&self) -> bool {
        self.is_clean()
    }
}

// ------------------------------------------------ spec engine y tickets (T1.3)

/// Estado de un ticket de especificación o desarrollo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    Pendiente,
    EnProgreso,
    EnRevision,
    Completado,
}

impl TicketStatus {
    pub fn label(&self) -> &'static str {
        match self {
            TicketStatus::Pendiente => "Pending",
            TicketStatus::EnProgreso => "In Progress",
            TicketStatus::EnRevision => "In Review",
            TicketStatus::Completado => "Completed",
        }
    }

    pub fn etiqueta(&self) -> &'static str {
        match self {
            TicketStatus::Pendiente => "⏳ Pendiente",
            TicketStatus::EnProgreso => "🔄 En Progreso",
            TicketStatus::EnRevision => "🔍 En Revisión",
            TicketStatus::Completado => "✅ Completado",
        }
    }
}

/// Resumen de un ticket para listados y tableros Kanban.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketSummary {
    pub id: String,
    pub fase: String,
    pub titulo: String,
    pub estado: TicketStatus,
    pub ruta_archivo: String,
}

/// Detalle completo de un ticket parseado desde Markdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketDetail {
    pub id: String,
    pub fase: String,
    pub titulo: String,
    pub estado: TicketStatus,
    pub ruta_archivo: String,
    pub descripcion: String,
    pub alcance_tecnico: Vec<String>,
    pub criterios_aceptacion: Vec<String>,
}

/// Información de diagnóstico de un puerto TCP en escucha.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortDiagnosticInfo {
    pub port: u16,
    pub pid: u32,
    pub process_name: String,
    pub command: String,
    pub working_dir: Option<String>,
}

// ---------------------------------------------------------------- antFlow: multi-agente (T3.1)

/// Rol especializado de un agente dentro del flujo antFlow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Arquitecto,
    Coder,
    QA,
    Auditor,
}

impl AgentRole {
    pub fn name(&self) -> &'static str {
        match self {
            AgentRole::Arquitecto => "Architect",
            AgentRole::Coder => "Coder",
            AgentRole::QA => "QA / Tester",
            AgentRole::Auditor => "Security Auditor",
        }
    }

    pub fn nombre(&self) -> &'static str {
        match self {
            AgentRole::Arquitecto => "Arquitecto",
            AgentRole::Coder => "Coder",
            AgentRole::QA => "QA / Tester",
            AgentRole::Auditor => "Auditor de Seguridad",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            AgentRole::Arquitecto => "Technical planning, ticket breakdown and architecture design.",
            AgentRole::Coder => "Modular implementation of changes and refactoring in the worktree.",
            AgentRole::QA => "Automated test suite generation and execution in sandbox.",
            AgentRole::Auditor => "Review of diffs, security, style and blast radius.",
        }
    }

    pub fn descripcion(&self) -> &'static str {
        match self {
            AgentRole::Arquitecto => "Planificación técnica, descomposición de tickets y diseño de arquitectura.",
            AgentRole::Coder => "Implementación modular de cambios y refactorización en el worktree.",
            AgentRole::QA => "Generación y ejecución de pruebas automatizadas en sandbox.",
            AgentRole::Auditor => "Revisión de diffs, seguridad, estilo y radio de impacto.",
        }
    }

    pub fn system_prompt(&self) -> &'static str {
        match self {
            AgentRole::Arquitecto => {
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
        }
    }

    pub fn prompt_sistema(&self) -> &'static str {
        self.system_prompt()
    }
}

/// Estado en la máquina de estados del ciclo de vida de una tarea en antFlow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowState {
    Pendiente,
    Planificando,
    Implementando,
    VerificandoTests,
    RevisionAuditor,
    ListoParaAprobacion,
    Fusionado,
    Fallido,
}

impl FlowState {
    pub fn label(&self) -> &'static str {
        match self {
            FlowState::Pendiente => "Pending",
            FlowState::Planificando => "Planning (Architect)",
            FlowState::Implementando => "Implementing (Coder)",
            FlowState::VerificandoTests => "Running Tests (QA)",
            FlowState::RevisionAuditor => "Reviewing (Auditor)",
            FlowState::ListoParaAprobacion => "Ready for Approval",
            FlowState::Fusionado => "Merged",
            FlowState::Fallido => "Failed",
        }
    }

    pub fn etiqueta(&self) -> &'static str {
        match self {
            FlowState::Pendiente => "⏳ Pendiente",
            FlowState::Planificando => "📐 Planificando (Arquitecto)",
            FlowState::Implementando => "💻 Implementando (Coder)",
            FlowState::VerificandoTests => "🧪 Verificando Tests (QA)",
            FlowState::RevisionAuditor => "🛡️ Revisión (Auditor)",
            FlowState::ListoParaAprobacion => "✨ Listo para Aprobación",
            FlowState::Fusionado => "✅ Fusionado",
            FlowState::Fallido => "❌ Fallido",
        }
    }

    pub fn active_role(&self) -> Option<AgentRole> {
        match self {
            FlowState::Planificando => Some(AgentRole::Arquitecto),
            FlowState::Implementando => Some(AgentRole::Coder),
            FlowState::VerificandoTests => Some(AgentRole::QA),
            FlowState::RevisionAuditor => Some(AgentRole::Auditor),
            _ => None,
        }
    }

    pub fn rol_activo(&self) -> Option<AgentRole> {
        self.active_role()
    }
}

/// Registro de una transición de estado en el flujo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowTransition {
    pub timestamp_segundos: u64,
    pub estado_anterior: FlowState,
    pub estado_nuevo: FlowState,
    pub rol: Option<AgentRole>,
    pub detalle: String,
}

/// Tarea activa o histórica gestionada por el orquestador antFlow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowTask {
    pub id: String,
    pub ticket_id: String,
    pub estado: FlowState,
    pub rol_actual: Option<AgentRole>,
    pub worktree_path: Option<String>,
    pub branch_name: Option<String>,
    pub reintentos_qa: u32,
    pub max_reintentos_qa: u32,
    pub diff_preview: Option<String>,
    pub resumen_auditoria: Option<String>,
    pub historial: Vec<FlowTransition>,
}

// ---------------------------------------------------------------- mensajes

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Peticion {
    Intencion {
        texto: String,
        planificador: Option<String>,
        seco: bool,
    },
    /// La respuesta a una propuesta. Es lo ÚNICO que un cliente decide.
    Aprobacion(bool),
    /// Consulta el estado del repositorio Git en la ruta del espacio de trabajo.
    ConsultarEstadoGit {
        workspace_path: String,
    },
    /// Lista todos los tickets disponibles en el espacio de trabajo.
    ListarTickets {
        workspace_path: String,
    },
    /// Obtiene el detalle de un ticket específico en el espacio de trabajo.
    ObtenerTicket {
        workspace_path: String,
        ticket_id: String,
    },
    /// Diagnostica puertos TCP en escucha y los procesos asociados.
    DiagnosticarPuertos {
        port: Option<u16>,
    },
    /// Inicia el flujo multi-agente antFlow para un ticket.
    IniciarFlow {
        workspace_path: String,
        ticket_id: String,
    },
    /// Consulta el estado de la tarea antFlow para un ticket.
    ConsultarFlow {
        ticket_id: String,
    },
    /// Lista todas las tareas de agentes activas.
    ListarFlows {
        workspace_path: String,
    },
    /// Aprueba o rechaza los cambios finales de una tarea en antFlow.
    AprobarFlow {
        ticket_id: String,
        decision: bool,
    },
    /// Consulta el diff estructurado y sintáctico para un ticket, archivo o commit (T8.1)
    ConsultarDiff {
        workspace_path: String,
        target: Option<String>,
    },
    /// Lista notificaciones pendientes de agentes y del sistema (T8.2)
    ListarNotificaciones {
        workspace_path: String,
    },
    /// Ejecuta una acción sobre una notificación (aprobación, rechazo, descarte) (T8.2)
    AccionNotificacion {
        workspace_path: String,
        notification_id: String,
        action: NotificationAction,
    },
    /// Consulta el estado de la red P2P antMesh y los peers conocidos (T9.1)
    ConsultarMesh {
        workspace_path: String,
    },
    /// Conecta a un nodo peer por dirección IP/puerto o multiaddr (T9.1)
    ConectarPeer {
        workspace_path: String,
        address: String,
    },
    /// Genera un token seguro de emparejamiento con expiración (T9.1)
    GenerarTokenEmparejamiento {
        workspace_path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Evento {
    Inicio { intencion: String, planificador: String },
    Nota(String),
    Propuesta(Box<Propuesta>),
    Salida(String),
    Resultado(Resultado),
    /// Respuesta con el estado detallado del repositorio Git.
    EstadoGit(GitRepoStatus),
    /// Respuesta cuando el directorio consultado no es un repositorio Git válido.
    NoEsRepoGit,
    /// Respuesta con el listado de tickets encontrados en el workspace.
    ListaTickets(Vec<TicketSummary>),
    /// Respuesta con el detalle de un ticket específico.
    DetalleTicket(Option<TicketDetail>),
    /// Respuesta con el listado de puertos diagnosticados.
    EstadoPuertos(Vec<PortDiagnosticInfo>),
    /// Estado detallado de una tarea de agentes antFlow.
    EstadoFlow(Option<FlowTask>),
    /// Listado de todas las tareas antFlow.
    ListaFlows(Vec<FlowTask>),
    /// Notificación de transición de estado en antFlow.
    TransicionFlow {
        ticket_id: String,
        estado_anterior: FlowState,
        estado_nuevo: FlowState,
        rol: Option<AgentRole>,
        detalle: String,
    },
    /// Respuesta con diffs estructurados y coloreados sintácticamente (T8.1)
    DiffEstructurado(Vec<DiffFile>),
    /// Respuesta con la lista de notificaciones activas (T8.2)
    ListaNotificaciones(Vec<NotificationItem>),
    /// Respuesta al ejecutar una acción sobre una notificación (T8.2)
    ResultadoNotificacion {
        id: String,
        success: bool,
        message: String,
    },
    /// Respuesta con el estado de la malla antMesh y lista de peers (T9.1)
    EstadoMesh(MeshStatus),
    /// Token de emparejamiento generado para un nuevo nodo (T9.1)
    TokenEmparejamientoGenerado(PairingToken),
    /// Resultado de la conexión a un nodo peer (T9.1)
    ResultadoConexionPeer {
        address: String,
        success: bool,
        message: String,
    },
    Error(String),
}

/// Alias semánticos para clientes y especificaciones IPC.
pub type Mensaje = Peticion;
pub type Respuesta = Evento;

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serializacion_git_repo_status() {
        let status = GitRepoStatus {
            rama: Some("main".into()),
            head_commit: Some("a1b2c3d".into()),
            delante: 2,
            detras: 0,
            modificados: vec![GitFileDiffSummary {
                ruta: "system/protocolo/src/lib.rs".into(),
                lineas_anadidas: 45,
                lineas_borradas: 2,
                estado: GitFileStatus::Modified,
            }],
            staged: vec![GitFileDiffSummary {
                ruta: "Cargo.toml".into(),
                lineas_anadidas: 1,
                lineas_borradas: 0,
                estado: GitFileStatus::Created,
            }],
            sin_seguimiento: vec!["scratch.txt".into()],
            limpio: false,
        };

        let json = serde_json::to_string(&status).expect("debe serializar a JSON");
        let deserializado: GitRepoStatus =
            serde_json::from_str(&json).expect("debe deserializar desde JSON");

        assert_eq!(status, deserializado);
        assert!(!deserializado.es_limpio());
    }

    #[test]
    fn test_serializacion_peticion_consultar_estado_git() {
        let peticion = Mensaje::ConsultarEstadoGit {
            workspace_path: "/Users/dev/workspace".into(),
        };

        let json = serde_json::to_string(&peticion).expect("debe serializar petición");
        let deserializado: Peticion =
            serde_json::from_str(&json).expect("debe deserializar petición");

        assert_eq!(peticion, deserializado);
    }

    #[test]
    fn test_serializacion_respuesta_estado_git_y_no_es_repo() {
        let resp_ok = Respuesta::EstadoGit(GitRepoStatus {
            rama: Some("feature/git-inspector".into()),
            limpio: true,
            ..Default::default()
        });

        let json_ok = serde_json::to_string(&resp_ok).expect("debe serializar EstadoGit");
        let deserializado_ok: Evento =
            serde_json::from_str(&json_ok).expect("debe deserializar EstadoGit");
        assert_eq!(resp_ok, deserializado_ok);

        let resp_no_repo = Respuesta::NoEsRepoGit;
        let json_no_repo =
            serde_json::to_string(&resp_no_repo).expect("debe serializar NoEsRepoGit");
        let deserializado_no_repo: Evento =
            serde_json::from_str(&json_no_repo).expect("debe deserializar NoEsRepoGit");
        assert_eq!(resp_no_repo, deserializado_no_repo);
    }

    #[test]
    fn test_compatibilidad_mensajes_existentes() {
        let peticion_intencion = Peticion::Intencion {
            texto: "compilar kernel".into(),
            planificador: Some("reglas".into()),
            seco: false,
        };
        let json = serde_json::to_string(&peticion_intencion).expect("serializar intencion");
        let deserializado: Peticion = serde_json::from_str(&json).expect("deserializar intencion");
        assert_eq!(peticion_intencion, deserializado);

        let evento_nota = Evento::Nota("analizando dependencias".into());
        let json_nota = serde_json::to_string(&evento_nota).expect("serializar nota");
        let deserializado_nota: Evento =
            serde_json::from_str(&json_nota).expect("deserializar nota");
        assert_eq!(evento_nota, deserializado_nota);
    }

    #[test]
    fn test_serializacion_tickets_protocolo() {
        let ticket = TicketDetail {
            id: "T1.3".into(),
            fase: "Fase 1".into(),
            titulo: "Indexador y parser de tickets".into(),
            estado: TicketStatus::EnProgreso,
            ruta_archivo: "docs/tickets/T1.3-spec-engine-tickets-parser.md".into(),
            descripcion: "Construir indexador de tickets".into(),
            alcance_tecnico: vec!["Parser markdown".into(), "Mensajes IPC".into()],
            criterios_aceptacion: vec!["Comando antos tickets".into()],
        };

        let json = serde_json::to_string(&ticket).expect("serializar ticket");
        let deserializado: TicketDetail = serde_json::from_str(&json).expect("deserializar ticket");
        assert_eq!(ticket, deserializado);

        let peticion_listar = Peticion::ListarTickets {
            workspace_path: "/workspace".into(),
        };
        let json_peticion = serde_json::to_string(&peticion_listar).expect("serializar peticion listar");
        let des_peticion: Peticion = serde_json::from_str(&json_peticion).expect("deserializar peticion listar");
        assert_eq!(peticion_listar, des_peticion);

        let respuesta_lista = Evento::ListaTickets(vec![TicketSummary {
            id: "T1.3".into(),
            fase: "Fase 1".into(),
            titulo: "Indexador y parser de tickets".into(),
            estado: TicketStatus::EnProgreso,
            ruta_archivo: "docs/tickets/T1.3-spec-engine-tickets-parser.md".into(),
        }]);
        let json_resp = serde_json::to_string(&respuesta_lista).expect("serializar lista tickets");
        let des_resp: Evento = serde_json::from_str(&json_resp).expect("deserializar lista tickets");
        assert_eq!(respuesta_lista, des_resp);

        let info_puerto = PortDiagnosticInfo {
            port: 3000,
            pid: 12345,
            process_name: "node".into(),
            command: "node server.js".into(),
            working_dir: Some("/app".into()),
        };
        let json_puerto = serde_json::to_string(&info_puerto).expect("serializar puerto");
        let des_puerto: PortDiagnosticInfo = serde_json::from_str(&json_puerto).expect("deserializar puerto");
        assert_eq!(info_puerto, des_puerto);

        let task = FlowTask {
            id: "flow-1".into(),
            ticket_id: "T3.1".into(),
            estado: FlowState::Planificando,
            rol_actual: Some(AgentRole::Arquitecto),
            worktree_path: Some("/state/worktrees/t3.1".into()),
            branch_name: Some("agent/t3.1".into()),
            reintentos_qa: 0,
            max_reintentos_qa: 3,
            diff_preview: Some("+ nuevo modulo flow".into()),
            resumen_auditoria: Some("arquitectura aprobada".into()),
            historial: vec![FlowTransition {
                timestamp_segundos: 1700000000,
                estado_anterior: FlowState::Pendiente,
                estado_nuevo: FlowState::Planificando,
                rol: Some(AgentRole::Arquitecto),
                detalle: "asignando tarea al arquitecto".into(),
            }],
        };

        let json_task = serde_json::to_string(&task).expect("serializar task");
        let des_task: FlowTask = serde_json::from_str(&json_task).expect("deserializar task");
        assert_eq!(task, des_task);
    }

    #[test]
    fn test_serializacion_diff_estructurado() {
        let diff_file = DiffFile {
            old_path: "src/main.rs".into(),
            new_path: "src/main.rs".into(),
            additions: 2,
            deletions: 1,
            hunks: vec![DiffHunk {
                header: "@@ -10,4 +10,5 @@".into(),
                old_start: 10,
                old_lines: 4,
                new_start: 10,
                new_lines: 5,
                lines: vec![
                    DiffLine {
                        kind: DiffLineKind::Context,
                        old_line_num: Some(10),
                        new_line_num: Some(10),
                        content: "fn main() {".into(),
                        tokens: vec![
                            SyntaxToken { text: "fn".into(), token_type: SyntaxTokenType::Keyword },
                            SyntaxToken { text: " main() {".into(), token_type: SyntaxTokenType::Normal },
                        ],
                    },
                    DiffLine {
                        kind: DiffLineKind::Addition,
                        old_line_num: None,
                        new_line_num: Some(11),
                        content: "    println!(\"antOS\");".into(),
                        tokens: vec![
                            SyntaxToken { text: "    println!".into(), token_type: SyntaxTokenType::Keyword },
                            SyntaxToken { text: "(\"antOS\");".into(), token_type: SyntaxTokenType::StringLit },
                        ],
                    },
                ],
            }],
        };

        let req = Peticion::ConsultarDiff {
            workspace_path: "/ws".into(),
            target: Some("T8.1".into()),
        };
        let json_req = serde_json::to_string(&req).expect("serialize req");
        let des_req: Peticion = serde_json::from_str(&json_req).expect("deserialize req");
        assert_eq!(req, des_req);

        let event = Evento::DiffEstructurado(vec![diff_file.clone()]);
        let json_ev = serde_json::to_string(&event).expect("serialize event");
        let des_ev: Evento = serde_json::from_str(&json_ev).expect("deserialize event");
        assert_eq!(event, des_ev);
    }

    #[test]
    fn test_serializacion_notificaciones() {
        let notif = NotificationItem {
            id: "notif-1".into(),
            ticket_id: "T8.2".into(),
            title: "Revisión requerida para T8.2".into(),
            body: "Agente QA validó todos los tests con éxito.".into(),
            kind: NotificationKind::ApprovalRequired,
            created_at: 1700000000,
            read: false,
            actions: vec![NotificationAction::Approve, NotificationAction::Reject, NotificationAction::ViewDiff],
        };

        let req = Peticion::AccionNotificacion {
            workspace_path: "/ws".into(),
            notification_id: "notif-1".into(),
            action: NotificationAction::Approve,
        };
        let json_req = serde_json::to_string(&req).expect("serialize req notif");
        let des_req: Peticion = serde_json::from_str(&json_req).expect("deserialize req notif");
        assert_eq!(req, des_req);

        let event = Evento::ListaNotificaciones(vec![notif.clone()]);
        let json_ev = serde_json::to_string(&event).expect("serialize event notif");
        let des_ev: Evento = serde_json::from_str(&json_ev).expect("deserialize event notif");
        assert_eq!(event, des_ev);
    }

    #[test]
    fn test_serializacion_antmesh() {
        let peer = PeerNode {
            id: "node-e4f812".into(),
            hostname: "workstation-gpu".into(),
            address: "192.168.1.50:9042".into(),
            latency_ms: 12,
            connected: true,
            resources: NodeResources {
                cpu_cores: 16,
                memory_mb: 65536,
                vram_mb: Some(24576),
                available_models: vec!["qwen2.5-coder:7b".into(), "deepseek-coder:33b".into()],
            },
            last_seen_secs: 1700000000,
        };

        let status = MeshStatus {
            local_node: peer.clone(),
            peers: vec![peer.clone()],
        };

        let req = Peticion::ConectarPeer {
            workspace_path: "/ws".into(),
            address: "192.168.1.50:9042".into(),
        };
        let json_req = serde_json::to_string(&req).expect("serialize mesh req");
        let des_req: Peticion = serde_json::from_str(&json_req).expect("deserialize mesh req");
        assert_eq!(req, des_req);

        let event = Evento::EstadoMesh(status.clone());
        let json_ev = serde_json::to_string(&event).expect("serialize mesh event");
        let des_ev: Evento = serde_json::from_str(&json_ev).expect("deserialize mesh event");
        assert_eq!(event, des_ev);
    }
}
