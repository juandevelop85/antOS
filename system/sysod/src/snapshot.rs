//! Instantáneas: lo que hace posible deshacer.
//!
//! En M1 una instantánea es una copia de las rutas afectadas. Es correcto
//! para efectos acotados a ficheros y no necesita nada del sistema, pero no
//! escala: M2 lo sustituye por instantáneas reales del sistema de ficheros.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    pub original: PathBuf,
    /// Si no existía, deshacer significa borrarla, no restaurarla.
    pub existed: bool,
    pub stored: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub entries: Vec<Entry>,
}

pub fn take(id: &str, paths: &[PathBuf], snapshots_dir: &Path) -> Result<Snapshot> {
    let dir = snapshots_dir.join(id);
    std::fs::create_dir_all(&dir)?;

    let mut entries = Vec::new();
    for (i, original) in paths.iter().enumerate() {
        if original.exists() {
            let stored = dir.join(format!("{i:03}"));
            copy_tree(original, &stored)
                .with_context(|| format!("fotografiando {}", original.display()))?;
            entries.push(Entry { original: original.clone(), existed: true, stored: Some(stored) });
        } else {
            entries.push(Entry { original: original.clone(), existed: false, stored: None });
        }
    }

    let snap = Snapshot { id: id.to_string(), entries };
    std::fs::write(dir.join("manifest.json"), serde_json::to_vec_pretty(&snap)?)?;
    Ok(snap)
}

pub fn load(id: &str, snapshots_dir: &Path) -> Result<Snapshot> {
    let path = snapshots_dir.join(id).join("manifest.json");
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("no encuentro la instantánea {id}"))?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn restore(snap: &Snapshot) -> Result<Vec<String>> {
    let mut done = Vec::new();
    for entry in &snap.entries {
        remove_any(&entry.original)?;
        match (&entry.stored, entry.existed) {
            (Some(stored), true) => {
                if let Some(parent) = entry.original.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                copy_tree(stored, &entry.original)?;
                done.push(format!("restaurado  {}", entry.original.display()));
            }
            _ => {
                // No existía antes del plan: deshacer es dejarlo sin existir.
                done.push(format!("eliminado   {}", entry.original.display()));
            }
        }
    }
    Ok(done)
}

fn remove_any(p: &Path) -> Result<()> {
    if p.is_dir() {
        std::fs::remove_dir_all(p)?;
    } else if p.exists() {
        std::fs::remove_file(p)?;
    }
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(from, to)?;
    }
    Ok(())
}
