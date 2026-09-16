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

    let snap = Snapshot {
        id: id.to_string(),
        entries,
    };
    std::fs::write(dir.join("manifest.json"), serde_json::to_vec_pretty(&snap)?)?;
    Ok(snap)
}

/// Amplía una instantánea existente con rutas nuevas (T33.2). Un run de
/// agente no sabe de antemano qué va a escribir: cada paso que escribe una
/// ruta todavía no cubierta la fotografía ANTES de tocarla, y el manifiesto
/// se reescribe. Las rutas ya cubiertas no se vuelven a capturar: lo que
/// interesa es el estado previo al run, no al paso.
pub fn extend(snap: &mut Snapshot, paths: &[PathBuf], snapshots_dir: &Path) -> Result<()> {
    let dir = snapshots_dir.join(&snap.id);
    std::fs::create_dir_all(&dir)?;
    for original in paths {
        if snap.entries.iter().any(|e| &e.original == original) {
            continue;
        }
        let i = snap.entries.len();
        if original.exists() {
            let stored = dir.join(format!("{i:03}"));
            let method = capture(original, &stored)
                .with_context(|| format!("fotografiando {}", original.display()))?;
            snap.entries.push(Entry {
                original: original.clone(),
                existed: true,
                stored: Some(stored),
                method,
            });
        } else {
            snap.entries.push(Entry {
                original: original.clone(),
                existed: false,
                stored: None,
                method: "inexistente".into(),
            });
        }
    }
    std::fs::write(dir.join("manifest.json"), serde_json::to_vec_pretty(&snap)?)?;
    Ok(())
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
    // Si un intento de clon falló a mitad y dejó entradas parciales en el destino,
    // limpiamos antes de proceder con copia recursiva estándar.
    if to.exists() {
        let _ = remove_any(to);
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

/// Clonado copy-on-write para Linux mediante ioctl(..., FICLONE, ...).
/// Soporta ficheros individuales directamente y árboles de directorios
/// recursivamente preservando la jerarquía.
#[cfg(target_os = "linux")]
fn clone_tree(from: &Path, to: &Path) -> bool {
    if from.is_file() {
        if let Some(parent) = to.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                return false;
            }
        }
        return clone_file_linux(from, to);
    }

    if from.is_dir() {
        if std::fs::create_dir_all(to).is_err() {
            return false;
        }
        let Ok(entries) = std::fs::read_dir(from) else {
            return false;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                return false;
            };
            let sub_from = entry.path();
            let sub_to = to.join(entry.file_name());
            if !clone_tree(&sub_from, &sub_to) {
                return false;
            }
        }
        return true;
    }

    false
}

/// Clona un archivo regular en Linux utilizando FICLONE (ioctl reflink de btrfs/xfs/zfs).
#[cfg(target_os = "linux")]
fn clone_file_linux(from: &Path, to: &Path) -> bool {
    use std::fs::File;
    use std::os::unix::io::AsRawFd;

    let Ok(src_file) = File::open(from) else {
        return false;
    };

    let Ok(dst_file) = File::options().write(true).create_new(true).open(to) else {
        return false;
    };

    // FICLONE ioctl: _IOW(0x94, 9, int) = 0x40049409
    const FICLONE: libc::c_ulong = 0x40049409;
    let ret = unsafe { libc::ioctl(dst_file.as_raw_fd(), FICLONE as _, src_file.as_raw_fd()) };
    if ret == 0 {
        true
    } else {
        let _ = std::fs::remove_file(to);
        false
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_snapshot_take_and_restore() {
        let temp_dir = std::env::temp_dir().join(format!("test_snap_{}", std::process::id()));
        let ws = temp_dir.join("workspace");
        let snaps = temp_dir.join("snaps");
        let _ = std::fs::create_dir_all(&ws);
        let _ = std::fs::create_dir_all(&snaps);

        let file_a = ws.join("file_a.txt");
        let sub_dir = ws.join("sub");
        let _ = std::fs::create_dir_all(&sub_dir);
        let file_b = sub_dir.join("file_b.txt");

        std::fs::write(&file_a, "Contenido A inicial").unwrap();
        std::fs::write(&file_b, "Contenido B inicial").unwrap();

        let paths = vec![file_a.clone(), sub_dir.clone()];
        let snap = take("snap_01", &paths, &snaps).expect("take snapshot");
        assert_eq!(snap.entries.len(), 2);
        assert!(snap.entries.iter().all(|e| e.existed));
        assert!(snap
            .entries
            .iter()
            .all(|e| e.method == "clon" || e.method == "copia"));

        // Modificar o eliminar archivos en workspace
        std::fs::write(&file_a, "Contenido A modificado").unwrap();
        std::fs::remove_file(&file_b).unwrap();
        let file_c = sub_dir.join("file_c_nuevo.txt");
        std::fs::write(&file_c, "Archivo nuevo no deseado").unwrap();

        // Restaurar instantánea
        let loaded = load("snap_01", &snaps).expect("load snapshot");
        let done = restore(&loaded).expect("restore snapshot");
        assert_eq!(done.len(), 2);

        // Verificar que los contenidos volvieron al estado original
        assert_eq!(
            std::fs::read_to_string(&file_a).unwrap(),
            "Contenido A inicial"
        );
        assert_eq!(
            std::fs::read_to_string(&file_b).unwrap(),
            "Contenido B inicial"
        );
        assert!(!file_c.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_snapshot_non_existent_path_removal_on_restore() {
        let temp_dir =
            std::env::temp_dir().join(format!("test_snap_nonexist_{}", std::process::id()));
        let ws = temp_dir.join("workspace");
        let snaps = temp_dir.join("snaps");
        let _ = std::fs::create_dir_all(&ws);
        let _ = std::fs::create_dir_all(&snaps);

        let file_ghost = ws.join("ghost.txt");
        let paths = vec![file_ghost.clone()];
        let snap = take("snap_ghost", &paths, &snaps).expect("take snapshot");
        assert!(!snap.entries[0].existed);
        assert_eq!(snap.entries[0].method, "inexistente");

        // Creamos el archivo después de la instantánea
        std::fs::write(&file_ghost, "Aparecí después").unwrap();
        assert!(file_ghost.exists());

        // Al restaurar, debe desaparecer
        restore(&snap).expect("restore");
        assert!(!file_ghost.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
