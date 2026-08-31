//! Planificador local determinista: reglas de palabras clave, sin modelo.
//!
//! Existe por dos razones. Una, poder ejercitar el recorrido completo sin
//! clave de API ni latencia de red. Dos, y más importante: obliga a que el
//! resto del sistema no dependa de que el planificador sea listo. Si el
//! aislamiento y el deshacer solo funcionan cuando el modelo acierta, no
//! funcionan.

use super::Planner;
use crate::capability::Catalog;
use crate::plan::Step;
use anyhow::{bail, Result};
use std::collections::BTreeMap;

pub struct LocalPlanner;

impl Planner for LocalPlanner {
    fn name(&self) -> &'static str {
        "local"
    }

    fn plan(&self, intent: &str, _catalog: &Catalog) -> Result<Vec<Step>> {
        let lower = intent.to_lowercase();
        // El punto se conserva porque forma parte de nombres de fichero
        // (`main.rs`), pero un punto FINAL es puntuación de frase. Sin
        // quitarlo, dictar "crea un proyecto llamado demo." crea un
        // directorio que se llama literalmente `demo.`.
        let words: Vec<String> = lower
            .split_whitespace()
            .map(|w| {
                w.trim_matches(|c: char| {
                    !c.is_alphanumeric() && c != '.' && c != '/' && c != '-' && c != '_'
                })
                .trim_end_matches('.')
                .to_string()
            })
            .collect();

        if lower.contains("proyecto") && (lower.contains("cre") || lower.contains("nuev")) {
            let language = if lower.contains("typescript") || lower.contains(" ts ") {
                "typescript"
            } else if lower.contains("python") || lower.contains(" py ") {
                "python"
            } else {
                "rust"
            };
            let name = after(&words, &["llamado", "llamada", "nombre"])
                .unwrap_or_else(|| words.last().cloned().unwrap_or_default());
            return Ok(vec![step("project.scaffold", &[("language", language), ("name", &name)])]);
        }

        // El sistema se comprueba ANTES que el proyecto: "declara htop en el
        // sistema" y "declara serde en el proyecto demo" empiezan igual.
        if lower.contains("sistema")
            && (lower.contains("declar")
                || lower.contains("paquete")
                || lower.contains("instal")
                || lower.contains("añad"))
        {
            let package = after(&words, &["declara", "instala", "añade", "paquete"])
                .ok_or_else(|| anyhow::anyhow!("no veo qué paquete quieres declarar"))?;
            return Ok(vec![step("system.declare", &[("package", &package)])]);
        }

        if lower.contains("depend") || lower.contains("paquete") {
            let package = after(&words, &["dependencia", "dependencias", "paquete"])
                .ok_or_else(|| anyhow::anyhow!("no veo qué paquete quieres declarar"))?;
            let project = after(&words, &["proyecto"])
                .ok_or_else(|| anyhow::anyhow!("no veo en qué proyecto declararlo"))?;
            let version = after(&words, &["version", "versión", "v"]).unwrap_or_else(|| "*".into());
            return Ok(vec![step(
                "pkg.declare",
                &[("project", &project), ("package", &package), ("version", &version)],
            )]);
        }

        if lower.contains("borra") || lower.contains("elimin") {
            let path = words.last().cloned().unwrap_or_default();
            return Ok(vec![step("fs.delete", &[("path", &path)])]);
        }

        if lower.contains("lee") || lower.contains("muestra") || lower.contains("enseña") {
            let path = words.last().cloned().unwrap_or_default();
            return Ok(vec![step("fs.read", &[("path", &path)])]);
        }

        if lower.contains("escribe") {
            let path = after(&words, &["en", "fichero", "archivo"])
                .ok_or_else(|| anyhow::anyhow!("no veo en qué fichero escribir"))?;
            let content = intent.split_once(':').map(|(_, c)| c.trim().to_string()).unwrap_or_default();

            return Ok(vec![step("fs.write", &[("path", &path), ("content", &content)])]);
        }

        bail!(
            "el planificador local no sabe traducir esa intención.\n\
             Entiende: crear proyectos, declarar dependencias, leer, escribir y borrar.\n\
             Para lenguaje libre usa: syso --planificador claude \"…\""
        )
    }
}

/// Devuelve la palabra siguiente a la primera de `keys` que aparezca.
fn after(words: &[String], keys: &[&str]) -> Option<String> {
    for (i, w) in words.iter().enumerate() {
        if keys.contains(&w.as_str()) {
            if let Some(next) = words.get(i + 1) {
                if !next.is_empty() {
                    return Some(next.clone());
                }
            }
        }
    }
    None
}

fn step(capability: &str, args: &[(&str, &str)]) -> Step {
    Step {
        capability: capability.to_string(),
        args: args
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<BTreeMap<_, _>>(),
    }
}
