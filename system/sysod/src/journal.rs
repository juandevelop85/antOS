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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub at: String,
    pub intent: String,
    #[serde(default)]
    pub ticket_id: Option<String>,
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

/// Extrae un ID de ticket (ej. T1.2, T3.3) si está presente en el texto de la intención.
pub fn extraer_ticket_id(texto: &str) -> Option<String> {
    for palabra in texto.split_whitespace() {
        let limpia = palabra.trim_matches(|c: char| !c.is_alphanumeric() && c != '.');
        if (limpia.starts_with('T') || limpia.starts_with('t')) && limpia.contains('.') {
            let num_part = &limpia[1..];
            if num_part.chars().all(|c| c.is_ascii_digit() || c == '.') {
                return Some(limpia.to_uppercase());
            }
        }
    }
    None
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

pub fn rewrite(path: &Path, records: &[Record]) -> Result<()> {
    let mut out = String::new();
    for r in records {
        out.push_str(&serde_json::to_string(r)?);
        out.push('\n');
    }
    std::fs::write(path, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extraer_ticket_id() {
        assert_eq!(extraer_ticket_id("desarrolla el ticket T1.2 ahora"), Some("T1.2".into()));
        assert_eq!(extraer_ticket_id("corrige bug en t3.3"), Some("T3.3".into()));
        assert_eq!(extraer_ticket_id("haz commit normal"), None);
    }

    #[test]
    fn test_serializacion_record_con_ticket_id() {
        let temp_file = std::env::temp_dir().join("antos_journal_test.jsonl");
        let _ = std::fs::remove_file(&temp_file);

        let rec = Record {
            id: "rec-1".into(),
            at: "2026-09-01T12:00:00Z".into(),
            intent: "ejecutar T3.3".into(),
            ticket_id: Some("T3.3".into()),
            planner: "local".into(),
            plan: crate::plan::Plan {
                id: "p1".into(),
                intent: "ejecutar T3.3".into(),
                planner: "local".into(),
                steps: vec![],
            },
            tier: Tier::Auto,
            reasons: vec![],
            outcome: Outcome::Ejecutado,
            detail: None,
            snapshot: Some("snap-1".into()),
            sandbox: "broker".into(),
            reverted: false,
        };

        append(&temp_file, &rec).expect("append");
        let leidos = read_all(&temp_file).expect("read_all");
        assert_eq!(leidos.len(), 1);
        assert_eq!(leidos[0].ticket_id, Some("T3.3".into()));
        assert_eq!(leidos[0].outcome, Outcome::Ejecutado);

        let _ = std::fs::remove_file(&temp_file);
    }
}
