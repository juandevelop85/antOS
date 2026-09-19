//! Cliente HTTP de la API nativa de Ollama (T34.2), compartido por el
//! planificador, el proveedor de agente y `antos llm`.
//!
//! Sin SDK: `ureq` bloqueante como el resto del demonio. Solo habla con el
//! demonio de Ollama en loopback; la descarga de modelos la hace el propio
//! Ollama, aquí solo se sigue su progreso.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::BufRead;
use std::time::Duration;

pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:11434";

#[derive(Debug, Clone)]
pub struct OllamaClient {
    endpoint: String,
}

/// Un modelo descargado, tal como lo lista `/api/tags`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelTag {
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub details: ModelDetails,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModelDetails {
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub parameter_size: String,
    #[serde(default)]
    pub quantization_level: String,
}

/// Lo que `/api/show` cuenta de un modelo y a antOS le importa.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModelShow {
    /// `capabilities` (Ollama ≥ 0.6). `None` si el servidor no lo informa:
    /// no se sabe, que no es lo mismo que «no soporta».
    pub capabilities: Option<Vec<String>>,
    /// `model_info["<arquitectura>.context_length"]`.
    pub context_length: Option<u32>,
    pub details: ModelDetails,
}

impl ModelShow {
    /// `Some(true/false)` si el servidor informa capacidades; `None` si no.
    pub fn supports_tools(&self) -> Option<bool> {
        self.capabilities
            .as_ref()
            .map(|c| c.iter().any(|x| x == "tools"))
    }
}

/// Un modelo cargado en memoria (`/api/ps`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunningModel {
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub size_vram: u64,
}

/// Una línea del stream de `/api/pull`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
pub struct PullProgress {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub completed: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .into()
}

fn read_json(resp: &mut ureq::http::Response<ureq::Body>, what: &str) -> Result<Value> {
    let status = resp.status().as_u16();
    let text = resp
        .body_mut()
        .read_to_string()
        .with_context(|| format!("leyendo la respuesta de Ollama a {what}"))?;
    if !(200..300).contains(&status) {
        bail!(
            "Ollama devolvió HTTP {status} a {what}: {}",
            text.chars().take(300).collect::<String>()
        );
    }
    serde_json::from_str(&text).with_context(|| format!("respuesta de Ollama a {what} ilegible"))
}

impl OllamaClient {
    pub fn new(endpoint: impl Into<String>) -> Self {
        let raw: String = endpoint.into();
        // `OLLAMA_HOST` nativo es `host:puerto` sin esquema.
        let with_scheme = if raw.starts_with("http://") || raw.starts_with("https://") {
            raw
        } else {
            format!("http://{raw}")
        };
        Self {
            endpoint: with_scheme.trim_end_matches('/').to_string(),
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.endpoint)
    }

    /// `GET /api/tags` responde 2xx.
    pub fn is_available(&self) -> bool {
        agent(Duration::from_millis(1500))
            .get(self.url("/api/tags"))
            .call()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    pub fn version(&self) -> Option<String> {
        let mut resp = agent(Duration::from_secs(2))
            .get(self.url("/api/version"))
            .call()
            .ok()?;
        let v = read_json(&mut resp, "/api/version").ok()?;
        v.get("version").and_then(Value::as_str).map(str::to_string)
    }

    pub fn tags(&self) -> Result<Vec<ModelTag>> {
        let mut resp = agent(Duration::from_secs(5))
            .get(self.url("/api/tags"))
            .call()
            .with_context(|| format!("no pude contactar con Ollama en {}", self.endpoint))?;
        let v = read_json(&mut resp, "/api/tags")?;
        let models = v.get("models").cloned().unwrap_or(Value::Array(Vec::new()));
        serde_json::from_value(models).context("lista de modelos de Ollama ilegible")
    }

    pub fn show(&self, model: &str) -> Result<ModelShow> {
        let mut resp = agent(Duration::from_secs(10))
            .post(self.url("/api/show"))
            .header("content-type", "application/json")
            .send_json(json!({ "model": model }))
            .with_context(|| format!("no pude contactar con Ollama en {}", self.endpoint))?;
        let v = read_json(&mut resp, "/api/show")?;
        Ok(parse_show(&v))
    }

    pub fn ps(&self) -> Result<Vec<RunningModel>> {
        let mut resp = agent(Duration::from_secs(5))
            .get(self.url("/api/ps"))
            .call()
            .with_context(|| format!("no pude contactar con Ollama en {}", self.endpoint))?;
        let v = read_json(&mut resp, "/api/ps")?;
        let models = v.get("models").cloned().unwrap_or(Value::Array(Vec::new()));
        serde_json::from_value(models).context("lista de modelos cargados ilegible")
    }

    /// `POST /api/pull` en streaming; `on_progress` recibe cada línea. La
    /// descarga la hace Ollama: esto puede tardar minutos con modelos de GB.
    pub fn pull(&self, model: &str, on_progress: &mut dyn FnMut(&PullProgress)) -> Result<()> {
        let mut resp = agent(Duration::from_secs(6 * 60 * 60))
            .post(self.url("/api/pull"))
            .header("content-type", "application/json")
            .send_json(json!({ "model": model, "stream": true }))
            .with_context(|| format!("no pude contactar con Ollama en {}", self.endpoint))?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            let text = resp.body_mut().read_to_string().unwrap_or_default();
            bail!(
                "Ollama devolvió HTTP {status} a /api/pull: {}",
                text.chars().take(300).collect::<String>()
            );
        }
        let reader = std::io::BufReader::new(resp.body_mut().as_reader());
        for line in reader.lines() {
            let line = line.context("leyendo el progreso de la descarga")?;
            if line.trim().is_empty() {
                continue;
            }
            let progress: PullProgress = match serde_json::from_str(&line) {
                Ok(p) => p,
                Err(_) => continue,
            };
            if let Some(err) = &progress.error {
                bail!("Ollama no pudo descargar «{model}»: {err}");
            }
            on_progress(&progress);
        }
        Ok(())
    }

    pub fn delete(&self, model: &str) -> Result<()> {
        let mut resp = agent(Duration::from_secs(30))
            .delete(self.url("/api/delete"))
            .header("content-type", "application/json")
            // `DELETE` con cuerpo: la API de Ollama lo exige.
            .force_send_body()
            .send_json(json!({ "model": model }))
            .with_context(|| format!("no pude contactar con Ollama en {}", self.endpoint))?;
        let status = resp.status().as_u16();
        if status == 404 {
            bail!("el modelo «{model}» no está descargado");
        }
        if !(200..300).contains(&status) {
            let text = resp.body_mut().read_to_string().unwrap_or_default();
            bail!(
                "Ollama devolvió HTTP {status} a /api/delete: {}",
                text.chars().take(300).collect::<String>()
            );
        }
        Ok(())
    }
}

/// Extrae de `/api/show` lo que antOS usa. Separado del cliente para poder
/// probarlo con JSON de ejemplo.
pub fn parse_show(v: &Value) -> ModelShow {
    let capabilities = v.get("capabilities").and_then(Value::as_array).map(|a| {
        a.iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    });
    let context_length = v
        .get("model_info")
        .and_then(Value::as_object)
        .and_then(|info| {
            info.iter()
                .find(|(k, _)| k.ends_with(".context_length"))
                .and_then(|(_, val)| val.as_u64())
        })
        .and_then(|n| u32::try_from(n).ok());
    let details = v
        .get("details")
        .cloned()
        .and_then(|d| serde_json::from_value(d).ok())
        .unwrap_or_default();
    ModelShow {
        capabilities,
        context_length,
        details,
    }
}

/// Bytes en una unidad legible (GB con un decimal, MB por debajo).
pub fn human_size(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else {
        format!("{:.0} MB", b / MB)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn show_is_parsed_with_capabilities_and_context_length() {
        let v = json!({
            "capabilities": ["completion", "tools"],
            "details": {"family": "qwen2", "parameter_size": "7.6B", "quantization_level": "Q4_K_M"},
            "model_info": {
                "general.architecture": "qwen2",
                "qwen2.context_length": 32768,
                "qwen2.embedding_length": 3584
            }
        });
        let show = parse_show(&v);
        assert_eq!(show.supports_tools(), Some(true));
        assert_eq!(show.context_length, Some(32768));
        assert_eq!(show.details.parameter_size, "7.6B");

        // Servidor antiguo sin `capabilities`: no se sabe, no se niega.
        let old = parse_show(&json!({"model_info": {}}));
        assert_eq!(old.supports_tools(), None);
        assert_eq!(old.context_length, None);

        let no_tools = parse_show(&json!({"capabilities": ["completion"]}));
        assert_eq!(no_tools.supports_tools(), Some(false));
    }

    #[test]
    fn endpoint_gets_a_scheme_when_ollama_host_style() {
        assert_eq!(
            OllamaClient::new("127.0.0.1:11500").endpoint(),
            "http://127.0.0.1:11500"
        );
        assert_eq!(
            OllamaClient::new("http://127.0.0.1:11434/").endpoint(),
            "http://127.0.0.1:11434"
        );
    }

    #[test]
    fn pull_progress_lines_parse_and_sizes_are_human() {
        let p: PullProgress = serde_json::from_str(
            r#"{"status":"pulling abc","digest":"sha256:abc","total":4683087561,"completed":1000000}"#,
        )
        .unwrap();
        assert_eq!(p.total, Some(4_683_087_561));
        assert_eq!(human_size(4_683_087_561), "4.4 GB");
        assert_eq!(human_size(500 * 1024 * 1024), "500 MB");
    }
}
