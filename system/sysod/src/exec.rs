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
use std::path::{Path, PathBuf};

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

/// Lo que el plan ya ha decidido escribir, antes de haberlo escrito.
///
/// Sin esto, dos pasos que tocan el mismo fichero se pisan: cada uno lo lee
/// del disco tal y como estaba ANTES del plan, y al ejecutar gana el último.
/// Declarar tres dependencias dejaba una.
///
/// Y lo grave no era perder dos líneas: era que el diff aprobado dejaba de
/// describir el resultado. Que lo que ves sea lo que pasa es la propiedad
/// que sostiene todo lo demás.
#[derive(Default)]
pub struct Pendiente {
    escrituras: BTreeMap<PathBuf, String>,
    borrados: std::collections::BTreeSet<PathBuf>,
}

impl Pendiente {
    /// El contenido que tendrá el fichero cuando llegue este paso.
    /// `None` significa «pregúntale al disco».
    pub fn leer(&self, path: &Path) -> Option<String> {
        if self.borrados.contains(path) {
            return Some(String::new());
        }
        self.escrituras.get(path).cloned()
    }

    pub fn aplicar(&mut self, change: &Change) {
        match change {
            Change::Write { path, content } => {
                self.borrados.remove(path);
                self.escrituras.insert(path.clone(), content.clone());
            }
            Change::Delete { path } => {
                self.escrituras.remove(path);
                self.borrados.insert(path.clone());
            }
            Change::Mkdir { .. } | Change::Read { .. } => {}
        }
    }
}

/// Lee un fichero teniendo en cuenta lo que el plan ya ha decidido.
fn leer_con_pendiente(path: &Path, pendiente: &Pendiente) -> String {
    pendiente
        .leer(path)
        .unwrap_or_else(|| std::fs::read_to_string(path).unwrap_or_default())
}

pub fn changes_for(
    step: &Step,
    cap: &Capability,
    ctx: &Ctx,
    pendiente: &Pendiente,
) -> Result<Vec<Change>> {
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
            let previo = leer_con_pendiente(&path, pendiente);
            Ok(vec![Change::Write {
                content: declare_package(&previo, &a["package"], &a["version"])?,
                path,
            }])
        }

        "system.declare" => {
            let path = ctx.system_config.join("syso-paquetes.nix");
            let previo = leer_con_pendiente(&path, pendiente);
            Ok(vec![Change::Write {
                content: declare_system_package(&previo, &a["package"])?,
                path,
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

const NIX_HEADER: &str = "\
# Paquetes del sistema, declarados por syso.
#
# Esto NO instala nada: describe qué debe tener la máquina. Aplicarlo es un
# paso aparte, explícito y tuyo:
#
#     sudo nixos-rebuild switch
#
# Editarlo a mano es correcto: syso respeta lo que encuentre aquí.
{ pkgs, ... }:
{
  environment.systemPackages = with pkgs; [
";

const NIX_FOOTER: &str = "  ];\n}\n";

/// Devuelve el fichero Nix COMPLETO tras añadir el paquete.
///
/// Se lee lo que hay y se vuelve a escribir entero, en vez de aplicar un
/// parche. Es lo que permite fotografiarlo, previsualizarlo y revertirlo con
/// el mismo código que cualquier otro fichero.
fn declare_system_package(previo: &str, package: &str) -> Result<String> {
    let mut packages: BTreeMap<String, ()> = BTreeMap::new();

    {
        let existing = previo;
        // Un análisis por líneas basta porque este fichero lo genera syso.
        // Si alguien lo reescribe con Nix de verdad, lo peor que pasa es que
        // no reconozcamos sus paquetes — y eso se ve en el diff antes de
        // aprobar nada.
        let mut inside = false;
        for line in existing.lines() {
            let trimmed = line.trim();
            if trimmed.ends_with('[') {
                inside = true;
                continue;
            }
            if trimmed.starts_with(']') {
                inside = false;
                continue;
            }
            if inside && !trimmed.is_empty() && !trimmed.starts_with('#') {
                packages.insert(trimmed.to_string(), ());
            }
        }
    }
    packages.insert(package.to_string(), ());

    let cuerpo: String = packages
        .keys()
        .map(|name| format!("    {name}\n"))
        .collect();
    Ok(format!("{NIX_HEADER}{cuerpo}{NIX_FOOTER}"))
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
fn declare_package(previo: &str, package: &str, version: &str) -> Result<String> {
    let mut file: PackagesFile = toml::from_str(previo).unwrap_or_default();
    file.packages.insert(package.to_string(), version.to_string());
    Ok(format!("{PACKAGES_HEADER}\n{}", toml::to_string(&file)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_paso_ve_lo_que_decidio_el_anterior() {
        let mut pendiente = Pendiente::default();
        let ruta = PathBuf::from("/ws/paquetes.toml");

        assert_eq!(pendiente.leer(&ruta), None, "de partida, manda el disco");

        pendiente.aplicar(&Change::Write {
            path: ruta.clone(),
            content: "express".into(),
        });
        assert_eq!(
            pendiente.leer(&ruta).as_deref(),
            Some("express"),
            "el paso siguiente debe ver lo que este escribió, no el disco"
        );
    }

    #[test]
    fn escribir_despues_de_borrar_parte_de_cero() {
        let mut pendiente = Pendiente::default();
        let ruta = PathBuf::from("/ws/notas.txt");

        pendiente.aplicar(&Change::Delete { path: ruta.clone() });
        assert_eq!(
            pendiente.leer(&ruta).as_deref(),
            Some(""),
            "un fichero borrado por un paso anterior está vacío, no como en el disco"
        );

        pendiente.aplicar(&Change::Write {
            path: ruta.clone(),
            content: "nuevo".into(),
        });
        assert_eq!(pendiente.leer(&ruta).as_deref(), Some("nuevo"));
    }
}
