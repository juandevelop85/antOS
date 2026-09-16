//! Pipeline REAL de antFlow sobre el runtime de agente (T33.3).
//!
//! Cada fase es un `agent::run` con el modelo del rol, su toolset y su
//! prompt (`agent::roles`): Arquitecto (solo lectura, workspace) →
//! `ImplementationPlan`; Coder (edita en el worktree) → QA (`test.run`
//! real; el fallo vuelve al Coder hasta `max_qa_retries`) → Auditor (solo
//! lectura) → `AuditVerdict`. La máquina de estados, la persistencia y la
//! aprobación humana final viven en `flow/mod.rs`; aquí solo el recorrido.

use super::{calculate_worktree_diff, now_secs, save_task_to_disk, FlowEngine};
use antos_protocol::{
    AgentReportSummary, AgentRole, AgentStopReason, AuditVerdict, FlowBackend, FlowState, FlowTask,
    FlowTransition, ImplementationPlan,
};
use anyhow::Result;

impl FlowEngine {
    /// Pipeline REAL (T33.3): cada rol es un run de agente sobre el runtime
    /// de T33.2, con el proveedor que devuelve `providers` para ese rol.
    ///
    /// Arquitecto (solo lectura, workspace) → `ImplementationPlan`;
    /// Coder (edita en el worktree) → QA (`test.run` real; el fallo vuelve
    /// al Coder hasta `max_qa_retries`) → Auditor (solo lectura, worktree)
    /// → `AuditVerdict`: aprobado ⇒ `ReadyForApproval` con diff y resumen;
    /// rechazado con hallazgos ⇒ vuelve al Coder; sin hallazgos ⇒ `Failed`.
    /// La aprobación humana final (`approve_task`) no cambia.
    pub fn run_agent_pipeline(
        &self,
        ctx: &crate::ctx::Ctx,
        catalog: &crate::capability::Catalog,
        ticket_id: &str,
        providers: &mut dyn FnMut(
            AgentRole,
        )
            -> Result<Box<dyn crate::agent::providers::AgentProvider>>,
        handler: &mut dyn crate::agent::AgentHandler,
    ) -> Result<FlowTask> {
        self.run_agent_pipeline_with(
            ctx,
            catalog,
            ticket_id,
            providers,
            handler,
            crate::agent::Executor::Confined,
        )
    }

    /// `run_agent_pipeline` con el ejecutor elegido: `InProcess` solo lo
    /// usan los tests (el ejecutor confinado relanza el binario `antos`).
    pub(crate) fn run_agent_pipeline_with(
        &self,
        ctx: &crate::ctx::Ctx,
        catalog: &crate::capability::Catalog,
        ticket_id: &str,
        providers: &mut dyn FnMut(
            AgentRole,
        )
            -> Result<Box<dyn crate::agent::providers::AgentProvider>>,
        handler: &mut dyn crate::agent::AgentHandler,
        executor: crate::agent::Executor,
    ) -> Result<FlowTask> {
        use crate::agent::{roles, RunConfig};

        let ticket_upper = ticket_id.to_uppercase();
        let ticket_clean = ticket_upper.to_lowercase();
        let wt_path = ctx.state.join("worktrees").join(&ticket_clean);
        let branch_name = format!("agent/{ticket_clean}");
        let llm_config = crate::llm::LlmConfig::load_from_state(&ctx.state);

        // 1 · Tarea + ticket
        let _ = self.start_task(&ctx.workspace, &ctx.state, &ticket_upper)?;
        self.set_backend(&ticket_upper, FlowBackend::Agent)?;
        let ticket = crate::spec::SpecEngine::global()
            .get_ticket(&ctx.workspace, &ticket_upper)?
            .ok_or_else(|| anyhow::anyhow!("ticket «{ticket_upper}» no encontrado"))?;
        let ticket_text = format!(
            "Ticket {}: {}\n\n{}\n\nAlcance técnico:\n{}\n\nCriterios de aceptación:\n{}",
            ticket.id,
            ticket.title,
            ticket.description,
            bullet(&ticket.technical_scope),
            bullet(&ticket.acceptance_criteria)
        );

        // 2 · Worktree efímero (T2.2)
        if ctx.workspace.join(".git").exists() {
            crate::git::create_worktree(&ctx.workspace, &wt_path, &branch_name, "HEAD")?;
        } else {
            std::fs::create_dir_all(&wt_path)?;
        }
        let wt_ctx = crate::ctx::Ctx {
            workspace: wt_path.clone(),
            current_project: None,
            ..ctx.clone()
        };

        // 3 · Arquitecto → plan tipado
        let arch = roles::spec_for(
            AgentRole::Architect,
            llm_config.get_role_steps("architect", roles::default_steps(AgentRole::Architect)),
        );
        let mut provider = match providers(AgentRole::Architect) {
            Ok(p) => p,
            Err(e) => {
                return self.fail_task(
                    &ticket_upper,
                    &format!("sin proveedor para el Arquitecto: {e:#}"),
                    None,
                )
            }
        };
        let mut cfg = RunConfig::new(format!(
            "Planifica la implementación del ticket {ticket_upper}"
        ));
        cfg.toolset = arch.toolset.clone();
        cfg.system_prompt = Some(arch.system_prompt.clone());
        cfg.budget = arch.budget.clone();
        cfg.finish = Some(arch.finish.clone());
        cfg.context = Some(ticket_text.clone());
        cfg.executor = executor;
        let report = crate::agent::run(ctx, catalog, &mut *provider, &cfg, handler)?;
        let plan: ImplementationPlan = match parse_finish(&report) {
            Some(p) if report.stop_reason == AgentStopReason::Finished => p,
            _ => {
                let detail = format!(
                    "El Arquitecto no entregó un plan válido ({:?}): {}",
                    report.stop_reason, report.summary
                );
                return self.fail_task(&ticket_upper, &detail, Some(&report));
            }
        };
        if plan.files_to_touch.is_empty() {
            let detail = format!(
                "El Arquitecto declaró el ticket no realizable: {}",
                plan.summary
            );
            return self.fail_task(&ticket_upper, &detail, Some(&report));
        }
        self.advance_phase_with_report(
            &ticket_upper,
            &format!(
                "Plan del Arquitecto: {} ({} ficheros, {} pasos)",
                plan.summary,
                plan.files_to_touch.len(),
                plan.steps.len()
            ),
            true,
            &report,
        )?;

        // 4 · Coder ⇄ QA ⇄ Auditor
        let mut coder_goal = format!(
            "Implementa el plan del Arquitecto para el ticket {ticket_upper}.\n\nPlan: {}\nFicheros permitidos: {}\nPasos:\n{}\nVerificación:\n{}",
            plan.summary,
            plan.files_to_touch.join(", "),
            bullet(&plan.steps),
            bullet(&plan.acceptance_checks)
        );
        loop {
            // Coder
            let coder = roles::spec_for(
                AgentRole::Coder,
                llm_config.get_role_steps("coder", roles::default_steps(AgentRole::Coder)),
            );
            let mut provider = match providers(AgentRole::Coder) {
                Ok(p) => p,
                Err(e) => {
                    return self.fail_task(
                        &ticket_upper,
                        &format!("sin proveedor para el Coder: {e:#}"),
                        None,
                    )
                }
            };
            let mut cfg = RunConfig::new(coder_goal.clone());
            cfg.toolset = coder.toolset.clone();
            cfg.system_prompt = Some(coder.system_prompt.clone());
            cfg.budget = coder.budget.clone();
            cfg.finish = Some(coder.finish.clone());
            cfg.context = Some(ticket_text.clone());
            cfg.executor = executor;
            let report = crate::agent::run(&wt_ctx, catalog, &mut *provider, &cfg, handler)?;
            if report.stop_reason == AgentStopReason::Declined
                || report.stop_reason == AgentStopReason::Error
            {
                let detail = format!(
                    "El Coder no pudo continuar ({:?}): {}",
                    report.stop_reason, report.summary
                );
                return self.fail_task(&ticket_upper, &detail, Some(&report));
            }
            self.advance_phase_with_report(
                &ticket_upper,
                &format!("Coder: {} ({} pasos)", report.summary, report.steps),
                true,
                &report,
            )?;

            // QA: la suite real, sin modelo
            let (tests_ok, tests_output) = match crate::exec::fs::run_tests(&wt_path, None) {
                Ok(out) => (out.starts_with("TESTS EN VERDE"), out),
                Err(e) => (false, format!("no se pudo ejecutar la suite: {e:#}")),
            };
            let task = self.advance_phase_with_model(
                &ticket_upper,
                &if tests_ok {
                    "QA: suite en verde.".to_string()
                } else {
                    format!("QA: suite en rojo:\n{}", tail_lines(&tests_output, 40))
                },
                tests_ok,
                None,
            )?;
            self.mark_last_real(&ticket_upper)?;
            if !tests_ok {
                if task.state == FlowState::Failed {
                    return Ok(task);
                }
                coder_goal = format!(
                    "Los tests siguen en rojo tras tu cambio. Corrígelo tocando solo {}.\n\nSalida de la suite:\n{}",
                    plan.files_to_touch.join(", "),
                    tail_lines(&tests_output, 80)
                );
                continue;
            }

            // Auditor
            let diff = calculate_worktree_diff(&wt_path).unwrap_or_default();
            let touched = files_in_diff(&diff);
            let outside: Vec<&String> = touched
                .iter()
                .filter(|f| !plan.files_to_touch.iter().any(|p| p == *f))
                .collect();
            let auditor = roles::spec_for(
                AgentRole::Auditor,
                llm_config.get_role_steps("auditor", roles::default_steps(AgentRole::Auditor)),
            );
            let mut provider = match providers(AgentRole::Auditor) {
                Ok(p) => p,
                Err(e) => {
                    return self.fail_task(
                        &ticket_upper,
                        &format!("sin proveedor para el Auditor: {e:#}"),
                        None,
                    )
                }
            };
            let mut cfg = RunConfig::new(format!("Audita el diff del ticket {ticket_upper}"));
            cfg.toolset = auditor.toolset.clone();
            cfg.system_prompt = Some(auditor.system_prompt.clone());
            cfg.budget = auditor.budget.clone();
            cfg.finish = Some(auditor.finish.clone());
            cfg.executor = executor;
            cfg.context = Some(format!(
                "{ticket_text}\n\nPlan del Arquitecto: {}\nFicheros permitidos: {}\nFicheros tocados por el diff: {}{}\n\nInforme de QA: {}\n\nDiff consolidado:\n{}",
                plan.summary,
                plan.files_to_touch.join(", "),
                touched.join(", "),
                if outside.is_empty() { String::new() } else { format!("\nFUERA DEL PLAN: {}", outside.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")) },
                tail_lines(&tests_output, 20),
                tail_lines(&diff, 400)
            ));
            let report = crate::agent::run(&wt_ctx, catalog, &mut *provider, &cfg, handler)?;
            let mut verdict: AuditVerdict = match parse_finish(&report) {
                Some(v) if report.stop_reason == AgentStopReason::Finished => v,
                _ => {
                    let detail = format!(
                        "El Auditor no emitió veredicto ({:?}): {}",
                        report.stop_reason, report.summary
                    );
                    return self.fail_task(&ticket_upper, &detail, Some(&report));
                }
            };
            // La regla del plan no es negociable: un diff fuera de
            // `files_to_touch` se rechaza aunque el modelo apruebe.
            if !outside.is_empty() {
                verdict.approve = false;
                verdict.findings.push(format!(
                    "el diff toca ficheros fuera del plan: {}",
                    outside
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if verdict.approve {
                // Reviewing → ReadyForApproval: la aprobación humana no cambia.
                let mut task = self.advance_phase_with_report(
                    &ticket_upper,
                    &format!(
                        "Auditor: aprobado (riesgo {}). {} Listo para la aprobación del desarrollador.",
                        verdict.risk, verdict.summary
                    ),
                    true,
                    &report,
                )?;
                let mut lock = self
                    .state
                    .lock()
                    .map_err(|_| anyhow::anyhow!("mutex poisoned"))?;
                if let Some(t) = lock.get_mut(&ticket_upper) {
                    t.diff_preview = Some(diff.clone());
                    t.audit_summary = Some(format!(
                        "Auditor [{}]: {} (riesgo {})",
                        report.model, verdict.summary, verdict.risk
                    ));
                    task = t.clone();
                }
                save_task_to_disk(&ctx.state, &task);
                return Ok(task);
            }
            // Rechazado: con hallazgos accionables vuelve al Coder (cuenta
            // como reintento de QA); sin ellos, no hay nada que corregir.
            let findings = bullet(&verdict.findings);
            let task = self
                .get_task(&ticket_upper)
                .ok_or_else(|| anyhow::anyhow!("tarea perdida"))?;
            if verdict.findings.is_empty() || task.qa_retries >= task.max_qa_retries {
                let detail = format!(
                    "Auditor: rechazado sin vía de corrección. {}",
                    verdict.summary
                );
                return self.fail_task(&ticket_upper, &detail, Some(&report));
            }
            self.rewind_to_coder(
                &ticket_upper,
                &format!("Auditor: rechazado. {}\n{}", verdict.summary, findings),
                &report,
            )?;
            coder_goal = format!(
                "El Auditor rechazó tu cambio. Corrige estos hallazgos tocando solo {}:\n{}",
                plan.files_to_touch.join(", "),
                findings
            );
        }
    }

    fn set_backend(&self, ticket_id: &str, backend: FlowBackend) -> Result<()> {
        let mut lock = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("mutex poisoned"))?;
        if let Some(t) = lock.get_mut(ticket_id) {
            t.backend = backend;
            for h in &mut t.history {
                h.simulated = false;
            }
        }
        Ok(())
    }

    fn mark_last_real(&self, ticket_id: &str) -> Result<()> {
        let mut lock = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("mutex poisoned"))?;
        if let Some(t) = lock.get_mut(ticket_id) {
            if let Some(last) = t.history.last_mut() {
                last.simulated = false;
            }
        }
        Ok(())
    }

    fn fail_task(
        &self,
        ticket_id: &str,
        detail: &str,
        report: Option<&antos_protocol::AgentReport>,
    ) -> Result<FlowTask> {
        let mut lock = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("mutex poisoned"))?;
        let task = lock
            .get_mut(ticket_id)
            .ok_or_else(|| anyhow::anyhow!("no existe tarea activa para ticket «{ticket_id}»"))?;
        let previous = task.state;
        task.state = FlowState::Failed;
        task.current_role = None;
        task.history.push(FlowTransition {
            timestamp_seconds: now_secs(),
            old_state: previous,
            new_state: FlowState::Failed,
            role: None,
            detail: detail.to_string(),
            model: report.map(|r| r.model.clone()),
            simulated: false,
            report: report.map(AgentReportSummary::from),
        });
        Ok(task.clone())
    }

    /// El Auditor devuelve el trabajo al Coder: Reviewing → Implementing,
    /// contando como reintento (comparte el límite con QA a propósito).
    fn rewind_to_coder(
        &self,
        ticket_id: &str,
        detail: &str,
        report: &antos_protocol::AgentReport,
    ) -> Result<()> {
        let mut lock = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("mutex poisoned"))?;
        let task = lock
            .get_mut(ticket_id)
            .ok_or_else(|| anyhow::anyhow!("no existe tarea activa para ticket «{ticket_id}»"))?;
        let previous = task.state;
        task.qa_retries += 1;
        task.state = FlowState::Implementing;
        task.current_role = Some(AgentRole::Coder);
        task.history.push(FlowTransition {
            timestamp_seconds: now_secs(),
            old_state: previous,
            new_state: FlowState::Implementing,
            role: Some(AgentRole::Coder),
            detail: detail.to_string(),
            model: Some(report.model.clone()),
            simulated: false,
            report: Some(AgentReportSummary::from(report)),
        });
        Ok(())
    }
}

/// La entrada tipada de `finalizar` de un run, si la hubo y encaja.
fn parse_finish<T: serde::de::DeserializeOwned>(report: &antos_protocol::AgentReport) -> Option<T> {
    report
        .result_json
        .as_deref()
        .and_then(|j| serde_json::from_str(j).ok())
}

fn bullet(items: &[String]) -> String {
    if items.is_empty() {
        return "  (ninguno)".into();
    }
    items
        .iter()
        .map(|i| format!("  - {i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Rutas de un diff `git diff` (líneas `+++ b/...`).
fn files_in_diff(diff: &str) -> Vec<String> {
    diff.lines()
        .filter_map(|l| l.strip_prefix("+++ b/"))
        .map(|s| s.trim().to_string())
        .collect()
}

#[cfg(test)]
mod agent_pipeline_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::agent::fake::{FakeProvider, ScriptedCall, ScriptedTurn};
    use crate::agent::{AgentHandler, Executor};
    use crate::capability::Catalog;
    use crate::ctx::Ctx;
    use antos_protocol::{AgentReport, AgentStepEvent, Proposal};
    use anyhow::bail;
    use serde_json::json;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::Mutex;

    /// Workspace git con un crate cuyo test está en rojo y un ticket que pide
    /// arreglarlo, más un estado aislado.
    fn fixture(name: &str) -> (Ctx, PathBuf) {
        let discovered = Ctx::discover().expect("Ctx::discover dentro del árbol de antOS");
        let temp = std::env::temp_dir().join(format!("antos_flow_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        let ws = temp.join("workspace");
        let state = temp.join("state");
        std::fs::create_dir_all(ws.join("src")).unwrap();
        std::fs::create_dir_all(ws.join("docs/tickets")).unwrap();
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(
            ws.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
        )
        .unwrap();
        std::fs::write(
            ws.join("src/lib.rs"),
            "pub fn sum(a: i32, b: i32) -> i32 {\n    a * b\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn sums() {\n        assert_eq!(super::sum(2, 3), 5);\n    }\n}\n",
        )
        .unwrap();
        std::fs::write(ws.join("README.md"), "fixture\n").unwrap();
        std::fs::write(
            ws.join("docs/tickets/T99.1-arreglar-sum.md"),
            "# T99.1 · Arreglar sum\n\n> **Estado:** ⏳ Pendiente\n\n## Descripción\n`sum` multiplica en vez de sumar.\n\n## Alcance Técnico\n* `src/lib.rs`\n\n## Criterios de Aceptación\n* `cargo test` pasa.\n",
        )
        .unwrap();
        std::fs::write(ws.join(".gitignore"), "target/\n").unwrap();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["-c", "user.email=t@t", "-c", "user.name=t", "add", "."],
            vec![
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "-m",
                "fixture",
            ],
        ] {
            let st = Command::new("git")
                .args(&args)
                .current_dir(&ws)
                .status()
                .unwrap();
            assert!(st.success(), "git {args:?}");
        }
        let ctx = Ctx {
            workspace: ws,
            state,
            current_project: None,
            ..discovered
        };
        (ctx, temp)
    }

    #[derive(Default)]
    struct Quiet {
        confirms: usize,
        steps: Vec<AgentStepEvent>,
    }
    impl AgentHandler for Quiet {
        fn on_step(&mut self, e: &AgentStepEvent) -> Result<()> {
            self.steps.push(e.clone());
            Ok(())
        }
        fn on_confirm(&mut self, _p: &Proposal) -> Result<bool> {
            self.confirms += 1;
            Ok(true)
        }
        fn on_note(&mut self, _t: &str) -> Result<()> {
            Ok(())
        }
        fn on_done(&mut self, _r: &AgentReport) -> Result<()> {
            Ok(())
        }
    }

    fn call(tool: &str, input: serde_json::Value) -> ScriptedCall {
        ScriptedCall {
            tool: tool.into(),
            input,
        }
    }
    fn turn(calls: Vec<ScriptedCall>) -> ScriptedTurn {
        ScriptedTurn {
            calls,
            tokens: 10,
            ..Default::default()
        }
    }
    fn architect_ok() -> Vec<ScriptedTurn> {
        vec![
            turn(vec![call("fs.read", json!({"path": "src/lib.rs"}))]),
            turn(vec![call(
                "finalizar",
                json!({"resumen": "cambiar * por + en sum", "files_to_touch": ["src/lib.rs"],
                       "steps": ["editar sum"], "acceptance_checks": ["cargo test"]}),
            )]),
        ]
    }
    fn coder_fix(old: &str, new: &str) -> Vec<ScriptedTurn> {
        vec![
            turn(vec![call(
                "fs.patch",
                json!({"path": "src/lib.rs", "old": old, "new": new}),
            )]),
            turn(vec![call("finalizar", json!({"resumen": "sum corregida"}))]),
        ]
    }
    fn auditor(approve: bool, findings: Vec<&str>) -> Vec<ScriptedTurn> {
        vec![turn(vec![call(
            "finalizar",
            json!({"resumen": "revisado", "approve": approve, "findings": findings, "risk": "low"}),
        )])]
    }

    /// Fábrica de proveedores por rol: cada llamada consume el siguiente
    /// guion de ese rol (el Coder puede correr varias veces).
    struct Scripts {
        architect: Vec<Vec<ScriptedTurn>>,
        coder: Vec<Vec<ScriptedTurn>>,
        auditor: Vec<Vec<ScriptedTurn>>,
    }
    impl Scripts {
        fn factory(
            &mut self,
        ) -> impl FnMut(AgentRole) -> Result<Box<dyn crate::agent::providers::AgentProvider>> + '_
        {
            move |role| {
                let script = match role {
                    AgentRole::Architect => &mut self.architect,
                    AgentRole::Coder => &mut self.coder,
                    AgentRole::Auditor => &mut self.auditor,
                    other => bail!("rol sin guion: {other:?}"),
                };
                if script.is_empty() {
                    bail!("guion agotado para {role:?}");
                }
                Ok(Box::new(FakeProvider::new(script.remove(0))))
            }
        }
    }

    #[test]
    fn real_pipeline_plans_patches_tests_audits_and_waits_for_approval() {
        let (ctx, temp) = fixture("happy");
        let catalog = Catalog::load(&ctx.caps_dir).unwrap();
        let mut scripts = Scripts {
            architect: vec![architect_ok()],
            coder: vec![coder_fix("    a * b\n", "    a + b\n")],
            auditor: vec![auditor(true, vec![])],
        };
        let mut handler = Quiet::default();
        let engine = FlowEngine {
            state: Mutex::new(HashMap::new()),
        };
        let task = engine
            .run_agent_pipeline_with(
                &ctx,
                &catalog,
                "T99.1",
                &mut scripts.factory(),
                &mut handler,
                Executor::InProcess,
            )
            .unwrap();

        assert_eq!(task.state, FlowState::ReadyForApproval);
        assert_eq!(task.backend, FlowBackend::Agent);
        assert!(task.history.iter().all(|t| !t.simulated));
        assert!(task.history.iter().any(|t| t.report.is_some()));
        assert!(task.diff_preview.as_deref().unwrap_or("").contains("a + b"));
        assert!(task
            .audit_summary
            .as_deref()
            .unwrap_or("")
            .contains("riesgo low"));
        // El cambio está en el worktree/rama del agente, no en el workspace.
        let wt = PathBuf::from(task.worktree_path.as_ref().unwrap());
        assert!(std::fs::read_to_string(wt.join("src/lib.rs"))
            .unwrap()
            .contains("a + b"));
        assert!(std::fs::read_to_string(ctx.workspace.join("src/lib.rs"))
            .unwrap()
            .contains("a * b"));
        // Un solo paso confirm (el parche), aprobado por el handler.
        assert_eq!(handler.confirms, 1);
        // Tras aprobar, sigue sin fusionar (honestidad T33.1): estado Merged, rama intacta.
        let after = engine.approve_task("T99.1", true).unwrap();
        assert_eq!(after.state, FlowState::Merged);
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn qa_failure_feeds_the_coder_a_second_run() {
        let (ctx, temp) = fixture("retry");
        let catalog = Catalog::load(&ctx.caps_dir).unwrap();
        let mut scripts = Scripts {
            architect: vec![architect_ok()],
            // Primer intento equivocado (sigue en rojo), segundo correcto.
            coder: vec![
                coder_fix("    a * b\n", "    a - b\n"),
                coder_fix("    a - b\n", "    a + b\n"),
            ],
            auditor: vec![auditor(true, vec![])],
        };
        let mut handler = Quiet::default();
        let engine = FlowEngine {
            state: Mutex::new(HashMap::new()),
        };
        let task = engine
            .run_agent_pipeline_with(
                &ctx,
                &catalog,
                "T99.1",
                &mut scripts.factory(),
                &mut handler,
                Executor::InProcess,
            )
            .unwrap();
        assert_eq!(task.state, FlowState::ReadyForApproval);
        assert_eq!(task.qa_retries, 1);
        assert!(task
            .history
            .iter()
            .any(|t| t.detail.contains("suite en rojo")));
        assert!(
            scripts.coder.is_empty(),
            "los dos guiones del Coder se consumieron"
        );
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn auditor_rejects_a_diff_outside_the_plan_even_if_the_model_approves() {
        let (ctx, temp) = fixture("outside");
        let catalog = Catalog::load(&ctx.caps_dir).unwrap();
        // El Coder toca README.md (fuera del plan) además de arreglar sum.
        let coder = vec![
            turn(vec![
                call(
                    "fs.patch",
                    json!({"path": "src/lib.rs", "old": "    a * b\n", "new": "    a + b\n"}),
                ),
                call(
                    "fs.patch",
                    json!({"path": "README.md", "old": "fixture", "new": "cambiado"}),
                ),
            ]),
            turn(vec![call("finalizar", json!({"resumen": "hecho"}))]),
        ];
        let mut scripts = Scripts {
            architect: vec![architect_ok()],
            coder: vec![coder],
            auditor: vec![auditor(true, vec![])],
        };
        let mut handler = Quiet::default();
        let engine = FlowEngine {
            state: Mutex::new(HashMap::new()),
        };
        let task = engine
            .run_agent_pipeline_with(
                &ctx,
                &catalog,
                "T99.1",
                &mut scripts.factory(),
                &mut handler,
                Executor::InProcess,
            )
            .unwrap();
        // Rechazado con un hallazgo → vuelve al Coder, que no tiene más guion
        // (sin proveedor) → la tarea falla; nunca llega a aprobación.
        assert_eq!(task.state, FlowState::Failed);
        assert!(task
            .history
            .iter()
            .any(|t| t.detail.contains("fuera del plan")));
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn architect_without_a_plan_fails_the_task() {
        let (ctx, temp) = fixture("noplan");
        let catalog = Catalog::load(&ctx.caps_dir).unwrap();
        let mut scripts = Scripts {
            architect: vec![vec![turn(vec![call(
                "finalizar",
                json!({"resumen": "no realizable", "files_to_touch": [], "steps": [], "acceptance_checks": []}),
            )])]],
            coder: vec![],
            auditor: vec![],
        };
        let mut handler = Quiet::default();
        let engine = FlowEngine {
            state: Mutex::new(HashMap::new()),
        };
        let task = engine
            .run_agent_pipeline_with(
                &ctx,
                &catalog,
                "T99.1",
                &mut scripts.factory(),
                &mut handler,
                Executor::InProcess,
            )
            .unwrap();
        assert_eq!(task.state, FlowState::Failed);
        assert!(task
            .history
            .last()
            .unwrap()
            .detail
            .contains("no realizable"));
        let _ = std::fs::remove_dir_all(temp);
    }
}
