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

pub trait Planner {
    fn name(&self) -> &'static str;
    fn plan(&self, intent: &str, catalog: &Catalog) -> Result<Vec<Step>>;
}
