//! El contrato entre el demonio de antOS y sus clientes.
//!
//! Vivía dentro de `sysod` mientras el único cliente era su propio terminal.
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
    /// Helper para verificar si el estado no tiene modificaciones ni archivos pendientes.
    pub fn es_limpio(&self) -> bool {
        self.modificados.is_empty() && self.staged.is_empty() && self.sin_seguimiento.is_empty()
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
    }
}
