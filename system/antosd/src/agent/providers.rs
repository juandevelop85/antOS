//! Clientes multi-turno con herramientas (T33.2), uno por familia de API.
//!
//! Cada proveedor guarda su propio historial en el formato de su API, porque
//! los tres representan las llamadas a herramienta de forma distinta:
//! - Claude (Messages API): bloques `tool_use` en el turno del asistente y
//!   bloques `tool_result` en el siguiente turno de usuario. El contenido
//!   del asistente se reenvía ÍNTEGRO (incluidos bloques `thinking`).
//! - Ollama (`/api/chat`): `message.tool_calls[].function` sin id; el
//!   resultado vuelve como mensaje `role: tool` con `tool_name`.
//! - OpenAI-compatible (`/chat/completions`): `tool_calls[]` con id y
//!   `arguments` como JSON serializado; resultado `role: tool` con
//!   `tool_call_id`.
//!
//! Sin SDK: HTTP a mano con `ureq`, como los planificadores. Las claves se
//! leen por los mismos caminos que ellos (`~/.config/antos/*.key`, entorno).

use super::tools::ToolSpec;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

/// Una llamada a herramienta pedida por el modelo.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// Un turno del asistente.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Turn {
    pub text: String,
    pub calls: Vec<ToolCall>,
    /// Tokens de entrada+salida de ESTE turno según el proveedor (0 si no
    /// los informa).
    pub tokens: u64,
}

/// Resultado de una herramienta, para devolver al modelo.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    pub id: String,
    pub name: String,
    pub content: String,
    pub is_error: bool,
}

pub trait AgentProvider {
    fn name(&self) -> String;
    fn model(&self) -> String;
    /// Primer turno: prompt de sistema, objetivo y herramientas.
    fn start(&mut self, system: &str, user: &str, tools: &[ToolSpec]) -> Result<Turn>;
    /// Turnos siguientes: los resultados de TODAS las herramientas del turno
    /// anterior, en un solo mensaje.
    fn continue_with(&mut self, results: &[ToolResult], tools: &[ToolSpec]) -> Result<Turn>;
}

fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(600)))
        .build()
        .into()
}

fn read_body(resp: &mut ureq::http::Response<ureq::Body>, who: &str) -> Result<Value> {
    let status = resp.status().as_u16();
    let text = resp
        .body_mut()
        .read_to_string()
        .with_context(|| format!("leyendo la respuesta de {who}"))?;
    if !(200..300).contains(&status) {
        bail!(
            "{who} devolvió HTTP {status}: {}",
            text.chars().take(600).collect::<String>()
        );
    }
    serde_json::from_str(&text).with_context(|| format!("la respuesta de {who} no es JSON"))
}

// ───────────────────────────── Claude ─────────────────────────────

pub struct ClaudeAgentProvider {
    api_key: String,
    model: String,
    system: String,
    messages: Vec<Value>,
}

impl ClaudeAgentProvider {
    pub const URL: &'static str = "https://api.anthropic.com/v1/messages";

    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            system: String::new(),
            messages: Vec::new(),
        }
    }

    fn request(&mut self, tools: &[ToolSpec]) -> Result<Turn> {
        let tool_defs: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })
            })
            .collect();
        let body = json!({
            "model": self.model,
            "max_tokens": 16000,
            "system": self.system,
            "tools": tool_defs,
            "fallbacks": "default",
            "messages": self.messages,
        });
        let mut resp = http_agent()
            .post(Self::URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("anthropic-beta", "server-side-fallback-2026-07-01")
            .header("content-type", "application/json")
            .send_json(&body)
            .context("no pude contactar con la API de Anthropic")?;
        let v = read_body(&mut resp, "Anthropic")?;
        if v.get("stop_reason").and_then(Value::as_str) == Some("refusal") {
            bail!(
                "el modelo rechazó continuar (refusal): {}",
                v.pointer("/stop_details/explanation")
                    .and_then(Value::as_str)
                    .unwrap_or("sin explicación")
            );
        }
        let content = v
            .get("content")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        // El contenido del asistente se guarda tal cual llegó: los bloques
        // `thinking` deben volver intactos en el siguiente turno.
        self.messages
            .push(json!({"role": "assistant", "content": content}));

        let mut turn = Turn::default();
        for block in content.as_array().into_iter().flatten() {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(t) = block.get("text").and_then(Value::as_str) {
                        turn.text.push_str(t);
                    }
                }
                Some("tool_use") => turn.calls.push(ToolCall {
                    id: block
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    name: block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    input: block.get("input").cloned().unwrap_or(Value::Null),
                }),
                _ => {}
            }
        }
        turn.tokens = v
            .pointer("/usage/input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            + v.pointer("/usage/output_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0);
        Ok(turn)
    }
}

impl AgentProvider for ClaudeAgentProvider {
    fn name(&self) -> String {
        "claude".into()
    }
    fn model(&self) -> String {
        self.model.clone()
    }
    fn start(&mut self, system: &str, user: &str, tools: &[ToolSpec]) -> Result<Turn> {
        self.system = system.to_string();
        self.messages = vec![json!({"role": "user", "content": user})];
        self.request(tools)
    }
    fn continue_with(&mut self, results: &[ToolResult], tools: &[ToolSpec]) -> Result<Turn> {
        let blocks: Vec<Value> = results
            .iter()
            .map(|r| {
                json!({
                    "type": "tool_result",
                    "tool_use_id": r.id,
                    "content": r.content,
                    "is_error": r.is_error,
                })
            })
            .collect();
        self.messages
            .push(json!({"role": "user", "content": blocks}));
        self.request(tools)
    }
}

// ───────────────────────────── Ollama ─────────────────────────────

pub struct OllamaAgentProvider {
    endpoint: String,
    model: String,
    messages: Vec<Value>,
    next_id: u32,
}

impl OllamaAgentProvider {
    pub fn new(endpoint: String, model: String) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            model,
            messages: Vec::new(),
            next_id: 0,
        }
    }

    fn request(&mut self, tools: &[ToolSpec]) -> Result<Turn> {
        let tool_defs: Vec<Value> = tools.iter().map(openai_style_tool).collect();
        let body = json!({
            "model": self.model,
            "messages": self.messages,
            "tools": tool_defs,
            "stream": false,
        });
        let url = format!("{}/api/chat", self.endpoint);
        let mut resp = http_agent()
            .post(&url)
            .header("content-type", "application/json")
            .send_json(&body)
            .with_context(|| format!("no pude contactar con Ollama en {url}"))?;
        let v = read_body(&mut resp, "Ollama")?;
        let message = v.get("message").cloned().unwrap_or(Value::Null);
        self.messages.push(message.clone());
        let mut turn = Turn {
            text: message
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            ..Default::default()
        };
        for call in message
            .get("tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            self.next_id += 1;
            turn.calls.push(ToolCall {
                id: format!("call-{}", self.next_id),
                name: call
                    .pointer("/function/name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                input: call
                    .pointer("/function/arguments")
                    .cloned()
                    .unwrap_or(Value::Null),
            });
        }
        turn.tokens = v
            .get("prompt_eval_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            + v.get("eval_count").and_then(Value::as_u64).unwrap_or(0);
        Ok(turn)
    }
}

impl AgentProvider for OllamaAgentProvider {
    fn name(&self) -> String {
        "ollama".into()
    }
    fn model(&self) -> String {
        self.model.clone()
    }
    fn start(&mut self, system: &str, user: &str, tools: &[ToolSpec]) -> Result<Turn> {
        self.messages = vec![
            json!({"role": "system", "content": system}),
            json!({"role": "user", "content": user}),
        ];
        self.request(tools)
    }
    fn continue_with(&mut self, results: &[ToolResult], tools: &[ToolSpec]) -> Result<Turn> {
        for r in results {
            self.messages.push(json!({
                "role": "tool",
                "tool_name": r.name,
                "content": if r.is_error { format!("ERROR: {}", r.content) } else { r.content.clone() },
            }));
        }
        self.request(tools)
    }
}

// ───────────────────────── OpenAI-compatible ──────────────────────

pub struct OpenAiCompatAgentProvider {
    provider_id: String,
    endpoint: String,
    model: String,
    api_key: Option<String>,
    custom_headers: Vec<(String, String)>,
    messages: Vec<Value>,
}

impl OpenAiCompatAgentProvider {
    pub fn new(
        provider_id: String,
        endpoint: String,
        model: String,
        api_key: Option<String>,
        custom_headers: Vec<(String, String)>,
    ) -> Self {
        Self {
            provider_id,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            model,
            api_key,
            custom_headers,
            messages: Vec::new(),
        }
    }

    fn request(&mut self, tools: &[ToolSpec]) -> Result<Turn> {
        let tool_defs: Vec<Value> = tools.iter().map(openai_style_tool).collect();
        let body = json!({
            "model": self.model,
            "messages": self.messages,
            "tools": tool_defs,
            "tool_choice": "auto",
        });
        let url = format!("{}/chat/completions", self.endpoint);
        let mut req = http_agent()
            .post(&url)
            .header("content-type", "application/json");
        if let Some(k) = &self.api_key {
            req = req.header("authorization", &format!("Bearer {k}"));
        }
        for (k, v) in &self.custom_headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let mut resp = req
            .send_json(&body)
            .with_context(|| format!("no pude contactar con {} en {url}", self.provider_id))?;
        let v = read_body(&mut resp, &self.provider_id)?;
        let message = v
            .pointer("/choices/0/message")
            .cloned()
            .unwrap_or(Value::Null);
        self.messages.push(message.clone());
        let mut turn = Turn {
            text: message
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            ..Default::default()
        };
        for call in message
            .get("tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let raw_args = call
                .pointer("/function/arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            let input = serde_json::from_str(raw_args).unwrap_or(Value::Null);
            turn.calls.push(ToolCall {
                id: call
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                name: call
                    .pointer("/function/name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                input,
            });
        }
        turn.tokens = v
            .pointer("/usage/total_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        Ok(turn)
    }
}

impl AgentProvider for OpenAiCompatAgentProvider {
    fn name(&self) -> String {
        self.provider_id.clone()
    }
    fn model(&self) -> String {
        self.model.clone()
    }
    fn start(&mut self, system: &str, user: &str, tools: &[ToolSpec]) -> Result<Turn> {
        self.messages = vec![
            json!({"role": "system", "content": system}),
            json!({"role": "user", "content": user}),
        ];
        self.request(tools)
    }
    fn continue_with(&mut self, results: &[ToolResult], tools: &[ToolSpec]) -> Result<Turn> {
        for r in results {
            self.messages.push(json!({
                "role": "tool",
                "tool_call_id": r.id,
                "content": if r.is_error { format!("ERROR: {}", r.content) } else { r.content.clone() },
            }));
        }
        self.request(tools)
    }
}

/// Formato `function` que comparten Ollama y las APIs compatibles con OpenAI.
fn openai_style_tool(t: &ToolSpec) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": t.name,
            "description": t.description,
            "parameters": t.input_schema,
        }
    })
}

// ───────────────────────── Resolución ─────────────────────────

/// Construye el proveedor a partir de una especificación `proveedor` o
/// `proveedor:modelo` (`claude`, `ollama`, `groq`, `openrouter`, `gemini`,
/// `opencode`, `openai`, `fake`), o del proveedor activo de
/// `llm_config.json` si no se indica ninguno. Reutiliza los constructores
/// de los planificadores para leer claves y endpoints.
pub fn resolve(state_dir: &std::path::Path, spec: Option<&str>) -> Result<Box<dyn AgentProvider>> {
    let config = crate::llm::LlmConfig::load_from_state(state_dir);
    let spec = match spec {
        Some(s) => s.to_string(),
        None => {
            if config.active_provider == "auto" || config.active_provider == "local" {
                bail!(
                    "no hay proveedor de modelo activo para un agente: elige uno con \
                     `antos llm use <proveedor>` o pásalo con --provider (claude, ollama, \
                     groq, openrouter, gemini, opencode, openai)"
                );
            }
            config.active_provider.clone()
        }
    };
    let (provider, model_override) = match spec.split_once(':') {
        Some((p, m)) if !m.is_empty() => (p.to_string(), Some(m.to_string())),
        _ => (spec.clone(), None),
    };
    let settings = config.get_provider_settings(&provider);
    let model_override = model_override.or_else(|| settings.and_then(|s| s.model.clone()));
    let endpoint_override = settings.and_then(|s| s.endpoint.clone());

    match provider.as_str() {
        "claude" => {
            let planner = crate::planner::claude::ClaudePlanner::from_env()?;
            let (key, model) = planner.credentials();
            Ok(Box::new(ClaudeAgentProvider::new(
                key.to_string(),
                model_override.unwrap_or_else(|| model.to_string()),
            )))
        }
        "ollama" | "local-llm" | "local_llm" => {
            let p = crate::planner::ollama::OllamaPlanner::from_env()?;
            Ok(Box::new(OllamaAgentProvider::new(
                endpoint_override.unwrap_or(p.endpoint),
                model_override.unwrap_or(p.model),
            )))
        }
        "groq" | "openrouter" | "gemini" | "opencode" | "openai" => {
            let p = crate::planner::openai_compat::OpenAiCompatPlanner::from_preset(&provider)?;
            Ok(Box::new(OpenAiCompatAgentProvider::new(
                p.provider_id,
                endpoint_override.unwrap_or(p.endpoint),
                model_override.unwrap_or(p.model),
                p.api_key,
                p.custom_headers,
            )))
        }
        "fake" => {
            let path = std::env::var_os("ANTOS_AGENT_FAKE_SCRIPT").ok_or_else(|| {
                anyhow::anyhow!("el proveedor `fake` necesita ANTOS_AGENT_FAKE_SCRIPT=<guion.json>")
            })?;
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("leyendo el guion {}", path.to_string_lossy()))?;
            Ok(Box::new(super::fake::FakeProvider::from_json(&text)?))
        }
        other => bail!("proveedor de agente desconocido: {other}"),
    }
}
