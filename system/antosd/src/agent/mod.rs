//! Runtime de agente: bucle de herramientas sobre el catálogo (T33.2).
//!
//! Un `Planner` (`session::intent_session`) convierte UNA intención en UN
//! plan, de un solo disparo y sin ver el resultado. Construir software
//! exige un bucle: leer, razonar, editar, probar, corregir. Este módulo lo
//! cierra sin abrir ninguna puerta nueva:
//!
//! - las herramientas del modelo SON capacidades del catálogo (un
//!   subconjunto, el *toolset*), más la terminal `finalizar`;
//! - cada `tool_use` pasa por lo mismo que un paso de intención:
//!   `Catalog::validate` → `Blast::compute` → tier (`confirm`/`grant` piden
//!   permiso por el `AgentHandler`, como `on_proposal`) → instantánea
//!   incremental del run → ejecución en el recinto (`sandbox::run`) →
//!   registro. El resultado (o el rechazo) vuelve al modelo como
//!   `tool_result`;
//! - el modelo nunca ejecuta texto libre: no hay `shell.run`, y lo que no
//!   está en el toolset no existe (T31.4). Un rechazo de validación se le
//!   devuelve como error de herramienta, no se corrige en silencio;
//! - presupuesto de pasos, tokens y tiempo; al agotarse, el run termina
//!   limpio con su `AgentReport`;
//! - un único registro de journal por run con la instantánea de todo lo
//!   escrito: `antos undo` deshace el run entero.
//!
//! ## Estado de implementación
//!
//! Real: todo lo anterior, con proveedores Claude (Messages API, `tool_use`),
//! Ollama (`/api/chat`) y OpenAI-compatible (`/chat/completions`), y el
//! proveedor `fake` determinista para tests y CI. Lo que NO hace: roles
//! antFlow (T33.3), interfaz en vivo en la barra más allá de los eventos
//! (T33.4), evaluación (T33.5).

pub mod eval;
pub mod fake;
pub mod providers;
pub mod roles;
pub mod tools;

use crate::blast::Blast;
use crate::capability::{Catalog, Reversible, Tier};
use crate::ctx::Ctx;
use crate::grants::Grants;
use crate::journal::{self, Outcome, Record};
use crate::plan::{self, Plan, Step};
use crate::protocol::{BlastRadius, Enclosure, Proposal};
use crate::{exec, preview, sandbox, snapshot};
use antos_protocol::{AgentBudget, AgentReport, AgentStepEvent, AgentStepOutcome, AgentStopReason};
use anyhow::{Context, Result};
use providers::{AgentProvider, ToolCall, ToolResult};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Instant;
use tools::FINISH_TOOL;

/// Máximo de caracteres de un resultado de herramienta devuelto al modelo.
/// Más que esto no cabe con provecho en el contexto; se recorta el centro.
const MAX_TOOL_OUTPUT: usize = 24_000;
/// Máximo de caracteres del `output_preview` de un evento (para la barra).
const MAX_PREVIEW: usize = 400;
/// Recordatorios que se envían a un modelo que responde sin herramientas
/// antes de darlo por parado (`ModelStopped`).
const MAX_NUDGES: u32 = 1;
const NUDGE_TEXT: &str = "Recuerda: solo puedes actuar llamando a las herramientas disponibles, \
    no describiendo lo que harías. Si el objetivo ya está cumplido y verificado, llama a \
    `finalizar` con el resumen; si no, llama a la siguiente herramienta.";

/// Petición de cierre estructurado (T34.4), tras agotar los recordatorios.
const STRUCTURED_FINISH_TEXT: &str = "No has llamado a ninguna herramienta. Da la tarea por \
    terminada ahora: responde ÚNICAMENTE con el JSON de `finalizar` según el esquema, con un \
    `resumen` honesto de lo hecho y de lo que no pudiste hacer.";

/// Petición de cierre cuando el modelo repite la misma llamada fallida
/// (T35.3): que resuma lo hecho en vez de seguir chocando.
const LOOPING_FINISH_TEXT: &str = "Has repetido varias veces la misma llamada y sigue fallando. \
    Para aquí. Responde ÚNICAMENTE con el JSON de `finalizar` según el esquema: en `resumen`, \
    qué conseguiste (por ejemplo, si los tests ya pasan) y qué quedó sin hacer.";

/// Prefijo de los rechazos por argumentos (el reintento gratuito de T34.4
/// distingue así un argumento mal formado de una denegación de política).
pub(crate) const ARGS_REJECT_PREFIX: &str = "argumentos rechazados";

/// Veces que la MISMA llamada (herramienta + argumentos) puede fallar en un
/// run antes de cortarlo como `Looping` (T34.4; desde T35.3 no hace falta
/// que sean seguidas). A la segunda se le dice al modelo, con claridad, que
/// cambie de estrategia; a la tercera se para: gastar 18 pasos en el mismo
/// `fs.patch` no arregla nada.
const MAX_IDENTICAL_FAILURES: u32 = 3;

/// Configuración de un run.
#[derive(Debug, Clone)]
pub struct RunConfig {
    pub goal: String,
    /// Capacidades permitidas (sin `finalizar`, que siempre está).
    pub toolset: Vec<String>,
    pub budget: AgentBudget,
    /// Con `dry_run`, cada herramienta que escribe se muestra pero no se
    /// ejecuta; al modelo se le devuelve que fue «simulada».
    pub dry_run: bool,
    /// Prompt de sistema; `None` → el genérico de `system_prompt()`.
    pub system_prompt: Option<String>,
    /// Contexto extra que se antepone al objetivo (ticket, fallo de tests…).
    pub context: Option<String>,
    /// Esquema tipado de `finalizar` (T33.3); `None` → solo `resumen`.
    pub finish: Option<tools::FinishSpec>,
    /// Toolset alternativo cuando el proveedor declara un modelo pequeño
    /// (`ModelScale::Small`, T34.4). `None` → siempre `toolset`.
    pub toolset_compact: Option<Vec<String>>,
    /// Cómo se ejecutan los cambios. Siempre `Confined` salvo en los tests
    /// del propio runtime: el ejecutor confinado relanza el binario de
    /// `antos`, que no existe dentro de un binario de `cargo test`.
    pub(crate) executor: Executor,
}

/// Dónde corren los cambios de una herramienta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Executor {
    /// El recinto (`sandbox::run`): Landlock/Seatbelt, cuota, sin secretos.
    Confined,
    /// En este mismo proceso (`exec::apply`). Solo tests.
    #[cfg_attr(not(test), allow(dead_code))]
    InProcess,
}

impl RunConfig {
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            goal: goal.into(),
            toolset: tools::DEFAULT_TOOLSET
                .iter()
                .map(|s| s.to_string())
                .collect(),
            budget: AgentBudget::default(),
            dry_run: false,
            system_prompt: None,
            context: None,
            finish: None,
            toolset_compact: Some(
                tools::DEFAULT_TOOLSET_COMPACT
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            ),
            executor: Executor::Confined,
        }
    }
}

/// Quien mira el run: la barra, el CLI o un test.
pub trait AgentHandler {
    /// Un paso ejecutado (o rechazado, o fallido), en cuanto ocurre.
    fn on_step(&mut self, event: &AgentStepEvent) -> Result<()>;
    /// Un paso `confirm`/`grant` pide permiso; devolver `false` aborta el run.
    fn on_confirm(&mut self, proposal: &Proposal) -> Result<bool>;
    /// Texto del asistente entre herramientas.
    fn on_note(&mut self, text: &str) -> Result<()>;
    /// Informe final.
    fn on_done(&mut self, report: &AgentReport) -> Result<()>;
    /// ¿Ha pedido el usuario detener el run? Se consulta antes de cada
    /// herramienta y entre turnos; la herramienta en curso termina. Por
    /// defecto nunca.
    fn should_stop(&mut self) -> bool {
        false
    }
}

/// Prompt de sistema genérico. Los roles de T33.3 traen el suyo.
///
/// Redactado para que también lo siga un modelo de 7B (T34.4): frases
/// cortas, un ejemplo de llamada, y la regla más violada en las medidas
/// («no pidas el fichero al usuario: léelo») en primera posición.
pub fn system_prompt() -> String {
    "Eres un agente de antOS. Trabajas sobre un repositorio de software al que tienes acceso \
     con herramientas. Actúas SOLO llamando a herramientas; nunca pidas al usuario que te \
     pegue un fichero ni preguntes nada: si necesitas ver un fichero, llámalo tú con fs.read.\n\
     Ejemplo de llamada: fs.read con {\"path\": \"src/lib.rs\"}.\n\
     Método, en este orden:\n\
     1. fs.read del fichero que nombra el objetivo (y fs.list si no sabes dónde está).\n\
     2. fs.patch con un cambio pequeño: `old` es un fragmento EXACTO del fichero que acabas de \
     leer (copia el texto tal cual, con su sangría), `new` es el texto nuevo.\n\
     3. test.run para verificar.\n\
     4. Si test.run falla, vuelve a leer y corrige. Si pasa, llama a `finalizar` con un \
     resumen honesto.\n\
     Reglas: nunca modifiques un test para que pase (arregla el código que prueba); no repitas \
     una llamada que ya falló igual; cada paso consume presupuesto. En cuanto test.run esté en \
     verde, `finalizar` inmediatamente. Si no puedes cumplir el objetivo, `finalizar` diciendo \
     por qué."
        .to_string()
}

/// Ejecuta un run completo. Devuelve el informe también en caso de error del
/// proveedor: el llamador siempre sabe qué pasó y qué instantánea existe.
pub fn run(
    ctx: &Ctx,
    catalog: &Catalog,
    provider: &mut dyn AgentProvider,
    cfg: &RunConfig,
    handler: &mut dyn AgentHandler,
) -> Result<AgentReport> {
    let started = Instant::now();
    let run_id = format!("agent-{}", plan::new_id());
    // Modelo pequeño → toolset compacto (T34.4), si el run lo define.
    let toolset: &[String] = match (&cfg.toolset_compact, provider.model_scale()) {
        (Some(compact), providers::ModelScale::Small) => compact,
        _ => &cfg.toolset,
    };
    if toolset.len() != cfg.toolset.len() {
        handler.on_note(&format!(
            "modelo pequeño ({}): toolset compacto [{}]",
            provider.model(),
            toolset.join(", ")
        ))?;
    }
    let tool_specs = tools::build_toolset(catalog, toolset, cfg.finish.as_ref())?;
    let allowed: BTreeSet<&str> = toolset.iter().map(String::as_str).collect();
    // Herramientas que ya tuvieron su reintento gratuito por argumentos
    // malformados (uno por herramienta y run, T34.4).
    let mut retried_args: BTreeSet<String> = BTreeSet::new();
    // Cuántas veces ha fallado cada llamada idéntica (herramienta +
    // argumentos) en el run. No hace falta que sean seguidas: el patrón
    // real observado es fs.read → fs.patch (falla) → fs.read → el MISMO
    // fs.patch…, y una lectura entre medias no lo convierte en progreso.
    let mut failure_counts: BTreeMap<String, u32> = BTreeMap::new();
    let grants = Grants::load(&ctx.grants_path()).unwrap_or_default();
    let jail = sandbox::for_host();

    let mut state = RunState {
        run_id: run_id.clone(),
        steps: 0,
        tokens: 0,
        snapshot: None,
        files_written: BTreeSet::new(),
        executed_steps: Vec::new(),
    };
    let mut finish_input: Option<String> = None;

    let system = cfg.system_prompt.clone().unwrap_or_else(system_prompt);
    let user = match &cfg.context {
        Some(c) if !c.trim().is_empty() => format!("{c}\n\nObjetivo: {}", cfg.goal),
        _ => cfg.goal.clone(),
    };

    let mut turn = provider.start(&system, &user, &tool_specs);
    let mut nudges = 0u32;
    let mut last_text = String::new();
    let (stop_reason, summary, error) = loop {
        let current = match turn {
            Ok(t) => t,
            Err(e) => {
                break (
                    AgentStopReason::Error,
                    String::new(),
                    Some(format!("{e:#}")),
                )
            }
        };
        state.tokens += current.tokens;
        if !current.text.trim().is_empty() {
            handler.on_note(current.text.trim())?;
            last_text = current.text.trim().to_string();
        }
        if current.calls.is_empty() {
            // El modelo respondió en prosa sin herramientas. Un modelo pequeño
            // hace esto a menudo aunque le quede trabajo (o aunque haya
            // terminado, sin llamar a `finalizar`): un recordatorio, y solo
            // uno, antes de darlo por parado.
            if nudges < MAX_NUDGES && !handler.should_stop() {
                nudges += 1;
                turn = provider.nudge(NUDGE_TEXT, &tool_specs);
                continue;
            }
            // Último recurso (T34.4): si el proveedor sabe forzar formato, se
            // le pide el cierre como JSON del esquema de `finalizar`. Un
            // modelo pequeño que «tiene» el plan pero lo cuenta en prosa
            // termina así con un cierre tipado en vez de `ModelStopped`.
            if !handler.should_stop() {
                if let Some(summary) = structured_finish(
                    provider,
                    cfg,
                    &tool_specs,
                    STRUCTURED_FINISH_TEXT,
                    &run_id,
                    &mut state,
                    &mut finish_input,
                    handler,
                )? {
                    break (AgentStopReason::Finished, summary, None);
                }
            }
            break (AgentStopReason::ModelStopped, last_text.clone(), None);
        }

        let mut results = Vec::with_capacity(current.calls.len());
        let mut finished: Option<String> = None;
        let mut declined = false;
        let mut stopped = false;
        let mut looping = false;
        for call in &current.calls {
            if finished.is_some() || declined || stopped {
                // Ya no se ejecuta nada más de este turno; pero cada llamada
                // recibe SU resultado para no dejar `tool_use` sin respuesta.
                results.push(ToolResult {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    content: "no ejecutada: el run ya terminó en este turno".into(),
                    is_error: true,
                });
                continue;
            }
            if call.name == FINISH_TOOL {
                let summary = call
                    .input
                    .get("resumen")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(sin resumen)")
                    .to_string();
                finish_input = Some(call.input.to_string());
                state.steps += 1;
                handler.on_step(&AgentStepEvent {
                    run_id: run_id.clone(),
                    step: state.steps,
                    tool: FINISH_TOOL.into(),
                    args_summary: preview_text(&summary, MAX_PREVIEW),
                    outcome: AgentStepOutcome::Finished,
                    output_preview: String::new(),
                    tokens_used: state.tokens,
                })?;
                finished = Some(summary);
                results.push(ToolResult {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    content: "ok".into(),
                    is_error: false,
                });
                continue;
            }
            // Parada pedida por el usuario (T33.4): ninguna herramienta más.
            if handler.should_stop() {
                stopped = true;
                results.push(ToolResult {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    content: "no ejecutada: el usuario detuvo el run".into(),
                    is_error: true,
                });
                continue;
            }
            // Presupuesto antes de cada herramienta.
            if let Some(reason) = over_budget(&cfg.budget, &state, started) {
                results.push(ToolResult {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    content: format!("no ejecutada: {reason}"),
                    is_error: true,
                });
                continue;
            }
            state.steps += 1;
            let (mut result, outcome, declined_now) = execute_tool(
                ctx, catalog, &allowed, &grants, &*jail, cfg, &mut state, call, handler,
            )?;
            declined = declined_now;
            // La misma llamada fallando una y otra vez (T34.4).
            let failed = matches!(
                outcome,
                AgentStepOutcome::Failed | AgentStepOutcome::Rejected
            );
            if failed {
                let key = format!("{}:{}", call.name, call.input);
                let count = failure_counts.entry(key).or_insert(0);
                *count += 1;
                if *count >= MAX_IDENTICAL_FAILURES {
                    looping = true;
                } else if *count == 2 {
                    result.content.push_str(
                        "\nYa has hecho exactamente esta misma llamada y ha fallado igual. \
                         No la repitas: corrige lo que dice el error (o lee el fichero con \
                         fs.read y usa el texto EXACTO), o cambia de enfoque.",
                    );
                }
            }
            // Argumentos malformados (no política): la primera vez por
            // herramienta se devuelve el esquema y no cuenta como paso, para
            // que un modelo pequeño corrija sin pagar presupuesto (T34.4).
            if outcome == AgentStepOutcome::Rejected
                && result.content.starts_with(ARGS_REJECT_PREFIX)
                && retried_args.insert(call.name.clone())
            {
                state.steps -= 1;
                if let Some(spec) = tool_specs.iter().find(|t| t.name == call.name) {
                    result.content.push_str(&format!(
                        "\nEsquema de entrada de {}: {}\nCorrige los argumentos y vuelve a llamar (este intento no consume presupuesto).",
                        call.name, spec.input_schema
                    ));
                }
            }
            handler.on_step(&AgentStepEvent {
                run_id: run_id.clone(),
                step: state.steps,
                tool: call.name.clone(),
                args_summary: tools::args_from_input(&call.input)
                    .map(|a| tools::summarize_args(&a))
                    .unwrap_or_else(|_| "(argumentos no válidos)".into()),
                outcome,
                output_preview: preview_text(&result.content, MAX_PREVIEW),
                tokens_used: state.tokens,
            })?;
            results.push(result);
        }

        if let Some(summary) = finished {
            break (AgentStopReason::Finished, summary, None);
        }
        if declined {
            break (
                AgentStopReason::Declined,
                current.text.trim().to_string(),
                None,
            );
        }
        if stopped || handler.should_stop() {
            break (
                AgentStopReason::Stopped,
                "detenido por el usuario".to_string(),
                None,
            );
        }
        if looping {
            // Antes de cortar, una salida digna (T35.3): a veces el trabajo
            // ya está hecho (tests en verde) y el modelo se atasca en un
            // parche que ya no aplica. Se le pide el cierre tipado; si no lo
            // da, `Looping`.
            if !handler.should_stop() {
                if let Some(summary) = structured_finish(
                    provider,
                    cfg,
                    &tool_specs,
                    LOOPING_FINISH_TEXT,
                    &run_id,
                    &mut state,
                    &mut finish_input,
                    handler,
                )? {
                    break (AgentStopReason::Finished, summary, None);
                }
            }
            break (
                AgentStopReason::Looping,
                format!(
                    "el modelo repitió {MAX_IDENTICAL_FAILURES} veces la misma llamada fallida; run cortado"
                ),
                None,
            );
        }
        if let Some(reason) = over_budget(&cfg.budget, &state, started) {
            break (
                AgentStopReason::BudgetExhausted,
                format!("presupuesto agotado: {reason}"),
                None,
            );
        }
        turn = provider.continue_with(&results, &tool_specs);
    };

    // Un registro por run: `antos undo` deshace el run entero.
    if !state.executed_steps.is_empty() && !cfg.dry_run {
        let record = Record {
            id: run_id.clone(),
            at: chrono::Local::now().to_rfc3339(),
            intent: format!("agente: {}", cfg.goal),
            ticket_id: journal::extract_ticket_id(&cfg.goal),
            planner: format!("agent:{}", provider.name()),
            plan: Plan {
                id: run_id.clone(),
                intent: cfg.goal.clone(),
                planner: provider.name(),
                steps: state.executed_steps.clone(),
            },
            tier: Tier::Confirm,
            reasons: vec![format!(
                "run de agente con {} pasos",
                state.executed_steps.len()
            )],
            outcome: Outcome::Executed,
            detail: Some(format!("{stop_reason:?}")),
            snapshot: state.snapshot.as_ref().map(|s| s.id.clone()),
            sandbox: jail.name().to_string(),
            reverted: false,
        };
        journal::append(&ctx.journal_path(), &record)?;
    }

    let report = AgentReport {
        run_id,
        goal: cfg.goal.clone(),
        provider: provider.name(),
        model: provider.model(),
        stop_reason,
        summary,
        steps: state.steps,
        tokens_used: state.tokens,
        seconds: started.elapsed().as_secs(),
        snapshot_id: state.snapshot.as_ref().map(|s| s.id.clone()),
        files_written: state.files_written.iter().map(|p| ctx.display(p)).collect(),
        error,
        result_json: finish_input,
        context_window: provider.context_window(),
        context_note: provider.context_note(),
    };
    handler.on_done(&report)?;
    Ok(report)
}

/// Pide al proveedor el JSON de `finalizar` y, si lo da, lo registra como
/// el paso terminal. `None` si el proveedor no sabe forzar formato o el
/// modelo no devuelve JSON válido.
#[allow(clippy::too_many_arguments)]
fn structured_finish(
    provider: &mut dyn AgentProvider,
    cfg: &RunConfig,
    tool_specs: &[tools::ToolSpec],
    prompt: &str,
    run_id: &str,
    state: &mut RunState,
    finish_input: &mut Option<String>,
    handler: &mut dyn AgentHandler,
) -> Result<Option<String>> {
    let schema = cfg
        .finish
        .as_ref()
        .map(|f| f.input_schema.clone())
        .unwrap_or_else(tools::default_finish_schema);
    let Some(Ok(value)) = provider.finish_structured(prompt, &schema, tool_specs) else {
        return Ok(None);
    };
    let summary = value
        .get("resumen")
        .and_then(|v| v.as_str())
        .unwrap_or("(sin resumen)")
        .to_string();
    *finish_input = Some(value.to_string());
    state.steps += 1;
    handler.on_step(&AgentStepEvent {
        run_id: run_id.to_string(),
        step: state.steps,
        tool: FINISH_TOOL.into(),
        args_summary: preview_text(&summary, MAX_PREVIEW),
        outcome: AgentStepOutcome::Finished,
        output_preview: "(cierre estructurado)".into(),
        tokens_used: state.tokens,
    })?;
    Ok(Some(summary))
}

struct RunState {
    run_id: String,
    steps: u32,
    tokens: u64,
    snapshot: Option<snapshot::Snapshot>,
    files_written: BTreeSet<PathBuf>,
    executed_steps: Vec<Step>,
}

fn over_budget(budget: &AgentBudget, state: &RunState, started: Instant) -> Option<String> {
    if state.steps >= budget.max_steps {
        return Some(format!(
            "{} pasos (máximo {})",
            state.steps, budget.max_steps
        ));
    }
    if state.tokens >= budget.max_tokens {
        return Some(format!(
            "{} tokens (máximo {})",
            state.tokens, budget.max_tokens
        ));
    }
    let secs = started.elapsed().as_secs();
    if secs >= budget.max_seconds {
        return Some(format!("{secs} s (máximo {})", budget.max_seconds));
    }
    None
}

/// Una llamada a herramienta, de principio a fin. Devuelve el resultado
/// para el modelo, el desenlace para la interfaz y si el usuario rechazó.
#[allow(clippy::too_many_arguments)]
fn execute_tool(
    ctx: &Ctx,
    catalog: &Catalog,
    allowed: &BTreeSet<&str>,
    grants: &Grants,
    jail: &dyn sandbox::Sandbox,
    cfg: &RunConfig,
    state: &mut RunState,
    call: &ToolCall,
    handler: &mut dyn AgentHandler,
) -> Result<(ToolResult, AgentStepOutcome, bool)> {
    let reject = |msg: String| {
        (
            ToolResult {
                id: call.id.clone(),
                name: call.name.clone(),
                content: msg,
                is_error: true,
            },
            AgentStepOutcome::Rejected,
            false,
        )
    };

    // 1 · ¿Está en el toolset? Lo que no está, no existe.
    if !allowed.contains(call.name.as_str()) {
        return Ok(reject(format!(
            "herramienta no disponible en este run: {}",
            call.name
        )));
    }
    let cap = match catalog.get(&call.name) {
        Ok(c) => c,
        Err(e) => return Ok(reject(format!("{e:#}"))),
    };

    // 2 · Argumentos validados por el catálogo (como un paso de intención).
    let mut args = match tools::args_from_input(&call.input) {
        Ok(a) => a,
        Err(e) => return Ok(reject(format!("{ARGS_REJECT_PREFIX}: {e:#}"))),
    };
    if let Err(e) = catalog.validate(cap, &mut args) {
        return Ok(reject(format!(
            "{ARGS_REJECT_PREFIX} por el catálogo: {e:#}"
        )));
    }
    let step = Step {
        capability: call.name.clone(),
        args,
    };

    // 3 · Radio de impacto del paso, antes de tocar nada.
    let step_plan = Plan {
        id: format!("{}-{}", state.run_id, state.steps),
        intent: cfg.goal.clone(),
        planner: "agent".into(),
        steps: vec![step.clone()],
    };
    let radius = match Blast::compute(
        &step_plan,
        catalog,
        &ctx.workspace,
        &ctx.system_config,
        &ctx.state,
    ) {
        Ok(r) => r,
        Err(e) => return Ok(reject(format!("radio de impacto no calculable: {e:#}"))),
    };
    if !radius.escapes.is_empty() {
        return Ok(reject(format!(
            "denegado: el paso toca rutas fuera del espacio de trabajo: {}",
            radius
                .escapes
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    let (tier, reasons) = radius.required_tier();
    if tier == Tier::Grant && !grants.is_granted(&call.name) {
        return Ok(reject(format!(
            "denegado: {} requiere una concesión explícita (antos grant {} --minutes 10)",
            call.name, call.name
        )));
    }

    // 4 · Cambios concretos.
    let pending = exec::PendingChanges::default();
    let changes = match exec::changes_for(&step, cap, ctx, &pending) {
        Ok(c) => c,
        Err(e) => return Ok(reject(format!("{e:#}"))),
    };

    // 5 · Puerta de confirmación: la misma `Proposal` que ve una intención.
    if tier != Tier::Auto {
        let proposal = Proposal {
            changes: preview::render(ctx, &changes),
            blast_radius: BlastRadius {
                writes: radius.writes.iter().map(|p| ctx.display(p)).collect(),
                deletes: radius.deletes.iter().map(|p| ctx.display(p)).collect(),
                reads: radius.reads.iter().map(|p| ctx.display(p)).collect(),
                system: radius
                    .system
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect(),
                network: radius.network.iter().cloned().collect(),
            },
            tier,
            reasons,
            enclosure: Enclosure {
                engine: jail.name().to_string(),
                guarantees: jail.guarantees().to_string(),
            },
            dry_run: cfg.dry_run,
            plan: step_plan.clone(),
        };
        if !handler.on_confirm(&proposal)? {
            return Ok((
                ToolResult {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    content: "el usuario rechazó este paso; el run termina".into(),
                    is_error: true,
                },
                AgentStepOutcome::Declined,
                true,
            ));
        }
    }

    if cfg.dry_run && cap.policy.reversible != Reversible::Unnecessary {
        return Ok((
            ToolResult {
                id: call.id.clone(),
                name: call.name.clone(),
                content: "simulada (dry-run): no se ha ejecutado".into(),
                is_error: false,
            },
            AgentStepOutcome::Executed,
            false,
        ));
    }

    // 6 · Instantánea incremental del run: solo lo que declara `snapshot`.
    //     `test.run` escribe `target/` pero es `unnecessary`: fotografiarlo
    //     sería fotografiar gigabytes por cada ejecución de tests.
    if cap.policy.reversible == Reversible::Snapshot {
        let to_snapshot = radius.paths_to_snapshot();
        if !to_snapshot.is_empty() {
            let snap = state.snapshot.get_or_insert_with(|| snapshot::Snapshot {
                id: state.run_id.clone(),
                entries: Vec::new(),
            });
            snapshot::extend(snap, &to_snapshot, &ctx.snapshots_dir())
                .context("fotografiando antes del paso del agente")?;
            state.files_written.extend(to_snapshot);
        }
    }

    // 7 · Ejecución en el recinto, con la política del paso.
    let policy = sandbox::Policy::from_blast(&radius).with_grants(grants, &ctx.workspace);
    let outcome = match cfg.executor {
        Executor::Confined => sandbox::run(jail, &changes, &policy),
        Executor::InProcess => exec::apply(&changes),
    };
    match outcome {
        Ok(outputs) => {
            state.executed_steps.push(step);
            let joined = outputs
                .iter()
                .filter(|o| !o.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            let content = if joined.is_empty() {
                "ok".to_string()
            } else {
                clip_middle(&joined, MAX_TOOL_OUTPUT)
            };
            Ok((
                ToolResult {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    content,
                    is_error: false,
                },
                AgentStepOutcome::Executed,
                false,
            ))
        }
        Err(e) => Ok((
            ToolResult {
                id: call.id.clone(),
                name: call.name.clone(),
                content: clip_middle(&format!("error al ejecutar: {e:#}"), MAX_TOOL_OUTPUT),
                is_error: true,
            },
            AgentStepOutcome::Failed,
            false,
        )),
    }
}

/// Recorta por el centro conservando cabeza y cola: en una salida de tests,
/// el resumen está al final y el primer fallo al principio.
fn clip_middle(text: &str, max: usize) -> String {
    let n = text.chars().count();
    if n <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max / 2).collect();
    let tail: String = text.chars().skip(n - max / 2).collect();
    format!("{head}\n… [{} caracteres omitidos] …\n{tail}", n - max)
}

fn preview_text(text: &str, max: usize) -> String {
    let flat = text.replace('\n', " ⏎ ");
    if flat.chars().count() <= max {
        flat
    } else {
        format!("{}…", flat.chars().take(max).collect::<String>())
    }
}

/// La terminal como observador de un run: pasos como notas, confirmación
/// por la misma pregunta que una intención.
impl AgentHandler for crate::terminal::Terminal {
    fn on_step(&mut self, event: &AgentStepEvent) -> Result<()> {
        crate::protocol::SessionHandler::on_note(self, &crate::ipc::format_agent_step(event))
    }
    fn on_confirm(&mut self, proposal: &Proposal) -> Result<bool> {
        crate::protocol::SessionHandler::on_proposal(self, proposal)
    }
    fn on_note(&mut self, text: &str) -> Result<()> {
        crate::protocol::SessionHandler::on_note(self, text)
    }
    fn on_done(&mut self, report: &AgentReport) -> Result<()> {
        crate::protocol::SessionHandler::on_note(self, &crate::ipc::format_agent_report(report))
    }
}

#[cfg(test)]
mod tests;
