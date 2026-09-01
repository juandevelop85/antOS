//! Planificadores: convierten una intención en lenguaje natural en una lista
//! de invocaciones de capacidad.
//!
//! Es la ÚNICA etapa donde participa un modelo, y su salida se valida entera
//! contra el catálogo antes de que nadie la mire. Nada de lo que devuelva un
//! planificador se considera de fiar.

pub mod claude;
pub mod local;

use crate::capability::Catalog;
use crate::plan::Step;
use anyhow::Result;

/// Lo que devuelve un planificador: los pasos, y lo que haya querido decir
/// de palabra.
///
/// La nota importa. Un modelo que propone un plan parcial suele explicar por
/// qué —«primero hay que crear el proyecto»—, y tirar ese texto convierte una
/// limitación explicada en un plan misteriosamente incompleto.
pub struct Propuesta {
    pub steps: Vec<Step>,
    pub nota: Option<String>,
}

impl Propuesta {
    pub fn solo(steps: Vec<Step>) -> Self {
        Propuesta { steps, nota: None }
    }
}

pub trait Planner {
    fn name(&self) -> &'static str;
    fn plan(&self, intent: &str, catalog: &Catalog) -> Result<Propuesta>;
}
