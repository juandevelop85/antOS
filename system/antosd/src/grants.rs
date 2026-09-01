//! Concesiones: permisos explícitos, acotados y con caducidad para las
//! capacidades de nivel «concesión» y blindaje de secretos (T5.2).
//!
//! La caducidad no es un adorno. Un permiso permanente se concede una vez y
//! se olvida, que es como la autoridad ambiental de Unix vuelve por la puerta
//! de atrás.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Metadata for an active grant including expiration and purpose.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrantEntry {
    pub cap: String,
    pub expires_at: i64,
    pub granted_at: i64,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Grants {
    /// capacidad -> instante de caducidad (epoch en segundos)
    pub until: BTreeMap<String, i64>,
    /// Metadatos detallados de cada concesión (motivo, fecha de creación)
    #[serde(default)]
    pub entries: BTreeMap<String, GrantEntry>,
}

impl Grants {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Grants::default());
        }
        let content = std::fs::read_to_string(path)?;
        let grants: Grants = serde_json::from_str(&content)?;
        Ok(grants)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn is_granted(&self, cap: &str) -> bool {
        self.until.get(cap).map(|&t| t > now()).unwrap_or(false)
    }

    pub fn grant(&mut self, cap: &str, minutes: i64) {
        self.grant_with_reason(cap, minutes, None);
    }

    pub fn grant_with_reason(&mut self, cap: &str, minutes: i64, reason: Option<String>) {
        let current = now();
        let exp = current + minutes * 60;
        self.until.insert(cap.to_string(), exp);
        self.entries.insert(
            cap.to_string(),
            GrantEntry {
                cap: cap.to_string(),
                expires_at: exp,
                granted_at: current,
                reason,
            },
        );
    }

    pub fn revoke(&mut self, cap: &str) {
        self.until.remove(cap);
        self.entries.remove(cap);
    }

    /// Devuelve la lista de concesiones activas que aún no han expirado.
    pub fn list_active(&self) -> Vec<GrantEntry> {
        let current = now();
        let mut list = Vec::new();
        for (cap, &exp) in &self.until {
            if exp > current {
                if let Some(entry) = self.entries.get(cap) {
                    list.push(entry.clone());
                } else {
                    list.push(GrantEntry {
                        cap: cap.clone(),
                        expires_at: exp,
                        granted_at: current,
                        reason: None,
                    });
                }
            }
        }
        list.sort_by(|a, b| a.cap.cmp(&b.cap));
        list
    }
}

fn now() -> i64 {
    chrono::Local::now().timestamp()
}
