//! Gestión de manifiestos, instalación y ejecución de plugins WebAssembly (T14.1).

use super::parser::WasmModule;
use super::vm::WasmInstance;
use antos_protocol::{PluginResult, PluginSummary};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_FUEL_LIMIT: u64 = 1_000_000; // 1 millón de ciclos de instrucción

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_entrypoint")]
    pub entrypoint: String,
}

fn default_entrypoint() -> String {
    "plugin.wasm".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginPermissions {
    #[serde(default)]
    pub filesystem_read: bool,
    #[serde(default)]
    pub filesystem_write: bool,
    #[serde(default)]
    pub network: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginCapabilities {
    #[serde(default)]
    pub actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub plugin: PluginMetadata,
    #[serde(default)]
    pub permissions: PluginPermissions,
    #[serde(default)]
    pub capabilities: PluginCapabilities,
}

pub struct PluginManager;

impl PluginManager {
    pub fn global() -> Self {
        Self
    }

    /// Obtiene el directorio base de plugins (~/.antos/plugins o $WORKSPACE/.antos/plugins)
    pub fn get_plugins_dir(workspace: &Path) -> PathBuf {
        let local_dir = workspace.join(".antos/plugins");
        if local_dir.exists() {
            return local_dir;
        }

        if let Ok(home) = std::env::var("HOME") {
            let global_dir = PathBuf::from(home).join(".antos/plugins");
            if global_dir.exists() {
                return global_dir;
            }
        }

        local_dir
    }

    /// Lista todos los plugins instalados y sus capacidades expuestas
    pub fn list_plugins(plugins_dir: &Path) -> Vec<PluginSummary> {
        let mut summaries = Vec::new();
        if !plugins_dir.exists() {
            return summaries;
        }

        let entries = match fs::read_dir(plugins_dir) {
            Ok(e) => e,
            Err(_) => return summaries,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let manifest_path = path.join("plugin.toml");
                if manifest_path.exists() {
                    if let Ok(content) = fs::read_to_string(&manifest_path) {
                        if let Ok(manifest) = toml::from_str::<PluginManifest>(&content) {
                            let wasm_file = path.join(&manifest.plugin.entrypoint);
                            let wasm_size_bytes = if wasm_file.exists() {
                                fs::metadata(&wasm_file).map(|m| m.len()).unwrap_or(0)
                            } else {
                                0
                            };

                            summaries.push(PluginSummary {
                                name: manifest.plugin.name,
                                version: manifest.plugin.version,
                                description: manifest.plugin.description,
                                capabilities: manifest.capabilities.actions,
                                wasm_size_bytes,
                            });
                        }
                    }
                }
            }
        }

        summaries.sort_by(|a, b| a.name.cmp(&b.name));
        summaries
    }

    /// Instala un plugin copiando su directorio y validando el manifiesto plugin.toml
    pub fn install_plugin(plugins_dir: &Path, source_dir: &Path) -> Result<PluginSummary> {
        let manifest_path = source_dir.join("plugin.toml");
        if !manifest_path.exists() {
            bail!("No se encontró 'plugin.toml' en {}", source_dir.display());
        }

        let content = fs::read_to_string(&manifest_path)
            .context("Error leyendo 'plugin.toml'")?;
        let manifest: PluginManifest = toml::from_str(&content)
            .context("Error analizando sintaxis de 'plugin.toml'")?;

        let wasm_path = source_dir.join(&manifest.plugin.entrypoint);
        if !wasm_path.exists() {
            bail!("No se encontró el binario WASM '{}' en {}", manifest.plugin.entrypoint, source_dir.display());
        }

        // Validar que el binario WASM tenga la estructura mágica correcta
        let wasm_bytes = fs::read(&wasm_path).context("Error leyendo archivo .wasm")?;
        let module = WasmModule::from_bytes(&wasm_bytes)
            .context("El binario .wasm no es un módulo WebAssembly válido")?;

        let target_dir = plugins_dir.join(&manifest.plugin.name);
        fs::create_dir_all(&target_dir)
            .context("Error creando directorio de instalación del plugin")?;

        fs::copy(&manifest_path, target_dir.join("plugin.toml"))
            .context("Error copiando plugin.toml")?;
        fs::copy(&wasm_path, target_dir.join(&manifest.plugin.entrypoint))
            .context("Error copiando binario .wasm")?;

        let wasm_size_bytes = wasm_bytes.len() as u64;
        let mut caps = manifest.capabilities.actions;
        if caps.is_empty() {
            // Deducir exportaciones de funciones
            caps = module.exports.keys().cloned().collect();
        }

        Ok(PluginSummary {
            name: manifest.plugin.name,
            version: manifest.plugin.version,
            description: manifest.plugin.description,
            capabilities: caps,
            wasm_size_bytes,
        })
    }

    /// Ejecuta una acción sobre un plugin en sandbox aislado WebAssembly
    pub fn run_plugin(
        plugins_dir: &Path,
        name: &str,
        action: &str,
        params: &BTreeMap<String, String>,
    ) -> PluginResult {
        let plugin_dir = plugins_dir.join(name);
        if !plugin_dir.exists() {
            return PluginResult {
                plugin: name.to_string(),
                action: action.to_string(),
                output: String::new(),
                fuel_consumed: 0,
                memory_allocated_bytes: 0,
                success: false,
                error: Some(format!("El plugin «{}» no está instalado en {}", name, plugins_dir.display())),
            };
        }

        let manifest_path = plugin_dir.join("plugin.toml");
        let manifest: PluginManifest = match fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|c| toml::from_str(&c).ok())
        {
            Some(m) => m,
            None => {
                return PluginResult {
                    plugin: name.to_string(),
                    action: action.to_string(),
                    output: String::new(),
                    fuel_consumed: 0,
                    memory_allocated_bytes: 0,
                    success: false,
                    error: Some("Error al cargar el manifiesto 'plugin.toml'".into()),
                };
            }
        };

        let wasm_file = plugin_dir.join(&manifest.plugin.entrypoint);
        let wasm_bytes = match fs::read(&wasm_file) {
            Ok(b) => b,
            Err(e) => {
                return PluginResult {
                    plugin: name.to_string(),
                    action: action.to_string(),
                    output: String::new(),
                    fuel_consumed: 0,
                    memory_allocated_bytes: 0,
                    success: false,
                    error: Some(format!("No se pudo leer el binario {}: {}", wasm_file.display(), e)),
                };
            }
        };

        let module = match WasmModule::from_bytes(&wasm_bytes) {
            Ok(m) => m,
            Err(e) => {
                return PluginResult {
                    plugin: name.to_string(),
                    action: action.to_string(),
                    output: String::new(),
                    fuel_consumed: 0,
                    memory_allocated_bytes: 0,
                    success: false,
                    error: Some(format!("Módulo WebAssembly corrupto o inválido: {}", e)),
                };
            }
        };

        let param_map: HashMap<String, String> = params.clone().into_iter().collect();
        let mut instance = match WasmInstance::new(module, DEFAULT_FUEL_LIMIT, param_map) {
            Ok(inst) => inst,
            Err(trap) => {
                return PluginResult {
                    plugin: name.to_string(),
                    action: action.to_string(),
                    output: String::new(),
                    fuel_consumed: 0,
                    memory_allocated_bytes: 0,
                    success: false,
                    error: Some(format!("Fallo de inicialización de la VM: {}", trap)),
                };
            }
        };

        let mem_allocated = instance.memory.len();

        // Buscar la función a ejecutar (el nombre de la acción o "run" o "_start")
        let target_func = if instance.module.exports.contains_key(action) {
            action
        } else if instance.module.exports.contains_key("run") {
            "run"
        } else if instance.module.exports.contains_key("_start") {
            "_start"
        } else {
            return PluginResult {
                plugin: name.to_string(),
                action: action.to_string(),
                output: String::new(),
                fuel_consumed: instance.fuel_consumed,
                memory_allocated_bytes: mem_allocated,
                success: false,
                error: Some(format!("La acción «{}» no corresponde a ninguna función exportada por el plugin", action)),
            };
        };

        match instance.execute_export(target_func, &[]) {
            Ok(ret_val) => {
                let mut out_str = String::from_utf8_lossy(&instance.host_env.output).to_string();
                if out_str.is_empty() {
                    if let Some(code) = ret_val {
                        out_str = format!("Código de retorno: {}", code);
                    } else {
                        out_str = "Ejecución completada con éxito.".into();
                    }
                }

                PluginResult {
                    plugin: name.to_string(),
                    action: action.to_string(),
                    output: out_str,
                    fuel_consumed: instance.fuel_consumed,
                    memory_allocated_bytes: mem_allocated,
                    success: true,
                    error: None,
                }
            }
            Err(trap) => PluginResult {
                plugin: name.to_string(),
                action: action.to_string(),
                output: String::from_utf8_lossy(&instance.host_env.output).to_string(),
                fuel_consumed: instance.fuel_consumed,
                memory_allocated_bytes: mem_allocated,
                success: false,
                error: Some(format!("Fallo durante ejecución en sandbox WASM: {}", trap)),
            },
        }
    }
}
