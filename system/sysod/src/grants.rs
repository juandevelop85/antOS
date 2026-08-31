//! Concesiones: permisos explícitos, acotados y con caducidad para las
//! capacidades de nivel «concesión».
//!
//! La caducidad no es un adorno. Un permiso permanente se concede una vez y
//! se olvida, que es como la autoridad ambiental de Unix vuelve por la puerta
//! de atrás.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Grants {
    /// capacidad -> instante de caducidad (epoch en segundos)
    pub until: BTreeMap<String, i64>,
}

impl Grants {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Grants::default());
        }
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn is_granted(&self, cap: &str) -> bool {
        self.until.get(cap).map(|&t| t > now()).unwrap_or(false)
    }

    pub fn grant(&mut self, cap: &str, minutes: i64) {
        self.until.insert(cap.to_string(), now() + minutes * 60);
    }

    pub fn revoke(&mut self, cap: &str) {
        self.until.remove(cap);
    }

}

fn now() -> i64 {
    chrono::Local::now().timestamp()
}
