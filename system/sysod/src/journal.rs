//! Bitácora append-only: qué se pidió, qué plan salió, con qué permiso y
//! qué pasó. Es lo que convierte «la IA hizo algo» en algo auditable.

use crate::capability::Tier;
use crate::plan::Plan;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Ejecutado,
    Cancelado,
    Denegado,
    Fallido,
    Revertido,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub at: String,
    pub intent: String,
    pub planner: String,
    pub plan: Plan,
    pub tier: Tier,
    pub reasons: Vec<String>,
    pub outcome: Outcome,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub snapshot: Option<String>,
    /// Qué recinto confinó la ejecución. Sin esto, la bitácora no puede
    /// responder a «¿esto corrió confinado o no?».
    #[serde(default)]
    pub sandbox: String,
    /// Se marca al revertir, para que `undo` no deshaga dos veces lo mismo.
    #[serde(default)]
    pub reverted: bool,
}

pub fn append(path: &Path, record: &Record) -> Result<()> {
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", serde_json::to_string(record)?)?;
    Ok(())
}

pub fn read_all(path: &Path) -> Result<Vec<Record>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(path)?;
    Ok(raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect())
}

/// Reescribe la bitácora entera. Solo se usa para marcar una entrada como
/// revertida: el fichero sigue siendo append-only para todo lo demás.
pub fn rewrite(path: &Path, records: &[Record]) -> Result<()> {
    let mut out = String::new();
    for r in records {
        out.push_str(&serde_json::to_string(r)?);
        out.push('\n');
    }
    std::fs::write(path, out)?;
    Ok(())
}
