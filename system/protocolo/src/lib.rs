//! El contrato entre el demonio de syso y sus clientes.
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub capability: String,
    pub args: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub intent: String,
    pub planner: String,
    pub steps: Vec<Step>,
}

// ------------------------------------------------------------------- diff

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Line {
    Info(String),
    Add(String),
    Del(String),
}

// -------------------------------------------------------------- propuesta

/// Lo que se le enseña a alguien antes de tocar nada.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Radio {
    pub escribe: Vec<String>,
    pub borra: Vec<String>,
    pub lee: Vec<String>,
    pub sistema: Vec<String>,
    pub red: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recinto {
    pub motor: String,
    pub garantiza: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resultado {
    pub ok: bool,
    pub mensaje: String,
    pub instantanea: Option<String>,
}

// ---------------------------------------------------------------- mensajes

#[derive(Debug, Serialize, Deserialize)]
pub enum Peticion {
    Intencion {
        texto: String,
        planificador: Option<String>,
        seco: bool,
    },
    /// La respuesta a una propuesta. Es lo ÚNICO que un cliente decide.
    Aprobacion(bool),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Evento {
    Inicio { intencion: String, planificador: String },
    Nota(String),
    Propuesta(Box<Propuesta>),
    Salida(String),
    Resultado(Resultado),
    Error(String),
}
