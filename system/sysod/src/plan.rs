//! Un plan es lo único que el planificador puede producir: una lista
//! ordenada de invocaciones de capacidad. Nunca una línea de shell.


pub use antos_protocolo::{Plan, Step};

/// Identificador legible y ordenable: la bitácora se lee en orden cronológico
/// sin tener que interpretar nada. Los milisegundos evitan colisiones entre
/// dos planes del mismo segundo.
pub fn nuevo_id() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string()
}
