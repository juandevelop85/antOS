//! Un plan es lo único que el planificador puede producir: una lista
//! ordenada de invocaciones de capacidad. Nunca una línea de shell.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

impl Plan {
    /// Identificador legible y ordenable: la bitácora se lee en orden
    /// cronológico sin tener que interpretar nada.
    pub fn new_id() -> String {
        chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string()
    }
}
