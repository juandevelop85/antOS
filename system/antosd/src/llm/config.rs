//! Persistent configuration for LLM providers in antOS (Ticket T19.2).
//!
//! Stores provider endpoints, default models, API keys and per-role overrides
//! in `state/llm_config.json`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const LLM_CONFIG_FILE: &str = "llm_config.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmConfig {
    /// Active provider: "auto", "ollama", "groq", "openrouter", "gemini", "opencode", "claude", "local"
    pub active_provider: String,
    /// Specific settings per provider (custom models, endpoints, etc.)
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderSettings>,
    /// Role-specific model overrides for antFlow (T19.4)
    #[serde(default)]
    pub role_models: BTreeMap<String, String>,
    /// Presupuesto en pasos por rol para el pipeline de agentes (T33.3).
    #[serde(default)]
    pub role_steps: BTreeMap<String, u32>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        let mut providers = BTreeMap::new();
        providers.insert(
            "ollama".into(),
            ProviderSettings {
                endpoint: Some("http://127.0.0.1:11434".into()),
                model: Some("qwen2.5-coder:latest".into()),
                api_key: None,
            },
        );
        providers.insert(
            "groq".into(),
            ProviderSettings {
                endpoint: Some("https://api.groq.com/openai/v1".into()),
                model: Some("llama-3.3-70b-versatile".into()),
                api_key: None,
            },
        );
        providers.insert(
            "openrouter".into(),
            ProviderSettings {
                endpoint: Some("https://openrouter.ai/api/v1".into()),
                model: Some("deepseek/deepseek-r1:free".into()),
                api_key: None,
            },
        );
        providers.insert(
            "gemini".into(),
            ProviderSettings {
                endpoint: Some("https://generativelanguage.googleapis.com/v1beta/openai".into()),
                model: Some("gemini-2.0-flash".into()),
                api_key: None,
            },
        );
        providers.insert(
            "opencode".into(),
            ProviderSettings {
                endpoint: Some("http://127.0.0.1:8080/v1".into()),
                model: Some("qwen2.5-coder".into()),
                api_key: None,
            },
        );

        Self {
            active_provider: "auto".into(),
            providers,
            role_models: BTreeMap::new(),
            role_steps: BTreeMap::new(),
        }
    }
}

impl LlmConfig {
    pub fn config_path(state_dir: &Path) -> PathBuf {
        state_dir.join(LLM_CONFIG_FILE)
    }

    pub fn load_from_state(state_dir: &Path) -> Self {
        let path = Self::config_path(state_dir);
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(config) = serde_json::from_str::<Self>(&content) {
                    return config;
                }
            }
        }
        Self::default()
    }

    pub fn save_to_state(&self, state_dir: &Path) -> Result<()> {
        let path = Self::config_path(state_dir);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let serialized = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, serialized).with_context(|| {
            format!("no se pudo escribir la configuración en {}", path.display())
        })?;
        Ok(())
    }

    pub fn set_active_provider(
        &mut self,
        provider: &str,
        model: Option<&str>,
        endpoint: Option<&str>,
    ) {
        self.active_provider = provider.to_lowercase();
        let entry = self
            .providers
            .entry(self.active_provider.clone())
            .or_insert_with(|| ProviderSettings {
                endpoint: None,
                model: None,
                api_key: None,
            });

        if let Some(m) = model {
            entry.model = Some(m.to_string());
        }
        if let Some(e) = endpoint {
            entry.endpoint = Some(e.to_string());
        }
    }

    pub fn clear_active(&mut self) {
        self.active_provider = "auto".into();
    }

    pub fn get_provider_settings(&self, provider: &str) -> Option<&ProviderSettings> {
        self.providers.get(&provider.to_lowercase())
    }

    /// Returns the assigned model for an antFlow role, or default recommendation (T19.4).
    pub fn get_role_model(&self, role: &str) -> String {
        let role_clean = role.to_lowercase();
        if let Some(m) = self.role_models.get(&role_clean) {
            return m.clone();
        }
        match role_clean.as_str() {
            "architect" | "arquitecto" => "openrouter:deepseek/deepseek-r1:free".to_string(),
            "coder" => "ollama:qwen2.5-coder:latest".to_string(),
            "qa" | "tester" => "groq:llama-3.3-70b-versatile".to_string(),
            "auditor" => "groq:llama-3.3-70b-versatile".to_string(),
            _ => self.active_provider.clone(),
        }
    }

    /// Pasos máximos de un rol (T33.3); `default` si no está configurado.
    pub fn get_role_steps(&self, role: &str, default: u32) -> u32 {
        self.role_steps
            .get(&role.to_lowercase())
            .copied()
            .unwrap_or(default)
    }

    /// Sets the assigned model for an antFlow role (T19.4).
    pub fn set_role_model(&mut self, role: &str, model: &str) {
        self.role_models
            .insert(role.to_lowercase(), model.to_string());
    }

    /// Returns the full map of role-to-model assignments (T19.4).
    pub fn list_role_models(&self) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        let roles = ["architect", "coder", "qa", "auditor"];
        for r in roles {
            map.insert(r.to_string(), self.get_role_model(r));
        }
        map
    }
}

// ───────────────────────────────────────────────────────────────────── tests ──

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_llm_config_defaults() {
        let config = LlmConfig::default();
        assert_eq!(config.active_provider, "auto");
        assert!(config.providers.contains_key("groq"));
        assert!(config.providers.contains_key("openrouter"));
        assert!(config.providers.contains_key("ollama"));
        assert_eq!(
            config.providers.get("groq").unwrap().model.as_deref(),
            Some("llama-3.3-70b-versatile")
        );
    }

    #[test]
    fn test_llm_config_save_and_load() {
        let unique = format!(
            "antos_llm_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_state = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&temp_state).expect("create temp state");

        let mut config = LlmConfig::default();
        config.set_active_provider("groq", Some("deepseek-r1-distill-llama-70b"), None);
        assert_eq!(config.active_provider, "groq");

        config.save_to_state(&temp_state).expect("save config");

        let loaded = LlmConfig::load_from_state(&temp_state);
        assert_eq!(loaded.active_provider, "groq");
        assert_eq!(
            loaded.providers.get("groq").unwrap().model.as_deref(),
            Some("deepseek-r1-distill-llama-70b")
        );

        let _ = std::fs::remove_dir_all(&temp_state);
    }

    #[test]
    fn test_llm_config_clear_active() {
        let mut config = LlmConfig::default();
        config.set_active_provider("openrouter", None, None);
        assert_eq!(config.active_provider, "openrouter");
        config.clear_active();
        assert_eq!(config.active_provider, "auto");
    }

    #[test]
    fn test_role_models_configuration() {
        let mut config = LlmConfig::default();
        assert_eq!(
            config.get_role_model("architect"),
            "openrouter:deepseek/deepseek-r1:free"
        );
        assert_eq!(
            config.get_role_model("coder"),
            "ollama:qwen2.5-coder:latest"
        );

        config.set_role_model("coder", "groq:qwen2.5-coder");
        assert_eq!(config.get_role_model("coder"), "groq:qwen2.5-coder");

        let roles = config.list_role_models();
        assert_eq!(roles.len(), 4);
        assert!(roles.contains_key("architect"));
        assert!(roles.contains_key("coder"));
        assert!(roles.contains_key("qa"));
        assert!(roles.contains_key("auditor"));
    }
}
