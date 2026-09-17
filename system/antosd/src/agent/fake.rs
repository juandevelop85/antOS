//! Proveedor determinista para tests y CI (T33.2).
//!
//! Sin red ni clave: sigue un guion de turnos, cada uno con el texto del
//! asistente y las herramientas que «pide». Es lo que permite probar el
//! CONTRATO del runtime —validación, tiers, instantáneas, journal,
//! presupuesto, informe— sin depender de que un modelo acierte. Registra
//! los resultados que recibe para que los tests comprueben qué vio el
//! modelo.

use super::providers::{AgentProvider, ToolCall, ToolResult, Turn};
use super::tools::ToolSpec;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Una llamada del guion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptedCall {
    pub tool: String,
    #[serde(default)]
    pub input: Value,
}

/// Un turno del guion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ScriptedTurn {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub calls: Vec<ScriptedCall>,
    /// Tokens que «cuesta» el turno (para probar el presupuesto).
    #[serde(default)]
    pub tokens: u64,
}

pub struct FakeProvider {
    script: std::collections::VecDeque<ScriptedTurn>,
    next_id: u32,
    /// Todo lo que el runtime devolvió al «modelo», en orden.
    pub received: Vec<ToolResult>,
    /// Prompt de sistema y objetivo con los que arrancó (para aserciones).
    pub system_seen: String,
    pub goal_seen: String,
    /// Recordatorios recibidos del runtime.
    pub nudges: u32,
}

impl FakeProvider {
    pub fn new(script: Vec<ScriptedTurn>) -> Self {
        Self {
            script: script.into(),
            next_id: 0,
            received: Vec::new(),
            system_seen: String::new(),
            goal_seen: String::new(),
            nudges: 0,
        }
    }

    /// Guion en JSON: `[{"text": "...", "calls": [{"tool": "fs.read", "input": {...}}]}, …]`.
    pub fn from_json(text: &str) -> Result<Self> {
        let script: Vec<ScriptedTurn> =
            serde_json::from_str(text).context("guion del proveedor fake inválido")?;
        Ok(Self::new(script))
    }

    fn next_turn(&mut self) -> Turn {
        let Some(scripted) = self.script.pop_front() else {
            // Guion agotado: el «modelo» deja de pedir herramientas, sin
            // texto (así el resumen del run conserva lo último que dijo).
            return Turn::default();
        };
        let calls = scripted
            .calls
            .into_iter()
            .map(|c| {
                self.next_id += 1;
                ToolCall {
                    id: format!("fake-{}", self.next_id),
                    name: c.tool,
                    input: c.input,
                }
            })
            .collect();
        Turn {
            text: scripted.text,
            calls,
            tokens: scripted.tokens,
        }
    }
}

impl AgentProvider for FakeProvider {
    fn name(&self) -> String {
        "fake".into()
    }
    fn model(&self) -> String {
        "fake-script".into()
    }
    fn start(&mut self, system: &str, user: &str, _tools: &[ToolSpec]) -> Result<Turn> {
        self.system_seen = system.to_string();
        self.goal_seen = user.to_string();
        Ok(self.next_turn())
    }
    fn continue_with(&mut self, results: &[ToolResult], _tools: &[ToolSpec]) -> Result<Turn> {
        self.received.extend(results.iter().cloned());
        Ok(self.next_turn())
    }
    fn nudge(&mut self, _text: &str, _tools: &[ToolSpec]) -> Result<Turn> {
        self.nudges += 1;
        Ok(self.next_turn())
    }
}
