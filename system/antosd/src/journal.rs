//! Append-only journal: what was requested, what plan came out, with what
//! permission, and what happened. This is what turns "the AI did something"
//! into something auditable.

use crate::capability::Tier;
use crate::plan::Plan;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// Plan was executed successfully.
    #[serde(alias = "ejecutado")]
    Executed,
    /// User cancelled the plan.
    #[serde(alias = "cancelado")]
    Cancelled,
    /// Plan was denied by policy.
    #[serde(alias = "denegado")]
    Denied,
    /// Execution failed.
    #[serde(alias = "fallido")]
    Failed,
    /// Plan was reverted after execution.
    #[serde(alias = "revertido")]
    Reverted,
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
    /// Which enclosure confined the execution.
    #[serde(default)]
    pub sandbox: String,
    /// Marked when reverted, so `undo` does not revert twice.
    #[serde(default)]
    pub reverted: bool,
}

/// Extracts ticket ID (e.g. T1.2, T3.3) from intent text if present.
pub fn extract_ticket_id(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.');
        if (clean.starts_with('T') || clean.starts_with('t')) && clean.contains('.') {
            let num_part = &clean[1..];
            if num_part.chars().all(|c| c.is_ascii_digit() || c == '.') {
                return Some(clean.to_uppercase());
            }
        }
    }
    None
}

/// Backwards compatibility alias.
#[deprecated(note = "use extract_ticket_id")]
pub fn extraer_ticket_id(texto: &str) -> Option<String> {
    extract_ticket_id(texto)
}

pub fn append(path: &Path, record: &Record) -> Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
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
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_extract_ticket_id() {
        assert_eq!(
            extract_ticket_id("desarrolla el ticket T1.2 ahora"),
            Some("T1.2".into())
        );
        assert_eq!(
            extract_ticket_id("corrige bug en t3.3"),
            Some("T3.3".into())
        );
        assert_eq!(extract_ticket_id("haz commit normal"), None);
    }

    #[test]
    fn test_record_serialization_with_ticket_id() {
        let temp_file = std::env::temp_dir().join("antos_journal_test.jsonl");
        let _ = std::fs::remove_file(&temp_file);

        let rec = Record {
            id: "rec-1".into(),
            at: "2026-09-01T12:00:00Z".into(),
            intent: "execute T3.3".into(),
            ticket_id: Some("T3.3".into()),
            planner: "local".into(),
            plan: crate::plan::Plan {
                id: "p1".into(),
                intent: "execute T3.3".into(),
                planner: "local".into(),
                steps: vec![],
            },
            tier: Tier::Auto,
            reasons: vec![],
            outcome: Outcome::Executed,
            detail: None,
            snapshot: Some("snap-1".into()),
            sandbox: "broker".into(),
            reverted: false,
        };

        append(&temp_file, &rec).expect("append");
        let records = read_all(&temp_file).expect("read_all");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].ticket_id, Some("T3.3".into()));
        assert_eq!(records[0].outcome, Outcome::Executed);

        let _ = std::fs::remove_file(&temp_file);
    }
}
