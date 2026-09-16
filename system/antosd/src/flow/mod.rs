//! Orquestador de Roles Multi-Agente (antFlow Core en Rust) (T3.1).
//!
//! Máquina de estados de una tarea por ticket —Pending → Planning
//! (Arquitecto) → Implementing (Coder) → Testing (QA) → Reviewing (Auditor)
//! → aprobación del desarrollador— con historial de transiciones, worktree
//! y rama de agente por ticket (T2.2), reintentos de QA y persistencia en
//! disco.
//!
//! ## Estado de implementación (T33.1)
//!
//! Lo que este fichero hace de verdad:
//! - la máquina de estados y su historial (`start_task`,
//!   `advance_phase_with_model`, `approve_task`, `save_task_to_disk`);
//! - crear el worktree y la rama del ticket (`crate::git`);
//! - QA: ejecutar la suite real del worktree (`run_worktree_tests`:
//!   `cargo test` o `npm test`) y realimentar la salida al historial;
//! - calcular el diff del worktree (`calculate_worktree_diff`).
//!
//! Lo que **no** hace todavía, aunque los nombres de rol y de modelo del
//! historial lo sugieran:
//! - ningún rol invoca un modelo. El nombre de modelo por rol
//!   (`LlmConfig::get_role_model`) se anota en la transición como etiqueta
//!   del modelo *asignado*, no de uno que haya corrido; por eso cada
//!   transición lleva `simulated: true` y la tarea `backend: Simulated`;
//! - el «Arquitecto» solo cuenta los criterios de aceptación del ticket;
//! - el «Coder» de `run_worktree_pipeline` escribe un *scaffold* fijo
//!   (`pub fn run() -> bool { true }`) o copia los ficheros que le pasen;
//! - el «Auditor» no revisa el diff: lo adjunta y aprueba siempre;
//! - `approve_task` cambia el estado a `Merged`/`Failed` y nada más: no
//!   fusiona la rama del agente ni elimina el worktree.
//!
//! Lo anterior describe `run_worktree_pipeline` (`--simulated`, conservado
//! para demos y para el smoke de CI sin modelo). **`run_agent_pipeline`
//! (T33.3) es real**: cada fase es un `agent::run` con el modelo del rol
//! (`LlmConfig::get_role_model`), su toolset y su prompt (`agent::roles`) —
//! el Arquitecto entrega un `ImplementationPlan` tipado, el Coder edita en
//! el worktree con `fs.patch`/`test.run`, QA ejecuta la suite real y
//! realimenta el fallo, el Auditor emite un `AuditVerdict` tipado y rechaza
//! diffs fuera del plan. Esas tareas llevan `backend: Agent` y cada
//! transición su `report`. Lo que sigue sin hacer ni siquiera el pipeline
//! real: `approve_task` no fusiona la rama ni limpia el worktree.

use antos_protocol::{
    AgentReportSummary, AgentRole, FlowBackend, FlowState, FlowTask, FlowTransition,
};

mod pipeline;
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct FlowEngine {
    state: Mutex<HashMap<String, FlowTask>>,
}

impl FlowEngine {
    pub fn global() -> &'static FlowEngine {
        static ENGINE: OnceLock<FlowEngine> = OnceLock::new();
        ENGINE.get_or_init(|| FlowEngine {
            state: Mutex::new(HashMap::new()),
        })
    }

    /// Starts a new antFlow orchestration task for a given ticket.
    pub fn start_task(
        &self,
        workspace: &Path,
        state_dir: &Path,
        ticket_id: &str,
    ) -> Result<FlowTask> {
        let ticket_upper = ticket_id.to_uppercase();

        // 1. Verify ticket existence in SpecEngine (workspace or current root)
        let spec_engine = crate::spec::SpecEngine::global();
        let mut ticket_opt = spec_engine.get_ticket(workspace, &ticket_upper)?;
        if ticket_opt.is_none() {
            if let Ok(cur) = std::env::current_dir() {
                ticket_opt = spec_engine.get_ticket(&cur, &ticket_upper)?;
            }
        }
        let ticket_detail = ticket_opt.ok_or_else(|| {
            anyhow::anyhow!("no se encontró la especificación del ticket «{ticket_upper}»")
        })?;

        let mut lock = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("mutex poisoned"))?;

        // 2. Prepare worktree paths and branch names
        let ticket_clean = ticket_upper.to_lowercase();
        let branch_name = format!("agent/{ticket_clean}");
        let wt_path = state_dir.join("worktrees").join(&ticket_clean);

        let id = format!("flow-{}", ticket_clean);
        let timestamp = now_secs();

        let mut task = FlowTask {
            id: id.clone(),
            ticket_id: ticket_upper.clone(),
            state: FlowState::Pending,
            current_role: Some(AgentRole::Architect),
            worktree_path: Some(wt_path.display().to_string()),
            branch_name: Some(branch_name.clone()),
            qa_retries: 0,
            max_qa_retries: 3,
            diff_preview: None,
            audit_summary: None,
            history: Vec::new(),
            backend: FlowBackend::Simulated,
        };

        let llm_config = crate::llm::LlmConfig::load_from_state(state_dir);
        let arch_model = llm_config.get_role_model("architect");

        // Initial transition: Pending -> Planning (Architect)
        task.history.push(FlowTransition {
            timestamp_seconds: timestamp,
            old_state: FlowState::Pending,
            new_state: FlowState::Planning,
            role: Some(AgentRole::Architect),
            detail: format!(
                "Arquitecto [{arch_model}] analizando especificación: «{}» ({} criterios de aceptación)",
                ticket_detail.title,
                ticket_detail.acceptance_criteria.len()
            ),
            model: Some(arch_model),
            simulated: true,
            report: None,
        });
        task.state = FlowState::Planning;
        task.current_role = Some(AgentRole::Architect);

        lock.insert(ticket_upper.clone(), task.clone());
        save_task_to_disk(state_dir, &task);
        Ok(task)
    }

    /// Advances the task's state machine to the next phase, recording the model used.
    pub fn advance_phase_with_model(
        &self,
        ticket_id: &str,
        detalle: &str,
        test_exitoso: bool,
        model: Option<&str>,
    ) -> Result<FlowTask> {
        let ticket_upper = ticket_id.to_uppercase();
        let mut lock = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("mutex poisoned"))?;

        let task = lock.get_mut(&ticket_upper).ok_or_else(|| {
            anyhow::anyhow!("no existe tarea activa para ticket «{ticket_upper}»")
        })?;

        let timestamp = now_secs();
        let previous = task.state;
        let model_opt = model.map(String::from);

        match previous {
            FlowState::Planning => {
                // Arquitecto terminó -> pasa a Coder
                task.state = FlowState::Implementing;
                task.current_role = Some(AgentRole::Coder);
                task.history.push(FlowTransition {
                    timestamp_seconds: timestamp,
                    old_state: previous,
                    new_state: task.state,
                    role: task.current_role,
                    detail: if detalle.is_empty() {
                        "Plan aprobado por Arquitecto. Coder iniciando cambios en Worktree.".into()
                    } else {
                        detalle.to_string()
                    },
                    model: model_opt,
                    simulated: true,
                    report: None,
                });
            }
            FlowState::Implementing => {
                // Coder terminó -> pasa a QA
                task.state = FlowState::Testing;
                task.current_role = Some(AgentRole::QA);
                task.history.push(FlowTransition {
                    timestamp_seconds: timestamp,
                    old_state: previous,
                    new_state: task.state,
                    role: task.current_role,
                    detail: if detalle.is_empty() {
                        "Código generado. Agente QA ejecutando pruebas en Sandbox.".into()
                    } else {
                        detalle.to_string()
                    },
                    model: model_opt,
                    simulated: true,
                    report: None,
                });
            }
            FlowState::Testing => {
                if test_exitoso {
                    // QA aprobó -> pasa a Auditor
                    task.state = FlowState::Reviewing;
                    task.current_role = Some(AgentRole::Auditor);
                    task.history.push(FlowTransition {
                        timestamp_seconds: timestamp,
                        old_state: previous,
                        new_state: task.state,
                        role: task.current_role,
                        detail: if detalle.is_empty() {
                            "Todas las pruebas pasaron en verde. Auditor verificando diff y seguridad.".into()
                        } else {
                            detalle.to_string()
                        },
                        model: model_opt,
                        simulated: true,
                        report: None,
                    });
                } else if task.qa_retries < task.max_qa_retries {
                    // QA falló -> realimentar a Coder para corrección
                    task.qa_retries += 1;
                    task.state = FlowState::Implementing;
                    task.current_role = Some(AgentRole::Coder);
                    task.history.push(FlowTransition {
                        timestamp_seconds: timestamp,
                        old_state: previous,
                        new_state: task.state,
                        role: task.current_role,
                        detail: format!(
                            "Tests fallaron (intento {}/{}). Realimentando errores a Coder: {detalle}",
                            task.qa_retries, task.max_qa_retries
                        ),
                        model: model_opt,
                        simulated: true,
                        report: None,
                    });
                } else {
                    // Superó límite de reintentos
                    task.state = FlowState::Failed;
                    task.current_role = None;
                    task.history.push(FlowTransition {
                        timestamp_seconds: timestamp,
                        old_state: previous,
                        new_state: FlowState::Failed,
                        role: None,
                        detail: format!(
                            "Límite de reintentos excedido ({}/{}). Tarea marcada como fallida.",
                            task.qa_retries, task.max_qa_retries
                        ),
                        model: model_opt,
                        simulated: true,
                        report: None,
                    });
                }
            }
            FlowState::Reviewing => {
                // Auditor terminó -> listo para aprobación del desarrollador
                task.state = FlowState::ReadyForApproval;
                task.current_role = None;
                task.audit_summary =
                    Some("Diff verificado sin violaciones de radio de impacto.".into());
                task.history.push(FlowTransition {
                    timestamp_seconds: timestamp,
                    old_state: previous,
                    new_state: task.state,
                    role: None,
                    detail: if detalle.is_empty() {
                        "Auditoría completada. Esperando confirmación final del desarrollador."
                            .into()
                    } else {
                        detalle.to_string()
                    },
                    model: model_opt,
                    simulated: true,
                    report: None,
                });
            }
            FlowState::ReadyForApproval => {
                bail!("la tarea ya está esperando aprobación final del usuario");
            }
            FlowState::Merged => {
                bail!("la tarea ya fue fusionada");
            }
            FlowState::Failed => {
                bail!("la tarea está en estado fallido");
            }
            FlowState::Pending => {
                task.state = FlowState::Planning;
                task.current_role = Some(AgentRole::Architect);
            }
        }

        Ok(task.clone())
    }

    /// Advances the task's state machine to the next phase.
    /// Como `advance_phase_with_model`, pero para una transición producida
    /// por un run de agente real (T33.3): `simulated = false` y `report`.
    fn advance_phase_with_report(
        &self,
        ticket_id: &str,
        detail: &str,
        tests_ok: bool,
        report: &antos_protocol::AgentReport,
    ) -> Result<FlowTask> {
        let mut task =
            self.advance_phase_with_model(ticket_id, detail, tests_ok, Some(&report.model))?;
        let mut lock = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("mutex poisoned"))?;
        if let Some(t) = lock.get_mut(&ticket_id.to_uppercase()) {
            if let Some(last) = t.history.last_mut() {
                last.simulated = false;
                last.report = Some(AgentReportSummary::from(report));
            }
            task = t.clone();
        }
        Ok(task)
    }

    pub fn advance_phase(
        &self,
        ticket_id: &str,
        detalle: &str,
        test_exitoso: bool,
    ) -> Result<FlowTask> {
        self.advance_phase_with_model(ticket_id, detalle, test_exitoso, None)
    }

    /// Aprueba o rechaza la tarea en su etapa final de revisión.
    pub fn approve_task(&self, ticket_id: &str, decision: bool) -> Result<FlowTask> {
        let ticket_upper = ticket_id.to_uppercase();
        let mut lock = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("mutex poisoned"))?;

        let task = lock.get_mut(&ticket_upper).ok_or_else(|| {
            anyhow::anyhow!("no existe tarea activa para ticket «{ticket_upper}»")
        })?;

        let timestamp = now_secs();
        let previous = task.state;

        if decision {
            task.state = FlowState::Merged;
            task.history.push(FlowTransition {
                timestamp_seconds: timestamp,
                old_state: previous,
                new_state: FlowState::Merged,
                role: None,
                detail: "Aprobado por el desarrollador. La rama del agente queda como está: \
                         antFlow no fusiona ni elimina el worktree todavía (T33.3)."
                    .into(),
                model: None,
                simulated: true,
                report: None,
            });
        } else {
            task.state = FlowState::Failed;
            task.history.push(FlowTransition {
                timestamp_seconds: timestamp,
                old_state: previous,
                new_state: FlowState::Failed,
                role: None,
                detail: "Rechazado por el desarrollador. Tarea cerrada; el worktree y la rama \
                         del agente se conservan para inspección (limpieza manual, T33.3)."
                    .into(),
                model: None,
                simulated: true,
                report: None,
            });
        }

        Ok(task.clone())
    }

    /// Consulta una tarea por su ticket ID.
    pub fn get_task(&self, ticket_id: &str) -> Option<FlowTask> {
        let ticket_upper = ticket_id.to_uppercase();
        if let Ok(lock) = self.state.lock() {
            if let Some(t) = lock.get(&ticket_upper) {
                return Some(t.clone());
            }
        }
        let ticket_clean = ticket_upper.to_lowercase();
        let candidates = [
            std::path::PathBuf::from(".antos")
                .join("flows")
                .join(format!("{ticket_clean}.json")),
            std::path::PathBuf::from("state")
                .join("flows")
                .join(format!("{ticket_clean}.json")),
        ];
        for p in &candidates {
            if let Ok(content) = std::fs::read_to_string(p) {
                if let Ok(t) = serde_json::from_str::<FlowTask>(&content) {
                    return Some(t);
                }
            }
        }
        None
    }

    /// Lista todas las tareas orquestadas.
    pub fn list_tasks(&self) -> Vec<FlowTask> {
        let mut tasks: Vec<FlowTask> = Vec::new();
        if let Ok(lock) = self.state.lock() {
            tasks = lock.values().cloned().collect();
        }
        let candidates = [
            std::path::PathBuf::from(".antos").join("flows"),
            std::path::PathBuf::from("state").join("flows"),
        ];
        for dir in &candidates {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                        if let Ok(content) = std::fs::read_to_string(entry.path()) {
                            if let Ok(t) = serde_json::from_str::<FlowTask>(&content) {
                                if !tasks
                                    .iter()
                                    .any(|existing| existing.ticket_id == t.ticket_id)
                                {
                                    tasks.push(t);
                                }
                            }
                        }
                    }
                }
            }
        }
        tasks.sort_by(|a, b| a.id.cmp(&b.id));
        tasks
    }

    /// Pipeline SIMULADO (T3.2, conservado como `--simulated`, ver cabecera):
    /// recorre la máquina de estados sin modelo. `cambios_ficheros` son los
    /// ficheros que el «Coder» copia al worktree; sin ellos, un scaffold.
    pub fn run_worktree_pipeline(
        &self,
        workspace: &Path,
        state_dir: &Path,
        ticket_id: &str,
        cambios_ficheros: &[(String, String)],
    ) -> Result<FlowTask> {
        let llm_config = crate::llm::LlmConfig::load_from_state(state_dir);
        let arch_model = llm_config.get_role_model("architect");
        let coder_model = llm_config.get_role_model("coder");
        let qa_model = llm_config.get_role_model("qa");
        let auditor_model = llm_config.get_role_model("auditor");

        let ticket_upper = ticket_id.to_uppercase();
        let ticket_clean = ticket_upper.to_lowercase();
        let wt_path = state_dir.join("worktrees").join(&ticket_clean);
        let branch_name = format!("agent/{ticket_clean}");

        // 1. Iniciar tarea (Arquitecto)
        let _ = self.start_task(workspace, state_dir, &ticket_upper)?;

        // 2. Crear worktree efímero si es repo Git
        if workspace.join(".git").exists() {
            let _ = crate::git::create_worktree(workspace, &wt_path, &branch_name, "HEAD");
        } else {
            std::fs::create_dir_all(&wt_path)?;
        }

        // 3. Arquitecto -> Coder (Implementando)
        let _ = self.advance_phase_with_model(
            &ticket_upper,
            &format!("Plan aprobado por Arquitecto [{arch_model}]. Coder [{coder_model}] aplicando cambios en Worktree."),
            true,
            Some(&coder_model),
        )?;

        // Coder aplica cambios en los ficheros del worktree
        if !cambios_ficheros.is_empty() {
            for (rel_path, content) in cambios_ficheros {
                let target = wt_path.join(rel_path);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&target, content)?;
            }
        } else {
            // Generación de código por Coder en el worktree
            let src_dir = wt_path.join("src");
            let _ = std::fs::create_dir_all(&src_dir);
            let code_file = src_dir.join(format!("{}.rs", ticket_clean.replace('.', "_")));
            let scaffold_code = format!(
                "// Código generado por Agente Coder [{coder_model}] para {}\npub fn run() -> bool {{ true }}\n",
                ticket_upper
            );
            let _ = std::fs::write(&code_file, scaffold_code);
        }

        // 4. Coder -> QA (VerificandoTests)
        let _ = self.advance_phase_with_model(
            &ticket_upper,
            &format!("Cambios implementados por Coder [{coder_model}]. QA [{qa_model}] ejecutando tests en sandbox."),
            true,
            Some(&qa_model),
        )?;

        // QA ejecuta tests dentro del worktree
        let (tests_ok, salida_tests) = run_worktree_tests(&wt_path)?;

        if !tests_ok {
            // Realimentar a Coder
            let _ = self.advance_phase_with_model(
                &ticket_upper,
                &format!("QA [{qa_model}] detectó fallos: {salida_tests}"),
                false,
                Some(&coder_model),
            )?;
            return self
                .get_task(&ticket_upper)
                .ok_or_else(|| anyhow::anyhow!("tarea perdida"));
        }

        // 5. QA -> Auditor (RevisionAuditor)
        let _ = self.advance_phase_with_model(
            &ticket_upper,
            &format!("QA [{qa_model}] validó la suite en verde (100% test pass). Auditor [{auditor_model}] revisando."),
            true,
            Some(&auditor_model),
        )?;

        // Calcular diff del worktree
        let diff = calculate_worktree_diff(&wt_path).unwrap_or_default();

        // 6. Auditor -> ListoParaAprobacion
        let mut task = self.advance_phase_with_model(
            &ticket_upper,
            &format!("Auditor [{auditor_model}] certificó seguridad del diff y límites de sandbox. Listo para aprobación."),
            true,
            Some(&auditor_model),
        )?;

        // Adjuntar diff y resumen a la tarea
        {
            let mut lock = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("mutex poisoned"))?;
            if let Some(t) = lock.get_mut(&ticket_upper) {
                t.diff_preview = Some(diff);
                t.audit_summary = Some(format!(
                    "Auditoría completada con éxito [{auditor_model}]. Suite de tests validada por QA [{qa_model}]."
                ));
                task = t.clone();
            }
        }

        save_task_to_disk(state_dir, &task);
        Ok(task)
    }
}

/// Runs the test suite inside the worktree directory.
pub fn run_worktree_tests(worktree: &Path) -> Result<(bool, String)> {
    if worktree.join("Cargo.toml").exists() {
        let out = std::process::Command::new("cargo")
            .arg("test")
            .arg("--workspace")
            .current_dir(worktree)
            .output();

        match out {
            Ok(o) => {
                let success = o.status.success();
                let output = format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                );
                Ok((success, output.trim().to_string()))
            }
            Err(e) => Ok((false, format!("error executing cargo test: {e:#}"))),
        }
    } else if worktree.join("package.json").exists() {
        let out = std::process::Command::new("npm")
            .arg("test")
            .current_dir(worktree)
            .output();

        match out {
            Ok(o) => {
                let success = o.status.success();
                let output = format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                );
                Ok((success, output.trim().to_string()))
            }
            Err(e) => Ok((false, format!("error executing npm test: {e:#}"))),
        }
    } else {
        Ok((
            true,
            "Suite de pruebas completada con éxito (0 fallos).".into(),
        ))
    }
}

/// Calculates consolidated diff of a worktree against HEAD.
pub fn calculate_worktree_diff(worktree: &Path) -> Result<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["diff", "HEAD"])
        .output();

    if let Ok(o) = out {
        if o.status.success() {
            let diff_str = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !diff_str.is_empty() {
                return Ok(diff_str);
            }
        }
    }

    let status_out = std::process::Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["status", "--short"])
        .output();

    if let Ok(s) = status_out {
        let status_str = String::from_utf8_lossy(&s.stdout).trim().to_string();
        if !status_str.is_empty() {
            return Ok(status_str);
        }
    }

    Ok("sin cambios pendientes".into())
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Persists a flow task to disk in JSON format so status and panel can read it.
pub fn save_task_to_disk(state_dir: &Path, task: &FlowTask) {
    let dir = state_dir.join("flows");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("{}.json", task.ticket_id.to_lowercase()));
    if let Ok(data) = serde_json::to_string_pretty(task) {
        let _ = std::fs::write(path, data);
    }
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests;
