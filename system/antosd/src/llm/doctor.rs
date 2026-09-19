//! `antos llm doctor` (T34.2): qué máquina es esta, qué modelo local le cabe
//! y con cuánto contexto.
//!
//! La RAM se lee de `/proc/meminfo` (Linux) o `sysctl -n hw.memsize` (macOS)
//! sin intérprete ni dependencia nueva. La tabla de recomendaciones vive en
//! `system/llm/models.toml`; se carga del árbol de antOS si está y, si no,
//! de la copia embebida en el binario.

use super::ollama_api::{human_size, ModelShow, ModelTag, OllamaClient, RunningModel};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

const EMBEDDED_TABLE: &str = include_str!("../../../llm/models.toml");
pub const TABLE_RELATIVE_PATH: &str = "system/llm/models.toml";

/// Memoria de la máquina, en bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SystemResources {
    pub total_ram: u64,
    /// `None` cuando la plataforma no lo dice de forma barata (macOS).
    pub available_ram: Option<u64>,
}

impl SystemResources {
    pub fn total_ram_gb(&self) -> f64 {
        self.total_ram as f64 / (1024.0 * 1024.0 * 1024.0)
    }

    pub fn detect() -> Option<Self> {
        #[cfg(target_os = "linux")]
        {
            let content = std::fs::read_to_string("/proc/meminfo").ok()?;
            return parse_meminfo(&content);
        }
        #[cfg(target_os = "macos")]
        {
            let out = std::process::Command::new("sysctl")
                .args(["-n", "hw.memsize"])
                .output()
                .ok()?;
            let total = String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse::<u64>()
                .ok()?;
            return Some(Self {
                total_ram: total,
                available_ram: None,
            });
        }
        #[allow(unreachable_code)]
        None
    }
}

/// `MemTotal:       16384000 kB` → bytes.
pub fn parse_meminfo(content: &str) -> Option<SystemResources> {
    let mut total = None;
    let mut available = None;
    for line in content.lines() {
        let mut parts = line.split_whitespace();
        let key = parts.next()?;
        let value = parts.next().and_then(|v| v.parse::<u64>().ok());
        match key {
            "MemTotal:" => total = value.map(|kb| kb * 1024),
            "MemAvailable:" => available = value.map(|kb| kb * 1024),
            _ => {}
        }
    }
    Some(SystemResources {
        total_ram: total?,
        available_ram: available,
    })
}

// ------------------------------------------------------------ la tabla

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Tier {
    pub name: String,
    pub min_ram_gb: u32,
    pub max_num_ctx: u32,
    pub code: String,
    pub large: String,
    pub fast: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ModelTable {
    #[serde(rename = "tier")]
    pub tiers: Vec<Tier>,
}

impl ModelTable {
    pub fn parse(text: &str) -> Result<Self> {
        let mut table: Self = toml::from_str(text).context("tabla de modelos ilegible")?;
        table.tiers.sort_by_key(|t| std::cmp::Reverse(t.min_ram_gb));
        Ok(table)
    }

    /// La del árbol de antOS si existe y se puede leer; si no, la embebida.
    pub fn load(antos_root: Option<&Path>) -> Self {
        let from_tree = antos_root
            .map(|r| r.join(TABLE_RELATIVE_PATH))
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| Self::parse(&t).ok());
        from_tree
            .unwrap_or_else(|| Self::parse(EMBEDDED_TABLE).unwrap_or(Self { tiers: Vec::new() }))
    }

    /// El primer tier (de mayor a menor) cuyo mínimo alcanza esta RAM.
    pub fn tier_for(&self, total_ram_gb: f64) -> Option<&Tier> {
        self.tiers
            .iter()
            .find(|t| total_ram_gb >= f64::from(t.min_ram_gb))
    }
}

// ---------------------------------------------------- contexto efectivo

/// Acota la ventana pedida al máximo del modelo y al techo del tier de RAM.
/// Devuelve el valor efectivo y, si se recortó, por qué.
pub fn effective_num_ctx(
    requested: u32,
    model_max: Option<u32>,
    ram_cap: Option<u32>,
) -> (u32, Option<String>) {
    let mut value = requested;
    let mut reasons = Vec::new();
    if let Some(m) = model_max {
        if m < value {
            value = m;
            reasons.push(format!("el modelo admite {m}"));
        }
    }
    if let Some(r) = ram_cap {
        if r < value {
            value = r;
            reasons.push(format!("la RAM de esta máquina aconseja {r}"));
        }
    }
    let note = if reasons.is_empty() {
        None
    } else {
        Some(format!(
            "contexto pedido {requested}, efectivo {value}: {}",
            reasons.join("; ")
        ))
    };
    (value, note)
}

// ------------------------------------------------------------ el informe

#[derive(Debug, Clone)]
pub struct ModelReport {
    pub tag: ModelTag,
    pub show: Option<ModelShow>,
}

#[derive(Debug, Clone)]
pub struct DoctorReport {
    pub endpoint: String,
    pub reachable: bool,
    pub version: Option<String>,
    pub resources: Option<SystemResources>,
    pub tier: Option<Tier>,
    pub models: Vec<ModelReport>,
    pub running: Vec<RunningModel>,
    pub active_model: Option<String>,
    pub requested_num_ctx: u32,
    /// `(efectivo, nota)` para el modelo activo, si está descargado.
    pub effective_ctx: Option<(u32, Option<String>)>,
}

impl DoctorReport {
    /// Recoge todo lo que `doctor` muestra. No falla si Ollama no responde:
    /// lo dice en `reachable`.
    pub fn collect(
        client: &OllamaClient,
        table: &ModelTable,
        active_model: Option<&str>,
        requested_num_ctx: u32,
    ) -> Self {
        let resources = SystemResources::detect();
        let tier = resources
            .and_then(|r| table.tier_for(r.total_ram_gb()))
            .cloned();
        let reachable = client.is_available();
        let (version, models, running) = if reachable {
            let tags = client.tags().unwrap_or_default();
            let models = tags
                .into_iter()
                .map(|tag| ModelReport {
                    show: client.show(&tag.name).ok(),
                    tag,
                })
                .collect();
            (client.version(), models, client.ps().unwrap_or_default())
        } else {
            (None, Vec::new(), Vec::new())
        };
        let models: Vec<ModelReport> = models;
        let effective_ctx = active_model.and_then(|m| {
            models.iter().find(|r| r.tag.name == m).map(|r| {
                effective_num_ctx(
                    requested_num_ctx,
                    r.show.as_ref().and_then(|s| s.context_length),
                    tier.as_ref().map(|t| t.max_num_ctx),
                )
            })
        });
        Self {
            endpoint: client.endpoint().to_string(),
            reachable,
            version,
            resources,
            tier,
            models,
            running,
            active_model: active_model.map(str::to_string),
            requested_num_ctx,
            effective_ctx,
        }
    }

    /// Modelo que `setup` debería descargar: el `code` del tier de RAM.
    pub fn recommended_model(&self) -> Option<&str> {
        self.tier.as_ref().map(|t| t.code.as_str())
    }

    /// Modelos descargados que declaran `tools` (o de los que no se sabe).
    pub fn agent_capable_models(&self) -> Vec<&ModelReport> {
        self.models
            .iter()
            .filter(|m| {
                m.show
                    .as_ref()
                    .and_then(ModelShow::supports_tools)
                    .unwrap_or(true)
            })
            .collect()
    }

    /// Texto plano de una pantalla; el CLI lo colorea si quiere.
    pub fn render(&self) -> String {
        let mut out = Vec::new();
        out.push(format!(
            "Servicio Ollama   {} · {}{}",
            self.endpoint,
            if self.reachable {
                "responde"
            } else {
                "NO responde"
            },
            self.version
                .as_deref()
                .map(|v| format!(" · v{v}"))
                .unwrap_or_default()
        ));
        match self.resources {
            Some(r) => out.push(format!(
                "RAM               {:.1} GB{}",
                r.total_ram_gb(),
                r.available_ram
                    .map(|a| format!(" ({} libres)", human_size(a)))
                    .unwrap_or_default()
            )),
            None => out.push("RAM               no se pudo leer".into()),
        }
        match &self.tier {
            Some(t) => out.push(format!(
                "Tier              {} → código {} · grande {} · rápido {} · contexto máx. {}",
                t.name, t.code, t.large, t.fast, t.max_num_ctx
            )),
            None => out.push("Tier              sin tabla de modelos".into()),
        }
        if !self.running.is_empty() {
            for r in &self.running {
                out.push(format!(
                    "Cargado           {} · {} en memoria ({} en GPU)",
                    r.name,
                    human_size(r.size),
                    human_size(r.size_vram)
                ));
            }
        }
        out.push(String::new());
        if self.models.is_empty() {
            out.push(if self.reachable {
                "Modelos           ninguno descargado".into()
            } else {
                "Modelos           (Ollama no responde)".into()
            });
        } else {
            out.push(format!(
                "{:<28} {:>8} {:<8} {:<10} {:<6} {:>8}",
                "MODELO", "TAMAÑO", "PARAMS", "CUANT.", "TOOLS", "CTX"
            ));
            for m in &self.models {
                let (tools, ctx) = match &m.show {
                    Some(s) => (
                        match s.supports_tools() {
                            Some(true) => "sí",
                            Some(false) => "no",
                            None => "?",
                        },
                        s.context_length
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| "?".into()),
                    ),
                    None => ("?", "?".into()),
                };
                let active = if self.active_model.as_deref() == Some(m.tag.name.as_str()) {
                    " ◀ activo"
                } else {
                    ""
                };
                out.push(format!(
                    "{:<28} {:>8} {:<8} {:<10} {:<6} {:>8}{active}",
                    m.tag.name,
                    human_size(m.tag.size),
                    m.tag.details.parameter_size,
                    m.tag.details.quantization_level,
                    tools,
                    ctx
                ));
            }
        }
        out.push(String::new());
        match (&self.active_model, &self.effective_ctx) {
            (Some(m), Some((eff, note))) => {
                out.push(format!(
                    "Contexto          {m}: pedido {} · efectivo {eff}",
                    self.requested_num_ctx
                ));
                if let Some(n) = note {
                    out.push(format!("                  ({n})"));
                }
            }
            (Some(m), None) => out.push(format!(
                "Contexto          {m} no está descargado; pedido {}",
                self.requested_num_ctx
            )),
            (None, _) => out.push(format!(
                "Contexto          sin modelo activo; pedido {}",
                self.requested_num_ctx
            )),
        }
        if let Some(rec) = self.recommended_model() {
            let have = self.models.iter().any(|m| m.tag.name == rec);
            out.push(format!(
                "Recomendado       {rec}{}",
                if have {
                    " (descargado)"
                } else {
                    " — descárgalo con: antos llm pull"
                }
            ));
        }
        if !self.reachable {
            out.push(String::new());
            out.push("Para arrancar Ollama: antos service up ollama  (o antos llm setup)".into());
        }
        out.join("\n")
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn meminfo_is_parsed_to_bytes() {
        let r = parse_meminfo(
            "MemTotal:       16384000 kB\nMemFree: 1 kB\nMemAvailable:    8192000 kB\n",
        )
        .unwrap();
        assert_eq!(r.total_ram, 16_384_000 * 1024);
        assert_eq!(r.available_ram, Some(8_192_000 * 1024));
        assert!(parse_meminfo("garbage").is_none());
    }

    #[test]
    fn embedded_table_parses_and_picks_the_tier_by_ram() {
        let t = ModelTable::parse(EMBEDDED_TABLE).unwrap();
        assert!(t.tiers.len() >= 4);
        assert_eq!(t.tier_for(18.0).unwrap().code, "qwen2.5-coder:7b");
        assert_eq!(t.tier_for(18.0).unwrap().max_num_ctx, 16_384);
        assert_eq!(t.tier_for(8.0).unwrap().code, "qwen2.5-coder:3b");
        assert_eq!(t.tier_for(64.0).unwrap().code, "qwen2.5-coder:32b");
        assert_eq!(t.tier_for(4.0).unwrap().code, "qwen2.5-coder:1.5b");
        // La embebida y la del árbol son la misma tabla.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert_eq!(ModelTable::load(Some(&root)), t);
    }

    #[test]
    fn effective_context_is_capped_by_model_and_ram_with_a_note() {
        assert_eq!(effective_num_ctx(16_384, None, None), (16_384, None));
        let (v, note) = effective_num_ctx(32_768, Some(32_768), Some(16_384));
        assert_eq!(v, 16_384);
        assert!(note.unwrap().contains("RAM"));
        let (v, note) = effective_num_ctx(16_384, Some(8_192), None);
        assert_eq!(v, 8_192);
        assert!(note.unwrap().contains("el modelo admite 8192"));
    }
}
