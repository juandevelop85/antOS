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
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Instant;
use tools::FINISH_TOOL;

/// Máximo de caracteres de un resultado de herramienta devuelto al modelo.
/// Más que esto no cabe con provecho en el contexto; se recorta el centro.
const MAX_TOOL_OUTPUT: usize = 24_000;
/// Máximo de caracteres del `output_preview` de un evento (para la barra).
const MAX_PREVIEW: usize = 400;

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
pub fn system_prompt() -> String {
    "Eres un agente de antOS que trabaja sobre un espacio de trabajo de software.\n\
     Solo puedes actuar con las herramientas que se te ofrecen: cada una es una capacidad \
     declarada del sistema, validada y confinada. No existe ninguna forma de ejecutar \
     comandos arbitrarios; si algo no se puede hacer con las herramientas, dilo.\n\
     Método: orienta primero (fs.list, fs.read, memory.search), cambia lo mínimo con \
     fs.patch (bloques exactos, únicos), verifica con test.run, y corrige lo que falle. \
     Cuando el objetivo esté cumplido y verificado —o cuando no puedas cumplirlo— llama a \
     `finalizar` con un resumen honesto. No repitas herramientas sin motivo: cada paso \
     consume presupuesto."
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
    let tool_specs = tools::build_toolset(catalog, &cfg.toolset, cfg.finish.as_ref())?;
    let allowed: BTreeSet<&str> = cfg.toolset.iter().map(String::as_str).collect();
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
        }
        if current.calls.is_empty() {
            // El modelo respondió con texto y sin herramientas: se acabó.
            break (
                AgentStopReason::ModelStopped,
                current.text.trim().to_string(),
                None,
            );
        }

        let mut results = Vec::with_capacity(current.calls.len());
        let mut finished: Option<String> = None;
        let mut declined = false;
        let mut stopped = false;
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
            let (result, outcome, declined_now) = execute_tool(
                ctx, catalog, &allowed, &grants, &*jail, cfg, &mut state, call, handler,
            )?;
            declined = declined_now;
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
    };
    handler.on_done(&report)?;
    Ok(report)
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
        Err(e) => return Ok(reject(format!("{e:#}"))),
    };
    if let Err(e) = catalog.validate(cap, &mut args) {
        return Ok(reject(format!(
            "argumentos rechazados por el catálogo: {e:#}"
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
