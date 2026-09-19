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
    /// Un mensaje de usuario sin resultados de herramienta: el recordatorio
    /// que el runtime envía cuando el modelo responde en prosa sin llamar a
    /// nada (T33.5, modelos locales pequeños).
    fn nudge(&mut self, text: &str, tools: &[ToolSpec]) -> Result<Turn>;
    /// Ventana de contexto efectiva que se está pidiendo al modelo, si el
    /// proveedor la controla (Ollama, T34.2). `None` = la decide el servicio.
    fn context_window(&self) -> Option<u32> {
        None
    }
    /// Por qué la ventana efectiva no es la pedida, si se acotó.
    fn context_note(&self) -> Option<String> {
        None
    }
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
    fn nudge(&mut self, text: &str, tools: &[ToolSpec]) -> Result<Turn> {
        self.messages.push(json!({"role": "user", "content": text}));
        self.request(tools)
    }
}

// ───────────────────────────── Ollama ─────────────────────────────

/// Parámetros que Ollama no fija solo (T34.2). Sin `num_ctx` usa 4096 y
/// recorta la conversación por el principio; sin `keep_alive` descarga el
/// modelo a los 5 min y el siguiente paso paga la carga otra vez.
#[derive(Debug, Clone, PartialEq)]
pub struct OllamaOptions {
    pub num_ctx: u32,
    pub temperature: f32,
    pub keep_alive: String,
    /// Si `num_ctx` se acotó respecto a lo pedido, por qué.
    pub context_note: Option<String>,
}

impl Default for OllamaOptions {
    fn default() -> Self {
        Self {
            num_ctx: crate::llm::DEFAULT_OLLAMA_NUM_CTX,
            temperature: crate::llm::DEFAULT_OLLAMA_TEMPERATURE,
            keep_alive: crate::llm::DEFAULT_OLLAMA_KEEP_ALIVE.to_string(),
            context_note: None,
        }
    }
}

pub struct OllamaAgentProvider {
    endpoint: String,
    model: String,
    options: OllamaOptions,
    messages: Vec<Value>,
    next_id: u32,
}

impl OllamaAgentProvider {
    pub fn new(endpoint: String, model: String) -> Self {
        Self::with_options(endpoint, model, OllamaOptions::default())
    }

    pub fn with_options(endpoint: String, model: String, options: OllamaOptions) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            model,
            options,
            messages: Vec::new(),
            next_id: 0,
        }
    }

    /// El cuerpo de `/api/chat`. Separado para poder comprobar en un test
    /// que `options` y `keep_alive` van siempre.
    pub(crate) fn request_body(&self, tools: &[ToolSpec]) -> Value {
        let tool_defs: Vec<Value> = tools.iter().map(openai_style_tool).collect();
        json!({
            "model": self.model,
            "messages": self.messages,
            "tools": tool_defs,
            "stream": false,
            "keep_alive": self.options.keep_alive,
            "options": {
                "num_ctx": self.options.num_ctx,
                "temperature": self.options.temperature,
            },
        })
    }

    fn request(&mut self, tools: &[ToolSpec]) -> Result<Turn> {
        let body = self.request_body(tools);
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
        // Modelos locales pequeños (7B) a veces escriben la llamada como
        // texto en vez de usar `tool_calls`: se rescata del texto. Lo que
        // salga sigue pasando por el catálogo igual que una llamada nativa.
        if turn.calls.is_empty() {
            for (name, input) in textual_tool_calls(&turn.text) {
                self.next_id += 1;
                turn.calls.push(ToolCall {
                    id: format!("call-{}", self.next_id),
                    name,
                    input,
                });
            }
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
    fn nudge(&mut self, text: &str, tools: &[ToolSpec]) -> Result<Turn> {
        self.messages.push(json!({"role": "user", "content": text}));
        self.request(tools)
    }
    fn context_window(&self) -> Option<u32> {
        Some(self.options.num_ctx)
    }
    fn context_note(&self) -> Option<String> {
        self.options.context_note.clone()
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
        if turn.calls.is_empty() {
            for (i, (name, input)) in textual_tool_calls(&turn.text).into_iter().enumerate() {
                turn.calls.push(ToolCall {
                    id: format!("text-{}-{i}", self.messages.len()),
                    name,
                    input,
                });
            }
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
    fn nudge(&mut self, text: &str, tools: &[ToolSpec]) -> Result<Turn> {
        self.messages.push(json!({"role": "user", "content": text}));
        self.request(tools)
    }
}

/// Llamadas a herramienta escritas como texto por el modelo: bloques
/// ```` ```json ```` (o JSON a pelo) con la forma `{"name": …,
/// "arguments": {…}}` — también `parameters`/`input`, `tool`/`function`
/// como clave del nombre, y arrays de ellas. Todo lo demás se ignora.
pub(crate) fn textual_tool_calls(text: &str) -> Vec<(String, Value)> {
    fn one(v: &Value) -> Option<(String, Value)> {
        let obj = v.as_object()?;
        let name = ["name", "tool", "function"]
            .iter()
            .find_map(|k| obj.get(*k).and_then(Value::as_str))?
            .to_string();
        let input = ["arguments", "parameters", "input", "args"]
            .iter()
            .find_map(|k| obj.get(*k))
            .cloned()
            .unwrap_or_else(|| json!({}));
        let input = match input {
            // Algunos modelos serializan los argumentos como cadena JSON.
            Value::String(s) => serde_json::from_str(&s).unwrap_or(Value::String(s)),
            other => other,
        };
        Some((name, input))
    }
    let mut out = Vec::new();
    let mut candidates: Vec<String> = Vec::new();
    // 1 · bloques de código
    let mut rest = text;
    while let Some(start) = rest.find("```") {
        let after = &rest[start + 3..];
        let body_start = after.find('\n').map(|i| i + 1).unwrap_or(0);
        let Some(end) = after[body_start..].find("```") else {
            break;
        };
        candidates.push(after[body_start..body_start + end].trim().to_string());
        rest = &after[body_start + end + 3..];
    }
    // 2 · el texto entero, por si es JSON a pelo
    candidates.push(text.trim().to_string());
    for c in candidates {
        let Ok(v) = serde_json::from_str::<Value>(&c) else {
            continue;
        };
        match &v {
            Value::Array(items) => out.extend(items.iter().filter_map(one)),
            other => out.extend(one(other)),
        }
        if !out.is_empty() {
            break;
        }
    }
    out
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
    resolve_for_role(state_dir, spec, None)
}

/// Endpoint de Ollama a usar cuando no hay uno explícito: el registrado por
/// `antos service up` (T34.1) manda sobre el del entorno/por defecto.
fn ollama_endpoint(state_dir: &std::path::Path, explicit: Option<String>) -> String {
    explicit
        .or_else(|| crate::service::registered_endpoint(state_dir, "ollama"))
        .or_else(|| {
            crate::planner::ollama::OllamaPlanner::from_env()
                .ok()
                .map(|p| p.endpoint)
        })
        .unwrap_or_else(|| crate::llm::ollama_api::DEFAULT_ENDPOINT.to_string())
}

/// Con `active_provider = auto`: el Ollama local si responde y tiene un
/// modelo con `tools`. Devuelve `(endpoint, modelo)`; `None` si no hay nada
/// utilizable (y entonces `auto` falla con un mensaje que apunta a `setup`).
pub fn local_ollama_candidate(
    state_dir: &std::path::Path,
    config: &crate::llm::LlmConfig,
) -> Option<(String, String)> {
    use crate::llm::ollama_api::OllamaClient;
    let endpoint = ollama_endpoint(
        state_dir,
        config
            .get_provider_settings("ollama")
            .and_then(|s| s.endpoint.clone()),
    );
    let client = OllamaClient::new(endpoint.clone());
    if !client.is_available() {
        return None;
    }
    let tags = client.tags().ok()?;
    let capable = |name: &str| -> bool {
        client
            .show(name)
            .ok()
            .and_then(|s| s.supports_tools())
            .unwrap_or(true)
    };
    // 1. El modelo configurado para Ollama, si está descargado y sirve.
    if let Some(m) = config
        .get_provider_settings("ollama")
        .and_then(|s| s.model.clone())
    {
        if tags.iter().any(|t| t.name == m) && capable(&m) {
            return Some((endpoint, m));
        }
    }
    // 2. El recomendado por RAM, si está descargado.
    let table = crate::llm::doctor::ModelTable::load(None);
    let recommended = crate::llm::doctor::SystemResources::detect()
        .and_then(|r| table.tier_for(r.total_ram_gb()).map(|t| t.code.clone()));
    if let Some(rec) = recommended {
        if tags.iter().any(|t| t.name == rec) && capable(&rec) {
            return Some((endpoint, rec));
        }
    }
    // 3. El primero que declare `tools` (o del que no se sepa).
    tags.iter()
        .find(|t| capable(&t.name))
        .map(|t| (endpoint, t.name.clone()))
}

/// Construye el proveedor de Ollama con el contexto efectivo (T34.2):
/// pedido (rol → proveedor → 16k) acotado al máximo del modelo y al techo
/// del tier de RAM. Rechaza un modelo que declara no soportar `tools`.
fn ollama_provider(
    config: &crate::llm::LlmConfig,
    role: Option<&str>,
    endpoint: String,
    model: String,
) -> Result<OllamaAgentProvider> {
    use crate::llm::doctor::{effective_num_ctx, ModelTable, SystemResources};
    use crate::llm::ollama_api::OllamaClient;

    let settings = config.get_provider_settings("ollama");
    let requested = config.requested_num_ctx(role);
    let client = OllamaClient::new(endpoint.clone());
    let mut model_max = None;
    if client.is_available() {
        match client.show(&model) {
            Ok(show) => {
                if show.supports_tools() == Some(false) {
                    let alternative = client
                        .tags()
                        .unwrap_or_default()
                        .into_iter()
                        .find(|t| {
                            t.name != model
                                && client
                                    .show(&t.name)
                                    .ok()
                                    .and_then(|s| s.supports_tools())
                                    .unwrap_or(false)
                        })
                        .map(|t| format!(" Descargado con `tools`: {}.", t.name))
                        .unwrap_or_else(|| {
                            " Descarga uno con `antos llm pull` (p. ej. qwen2.5-coder:7b)."
                                .to_string()
                        });
                    bail!(
                        "el modelo «{model}» no soporta llamadas a herramienta y un agente \
                         no puede usarlo.{alternative}"
                    );
                }
                model_max = show.context_length;
            }
            Err(e) => bail!(
                "Ollama responde en {endpoint} pero no conoce «{model}»: {e:#}\n  \
                 Descárgalo con `antos llm pull {model}` o elige otro con `antos llm list`."
            ),
        }
    }
    let ram_cap = SystemResources::detect().and_then(|r| {
        ModelTable::load(None)
            .tier_for(r.total_ram_gb())
            .map(|t| t.max_num_ctx)
    });
    let (num_ctx, context_note) = effective_num_ctx(requested, model_max, ram_cap);
    let options = OllamaOptions {
        num_ctx,
        temperature: settings
            .and_then(|s| s.temperature)
            .unwrap_or(crate::llm::DEFAULT_OLLAMA_TEMPERATURE),
        keep_alive: settings
            .and_then(|s| s.keep_alive.clone())
            .unwrap_or_else(|| crate::llm::DEFAULT_OLLAMA_KEEP_ALIVE.to_string()),
        context_note,
    };
    Ok(OllamaAgentProvider::with_options(endpoint, model, options))
}

/// Como `resolve`, con el rol (architect, coder, qa, auditor) para aplicar
/// su ventana de contexto (`LlmConfig::role_num_ctx`).
pub fn resolve_for_role(
    state_dir: &std::path::Path,
    spec: Option<&str>,
    role: Option<&str>,
) -> Result<Box<dyn AgentProvider>> {
    let config = crate::llm::LlmConfig::load_from_state(state_dir);
    let no_active = config.active_provider == "auto" || config.active_provider == "local";
    let mut auto_endpoint = None;
    let spec = match spec {
        Some(s) => s.to_string(),
        // Afordancia de pruebas (smoke de la barra, T33.4): con un guion
        // `fake` en el entorno y sin proveedor activo, el agente lo usa.
        // Explícito por variable de entorno; nunca por defecto.
        None if no_active && std::env::var_os("ANTOS_AGENT_FAKE_SCRIPT").is_some() => {
            "fake".to_string()
        }
        // `auto` (T34.2): local primero. Nunca se llama a un proveedor de
        // red sin que el usuario lo haya elegido con `antos llm use`.
        None if no_active => match local_ollama_candidate(state_dir, &config) {
            Some((endpoint, model)) => {
                auto_endpoint = Some(endpoint);
                format!("ollama:{model}")
            }
            None => bail!(
                "no hay proveedor de modelo activo para un agente y no hay un Ollama local \
                 con un modelo que soporte herramientas: ejecuta `antos llm setup`, elige \
                 uno con `antos llm use <proveedor>` o pásalo con --provider (claude, \
                 ollama, groq, openrouter, gemini, opencode, openai)"
            ),
        },
        None => config.active_provider.clone(),
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
            // Prioridad: `auto` ya resolvió uno → configuración explícita →
            // el Ollama que registró `antos service up` (T34.1) → entorno.
            let endpoint = ollama_endpoint(state_dir, auto_endpoint.or(endpoint_override));
            Ok(Box::new(ollama_provider(
                &config,
                role,
                endpoint,
                model_override.unwrap_or(p.model),
            )?))
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn textual_tool_calls_are_recovered_from_code_blocks_and_bare_json() {
        let fenced = "Voy a ejecutar los tests.\n```json\n{\"name\": \"test.run\", \"arguments\": {\"path\": \".\"}}\n```";
        let calls = textual_tool_calls(fenced);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "test.run");
        assert_eq!(calls[0].1["path"], ".");

        let bare = r#"{"tool": "fs.read", "input": {"path": "src/lib.rs"}}"#;
        assert_eq!(textual_tool_calls(bare)[0].0, "fs.read");

        let array = r#"[{"name": "fs.list", "parameters": {"path": "."}}, {"function": "fs.read", "args": "{\"path\": \"a\"}"}]"#;
        let calls = textual_tool_calls(array);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].1["path"], "a");

        assert!(textual_tool_calls("solo prosa, sin JSON").is_empty());
        assert!(textual_tool_calls("```rust\nfn x() {}\n```").is_empty());
    }

    /// T34.2: el cuerpo de `/api/chat` lleva SIEMPRE `options.num_ctx`,
    /// `options.temperature` y `keep_alive`. Sin ellos Ollama usa 4096 y
    /// recorta el prompt de sistema por el principio.
    #[test]
    fn ollama_request_body_always_carries_context_options() {
        let p = OllamaAgentProvider::new("http://127.0.0.1:1".into(), "m".into());
        let body = p.request_body(&[]);
        assert_eq!(
            body["options"]["num_ctx"],
            crate::llm::DEFAULT_OLLAMA_NUM_CTX
        );
        assert_eq!(
            body["options"]["temperature"],
            crate::llm::DEFAULT_OLLAMA_TEMPERATURE
        );
        assert_eq!(body["keep_alive"], crate::llm::DEFAULT_OLLAMA_KEEP_ALIVE);
        assert_eq!(body["stream"], false);

        let custom = OllamaAgentProvider::with_options(
            "http://127.0.0.1:1".into(),
            "m".into(),
            OllamaOptions {
                num_ctx: 8192,
                temperature: 0.3,
                keep_alive: "1h".into(),
                context_note: Some("acotado".into()),
            },
        );
        let body = custom.request_body(&[]);
        assert_eq!(body["options"]["num_ctx"], 8192);
        assert_eq!(body["keep_alive"], "1h");
        assert_eq!(custom.context_window(), Some(8192));
        assert_eq!(custom.context_note().as_deref(), Some("acotado"));
    }

    // ------------------------------------------------ Ollama simulado

    /// Un Ollama de mentira en loopback: responde `/api/tags` y `/api/show`
    /// con lo que se le diga. Suficiente para probar `resolve` sin red.
    fn fake_ollama(
        models: Vec<(&'static str, Vec<&'static str>, u64)>,
    ) -> (u16, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(false).unwrap();
        let handle = std::thread::spawn(move || {
            // Atiende peticiones hasta que el test termine (el hilo muere con
            // el proceso de tests); cada conexión es una petición.
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                // Cabeceras y cuerpo pueden llegar en segmentos distintos:
                // se lee hasta tener `content-length` bytes de cuerpo.
                let mut raw = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = stream.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    raw.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&raw).to_string();
                    if let Some(idx) = text.find("\r\n\r\n") {
                        let want: usize = text
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .and_then(|v| v.trim().parse().ok())
                            })
                            .unwrap_or(0);
                        if raw.len() - (idx + 4) >= want {
                            break;
                        }
                    }
                }
                let req = String::from_utf8_lossy(&raw).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("");
                let body = if path.starts_with("/api/tags") {
                    let list: Vec<Value> = models
                        .iter()
                        .map(|(name, _, ctx)| json!({"name": name, "size": ctx * 1000, "details": {}}))
                        .collect();
                    json!({"models": list}).to_string()
                } else if path.starts_with("/api/show") {
                    let wanted = req
                        .rsplit("\r\n\r\n")
                        .next()
                        .and_then(|b| serde_json::from_str::<Value>(b.trim_end_matches('\0')).ok())
                        .and_then(|v| v["model"].as_str().map(str::to_string))
                        .unwrap_or_default();
                    match models.iter().find(|(name, _, _)| *name == wanted) {
                        Some((_, caps, ctx)) => json!({
                            "capabilities": caps,
                            "model_info": {"qwen2.context_length": ctx}
                        })
                        .to_string(),
                        None => {
                            let _ = stream.write_all(
                                b"HTTP/1.1 404 Not Found\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}",
                            );
                            continue;
                        }
                    }
                } else {
                    "{}".to_string()
                };
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                );
            }
        });
        (port, handle)
    }

    fn state_with_ollama(port: u16, config: &crate::llm::LlmConfig) -> std::path::PathBuf {
        // Contador atómico además del reloj: varios tests arrancan en el
        // mismo microsegundo y compartían directorio (y se pisaban el
        // `llm_config.json` a medio escribir).
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let state = std::env::temp_dir().join(format!(
            "antos_resolve_{}_{seq}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&state).unwrap();
        let mut config = config.clone();
        config
            .providers
            .entry("ollama".into())
            .or_default()
            .endpoint = Some(format!("http://127.0.0.1:{port}"));
        config.save_to_state(&state).unwrap();
        state
    }

    #[test]
    fn auto_resolves_to_a_local_model_with_tools_and_caps_the_context() {
        let (port, _srv) = fake_ollama(vec![
            ("chat-only:latest", vec!["completion"], 32_768),
            ("coder:7b", vec!["completion", "tools"], 8_192),
        ]);
        let mut config = crate::llm::LlmConfig {
            active_provider: "auto".into(),
            ..Default::default()
        };
        // El modelo configurado no está descargado: `auto` debe saltárselo.
        config.providers.get_mut("ollama").unwrap().model = Some("no-existe".into());
        let state = state_with_ollama(port, &config);

        let p = resolve(&state, None).expect("auto debe resolver al Ollama local");
        assert_eq!(p.name(), "ollama");
        assert_eq!(p.model(), "coder:7b", "salta el que no tiene tools");
        // 16k pedidos, el modelo admite 8k: se acota y se explica.
        assert_eq!(p.context_window(), Some(8_192));
        assert!(p.context_note().unwrap().contains("el modelo admite 8192"));
        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn a_model_without_tools_is_rejected_with_an_alternative() {
        let (port, _srv) = fake_ollama(vec![
            ("chat-only:latest", vec!["completion"], 32_768),
            ("coder:7b", vec!["completion", "tools"], 32_768),
        ]);
        let state = state_with_ollama(port, &crate::llm::LlmConfig::default());
        let msg = match resolve(&state, Some("ollama:chat-only:latest")) {
            Ok(_) => panic!("un modelo sin tools no debe resolver"),
            Err(e) => format!("{e:#}"),
        };
        assert!(msg.contains("no soporta llamadas a herramienta"), "{msg}");
        assert!(msg.contains("coder:7b"), "sugiere el que sí: {msg}");

        let msg = match resolve(&state, Some("ollama:inexistente")) {
            Ok(_) => panic!("un modelo no descargado no debe resolver"),
            Err(e) => format!("{e:#}"),
        };
        assert!(msg.contains("antos llm pull inexistente"), "{msg}");
        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn role_context_overrides_provider_context() {
        let (port, _srv) = fake_ollama(vec![("coder:7b", vec!["tools"], 131_072)]);
        let mut config = crate::llm::LlmConfig::default();
        config.providers.get_mut("ollama").unwrap().num_ctx = Some(12_000);
        config.set_role_num_ctx("architect", 6_000);
        let state = state_with_ollama(port, &config);

        let coder = resolve_for_role(&state, Some("ollama:coder:7b"), Some("coder")).unwrap();
        let architect =
            resolve_for_role(&state, Some("ollama:coder:7b"), Some("architect")).unwrap();
        assert_eq!(coder.context_window(), Some(12_000));
        assert_eq!(architect.context_window(), Some(6_000));
        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn auto_without_a_usable_local_model_points_to_setup() {
        let (port, _srv) = fake_ollama(vec![("chat-only:latest", vec!["completion"], 4_096)]);
        let config = crate::llm::LlmConfig {
            active_provider: "auto".into(),
            ..Default::default()
        };
        let state = state_with_ollama(port, &config);
        // Sin guion `fake` en el entorno este test no puede confundirse con
        // la afordancia de la barra.
        if std::env::var_os("ANTOS_AGENT_FAKE_SCRIPT").is_some() {
            return;
        }
        let msg = match resolve(&state, None) {
            Ok(_) => panic!("sin modelo con tools, auto no debe resolver"),
            Err(e) => format!("{e:#}"),
        };
        assert!(msg.contains("antos llm setup"), "{msg}");
        let _ = std::fs::remove_dir_all(&state);
    }
}
