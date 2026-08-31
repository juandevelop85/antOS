//! Rutas del sistema: dónde está el espacio de trabajo, el estado y el catálogo.

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

pub struct Ctx {
    /// Único lugar donde las capacidades pueden tocar ficheros.
    pub workspace: PathBuf,
    /// Estado en ejecución: instantáneas, bitácora, concesiones.
    pub state: PathBuf,
    /// Directorio de manifiestos de capacidad.
    pub caps_dir: PathBuf,
    /// La configuración declarativa del sistema.
    ///
    /// Es la SEGUNDA raíz que syso reconoce, y no es un espacio de trabajo
    /// más: tocar aquí cambia la máquina entera. Por eso no cuenta como fuga
    /// —está declarada, tiene nombre— pero siempre exige concesión.
    pub system_config: PathBuf,
}

impl Ctx {
    pub fn discover() -> Result<Self> {
        let workspace = match std::env::var_os("SYSO_WORKSPACE") {
            Some(v) => PathBuf::from(v),
            None => PathBuf::from("workspace"),
        };
        std::fs::create_dir_all(&workspace)?;
        // Canonicalizar es imprescindible: la comprobación de contención
        // compara prefijos, y "workspace" y "/Users/.../workspace" no
        // comparten prefijo aunque sean el mismo sitio.
        let workspace = workspace.canonicalize()?;

        let state = match std::env::var_os("SYSO_STATE") {
            Some(v) => PathBuf::from(v),
            None => PathBuf::from(".syso"),
        };
        std::fs::create_dir_all(&state)?;
        let state = state.canonicalize()?;

        let caps_dir = match std::env::var_os("SYSO_CAPABILITIES") {
            Some(v) => PathBuf::from(v),
            None => ["system/capabilities", "capabilities", "../capabilities"]
                .iter()
                .map(PathBuf::from)
                .find(|p| p.is_dir())
                .ok_or_else(|| {
                    anyhow!("no encuentro el catálogo de capacidades; define SYSO_CAPABILITIES")
                })?,
        };

        let system_config = match std::env::var_os("SYSO_SYSTEM_CONFIG") {
            Some(v) => PathBuf::from(v),
            None => {
                let nixos = PathBuf::from("/etc/nixos");
                // Fuera de NixOS no hay configuración declarativa que
                // gobernar, así que se usa un sustituto dentro del estado y
                // se dice en pantalla. Fingir que hay una sería peor.
                if nixos.is_dir() { nixos } else { state.join("etc-nixos") }
            }
        };
        std::fs::create_dir_all(&system_config)?;
        let system_config = system_config.canonicalize()?;

        Ok(Ctx { workspace, state, caps_dir, system_config })
    }

    pub fn snapshots_dir(&self) -> PathBuf { self.state.join("snapshots") }
    pub fn journal_path(&self) -> PathBuf { self.state.join("journal.jsonl") }
    pub fn grants_path(&self) -> PathBuf { self.state.join("grants.json") }

    /// Muestra una ruta relativa al espacio de trabajo, para que la salida
    /// no esté llena de prefijos idénticos e ilegibles.
    pub fn display<'a>(&self, p: &'a Path) -> String {
        p.strip_prefix(&self.workspace)
            .map(|r| r.display().to_string())
            .unwrap_or_else(|_| p.display().to_string())
    }
}
