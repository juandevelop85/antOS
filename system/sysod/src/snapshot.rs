//! Instantáneas: lo que hace posible deshacer.
//!
//! M1 las hacía copiando byte a byte. M2 usa `clonefile(2)` de APFS: clones
//! copy-on-write que comparten bloques con el original hasta que uno de los
//! dos cambia. Es la misma idea que darían btrfs o ZFS en el destino Linux —
//! coste casi nulo en espacio y en tiempo, sin importar el tamaño del árbol.
//!
//! Si el clon no es posible (otro sistema de ficheros, otro volumen), se cae
//! a la copia de siempre. Fotografiar SIEMPRE tiene que funcionar: es lo
//! único que respalda el «deshacer».

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    pub original: PathBuf,
    /// Si no existía, deshacer significa borrarla, no restaurarla.
    pub existed: bool,
    pub stored: Option<PathBuf>,
    /// "clon" o "copia": queda registrado para poder decir la verdad sobre
    /// lo que respalda cada instantánea.
    #[serde(default)]
    pub method: String,
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
            let method = capture(original, &stored)
                .with_context(|| format!("fotografiando {}", original.display()))?;
            entries.push(Entry {
                original: original.clone(),
                existed: true,
                stored: Some(stored),
                method,
            });
        } else {
            entries.push(Entry {
                original: original.clone(),
                existed: false,
                stored: None,
                method: "inexistente".into(),
            });
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
                capture(stored, &entry.original)?;
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

/// Clona si el sistema de ficheros lo permite; si no, copia.
/// Devuelve cuál de las dos vías se usó.
fn capture(from: &Path, to: &Path) -> Result<String> {
    if clone_tree(from, to) {
        return Ok("clon".into());
    }
    copy_tree(from, to)?;
    Ok("copia".into())
}

/// `clonefile(2)`: clon copy-on-write, recursivo si el origen es un
/// directorio. Exige que el destino NO exista.
#[cfg(target_os = "macos")]
fn clone_tree(from: &Path, to: &Path) -> bool {
    use std::ffi::{c_char, c_int, c_uint, CString};
    use std::os::unix::ffi::OsStrExt;

    extern "C" {
        fn clonefile(src: *const c_char, dst: *const c_char, flags: c_uint) -> c_int;
    }

    let (Ok(src), Ok(dst)) = (
        CString::new(from.as_os_str().as_bytes()),
        CString::new(to.as_os_str().as_bytes()),
    ) else {
        return false;
    };
    // SAFETY: ambos punteros vienen de CString vivas durante la llamada.
    unsafe { clonefile(src.as_ptr(), dst.as_ptr(), 0) == 0 }
}

#[cfg(not(target_os = "macos"))]
fn clone_tree(_from: &Path, _to: &Path) -> bool {
    false
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
