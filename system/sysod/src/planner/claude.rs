//! Planificador con Claude, por HTTP directo contra la Messages API.
//!
//! Rust no tiene SDK oficial de Anthropic, así que hablamos HTTP a mano.
//!
//! El catálogo se traduce a definiciones de herramienta: cada capacidad es
//! una herramienta con su esquema. El modelo no «escribe comandos», elige
//! entre capacidades declaradas y rellena parámetros tipados — y aun así su
//! salida vuelve a validarse contra el catálogo antes de usarse.

use super::Planner;
use crate::capability::Catalog;
use crate::plan::Step;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

const URL: &str = "https://api.anthropic.com/v1/messages";
const DEFAULT_MODEL: &str = "claude-opus-5";
pub const CLAVE_ENV: &str = "ANTHROPIC_API_KEY";

pub struct ClaudePlanner {
    api_key: String,
    model: String,
}

impl ClaudePlanner {
    pub fn from_env() -> Result<Self> {
        let api_key = leer_clave()?;
        let model = std::env::var("SYSO_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
        Ok(Self { api_key, model })
    }
}

/// Dónde vive la clave por defecto.
fn ruta_clave() -> PathBuf {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(".config/syso/anthropic.key")
}

/// Lee la clave de un fichero, y solo como último recurso del entorno.
///
/// El orden importa. Una variable de entorno la hereda TODO proceso hijo, y
/// este programa lanza varios: whisper, ffmpeg, y sobre todo el ejecutor
/// confinado — que es precisamente el componente que el diseño entero trata
/// como no fiable. Un fichero que solo lee el broker no viaja a ninguna parte.
fn leer_clave() -> Result<String> {
    let ruta = match std::env::var_os("ANTHROPIC_API_KEY_FILE") {
        Some(p) => PathBuf::from(p),
        None => ruta_clave(),
    };

    if ruta.exists() {
        let clave = std::fs::read_to_string(&ruta)
            .with_context(|| format!("no pude leer {}", ruta.display()))?;
        avisar_si_es_legible_por_otros(&ruta);
        let clave = clave.trim().to_string();
        if !clave.is_empty() {
            return Ok(clave);
        }
    }

    // El entorno sigue funcionando por comodidad, pero se avisa.
    if let Ok(clave) = std::env::var(CLAVE_ENV) {
        if !clave.trim().is_empty() {
            eprintln!(
                "aviso: usando {CLAVE_ENV} del entorno. Todo proceso hijo la hereda; \n\
                 es preferible {}",
                ruta_clave().display()
            );
            return Ok(clave.trim().to_string());
        }
    }

    bail!(
        "no encuentro la clave de la API.\n\
         Ponla en {} (solo lectura para ti):\n\
         \n  mkdir -p ~/.config/syso && chmod 700 ~/.config/syso\n\
         \n  read -rs CLAVE && printf '%s' \"$CLAVE\" > {} && unset CLAVE\n\
         \n  chmod 600 {}\n\
         \nO usa el planificador local, que no necesita clave:\n\
         \n  syso --planificador local \"…\"",
        ruta.display(),
        ruta.display(),
        ruta.display()
    )
}

fn avisar_si_es_legible_por_otros(ruta: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(ruta) {
        let modo = meta.permissions().mode() & 0o077;
        if modo != 0 {
            eprintln!(
                "aviso: {} es legible por otros usuarios. Arréglalo con:\n  chmod 600 {}",
                ruta.display(),
                ruta.display()
            );
        }
    }
}

impl Planner for ClaudePlanner {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn plan(&self, intent: &str, catalog: &Catalog) -> Result<Vec<Step>> {
        let body = json!({
            "model": self.model,
            "max_tokens": 16000,
            "system": SYSTEM,
            "tools": tools_from(catalog),
            // tool_choice queda en automático a propósito: si la intención no
            // se puede expresar con las capacidades disponibles, el modelo
            // tiene que poder decirlo en vez de verse forzado a inventarse
            // una llamada que encaje a medias.
            "fallbacks": "default",
            "messages": [{"role": "user", "content": intent}],
        });

        // http_status_as_error(false): queremos LEER el cuerpo del error de
        // la API, no solo su código de estado.
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();

        let mut resp = agent
            .post(URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("anthropic-beta", "server-side-fallback-2026-07-01")
            .header("content-type", "application/json")
            .send_json(&body)
            .context("no pude contactar con la API de Anthropic")?;

        let status = resp.status().as_u16();
        let v: Value = resp.body_mut().read_json().context("respuesta ilegible")?;

        if status >= 400 {
            let msg = v["error"]["message"].as_str().unwrap_or("error desconocido");
            bail!("la API respondió {status}: {msg}");
        }

        // Un rechazo llega como HTTP 200. Hay que mirar stop_reason.
        if v["stop_reason"] == "refusal" {
            let why = v["stop_details"]["explanation"]
                .as_str()
                .unwrap_or("sin explicación");
            bail!("el modelo declinó planificar esta intención: {why}");
        }

        let blocks = v["content"]
            .as_array()
            .ok_or_else(|| anyhow!("respuesta sin contenido"))?;

        let mut steps = Vec::new();
        let mut said = String::new();
        for block in blocks {
            match block["type"].as_str() {
                Some("tool_use") => {
                    let tool = block["name"].as_str().unwrap_or_default();
                    let mut args = BTreeMap::new();
                    if let Some(obj) = block["input"].as_object() {
                        for (k, val) in obj {
                            // Los valores no-string se serializan tal cual:
                            // la validación del catálogo decidirá si valen.
                            let s = match val {
                                Value::String(s) => s.clone(),
                                other => other.to_string(),
                            };
                            args.insert(k.clone(), s);
                        }
                    }
                    steps.push(Step {
                        capability: from_tool_name(tool),
                        args,
                    });
                }
                Some("text") => said.push_str(block["text"].as_str().unwrap_or_default()),
                _ => {}
            }
        }

        if steps.is_empty() {
            let said = said.trim();
            if said.is_empty() {
                bail!("el modelo no propuso ninguna capacidad");
            }
            bail!("no se pudo planificar con las capacidades disponibles.\n{said}");
        }
        Ok(steps)
    }
}

const SYSTEM: &str = "\
Eres el planificador de syso, un sistema operativo para desarrolladores.

Traduces la intención del usuario a llamadas de las capacidades disponibles.
Reglas:
- Solo puedes usar las capacidades ofrecidas como herramientas. No existe una \
shell ni ninguna otra vía.
- Emite todas las llamadas necesarias para cubrir la intención completa, en orden.
- No ejecutas nada: tus llamadas son una propuesta que el usuario revisará.
- Si la intención no se puede expresar con las capacidades disponibles, no \
llames a ninguna: explica en texto qué falta.
- Las rutas son relativas al espacio de trabajo. Nunca uses rutas absolutas ni '..'.";

/// Los nombres de herramienta de la API no admiten puntos, y los de las
/// capacidades sí (`fs.read`). Traducimos en los dos sentidos.
fn to_tool_name(cap: &str) -> String {
    cap.replace('.', "__")
}

fn from_tool_name(tool: &str) -> String {
    tool.replace("__", ".")
}

fn tools_from(catalog: &Catalog) -> Vec<Value> {
    catalog
        .caps
        .values()
        .map(|cap| {
            let mut properties = serde_json::Map::new();
            let mut required = Vec::new();

            for (name, spec) in &cap.params {
                let mut schema = serde_json::Map::new();
                schema.insert("type".into(), json!("string"));
                if spec.kind == "enum" {
                    schema.insert("enum".into(), json!(spec.of));
                }
                let mut description = match spec.kind.as_str() {
                    "path" => "Ruta relativa al espacio de trabajo".to_string(),
                    "slug" => "Identificador: letras, dígitos, '-', '_' y '.'".to_string(),
                    "text" => "Contenido literal del fichero".to_string(),
                    _ => String::new(),
                };
                if let Some(max) = spec.max {
                    description.push_str(&format!(" (máximo {max} caracteres)"));
                }
                let description = description.trim().to_string();
                if !description.is_empty() {
                    schema.insert("description".into(), json!(description));
                }
                properties.insert(name.clone(), Value::Object(schema));

                if !spec.optional && spec.default.is_none() {
                    required.push(name.clone());
                }
            }

            json!({
                "name": to_tool_name(&cap.name),
                "description": cap.summary,
                "input_schema": {
                    "type": "object",
                    "properties": properties,
                    "required": required,
                    "additionalProperties": false,
                }
            })
        })
        .collect()
}
