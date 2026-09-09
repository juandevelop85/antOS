//! Local LLM Planner for antOS using Ollama or llama.cpp (Ticket T6.1).
//!
//! Connects via HTTP to a local Ollama instance (default http://127.0.0.1:11434).
//! Translates user intents into capability invocations with zero cloud dependency.

use super::{Planner, Propuesta};
use crate::capability::Catalog;
use crate::plan::Step;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

pub const DEFAULT_OLLAMA_ENDPOINT: &str = "http://127.0.0.1:11434";
pub const DEFAULT_OLLAMA_MODEL: &str = "qwen2.5-coder:latest";
const PLAN_TOOL: &str = "emitir_plan";

/// Local LLM planner interfacing with Ollama.
#[derive(Debug, Clone)]
pub struct OllamaPlanner {
    pub endpoint: String,
    pub model: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaTagsResponse {
    #[serde(default)]
    models: Vec<OllamaModelTag>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaModelTag {
    name: String,
    #[serde(default)]
    size: u64,
}

impl OllamaPlanner {
    /// Creates an instance inspecting environment variables or default local host.
    pub fn from_env() -> Result<Self> {
        let endpoint = std::env::var("ANTOS_OLLAMA_HOST")
            .or_else(|_| std::env::var("OLLAMA_HOST"))
            .unwrap_or_else(|_| DEFAULT_OLLAMA_ENDPOINT.to_string());

        let model = std::env::var("ANTOS_OLLAMA_MODEL")
            .or_else(|_| std::env::var("OLLAMA_MODEL"))
            .unwrap_or_else(|_| DEFAULT_OLLAMA_MODEL.to_string());

        Ok(Self { endpoint, model })
    }

    /// Fast probe with a 1.5-second timeout to check if Ollama is listening.
    pub fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.endpoint.trim_end_matches('/'));
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_millis(1500)))
            .build()
            .into();

        match agent.get(&url).call() {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// Returns list of downloaded models in the local Ollama instance.
    pub fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/api/tags", self.endpoint.trim_end_matches('/'));
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(3)))
            .build()
            .into();

        let mut resp = agent.get(&url).call().context("failed to query Ollama tags")?;
        let tags: OllamaTagsResponse = resp.body_mut().read_json().context("invalid Ollama tags response")?;
        Ok(tags.models.into_iter().map(|m| m.name).collect())
    }
}

impl Planner for OllamaPlanner {
    fn name(&self) -> &'static str {
        "ollama"
    }

    fn plan(&self, intent: &str, catalog: &Catalog) -> Result<Propuesta> {
        let url = format!("{}/api/chat", self.endpoint.trim_end_matches('/'));

        let system_prompt = format!(
            "{SYSTEM}\n\nCapacidades disponibles en antOS:\n\n{}",
            catalog_to_text(catalog)
        );

        let plan_tool_spec = build_plan_tool_schema(catalog);

        let body = json!({
            "model": self.model,
            "stream": false,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": intent }
            ],
            "tools": [plan_tool_spec],
            "options": {
                "temperature": 0.0
            }
        });

        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(120)))
            .http_status_as_error(false)
            .build()
            .into();

        let mut resp = agent
            .post(&url)
            .header("content-type", "application/json")
            .send_json(&body)
            .context("no se pudo conectar con el motor local de Ollama")?;

        let status = resp.status().as_u16();
        let value: Value = resp.body_mut().read_json().context("respuesta JSON inválida de Ollama")?;

        if status >= 400 {
            let err_msg = value["error"].as_str().unwrap_or("error desconocido en Ollama");
            bail!("Ollama respondió {status}: {err_msg}");
        }

        parse_ollama_chat_response(&value)
    }
}

/// Parses Ollama chat response checking both structured tool_calls and JSON text fallback.
pub fn parse_ollama_chat_response(v: &Value) -> Result<Propuesta> {
    let message = &v["message"];
    let mut steps = Vec::new();
    let mut said = String::new();

    // 1. Check for tool_calls array
    if let Some(tool_calls) = message["tool_calls"].as_array() {
        for call in tool_calls {
            let func = &call["function"];
            let name = func["name"].as_str().unwrap_or_default();
            let args_val = &func["arguments"];

            if name == PLAN_TOOL {
                if let Some(note) = args_val["nota"].as_str() {
                    said.push_str(note);
                }
                if let Some(raw_steps) = args_val["pasos"].as_array() {
                    for s in raw_steps {
                        let capability = s["capacidad"].as_str().unwrap_or_default().to_string();
                        let mut args_map = BTreeMap::new();
                        if let Some(obj) = s["argumentos"].as_object() {
                            for (k, val) in obj {
                                let string_val = match val {
                                    Value::String(st) => st.clone(),
                                    other => other.to_string(),
                                };
                                args_map.insert(k.clone(), string_val);
                            }
                        }
                        if !capability.is_empty() {
                            steps.push(Step { capability, args: args_map });
                        }
                    }
                }
            } else if !name.is_empty() {
                // The model invoked a capability directly
                let mut args_map = BTreeMap::new();
                if let Some(obj) = args_val.as_object() {
                    for (k, val) in obj {
                        let string_val = match val {
                            Value::String(st) => st.clone(),
                            other => other.to_string(),
                        };
                        args_map.insert(k.clone(), string_val);
                    }
                }
                steps.push(Step { capability: name.to_string(), args: args_map });
            }
        }
    }

    // 2. Parse text content if present
    if let Some(text) = message["content"].as_str() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            if said.is_empty() {
                said.push_str(trimmed);
            }
            // If steps were empty, try parsing embedded JSON block
            if steps.is_empty() {
                if let Some(parsed) = try_parse_json_from_text(trimmed) {
                    return Ok(parsed);
                }
            }
        }
    }

    let said = said.trim().to_string();
    if steps.is_empty() {
        if said.is_empty() {
            bail!("el modelo local no propuso ninguna capacidad");
        }
        bail!("no se pudo planificar con las capacidades disponibles.\n{said}");
    }

    Ok(Propuesta {
        steps,
        nota: (!said.is_empty()).then_some(said),
    })
}

/// Fallback parser if local model emitted raw JSON instead of formal tool calls.
fn try_parse_json_from_text(text: &str) -> Option<Propuesta> {
    let json_str = if let Some(start) = text.find("```json") {
        let rest = &text[start + 7..];
        let end = rest.find("```").unwrap_or(rest.len());
        rest[..end].trim()
    } else if let Some(start) = text.find('{') {
        let end = text.rfind('}').map(|i| i + 1).unwrap_or(text.len());
        &text[start..end]
    } else {
        return None;
    };

    let v: Value = serde_json::from_str(json_str).ok()?;
    let mut steps = Vec::new();
    let note = v["nota"].as_str().map(String::from);

    if let Some(pasos) = v["pasos"].as_array() {
        for p in pasos {
            let capability = p["capacidad"].as_str()?.to_string();
            let mut args = BTreeMap::new();
            if let Some(obj) = p["argumentos"].as_object() {
                for (k, val) in obj {
                    let s = match val {
                        Value::String(st) => st.clone(),
                        other => other.to_string(),
                    };
                    args.insert(k.clone(), s);
                }
            }
            steps.push(Step { capability, args });
        }
    }

    if steps.is_empty() {
        None
    } else {
        Some(Propuesta { steps, nota: note })
    }
}

fn build_plan_tool_schema(catalog: &Catalog) -> Value {
    let caps_enum: Vec<&str> = catalog.caps.keys().map(String::as_str).collect();

    json!({
        "type": "function",
        "function": {
            "name": PLAN_TOOL,
            "description": "Emite el plan de ejecución de antOS con la lista ordenada de pasos.",
            "parameters": {
                "type": "object",
                "properties": {
                    "nota": {
                        "type": "string",
                        "description": "Explicación breve de lo que se va a hacer"
                    },
                    "pasos": {
                        "type": "array",
                        "description": "Pasos ordenados a ejecutar",
                        "items": {
                            "type": "object",
                            "properties": {
                                "capacidad": {
                                    "type": "string",
                                    "enum": caps_enum,
                                    "description": "Nombre exacto de la capacidad"
                                },
                                "argumentos": {
                                    "type": "object",
                                    "description": "Argumentos para la capacidad según su manifiesto"
                                }
                            },
                            "required": ["capacidad", "argumentos"]
                        }
                    }
                },
                "required": ["pasos"]
            }
        }
    })
}

fn catalog_to_text(catalog: &Catalog) -> String {
    let mut lines = Vec::new();
    for cap in catalog.caps.values() {
        lines.push(format!("- `{}`: {}", cap.name, cap.summary));
    }
    lines.join("\n")
}

const SYSTEM: &str = "\
Eres el planificador local de antOS, un sistema operativo personal para desarrolladores.

Traduces la intención del usuario a llamadas de las capacidades disponibles.
Reglas estrictas:
- Solo puedes usar las capacidades ofrecidas como herramientas. No inventes comandos de shell.
- Emite el plan completo de una sola vez invocando la herramienta `emitir_plan`.
- Los pasos se ejecutan en orden secuencial.
- Si la intención no se puede resolver con las capacidades disponibles, indícalo claramente.
";

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_parse_ollama_tool_call_response() {
        let resp = json!({
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "function": {
                            "name": "emitir_plan",
                            "arguments": {
                                "nota": "creando proyecto demo",
                                "pasos": [
                                    {
                                        "capacidad": "project.scaffold",
                                        "argumentos": { "language": "rust", "name": "demo" }
                                    }
                                ]
                            }
                        }
                    }
                ]
            }
        });

        let propuesta = parse_ollama_chat_response(&resp).expect("debe parsear tool call");
        assert_eq!(propuesta.steps.len(), 1);
        assert_eq!(propuesta.steps[0].capability, "project.scaffold");
        assert_eq!(propuesta.steps[0].args.get("language").map(String::as_str), Some("rust"));
        assert_eq!(propuesta.steps[0].args.get("name").map(String::as_str), Some("demo"));
        assert_eq!(propuesta.nota.as_deref(), Some("creando proyecto demo"));
    }

    #[test]
    fn test_parse_ollama_json_fallback_response() {
        let resp = json!({
            "message": {
                "role": "assistant",
                "content": "```json\n{\n  \"nota\": \"levantando postgres\",\n  \"pasos\": [\n    {\n      \"capacidad\": \"env.service_up\",\n      \"argumentos\": { \"service\": \"postgres\" }\n    }\n  ]\n}\n```"
            }
        });

        let propuesta = parse_ollama_chat_response(&resp).expect("debe parsear json en bloque markdown");
        assert_eq!(propuesta.steps.len(), 1);
        assert_eq!(propuesta.steps[0].capability, "env.service_up");
        assert_eq!(propuesta.steps[0].args.get("service").map(String::as_str), Some("postgres"));
    }

    #[test]
    fn test_ollama_defaults() {
        let planner = OllamaPlanner::from_env().expect("planner from env");
        assert!(planner.endpoint.contains("11434") || !planner.endpoint.is_empty());
        assert_eq!(planner.name(), "ollama");
    }
}
