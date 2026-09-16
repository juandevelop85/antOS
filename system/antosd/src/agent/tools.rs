//! Herramientas de un agente = capacidades del catálogo (T33.2).
//!
//! No hay una segunda descripción que mantener: el esquema JSON de cada
//! herramienta sale del manifiesto `system/capabilities/<nombre>.toml`, y
//! los argumentos que devuelve el modelo se normalizan a cadenas para pasar
//! por el mismo `Catalog::validate` que una intención. La única herramienta
//! que no es una capacidad es `finalizar`, la terminal: el modelo la llama
//! para dar la tarea por hecha con un resumen.

use crate::capability::{Capability, Catalog};
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// Nombre de la herramienta terminal.
pub const FINISH_TOOL: &str = "finalizar";

/// Toolset por defecto: leer, orientarse, editar, probar y buscar.
pub const DEFAULT_TOOLSET: &[&str] = &[
    "fs.read",
    "fs.list",
    "fs.patch",
    "fs.write",
    "test.run",
    "git.status",
    "memory.search",
];

/// Una herramienta tal como la ve el proveedor (esquema neutro; cada
/// proveedor lo envuelve en su formato).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema del objeto de entrada.
    pub input_schema: Value,
}

/// Construye el toolset: cada nombre tiene que existir en el catálogo. Un
/// nombre desconocido es un error de configuración, no algo que se ignora.
pub fn build_toolset(catalog: &Catalog, names: &[String]) -> Result<Vec<ToolSpec>> {
    let mut specs = Vec::with_capacity(names.len() + 1);
    for name in names {
        if name == FINISH_TOOL {
            continue;
        }
        let cap = catalog.get(name)?;
        specs.push(spec_for(cap));
    }
    specs.push(finish_spec());
    Ok(specs)
}

fn spec_for(cap: &Capability) -> ToolSpec {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for (pname, pspec) in &cap.params {
        let mut prop = Map::new();
        let json_type = match pspec.kind.as_str() {
            "boolean" | "bool" => "boolean",
            "int" | "integer" | "number" | "port" => "integer",
            _ => "string",
        };
        prop.insert("type".into(), json!(json_type));
        if !pspec.of.is_empty() {
            prop.insert("enum".into(), json!(pspec.of));
        }
        let mut desc = pspec.kind.clone();
        if let Some(w) = &pspec.within {
            desc.push_str(&format!(" dentro de {w}"));
        }
        if let Some(m) = pspec.max {
            desc.push_str(&format!(", máximo {m}"));
        }
        if let Some(d) = &pspec.default {
            desc.push_str(&format!(", por defecto {d}"));
        }
        prop.insert("description".into(), json!(desc));
        properties.insert(pname.clone(), Value::Object(prop));
        if !pspec.optional && pspec.default.is_none() {
            required.push(pname.clone());
        }
    }
    ToolSpec {
        name: cap.name.clone(),
        description: cap.summary.clone(),
        input_schema: json!({
            "type": "object",
            "properties": properties,
            "required": required,
        }),
    }
}

fn finish_spec() -> ToolSpec {
    ToolSpec {
        name: FINISH_TOOL.into(),
        description: "Da la tarea por terminada. Llámala cuando el objetivo esté cumplido \
                      (o cuando no puedas cumplirlo, explicando por qué)."
            .into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "resumen": {
                    "type": "string",
                    "description": "Qué se hizo, qué se verificó y qué queda pendiente."
                }
            },
            "required": ["resumen"]
        }),
    }
}

/// Normaliza el `input` de un `tool_use` a los argumentos de cadena que
/// valida el catálogo. Los valores no escalares se serializan como JSON; el
/// catálogo los rechazará si no encajan, y ese rechazo vuelve al modelo.
pub fn args_from_input(input: &Value) -> Result<BTreeMap<String, String>> {
    let Some(obj) = input.as_object() else {
        bail!("los argumentos de la herramienta deben ser un objeto JSON");
    };
    let mut args = BTreeMap::new();
    for (k, v) in obj {
        let s = match v {
            Value::String(s) => s.clone(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::Null => continue,
            other => other.to_string(),
        };
        args.insert(k.clone(), s);
    }
    Ok(args)
}

/// Resumen de una línea de los argumentos, para la barra y el journal.
pub fn summarize_args(args: &BTreeMap<String, String>) -> String {
    const MAX: usize = 60;
    args.iter()
        .map(|(k, v)| {
            let flat = v.replace('\n', "⏎");
            if flat.chars().count() > MAX {
                let cut: String = flat.chars().take(MAX).collect();
                format!("{k}={cut}…")
            } else {
                format!("{k}={flat}")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn args_normalize_scalars_and_serialize_the_rest() {
        let input =
            json!({"path": "src/a.rs", "limit": 5, "fast": true, "list": [1,2], "nada": null});
        let args = args_from_input(&input).unwrap();
        assert_eq!(args["path"], "src/a.rs");
        assert_eq!(args["limit"], "5");
        assert_eq!(args["fast"], "true");
        assert_eq!(args["list"], "[1,2]");
        assert!(!args.contains_key("nada"));
        assert!(args_from_input(&json!("no-objeto")).is_err());
    }

    #[test]
    fn summary_truncates_long_values() {
        let mut args = BTreeMap::new();
        args.insert("content".to_string(), "x".repeat(200));
        let s = summarize_args(&args);
        assert!(s.ends_with('…'));
        assert!(s.chars().count() < 80);
    }
}
