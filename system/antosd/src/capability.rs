//! El catálogo de capacidades.
//!
//! Una capacidad es un contrato: qué parámetros acepta, qué toca y con qué
//! nivel de permiso. Es a la vez la herramienta que ve el modelo, la unidad
//! de permiso y la entrada de la bitácora.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub use antos_protocol::Tier;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Reversible {
    /// Se puede deshacer restaurando una instantánea previa.
    Snapshot,
    /// No modifica nada, así que no hay nada que deshacer.
    Unnecessary,
    /// Irreversible: ninguna instantánea la cubre.
    Never,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParamSpec {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub of: Vec<String>,
    #[serde(default)]
    pub within: Option<String>,
    #[serde(default)]
    pub max: Option<usize>,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Effects {
    #[serde(default)]
    pub reads: Vec<String>,
    #[serde(default)]
    pub writes: Vec<String>,
    #[serde(default)]
    pub deletes: Vec<String>,
    #[serde(default)]
    pub network: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Policy {
    pub tier: Tier,
    pub reversible: Reversible,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Capability {
    pub name: String,
    pub summary: String,
    #[serde(default)]
    pub params: BTreeMap<String, ParamSpec>,
    #[serde(default)]
    pub effects: Effects,
    pub policy: Policy,
}

pub struct Catalog {
    pub caps: BTreeMap<String, Capability>,
}

impl Catalog {
    pub fn load(dir: &Path) -> Result<Self> {
        let mut caps = BTreeMap::new();
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .with_context(|| format!("leyendo el catálogo en {}", dir.display()))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|e| e == "toml").unwrap_or(false))
            .collect();
        entries.sort();

        for path in entries {
            let raw = std::fs::read_to_string(&path)?;
            let cap: Capability = toml::from_str(&raw)
                .with_context(|| format!("manifiesto inválido: {}", path.display()))?;
            if caps.contains_key(&cap.name) {
                bail!("capacidad duplicada en el catálogo: {}", cap.name);
            }
            caps.insert(cap.name.clone(), cap);
        }
        if caps.is_empty() {
            bail!("el catálogo de {} está vacío", dir.display());
        }
        Ok(Catalog { caps })
    }

    pub fn get(&self, name: &str) -> Result<&Capability> {
        self.caps
            .get(name)
            .ok_or_else(|| anyhow!("capacidad desconocida: {name}"))
    }

    /// Rellena los parámetros opcionales que falten y rechaza cualquier
    /// argumento que no cumpla el contrato declarado.
    ///
    /// Esta validación es la primera línea de defensa: lo que el modelo
    /// devuelve no se considera de fiar hasta pasar por aquí.
    pub fn validate(&self, cap: &Capability, args: &mut BTreeMap<String, String>) -> Result<()> {
        for (name, spec) in &cap.params {
            if !args.contains_key(name) {
                match (&spec.default, spec.optional) {
                    (Some(d), _) => {
                        args.insert(name.clone(), d.clone());
                    }
                    (None, true) => continue,
                    (None, false) => bail!("{}: falta el parámetro «{name}»", cap.name),
                }
            }
            let value = &args[name];

            if let Some(max) = spec.max {
                if value.chars().count() > max {
                    bail!(
                        "{}: «{name}» excede el máximo de {max} caracteres",
                        cap.name
                    );
                }
            }

            match spec.kind.as_str() {
                "enum" => {
                    if !spec.of.iter().any(|o| o == value) {
                        bail!(
                            "{}: «{name}» debe ser uno de [{}], no «{value}»",
                            cap.name,
                            spec.of.join(", ")
                        );
                    }
                }
                "slug" => {
                    if value.is_empty()
                        || !value
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
                    {
                        bail!(
                            "{}: «{name}» no es un identificador válido: «{value}»",
                            cap.name
                        );
                    }
                }
                "branch" | "git_ref" => {
                    let valido =
                        |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/');
                    if value.is_empty()
                        || !value.chars().all(valido)
                        || value.starts_with('/')
                        || value.ends_with('/')
                        || value.contains("..")
                        || value.contains("//")
                    {
                        bail!(
                            "{}: «{name}» no es un nombre de rama válido: «{value}»",
                            cap.name
                        );
                    }
                }
                // Un nombre de paquete de un ecosistema real: npm admite
                // ámbitos (`@types/express`), y otros admiten `+`. Es un tipo
                // aparte de `slug` a propósito — `slug` lo usan parámetros que
                // acaban dentro de una RUTA, y ahí una barra sería otra cosa.
                "package" => {
                    let valido = |c: char| {
                        c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@' | '/' | '+')
                    };
                    if value.is_empty() || !value.chars().all(valido) {
                        bail!(
                            "{}: «{name}» no es un nombre de paquete válido: «{value}»",
                            cap.name
                        );
                    }
                    if value.starts_with('/') || value.starts_with('-') || value.contains("..") {
                        bail!(
                            "{}: «{name}» tiene una forma sospechosa: «{value}»",
                            cap.name
                        );
                    }
                }
                "path" => {
                    if value.is_empty() {
                        bail!("{}: «{name}» está vacío", cap.name);
                    }
                    // `within` se comprueba aquí para fallar pronto y con un
                    // mensaje claro. El radio de impacto vuelve a comprobar la
                    // contención de forma definitiva: esto es conveniencia,
                    // no la garantía.
                    if spec.within.as_deref() == Some("$WORKSPACE") {
                        if value.starts_with('/') || value.starts_with('~') {
                            bail!(
                                "{}: «{name}» debe ser relativa al espacio de trabajo: «{value}»",
                                cap.name
                            );
                        }
                        if value.split('/').any(|c| c == "..") {
                            bail!("{}: «{name}» no puede subir por encima del espacio de trabajo: «{value}»", cap.name);
                        }
                    }
                }
                _ => {}
            }
        }

        for name in args.keys() {
            if !cap.params.contains_key(name) {
                bail!("{}: parámetro no declarado «{name}»", cap.name);
            }
        }
        Ok(())
    }
}
