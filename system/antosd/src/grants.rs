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
        self.is_granted_at(cap, now())
    }

    /// Núcleo de `is_granted`, con el instante actual inyectado en vez de
    /// leído del reloj del sistema (T31.13). Sin esto, probar el límite
    /// exacto de caducidad —`expires_at == now`— sería una carrera contra el
    /// propio reloj: `now()` avanza entre el `insert` de la prueba y la
    /// comprobación real.
    fn is_granted_at(&self, cap: &str, current: i64) -> bool {
        self.until.get(cap).map(|&t| t > current).unwrap_or(false)
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_is_granted_true_while_expiry_is_in_the_future() {
        let mut grants = Grants::default();
        grants.until.insert("fs.delete".into(), 1_000);
        assert!(grants.is_granted_at("fs.delete", 999));
    }

    #[test]
    fn test_is_granted_false_once_expired() {
        let mut grants = Grants::default();
        grants.until.insert("fs.delete".into(), 1_000);
        assert!(!grants.is_granted_at("fs.delete", 1_001));
    }

    #[test]
    fn test_is_granted_false_at_the_exact_expiry_instant() {
        // Límite exacto (Alcance T31.13): `until` es el último instante en
        // que la concesión sigue viva, no uno más. `t > current`, no
        // `t >= current` — en el propio instante de caducidad ya no vale.
        let mut grants = Grants::default();
        grants.until.insert("fs.delete".into(), 1_000);
        assert!(!grants.is_granted_at("fs.delete", 1_000));
    }

    #[test]
    fn test_is_granted_false_for_a_capability_never_granted() {
        let grants = Grants::default();
        assert!(!grants.is_granted_at("fs.delete", 0));
    }

    #[test]
    fn test_revoke_removes_both_the_expiry_and_the_entry() {
        let mut grants = Grants::default();
        grants.grant("fs.delete", 5);
        assert!(grants.is_granted("fs.delete"));
        assert!(grants.entries.contains_key("fs.delete"));

        grants.revoke("fs.delete");

        assert!(!grants.is_granted("fs.delete"));
        assert!(!grants.entries.contains_key("fs.delete"));
    }

    #[test]
    fn test_grant_with_reason_records_it_in_the_entry() {
        let mut grants = Grants::default();
        grants.grant_with_reason("secret.read", 10, Some("depurar T31.13".into()));

        let entry = grants.entries.get("secret.read").expect("entry recorded");
        assert_eq!(entry.reason.as_deref(), Some("depurar T31.13"));
        assert!(entry.expires_at > entry.granted_at);
    }

    #[test]
    fn test_list_active_excludes_expired_grants() {
        let mut grants = Grants::default();
        // Concesión viva.
        grants.grant("fs.delete", 5);
        // Concesión caducada, insertada directamente para no depender del reloj.
        grants.until.insert("secret.read".into(), 1);
        grants.entries.insert(
            "secret.read".into(),
            GrantEntry {
                cap: "secret.read".into(),
                expires_at: 1,
                granted_at: 0,
                reason: None,
            },
        );

        let active = grants.list_active();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].cap, "fs.delete");
    }

    #[test]
    fn test_list_active_falls_back_to_a_synthetic_entry_when_metadata_is_missing() {
        // `until` sin su `GrantEntry` correspondiente en `entries` puede pasar
        // si el fichero de concesiones se editó a mano; `list_active` no debe
        // entrar en pánico, debe sintetizar una entrada mínima.
        let mut grants = Grants::default();
        grants.until.insert("fs.delete".into(), i64::MAX);

        let active = grants.list_active();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].cap, "fs.delete");
        assert_eq!(active[0].reason, None);
    }

    #[test]
    fn test_load_missing_file_returns_empty_grants() {
        let path = std::env::temp_dir().join(format!(
            "antos-grants-test-missing-{}-{}",
            std::process::id(),
            now()
        ));
        let grants = Grants::load(&path).expect("missing file is not an error");
        assert!(grants.until.is_empty());
    }

    #[test]
    fn test_save_and_load_roundtrip_preserves_grants() {
        let path = std::env::temp_dir().join(format!(
            "antos-grants-test-roundtrip-{}-{}",
            std::process::id(),
            now()
        ));
        let _ = std::fs::remove_file(&path);

        let mut grants = Grants::default();
        grants.grant_with_reason("fs.delete", 5, Some("roundtrip".into()));
        grants.save(&path).expect("save");

        let loaded = Grants::load(&path).expect("load");
        assert!(loaded.is_granted("fs.delete"));
        assert_eq!(
            loaded
                .entries
                .get("fs.delete")
                .and_then(|e| e.reason.clone()),
            Some("roundtrip".to_string())
        );

        let _ = std::fs::remove_file(&path);
    }
}
