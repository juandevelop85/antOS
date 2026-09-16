//! Planificador con Claude, por HTTP directo contra la Messages API.
//!
//! Rust no tiene SDK oficial de Anthropic, así que hablamos HTTP a mano.
//!
//! El catálogo se traduce a definiciones de herramienta: cada capacidad es
//! una herramienta con su esquema. El modelo no «escribe comandos», elige
//! entre capacidades declaradas y rellena parámetros tipados — y aun así su
//! salida vuelve a validarse contra el catálogo antes de usarse.

use super::{Planner, Proposal};
use crate::capability::Catalog;
use crate::plan::Step;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

const URL: &str = "https://api.anthropic.com/v1/messages";
const DEFAULT_MODEL: &str = "claude-opus-5";
pub const API_KEY_ENV_VAR: &str = "ANTHROPIC_API_KEY";
const PLAN_TOOL: &str = "emitir_plan";

pub struct ClaudePlanner {
    api_key: String,
    model: String,
}

impl ClaudePlanner {
    /// Clave y modelo resueltos, para que el runtime de agente (T33.2) use
    /// exactamente la misma configuración que el planificador.
    pub fn credentials(&self) -> (&str, &str) {
        (&self.api_key, &self.model)
    }

    pub fn from_env() -> Result<Self> {
        let api_key = read_key()?;
        let model = crate::util::env_with_legacy_fallback("ANTOS_MODEL", "SYSO_MODEL")
            .and_then(|v| v.into_string().ok())
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        Ok(Self { api_key, model })
    }
}

/// Dónde vive la clave por defecto.
fn key_path() -> PathBuf {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let new_path = base.join(".config/antos/anthropic.key");
    // Migración de una sola vez (T31.11): una instalación con la clave en
    // `~/.config/syso/anthropic.key` de antes del renombrado se mueve a
    // `~/.config/antos/anthropic.key` exactamente una vez. Por fichero, no
    // por directorio completo: `~/.config/antos/` puede existir ya por otras
    // claves migradas (openai_compat.rs) sin que esta lo haya sido todavía.
    let _ = crate::util::migrate_legacy_path(&base.join(".config/syso/anthropic.key"), &new_path);
    new_path
}

/// Lee la clave de un fichero, y solo como último recurso del entorno.
///
/// El orden importa. Una variable de entorno la hereda TODO proceso hijo, y
/// este programa lanza varios: whisper, ffmpeg, y sobre todo el ejecutor
/// confinado — que es precisamente el componente que el diseño entero trata
/// como no fiable. Un fichero que solo lee el broker no viaja a ninguna parte.
fn read_key() -> Result<String> {
    let path = match std::env::var_os("ANTHROPIC_API_KEY_FILE") {
        Some(p) => PathBuf::from(p),
        None => key_path(),
    };

    if path.exists() {
        let key = std::fs::read_to_string(&path)
            .with_context(|| format!("no pude leer {}", path.display()))?;
        warn_if_readable_by_others(&path);
        let key = key.trim().to_string();
        if !key.is_empty() {
            return Ok(key);
        }
    }

    // El entorno sigue funcionando por comodidad, pero se avisa.
    if let Ok(key) = std::env::var(API_KEY_ENV_VAR) {
        if !key.trim().is_empty() {
            eprintln!(
                "aviso: usando {API_KEY_ENV_VAR} del entorno. Todo proceso hijo la hereda; \n\
                 es preferible {}",
                key_path().display()
            );
            return Ok(key.trim().to_string());
        }
    }

    bail!(
        "no encuentro la clave de la API.\n\
         Ponla en {} (solo lectura para ti):\n\
         \n  mkdir -p ~/.config/antos && chmod 700 ~/.config/antos\n\
         \n  read -rs CLAVE && printf '%s' \"$CLAVE\" > {} && unset CLAVE\n\
         \n  chmod 600 {}\n\
         \nO usa el planificador local, que no necesita clave:\n\
         \n  antos --planificador local \"…\"",
        path.display(),
        path.display(),
        path.display()
    )
}

fn warn_if_readable_by_others(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let mode = meta.permissions().mode() & 0o077;
        if mode != 0 {
            eprintln!(
                "aviso: {} es legible por otros usuarios. Arréglalo con:\n  chmod 600 {}",
                path.display(),
                path.display()
            );
        }
    }
}

impl Planner for ClaudePlanner {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn plan(&self, intent: &str, catalog: &Catalog) -> Result<Proposal> {
        let body = json!({
            "model": self.model,
            "max_tokens": 16000,
            "system": format!("{SYSTEM}\n\nCapacidades disponibles:\n\n{}", catalog_as_text(catalog)),
            "tools": [plan_tool_schema(catalog)],
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
            let msg = v["error"]["message"]
                .as_str()
                .unwrap_or("error desconocido");
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
                Some("tool_use") if block["name"] == PLAN_TOOL => {
                    let input = &block["input"];
                    if let Some(note) = input["nota"].as_str() {
                        said.push_str(note);
                    }
                    for step_val in input["pasos"].as_array().into_iter().flatten() {
                        let capability = step_val["capacidad"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        let mut args = BTreeMap::new();
                        if let Some(obj) = step_val["argumentos"].as_object() {
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
                        steps.push(Step { capability, args });
                    }
                }
                Some("text") => said.push_str(block["text"].as_str().unwrap_or_default()),
                _ => {}
            }
        }

        let said = said.trim().to_string();
        if steps.is_empty() {
            if said.is_empty() {
                bail!("el modelo no propuso ninguna capacidad");
            }
            bail!("no se pudo planificar con las capacidades disponibles.\n{said}");
        }

        Ok(Proposal {
            steps,
            note: (!said.is_empty()).then_some(said),
        })
    }
}

const SYSTEM: &str = "\
Eres el planificador de antOS, un sistema operativo para desarrolladores.

Traduces la intención del usuario a llamadas de las capacidades disponibles.
Reglas:
- Solo puedes usar las capacidades ofrecidas como herramientas. No existe una \
shell ni ninguna otra vía.
- NO HAY SEGUNDA VUELTA. No vas a ver el resultado de tus llamadas ni podrás \
continuar después. Emite el plan COMPLETO en esta única respuesta.
- Los pasos se ejecutan en orden, así que puedes dar por hecho que lo que crea \
un paso existe para los siguientes: declarar una dependencia en un proyecto que \
creas dos líneas más arriba es correcto.
- No ejecutas nada: tus llamadas son una propuesta que el usuario revisará.
- Si la intención no se puede expresar con las capacidades disponibles, no \
llames a ninguna: explica en texto qué falta.
- Las rutas son relativas al espacio de trabajo. Nunca uses rutas absolutas ni '..'.";

/// UNA sola herramienta, cuyo argumento es el plan entero.
///
/// Antes había una herramienta por capacidad, y el modelo emitía una llamada
/// y paraba: es lo natural: una herramienta se invoca para VER su resultado y
/// seguir. Pero aquí no hay bucle de agente — el modelo emite un plan que una
/// persona aprueba de una vez.
///
/// Pedir «devuélveme un plan» en vez de «llama a las capacidades» alinea la
/// petición con lo que la arquitectura decía desde el principio, y de paso
/// hace imposible el plan a medias.
fn plan_tool_schema(catalog: &Catalog) -> Value {
    let names: Vec<&str> = catalog.caps.keys().map(String::as_str).collect();

    json!({
        "name": PLAN_TOOL,
        "description": "Devuelve el plan COMPLETO que cubre la intención del usuario.",
        "input_schema": {
            "type": "object",
            "properties": {
                "nota": {
                    "type": "string",
                    "description": "Una frase para el usuario explicando el plan."
                },
                "pasos": {
                    "type": "array",
                    "description": "Las invocaciones, en el orden en que deben ejecutarse.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "capacidad": { "type": "string", "enum": names },
                            "argumentos": {
                                "type": "object",
                                "description": "Los parámetros de esa capacidad. Todos los valores son cadenas.",
                                "additionalProperties": { "type": "string" }
                            }
                        },
                        "required": ["capacidad", "argumentos"]
                    }
                }
            },
            "required": ["pasos"]
        }
    })
}

/// El catálogo, para que el modelo sepa qué puede pedir. Sale de los
/// manifiestos: no hay una segunda descripción que mantener sincronizada.
fn catalog_as_text(catalog: &Catalog) -> String {
    let mut text = String::new();
    for cap in catalog.caps.values() {
        text.push_str(&format!("- {}: {}\n", cap.name, cap.summary));
        for (name, spec) in &cap.params {
            let optional_marker = if spec.optional || spec.default.is_some() {
                " (opcional)"
            } else {
                ""
            };
            let type_str = if spec.of.is_empty() {
                spec.kind.clone()
            } else {
                format!("uno de [{}]", spec.of.join(", "))
            };
            text.push_str(&format!("    {name}: {type_str}{optional_marker}\n"));
        }
    }
    text
}
