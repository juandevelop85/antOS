//! Universal OpenAI-Compatible LLM Planner for antOS (Ticket T19.1).
//!
//! Connects via HTTP to any inference provider implementing the `/v1/chat/completions`
//! standard with Tool Calling support.
//!
//! Includes built-in presets and adapters for free cloud tiers (OpenRouter Free, Groq Cloud Free,
//! Google Gemini API Free) and local independent engines (OpenCode, llama.cpp server, LocalAI, vLLM).

use super::{Planner, Propuesta};
use crate::capability::Catalog;
use crate::plan::Step;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

pub const PLAN_TOOL: &str = "emitir_plan";

/// Curated recommendation of free / low-cost models.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct FreeModelRecommendation {
    pub provider_id: &'static str,
    pub display_name: &'static str,
    pub provider_type: &'static str, // "local" | "cloud_free"
    pub default_model: &'static str,
    pub alternative_models: Vec<&'static str>,
    pub endpoint: &'static str,
    pub env_key: &'static str,
    pub description: &'static str,
    pub rate_limits: &'static str,
}

/// Returns the curated list of recommended 100% free models.
#[allow(dead_code)]
pub fn get_free_recommendations() -> Vec<FreeModelRecommendation> {
    vec![
        FreeModelRecommendation {
            provider_id: "groq",
            display_name: "Groq Cloud (Free Tier)",
            provider_type: "cloud_free",
            default_model: "llama-3.3-70b-versatile",
            alternative_models: vec![
                "deepseek-r1-distill-llama-70b",
                "mixtral-8x7b-32768",
                "gemma2-9b-it",
            ],
            endpoint: "https://api.groq.com/openai/v1",
            env_key: "GROQ_API_KEY",
            description: "Inferencia ultrarrápida (>300 tokens/s) sin coste con hardware LPU.",
            rate_limits: "30 peticiones/minuto, 14.400 peticiones/día (completamente gratis)",
        },
        FreeModelRecommendation {
            provider_id: "openrouter",
            display_name: "OpenRouter (Free Tier)",
            provider_type: "cloud_free",
            default_model: "deepseek/deepseek-r1:free",
            alternative_models: vec![
                "qwen/qwen-2.5-coder-32b-instruct:free",
                "meta-llama/llama-3.3-70b-instruct:free",
                "google/gemini-2.0-flash-exp:free",
            ],
            endpoint: "https://openrouter.ai/api/v1",
            env_key: "OPENROUTER_API_KEY",
            description: "Acceso a DeepSeek-R1 y Qwen-2.5-Coder 32B con coste $0.",
            rate_limits: "20 peticiones/minuto con cuenta gratuita de OpenRouter",
        },
        FreeModelRecommendation {
            provider_id: "gemini",
            display_name: "Google Gemini API (Free Tier)",
            provider_type: "cloud_free",
            default_model: "gemini-2.0-flash",
            alternative_models: vec!["gemini-1.5-flash", "gemini-1.5-pro"],
            endpoint: "https://generativelanguage.googleapis.com/v1beta/openai",
            env_key: "GEMINI_API_KEY",
            description: "API de Google AI Studio con generoso tier gratuito.",
            rate_limits: "15 peticiones/minuto, 1.500 peticiones/día gratis",
        },
        FreeModelRecommendation {
            provider_id: "opencode",
            display_name: "OpenCode / llama.cpp / LocalAI (Local)",
            provider_type: "local",
            default_model: "qwen2.5-coder",
            alternative_models: vec!["deepseek-coder", "mistral-nemo", "default"],
            endpoint: "http://127.0.0.1:8080/v1",
            env_key: "OPENCODE_API_KEY",
            description: "Servidor local OpenAI-compatible sin conexión a la nube.",
            rate_limits: "Ilimitado (depende de la CPU/GPU del equipo local)",
        },
        FreeModelRecommendation {
            provider_id: "ollama",
            display_name: "Ollama (Local Offline)",
            provider_type: "local",
            default_model: "qwen2.5-coder:latest",
            alternative_models: vec!["deepseek-coder:latest", "llama3.2:latest", "phi4:latest"],
            endpoint: "http://127.0.0.1:11434/v1",
            env_key: "OLLAMA_API_KEY",
            description: "Daemon nativo local de Ollama con endpoint /v1 estándar.",
            rate_limits: "Ilimitado (100% offline y privado)",
        },
    ]
}

/// Universal planner for OpenAI-compatible HTTP endpoints.
#[derive(Debug, Clone)]
pub struct OpenAiCompatPlanner {
    pub provider_id: String,
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
    pub custom_headers: Vec<(String, String)>,
}

impl OpenAiCompatPlanner {
    /// Creates a custom planner with given parameters.
    pub fn new(
        provider_id: impl Into<String>,
        endpoint: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        let mut planner = Self {
            provider_id: provider_id.into(),
            endpoint: endpoint.into().trim_end_matches('/').to_string(),
            model: model.into(),
            api_key,
            custom_headers: Vec::new(),
        };

        // OpenRouter requires specific headers for free tier ranking
        if planner.provider_id == "openrouter" || planner.endpoint.contains("openrouter.ai") {
            planner
                .custom_headers
                .push(("HTTP-Referer".into(), "https://antos.dev".into()));
            planner
                .custom_headers
                .push(("X-Title".into(), "antOS Developer OS".into()));
        }

        planner
    }

    /// Initializes a planner from a known provider preset and environment variables.
    pub fn from_preset(provider: &str) -> Result<Self> {
        match provider.to_lowercase().as_str() {
            "groq" => {
                let api_key = read_key_with_fallbacks("groq", "GROQ_API_KEY")?;
                let model = std::env::var("ANTOS_GROQ_MODEL")
                    .or_else(|_| std::env::var("GROQ_MODEL"))
                    .unwrap_or_else(|_| "llama-3.3-70b-versatile".into());
                let endpoint = std::env::var("ANTOS_GROQ_ENDPOINT")
                    .unwrap_or_else(|_| "https://api.groq.com/openai/v1".into());
                Ok(Self::new("groq", endpoint, model, Some(api_key)))
            }
            "openrouter" | "open-router" => {
                let api_key = read_key_with_fallbacks("openrouter", "OPENROUTER_API_KEY")?;
                let model = std::env::var("ANTOS_OPENROUTER_MODEL")
                    .or_else(|_| std::env::var("OPENROUTER_MODEL"))
                    .unwrap_or_else(|_| "deepseek/deepseek-r1:free".into());
                let endpoint = std::env::var("ANTOS_OPENROUTER_ENDPOINT")
                    .unwrap_or_else(|_| "https://openrouter.ai/api/v1".into());
                Ok(Self::new("openrouter", endpoint, model, Some(api_key)))
            }
            "gemini" | "google" => {
                let api_key = read_key_with_fallbacks("gemini", "GEMINI_API_KEY")?;
                let model = std::env::var("ANTOS_GEMINI_MODEL")
                    .or_else(|_| std::env::var("GEMINI_MODEL"))
                    .unwrap_or_else(|_| "gemini-2.0-flash".into());
                let endpoint = std::env::var("ANTOS_GEMINI_ENDPOINT").unwrap_or_else(|_| {
                    "https://generativelanguage.googleapis.com/v1beta/openai".into()
                });
                Ok(Self::new("gemini", endpoint, model, Some(api_key)))
            }
            "opencode" | "localai" | "llamacpp" | "vllm" => {
                let endpoint = std::env::var("ANTOS_OPENCODE_ENDPOINT")
                    .or_else(|_| std::env::var("OPENCODE_ENDPOINT"))
                    .unwrap_or_else(|_| "http://127.0.0.1:8080/v1".into());
                let model = std::env::var("ANTOS_OPENCODE_MODEL")
                    .or_else(|_| std::env::var("OPENCODE_MODEL"))
                    .unwrap_or_else(|_| "qwen2.5-coder".into());
                let api_key = std::env::var("OPENCODE_API_KEY").ok();
                Ok(Self::new("opencode", endpoint, model, api_key))
            }
            "ollama-v1" | "ollama_v1" => {
                let endpoint = std::env::var("ANTOS_OLLAMA_ENDPOINT")
                    .unwrap_or_else(|_| "http://127.0.0.1:11434/v1".into());
                let model = std::env::var("ANTOS_OLLAMA_MODEL")
                    .unwrap_or_else(|_| "qwen2.5-coder:latest".into());
                Ok(Self::new("ollama", endpoint, model, None))
            }
            other => {
                // Generic OpenAI fallback using OPENAI_BASE_URL and OPENAI_API_KEY
                let endpoint = std::env::var("OPENAI_BASE_URL")
                    .or_else(|_| std::env::var("ANTOS_LLM_ENDPOINT"))
                    .unwrap_or_else(|_| "http://127.0.0.1:8080/v1".into());
                let model = std::env::var("OPENAI_MODEL")
                    .or_else(|_| std::env::var("ANTOS_LLM_MODEL"))
                    .unwrap_or_else(|_| "default".into());
                let api_key = std::env::var("OPENAI_API_KEY").ok();
                Ok(Self::new(other, endpoint, model, api_key))
            }
        }
    }

    /// Fast health probe with a 1.5-second timeout to check if the endpoint responds.
    pub fn is_available(&self) -> bool {
        let url = format!("{}/models", self.endpoint);
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_millis(1500)))
            .http_status_as_error(false)
            .build()
            .into();

        let mut req = agent.get(&url);
        if let Some(ref key) = self.api_key {
            req = req.header("authorization", &format!("Bearer {key}"));
        }
        for (h, v) in &self.custom_headers {
            req = req.header(h, v);
        }

        match req.call() {
            Ok(resp) => {
                let status = resp.status().as_u16();
                // 200 is healthy; 401/403 means host is reachable but key might be missing/invalid
                status < 500
            }
            Err(_) => false,
        }
    }

    /// Fetches the list of models exposed by the endpoint.
    #[allow(dead_code)]
    pub fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/models", self.endpoint);
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(5)))
            .http_status_as_error(false)
            .build()
            .into();

        let mut req = agent.get(&url);
        if let Some(ref key) = self.api_key {
            req = req.header("authorization", &format!("Bearer {key}"));
        }
        for (h, v) in &self.custom_headers {
            req = req.header(h, v);
        }

        let mut resp = req.call().context("failed to query /models endpoint")?;
        let status = resp.status().as_u16();
        let value: Value = resp
            .body_mut()
            .read_json()
            .context("invalid models JSON response")?;

        if status >= 400 {
            let msg = value["error"]["message"]
                .as_str()
                .unwrap_or("error fetching models");
            bail!("endpoint responded {status}: {msg}");
        }

        let mut models = Vec::new();
        if let Some(data) = value["data"].as_array() {
            for item in data {
                if let Some(id) = item["id"].as_str() {
                    models.push(id.to_string());
                }
            }
        }
        models.sort();
        Ok(models)
    }
}

impl Planner for OpenAiCompatPlanner {
    fn name(&self) -> &'static str {
        "openai_compat"
    }

    fn plan(&self, intent: &str, catalog: &Catalog) -> Result<Propuesta> {
        let url = format!("{}/chat/completions", self.endpoint);

        let system_prompt = format!(
            "{SYSTEM_PROMPT}\n\nCapacidades declaradas en antOS:\n\n{}",
            catalog_to_text(catalog)
        );

        let plan_tool_spec = build_plan_tool_schema(catalog);

        let body = json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": intent }
            ],
            "tools": [plan_tool_spec],
            "tool_choice": "auto",
            "temperature": 0.0
        });

        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(120)))
            .http_status_as_error(false)
            .build()
            .into();

        let mut req = agent.post(&url).header("content-type", "application/json");

        if let Some(ref key) = self.api_key {
            req = req.header("authorization", &format!("Bearer {key}"));
        }
        for (h, v) in &self.custom_headers {
            req = req.header(h, v);
        }

        let mut resp = req.send_json(&body).with_context(|| {
            format!(
                "no se pudo conectar con el endpoint LLM en {}",
                self.endpoint
            )
        })?;

        let status = resp.status().as_u16();
        let value: Value = resp
            .body_mut()
            .read_json()
            .context("respuesta JSON inválida del proveedor LLM")?;

        if status >= 400 {
            let err_msg = value["error"]["message"]
                .as_str()
                .or_else(|| value["error"].as_str())
                .unwrap_or("error desconocido en llamada LLM");
            bail!(
                "Proveedor LLM ({}) respondió {status}: {err_msg}",
                self.provider_id
            );
        }

        parse_openai_chat_response(&value)
    }
}

/// Parses an OpenAI-compatible chat completion response.
///
/// Supports:
/// 1. Formal `tool_calls` in `choices[0].message.tool_calls` (both stringified JSON and Value objects).
/// 2. Embedded markdown code block JSON fallback in `choices[0].message.content`.
pub fn parse_openai_chat_response(v: &Value) -> Result<Propuesta> {
    let choice = v["choices"]
        .as_array()
        .and_then(|c| c.first())
        .ok_or_else(|| anyhow::anyhow!("respuesta sin choices"))?;

    let message = &choice["message"];
    let mut steps = Vec::new();
    let mut said = String::new();

    // 1. Inspect tool_calls
    if let Some(tool_calls) = message["tool_calls"].as_array() {
        for call in tool_calls {
            let func = &call["function"];
            let name = func["name"].as_str().unwrap_or_default();

            // function.arguments in standard OpenAI is a JSON string, but some servers emit a parsed JSON object.
            let args_val: Value = match func.get("arguments") {
                Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
                Some(obj @ Value::Object(_)) => obj.clone(),
                _ => Value::Null,
            };

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
                            steps.push(Step {
                                capability,
                                args: args_map,
                            });
                        }
                    }
                }
            } else if !name.is_empty() {
                // Direct capability invocation
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
                steps.push(Step {
                    capability: name.to_string(),
                    args: args_map,
                });
            }
        }
    }

    // 2. Inspect text content
    if let Some(text) = message["content"].as_str() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            if said.is_empty() {
                said.push_str(trimmed);
            }
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
            bail!("el modelo no propuso ninguna capacidad de antOS");
        }
        bail!("no se pudo planificar con las capacidades disponibles.\n{said}");
    }

    Ok(Propuesta {
        steps,
        nota: (!said.is_empty()).then_some(said),
    })
}

/// Fallback parser when model outputs markdown code fence JSON instead of structured tool call.
fn try_parse_json_from_text(text: &str) -> Option<Propuesta> {
    let json_str = if let Some(start) = text.find("```json") {
        let rest = &text[start + 7..];
        let end = rest.find("```").unwrap_or(rest.len());
        rest[..end].trim()
    } else if let Some(start) = text.find("```") {
        let rest = &text[start + 3..];
        let end = rest.find("```").unwrap_or(rest.len());
        rest[..end].trim()
    } else if text.trim_start().starts_with('{') {
        text.trim()
    } else {
        return None;
    };

    let parsed: Value = serde_json::from_str(json_str).ok()?;
    let note = parsed["nota"].as_str().map(|s| s.to_string());
    let raw_steps = parsed["pasos"].as_array()?;

    let mut steps = Vec::new();
    for s in raw_steps {
        let capability = s["capacidad"].as_str()?.to_string();
        let mut args = BTreeMap::new();
        if let Some(obj) = s["argumentos"].as_object() {
            for (k, val) in obj {
                let val_str = match val {
                    Value::String(st) => st.clone(),
                    other => other.to_string(),
                };
                args.insert(k.clone(), val_str);
            }
        }
        steps.push(Step { capability, args });
    }

    if steps.is_empty() {
        None
    } else {
        Some(Propuesta { steps, nota: note })
    }
}

/// Builds OpenAI-standard tool schema matching the antOS capability catalog.
fn build_plan_tool_schema(catalog: &Catalog) -> Value {
    let caps_enum: Vec<&str> = catalog.caps.keys().map(String::as_str).collect();

    json!({
        "type": "function",
        "function": {
            "name": PLAN_TOOL,
            "description": "Emite el plan de ejecución de antOS con la lista ordenada de capacidades a ejecutar.",
            "parameters": {
                "type": "object",
                "properties": {
                    "nota": {
                        "type": "string",
                        "description": "Explicación breve de lo que se va a hacer"
                    },
                    "pasos": {
                        "type": "array",
                        "description": "Pasos ordenados a ejecutar secuencialmente",
                        "items": {
                            "type": "object",
                            "properties": {
                                "capacidad": {
                                    "type": "string",
                                    "enum": caps_enum,
                                    "description": "Nombre exacto de la capacidad a invocar"
                                },
                                "argumentos": {
                                    "type": "object",
                                    "description": "Argumentos para la capacidad según su manifiesto tipado"
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

/// Helper to read an API key checking config files first and environment variables as fallback.
fn read_key_with_fallbacks(provider_name: &str, env_var: &str) -> Result<String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    // Check ~/.config/antos/<provider>.key or ~/.config/syso/<provider>.key
    let key_paths = [
        home.join(format!(".config/antos/{provider_name}.key")),
        home.join(format!(".config/syso/{provider_name}.key")),
    ];

    for path in &key_paths {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                let trimmed = content.trim().to_string();
                if !trimmed.is_empty() {
                    return Ok(trimmed);
                }
            }
        }
    }

    // Fall back to environment variable
    if let Ok(key) = std::env::var(env_var) {
        let trimmed = key.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }

    bail!(
        "sin clave API para {provider_name}. Define la variable {env_var} o guárdala en ~/.config/antos/{provider_name}.key"
    )
}

const SYSTEM_PROMPT: &str = "\
Eres el planificador del sistema operativo antOS, una plataforma para desarrolladores.
Tu función es traducir la intención del usuario a una secuencia de invocaciones de capacidades tipadas.

Reglas estrictas:
1. Solo puedes usar las capacidades declaradas en el catálogo. No inventes comandos ni capacidades inexistentes.
2. Emite el plan completo invocando la herramienta `emitir_plan`.
3. Si la intención no se puede resolver con las capacidades disponibles, indícalo claramente con una explicación.
";

// ───────────────────────────────────────────────────────────────────── tests ──

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_free_recommendations_not_empty() {
        let recs = get_free_recommendations();
        assert!(
            !recs.is_empty(),
            "recommendations catalogue should not be empty"
        );
        let has_groq = recs.iter().any(|r| r.provider_id == "groq");
        let has_openrouter = recs.iter().any(|r| r.provider_id == "openrouter");
        let has_ollama = recs.iter().any(|r| r.provider_id == "ollama");
        assert!(has_groq, "should include groq");
        assert!(has_openrouter, "should include openrouter");
        assert!(has_ollama, "should include ollama");
    }

    #[test]
    fn test_parse_openai_chat_response_stringified_arguments() {
        let raw = json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "tool_calls": [
                            {
                                "function": {
                                    "name": "emitir_plan",
                                    "arguments": "{\"nota\":\"Creando nuevo proyecto\",\"pasos\":[{\"capacidad\":\"project.init\",\"argumentos\":{\"name\":\"api-service\"}}]}"
                                }
                            }
                        ]
                    }
                }
            ]
        });

        let propuesta =
            parse_openai_chat_response(&raw).expect("should parse stringified tool_calls");
        assert_eq!(propuesta.steps.len(), 1);
        assert_eq!(propuesta.steps[0].capability, "project.init");
        assert_eq!(
            propuesta.steps[0].args.get("name").map(String::as_str),
            Some("api-service")
        );
        assert_eq!(propuesta.nota.as_deref(), Some("Creando nuevo proyecto"));
    }

    #[test]
    fn test_parse_openai_chat_response_object_arguments() {
        let raw = json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "tool_calls": [
                            {
                                "function": {
                                    "name": "emitir_plan",
                                    "arguments": {
                                        "nota": "Diagnóstico de red",
                                        "pasos": [
                                            {
                                                "capacidad": "net.diagnose_port",
                                                "argumentos": { "port": "8080" }
                                            }
                                        ]
                                    }
                                }
                            }
                        ]
                    }
                }
            ]
        });

        let propuesta = parse_openai_chat_response(&raw).expect("should parse object tool_calls");
        assert_eq!(propuesta.steps.len(), 1);
        assert_eq!(propuesta.steps[0].capability, "net.diagnose_port");
        assert_eq!(
            propuesta.steps[0].args.get("port").map(String::as_str),
            Some("8080")
        );
        assert_eq!(propuesta.nota.as_deref(), Some("Diagnóstico de red"));
    }

    #[test]
    fn test_parse_openai_chat_response_markdown_json_fallback() {
        let raw = json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Voy a crear la rama:\n```json\n{\n  \"nota\": \"Creando rama feature\",\n  \"pasos\": [\n    {\n      \"capacidad\": \"git.branch\",\n      \"argumentos\": { \"name\": \"feature-auth\" }\n    }\n  ]\n}\n```"
                    }
                }
            ]
        });

        let propuesta =
            parse_openai_chat_response(&raw).expect("should parse fallback markdown json");
        assert_eq!(propuesta.steps.len(), 1);
        assert_eq!(propuesta.steps[0].capability, "git.branch");
        assert_eq!(
            propuesta.steps[0].args.get("name").map(String::as_str),
            Some("feature-auth")
        );
        assert_eq!(propuesta.nota.as_deref(), Some("Creando rama feature"));
    }

    #[test]
    fn test_parse_openai_chat_response_empty_fails() {
        let raw = json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "No entiendo la petición y no hay capacidades."
                    }
                }
            ]
        });

        let res = parse_openai_chat_response(&raw);
        assert!(
            res.is_err(),
            "should fail when no capabilities are provided"
        );
    }

    #[test]
    fn test_build_plan_tool_schema() {
        let catalog = Catalog {
            caps: BTreeMap::new(),
        };
        let schema = build_plan_tool_schema(&catalog);
        assert_eq!(schema["type"], "function");
        assert_eq!(schema["function"]["name"], PLAN_TOOL);
        assert!(schema["function"]["parameters"]["properties"]["pasos"].is_object());
    }
}
