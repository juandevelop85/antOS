//! Planificador con Claude, por HTTP directo contra la Messages API.
//!
//! Rust no tiene SDK oficial de Anthropic, así que hablamos HTTP a mano.
//!
//! El catálogo se traduce a definiciones de herramienta: cada capacidad es
//! una herramienta con su esquema. El modelo no «escribe comandos», elige
//! entre capacidades declaradas y rellena parámetros tipados — y aun así su
//! salida vuelve a validarse contra el catálogo antes de usarse.

use super::{Planner, Propuesta};
use crate::capability::Catalog;
use crate::plan::Step;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

const URL: &str = "https://api.anthropic.com/v1/messages";
const DEFAULT_MODEL: &str = "claude-opus-5";
pub const CLAVE_ENV: &str = "ANTHROPIC_API_KEY";
const PLAN_TOOL: &str = "emitir_plan";

pub struct ClaudePlanner {
    api_key: String,
    model: String,
}

impl ClaudePlanner {
    pub fn from_env() -> Result<Self> {
        let api_key = leer_clave()?;
        let model = std::env::var("SYSO_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
        Ok(Self { api_key, model })
    }
}

/// Dónde vive la clave por defecto.
fn ruta_clave() -> PathBuf {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(".config/syso/anthropic.key")
}

/// Lee la clave de un fichero, y solo como último recurso del entorno.
///
/// El orden importa. Una variable de entorno la hereda TODO proceso hijo, y
/// este programa lanza varios: whisper, ffmpeg, y sobre todo el ejecutor
/// confinado — que es precisamente el componente que el diseño entero trata
/// como no fiable. Un fichero que solo lee el broker no viaja a ninguna parte.
fn leer_clave() -> Result<String> {
    let ruta = match std::env::var_os("ANTHROPIC_API_KEY_FILE") {
        Some(p) => PathBuf::from(p),
        None => ruta_clave(),
    };

    if ruta.exists() {
        let clave = std::fs::read_to_string(&ruta)
            .with_context(|| format!("no pude leer {}", ruta.display()))?;
        avisar_si_es_legible_por_otros(&ruta);
        let clave = clave.trim().to_string();
        if !clave.is_empty() {
            return Ok(clave);
        }
    }

    // El entorno sigue funcionando por comodidad, pero se avisa.
    if let Ok(clave) = std::env::var(CLAVE_ENV) {
        if !clave.trim().is_empty() {
            eprintln!(
                "aviso: usando {CLAVE_ENV} del entorno. Todo proceso hijo la hereda; \n\
                 es preferible {}",
                ruta_clave().display()
            );
            return Ok(clave.trim().to_string());
        }
    }

    bail!(
        "no encuentro la clave de la API.\n\
         Ponla en {} (solo lectura para ti):\n\
         \n  mkdir -p ~/.config/syso && chmod 700 ~/.config/syso\n\
         \n  read -rs CLAVE && printf '%s' \"$CLAVE\" > {} && unset CLAVE\n\
         \n  chmod 600 {}\n\
         \nO usa el planificador local, que no necesita clave:\n\
         \n  syso --planificador local \"…\"",
        ruta.display(),
        ruta.display(),
        ruta.display()
    )
}

fn avisar_si_es_legible_por_otros(ruta: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(ruta) {
        let modo = meta.permissions().mode() & 0o077;
        if modo != 0 {
            eprintln!(
                "aviso: {} es legible por otros usuarios. Arréglalo con:\n  chmod 600 {}",
                ruta.display(),
                ruta.display()
            );
        }
    }
}

impl Planner for ClaudePlanner {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn plan(&self, intent: &str, catalog: &Catalog) -> Result<Propuesta> {
        let body = json!({
            "model": self.model,
            "max_tokens": 16000,
            "system": format!("{SYSTEM}\n\nCapacidades disponibles:\n\n{}", catalogo_como_texto(catalog)),
            "tools": [herramienta_plan(catalog)],
            // tool_choice queda en automático a propósito: si la intención no
            // se puede expresar con las capacidades disponibles, el modelo
            // tiene que poder decirlo en vez de verse forzado a inventarse
            // una llamada que encaje a medias.
            "fallbacks": "default",
            "messages": [{"role": "user", "content": intent}],
        });

        // http_status_as_error(false): queremos LEER el cuerpo del error de
        // la API, no solo su código de estado.
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();

        let mut resp = agent
            .post(URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("anthropic-beta", "server-side-fallback-2026-07-01")
            .header("content-type", "application/json")
            .send_json(&body)
            .context("no pude contactar con la API de Anthropic")?;

        let status = resp.status().as_u16();
        let v: Value = resp.body_mut().read_json().context("respuesta ilegible")?;

        if status >= 400 {
            let msg = v["error"]["message"].as_str().unwrap_or("error desconocido");
            bail!("la API respondió {status}: {msg}");
        }

        // Un rechazo llega como HTTP 200. Hay que mirar stop_reason.
        if v["stop_reason"] == "refusal" {
            let why = v["stop_details"]["explanation"]
                .as_str()
                .unwrap_or("sin explicación");
            bail!("el modelo declinó planificar esta intención: {why}");
        }

        let blocks = v["content"]
            .as_array()
            .ok_or_else(|| anyhow!("respuesta sin contenido"))?;

        let mut steps = Vec::new();
        let mut said = String::new();
        for block in blocks {
            match block["type"].as_str() {
                Some("tool_use") if block["name"] == PLAN_TOOL => {
                    let entrada = &block["input"];
                    if let Some(nota) = entrada["nota"].as_str() {
                        said.push_str(nota);
                    }
                    for paso in entrada["pasos"].as_array().into_iter().flatten() {
                        let capability = paso["capacidad"].as_str().unwrap_or_default().to_string();
                        let mut args = BTreeMap::new();
                        if let Some(obj) = paso["argumentos"].as_object() {
                            for (k, val) in obj {
                                // Los valores no-string se serializan tal cual:
                                // la validación del catálogo decidirá si valen.
                                let s = match val {
                                    Value::String(s) => s.clone(),
                                    other => other.to_string(),
                                };
                                args.insert(k.clone(), s);
                            }
                        }
                        steps.push(Step { capability, args });
                    }
                }
                Some("text") => said.push_str(block["text"].as_str().unwrap_or_default()),
                _ => {}
            }
        }

        let said = said.trim().to_string();
        if steps.is_empty() {
            if said.is_empty() {
                bail!("el modelo no propuso ninguna capacidad");
            }
            bail!("no se pudo planificar con las capacidades disponibles.\n{said}");
        }

        Ok(Propuesta {
            steps,
            nota: (!said.is_empty()).then_some(said),
        })
    }
}

const SYSTEM: &str = "\
Eres el planificador de syso, un sistema operativo para desarrolladores.

Traduces la intención del usuario a llamadas de las capacidades disponibles.
Reglas:
- Solo puedes usar las capacidades ofrecidas como herramientas. No existe una \
shell ni ninguna otra vía.
- NO HAY SEGUNDA VUELTA. No vas a ver el resultado de tus llamadas ni podrás \
continuar después. Emite el plan COMPLETO en esta única respuesta.
- Los pasos se ejecutan en orden, así que puedes dar por hecho que lo que crea \
un paso existe para los siguientes: declarar una dependencia en un proyecto que \
creas dos líneas más arriba es correcto.
- No ejecutas nada: tus llamadas son una propuesta que el usuario revisará.
- Si la intención no se puede expresar con las capacidades disponibles, no \
llames a ninguna: explica en texto qué falta.
- Las rutas son relativas al espacio de trabajo. Nunca uses rutas absolutas ni '..'.";

/// UNA sola herramienta, cuyo argumento es el plan entero.
///
/// Antes había una herramienta por capacidad, y el modelo emitía una llamada
/// y paraba: es lo natural: una herramienta se invoca para VER su resultado y
/// seguir. Pero aquí no hay bucle de agente — el modelo emite un plan que una
/// persona aprueba de una vez.
///
/// Pedir «devuélveme un plan» en vez de «llama a las capacidades» alinea la
/// petición con lo que la arquitectura decía desde el principio, y de paso
/// hace imposible el plan a medias.
fn herramienta_plan(catalog: &Catalog) -> Value {
    let nombres: Vec<&str> = catalog.caps.keys().map(String::as_str).collect();

    json!({
        "name": PLAN_TOOL,
        "description": "Devuelve el plan COMPLETO que cubre la intención del usuario.",
        "input_schema": {
            "type": "object",
            "properties": {
                "nota": {
                    "type": "string",
                    "description": "Una frase para el usuario explicando el plan."
                },
                "pasos": {
                    "type": "array",
                    "description": "Las invocaciones, en el orden en que deben ejecutarse.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "capacidad": { "type": "string", "enum": nombres },
                            "argumentos": {
                                "type": "object",
                                "description": "Los parámetros de esa capacidad. Todos los valores son cadenas.",
                                "additionalProperties": { "type": "string" }
                            }
                        },
                        "required": ["capacidad", "argumentos"]
                    }
                }
            },
            "required": ["pasos"]
        }
    })
}

/// El catálogo, para que el modelo sepa qué puede pedir. Sale de los
/// manifiestos: no hay una segunda descripción que mantener sincronizada.
fn catalogo_como_texto(catalog: &Catalog) -> String {
    let mut texto = String::new();
    for cap in catalog.caps.values() {
        texto.push_str(&format!("- {}: {}\n", cap.name, cap.summary));
        for (nombre, spec) in &cap.params {
            let opcional = if spec.optional || spec.default.is_some() { " (opcional)" } else { "" };
            let tipo = if spec.of.is_empty() {
                spec.kind.clone()
            } else {
                format!("uno de [{}]", spec.of.join(", "))
            };
            texto.push_str(&format!("    {nombre}: {tipo}{opcional}\n"));
        }
    }
    texto
}
