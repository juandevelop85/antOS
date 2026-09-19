//! Gestión de LLM y hub multiproveedor de antOS (T19.2).
//!
//! - `config`: proveedor activo, modelos y contexto por rol (`llm_config.json`).
//! - `ollama_api`: cliente HTTP de la API nativa de Ollama (T34.2).
//! - `doctor`: recursos de la máquina, tabla de modelos por RAM y contexto
//!   efectivo (T34.2).

pub mod config;
pub mod doctor;
pub mod ollama_api;

pub use config::*;
