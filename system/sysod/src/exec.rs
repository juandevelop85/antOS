//! Traduce pasos en cambios concretos, y aplica esos cambios.
//!
//! `changes_for` es la única fuente de verdad: la previsualización y la
//! ejecución llaman a la MISMA función. Si fueran dos caminos distintos, el
//! diff que apruebas y lo que ocurre podrían divergir — y ese es justo el
//! fallo que hace inaceptable un sistema gobernado por IA.

use crate::blast::expand;
use crate::capability::Capability;
use crate::ctx::Ctx;
use crate::plan::Step;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Serializable porque cruza la frontera de proceso: el broker decide los
/// cambios, el ejecutor confinado los aplica.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "lowercase")]
pub enum Change {
    Write { path: PathBuf, content: String },
    Mkdir { path: PathBuf },
    Delete { path: PathBuf },
    Read { path: PathBuf },
}

pub fn changes_for(step: &Step, cap: &Capability, ctx: &Ctx) -> Result<Vec<Change>> {
    let a = &step.args;
    match cap.name.as_str() {
        "fs.read" => Ok(vec![Change::Read { path: abs(ctx, &a["path"]) }]),

        "fs.write" => Ok(vec![Change::Write {
            path: abs(ctx, &a["path"]),
            content: a["content"].clone(),
        }]),

        "fs.delete" => Ok(vec![Change::Delete { path: abs(ctx, &a["path"]) }]),

        "fs.mkdir" => Ok(vec![Change::Mkdir { path: abs(ctx, &a["path"]) }]),

        "project.scaffold" => {
            let root = ctx.workspace.join(&a["name"]);
            Ok(scaffold(&a["language"], &a["name"])
                .into_iter()
                .map(|(rel, content)| Change::Write { path: root.join(rel), content })
                .collect())
        }

        "pkg.declare" => {
            let path = ctx
                .workspace
                .join(&a["project"])
                .join("syso.packages.toml");
            Ok(vec![Change::Write {
                path: path.clone(),
                content: declare_package(&path, &a["package"], &a["version"])?,
            }])
        }

        other => bail!("no hay implementación para la capacidad «{other}»"),
    }
}

pub fn apply(changes: &[Change]) -> Result<Vec<String>> {
    let mut output = Vec::new();
    for change in changes {
        match change {
            Change::Write { path, content } => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("creando el directorio {}", parent.display()))?;
                }
                std::fs::write(path, content)
                    .with_context(|| format!("escribiendo {}", path.display()))?;
            }
            Change::Mkdir { path } => {
                std::fs::create_dir_all(path)
                    .with_context(|| format!("creando el directorio {}", path.display()))?;
            }
            Change::Delete { path } => {
                if path.is_dir() {
                    std::fs::remove_dir_all(path)?;
                } else if path.exists() {
                    std::fs::remove_file(path)?;
                } else {
                    bail!("no existe: {}", path.display());
                }
            }
            Change::Read { path } => {
                output.push(std::fs::read_to_string(path)?);
            }
        }
    }
    Ok(output)
}

fn abs(ctx: &Ctx, raw: &str) -> PathBuf {
    let expanded = expand(raw, &BTreeMap::new(), &ctx.workspace);
    let p = PathBuf::from(expanded);
    if p.is_absolute() { p } else { ctx.workspace.join(p) }
}

/// Los ficheros que produce un proyecto nuevo, por lenguaje.
fn scaffold(language: &str, name: &str) -> Vec<(&'static str, String)> {
    match language {
        "rust" => vec![
            ("Cargo.toml", format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n"
            )),
            ("src/main.rs", format!(
                "fn main() {{\n    println!(\"{name} en marcha\");\n}}\n"
            )),
        ],
        "typescript" => vec![
            ("package.json", format!(
                "{{\n  \"name\": \"{name}\",\n  \"version\": \"0.1.0\",\n  \"type\": \"module\",\n  \"scripts\": {{\n    \"start\": \"node --experimental-strip-types src/index.ts\"\n  }}\n}}\n"
            )),
            ("tsconfig.json",
                "{\n  \"compilerOptions\": {\n    \"target\": \"es2022\",\n    \"module\": \"esnext\",\n    \"moduleResolution\": \"bundler\",\n    \"strict\": true\n  }\n}\n".to_string()),
            ("src/index.ts", format!("console.log(\"{name} en marcha\");\n")),
        ],
        "python" => vec![
            ("pyproject.toml", format!(
                "[project]\nname = \"{name}\"\nversion = \"0.1.0\"\nrequires-python = \">=3.11\"\ndependencies = []\n"
            )),
            ("main.py", format!("def main() -> None:\n    print(\"{name} en marcha\")\n\n\nif __name__ == \"__main__\":\n    main()\n")),
        ],
        _ => Vec::new(),
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PackagesFile {
    #[serde(default)]
    packages: BTreeMap<String, String>,
}

const PACKAGES_HEADER: &str = "\
# Dependencias declaradas por syso.
#
# Declarar no es instalar: este fichero dice qué necesita el proyecto, y la
# instalación es un paso aparte y explícito. Así lo que apruebas es un diff
# legible, y deshacerlo es volver a la declaración anterior.
";

/// Devuelve el contenido COMPLETO que tendría el fichero de declaraciones
/// tras añadir el paquete. Devolver el fichero entero (y no un parche) es lo
/// que permite fotografiarlo y previsualizarlo con el mismo código.
fn declare_package(path: &PathBuf, package: &str, version: &str) -> Result<String> {
    let mut file: PackagesFile = if path.exists() {
        toml::from_str(&std::fs::read_to_string(path)?).unwrap_or_default()
    } else {
        PackagesFile::default()
    };
    file.packages.insert(package.to_string(), version.to_string());
    Ok(format!("{PACKAGES_HEADER}\n{}", toml::to_string(&file)?))
}
