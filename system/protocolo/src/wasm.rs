//! Schema definitions and serialization contracts for antOS.

use serde::{Deserialize, Serialize};

// ============================================================================
// WebAssembly (WASM) Capability Plugins and Sandboxed Execution
// ============================================================================

// ------------------------------------------------ WASM Plugins (T14.1)

/// Summary of an installed WebAssembly plugin in antOS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSummary {
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub wasm_size_bytes: u64,
}

/// Result of executing an action on a WASM plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginResult {
    pub plugin: String,
    pub action: String,
    pub output: String,
    pub fuel_consumed: u64,
    pub memory_allocated_bytes: usize,
    pub success: bool,
    pub error: Option<String>,
}

