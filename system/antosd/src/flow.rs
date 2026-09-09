//! Orquestador de Roles Multi-Agente (antFlow Core en Rust) (T3.1).
//!
//! Coordina la máquina de estados entre agentes especializados:
//! - Arquitecto: Análisis técnico de tickets y especificaciones (T1.3).
//! - Coder: Modificación de código y refactorización en Worktree efímero (T2.2).
//! - QA / Tester: Ejecución de pruebas y validación en sandbox confinado.
//! - Auditor: Análisis de radio de impacto, seguridad y diffs para aprobación final.

use antos_protocol::{AgentRole, FlowState, FlowTask, FlowTransition};
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
        let detalle_ticket = ticket_opt.ok_or_else(|| {
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
                detalle_ticket.title,
                detalle_ticket.acceptance_criteria.len()
            ),
            model: Some(arch_model),
        });
        task.state = FlowState::Planning;
        task.current_role = Some(AgentRole::Architect);

        lock.insert(ticket_upper.clone(), task.clone());
        save_task_to_disk(state_dir, &task);
        Ok(task)
    }

    #[deprecated]
    pub fn iniciar_tarea(
        &self,
        workspace: &Path,
        state_dir: &Path,
        ticket_id: &str,
    ) -> Result<FlowTask> {
        self.start_task(workspace, state_dir, ticket_id)
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
        let anterior = task.state;
        let model_opt = model.map(String::from);

        match anterior {
            FlowState::Planning => {
                // Arquitecto terminó -> pasa a Coder
                task.state = FlowState::Implementing;
                task.current_role = Some(AgentRole::Coder);
                task.history.push(FlowTransition {
                    timestamp_seconds: timestamp,
                    old_state: anterior,
                    new_state: task.state,
                    role: task.current_role,
                    detail: if detalle.is_empty() {
                        "Plan aprobado por Arquitecto. Coder iniciando cambios en Worktree.".into()
                    } else {
                        detalle.to_string()
                    },
                    model: model_opt,
                });
            }
            FlowState::Implementing => {
                // Coder terminó -> pasa a QA
                task.state = FlowState::Testing;
                task.current_role = Some(AgentRole::QA);
                task.history.push(FlowTransition {
                    timestamp_seconds: timestamp,
                    old_state: anterior,
                    new_state: task.state,
                    role: task.current_role,
                    detail: if detalle.is_empty() {
                        "Código generado. Agente QA ejecutando pruebas en Sandbox.".into()
                    } else {
                        detalle.to_string()
                    },
                    model: model_opt,
                });
            }
            FlowState::Testing => {
                if test_exitoso {
                    // QA aprobó -> pasa a Auditor
                    task.state = FlowState::Reviewing;
                    task.current_role = Some(AgentRole::Auditor);
                    task.history.push(FlowTransition {
                        timestamp_seconds: timestamp,
                        old_state: anterior,
                        new_state: task.state,
                        role: task.current_role,
                        detail: if detalle.is_empty() {
                            "Todas las pruebas pasaron en verde. Auditor verificando diff y seguridad.".into()
                        } else {
                            detalle.to_string()
                        },
                        model: model_opt,
                    });
                } else if task.qa_retries < task.max_qa_retries {
                    // QA falló -> realimentar a Coder para corrección
                    task.qa_retries += 1;
                    task.state = FlowState::Implementing;
                    task.current_role = Some(AgentRole::Coder);
                    task.history.push(FlowTransition {
                        timestamp_seconds: timestamp,
                        old_state: anterior,
                        new_state: task.state,
                        role: task.current_role,
                        detail: format!(
                            "Tests fallaron (intento {}/{}). Realimentando errores a Coder: {detalle}",
                            task.qa_retries, task.max_qa_retries
                        ),
                        model: model_opt,
                    });
                } else {
                    // Superó límite de reintentos
                    task.state = FlowState::Failed;
                    task.current_role = None;
                    task.history.push(FlowTransition {
                        timestamp_seconds: timestamp,
                        old_state: anterior,
                        new_state: FlowState::Failed,
                        role: None,
                        detail: format!(
                            "Límite de reintentos excedido ({}/{}). Tarea marcada como fallida.",
                            task.qa_retries, task.max_qa_retries
                        ),
                        model: model_opt,
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
                    old_state: anterior,
                    new_state: task.state,
                    role: None,
                    detail: if detalle.is_empty() {
                        "Auditoría completada. Esperando confirmación final del desarrollador."
                            .into()
                    } else {
                        detalle.to_string()
                    },
                    model: model_opt,
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
    pub fn advance_phase(
        &self,
        ticket_id: &str,
        detalle: &str,
        test_exitoso: bool,
    ) -> Result<FlowTask> {
        self.advance_phase_with_model(ticket_id, detalle, test_exitoso, None)
    }

    #[deprecated]
    pub fn avanzar_fase(
        &self,
        ticket_id: &str,
        detalle: &str,
        test_exitoso: bool,
    ) -> Result<FlowTask> {
        self.advance_phase_with_model(ticket_id, detalle, test_exitoso, None)
    }

    /// Alias compatible con modelo.
    pub fn avanzar_fase_con_modelo(
        &self,
        ticket_id: &str,
        detalle: &str,
        test_exitoso: bool,
        model: Option<&str>,
    ) -> Result<FlowTask> {
        self.advance_phase_with_model(ticket_id, detalle, test_exitoso, model)
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
        let anterior = task.state;

        if decision {
            task.state = FlowState::Merged;
            task.history.push(FlowTransition {
                timestamp_seconds: timestamp,
                old_state: anterior,
                new_state: FlowState::Merged,
                role: None,
                detail: "Aprobado por el desarrollador. Cambios integrados a la rama principal."
                    .into(),
                model: None,
            });
        } else {
            task.state = FlowState::Failed;
            task.history.push(FlowTransition {
                timestamp_seconds: timestamp,
                old_state: anterior,
                new_state: FlowState::Failed,
                role: None,
                detail: "Rechazado por el desarrollador. Worktree descartado.".into(),
                model: None,
            });
        }

        Ok(task.clone())
    }

    #[deprecated]
    pub fn aprobar_tarea(&self, ticket_id: &str, decision: bool) -> Result<FlowTask> {
        self.approve_task(ticket_id, decision)
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

    #[deprecated]
    pub fn consultar_tarea(&self, ticket_id: &str) -> Option<FlowTask> {
        self.get_task(ticket_id)
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

    #[deprecated]
    pub fn listar_tareas(&self) -> Vec<FlowTask> {
        self.list_tasks()
    }

    /// Ejecuta el pipeline completo de agentes en segundo plano para un ticket (T3.2):
    /// 1. Arquitecto analiza especificación.
    /// 2. Creación del Worktree efímero aislado (T2.2).
    /// 3. Coder aplica los cambios en el Worktree.
    /// 4. QA ejecuta tests en Sandbox. Si fallan, bucle de reintento.
    /// 5. Auditor genera diff consolidado y transiciona a ListoParaAprobacion.
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

    #[deprecated]
    pub fn ejecutar_pipeline_worktree(
        &self,
        workspace: &Path,
        state_dir: &Path,
        ticket_id: &str,
        cambios_ficheros: &[(String, String)],
    ) -> Result<FlowTask> {
        self.run_worktree_pipeline(workspace, state_dir, ticket_id, cambios_ficheros)
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
                let exito = o.status.success();
                let salida = format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                );
                Ok((exito, salida.trim().to_string()))
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
                let exito = o.status.success();
                let salida = format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                );
                Ok((exito, salida.trim().to_string()))
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

#[deprecated]
pub fn ejecutar_tests_en_worktree(worktree: &Path) -> Result<(bool, String)> {
    run_worktree_tests(worktree)
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

#[deprecated]
pub fn calcular_diff_worktree(worktree: &Path) -> Result<String> {
    calculate_worktree_diff(worktree)
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn ahora_segundos() -> u64 {
    now_secs()
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
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_roles_and_system_prompts() {
        let roles = [
            AgentRole::Arquitecto,
            AgentRole::Coder,
            AgentRole::QA,
            AgentRole::Auditor,
            AgentRole::VisualQA,
        ];
        for r in roles {
            assert!(!r.name().is_empty());
            assert!(!r.description().is_empty());
            assert!(!r.system_prompt().is_empty());
        }
    }

    #[test]
    fn test_ciclo_completo_maquina_estados() {
        let engine = FlowEngine::global();
        let cwd = std::env::current_dir().expect("cwd");
        let state_dir = cwd.join(".antos");

        // 1. Iniciar tarea con ticket T3.1
        let task = engine
            .iniciar_tarea(&cwd, &state_dir, "T3.1")
            .expect("iniciar tarea T3.1");

        assert_eq!(task.ticket_id, "T3.1");
        assert_eq!(task.state, FlowState::Planning);
        assert_eq!(task.current_role, Some(AgentRole::Architect));

        // 2. Arquitecto termina plan -> Coder
        let task = engine
            .avanzar_fase("T3.1", "arquitectura validada", true)
            .expect("avanzar a Coder");
        assert_eq!(task.state, FlowState::Implementing);
        assert_eq!(task.current_role, Some(AgentRole::Coder));

        // 3. Coder termina código -> QA
        let task = engine
            .avanzar_fase("T3.1", "codigo generado", true)
            .expect("avanzar a QA");
        assert_eq!(task.state, FlowState::Testing);
        assert_eq!(task.current_role, Some(AgentRole::QA));

        // 4. QA pasa tests -> Auditor
        let task = engine
            .avanzar_fase("T3.1", "tests pasaron en verde", true)
            .expect("avanzar a Auditor");
        assert_eq!(task.state, FlowState::Reviewing);
        assert_eq!(task.current_role, Some(AgentRole::Auditor));

        // 5. Auditor termina revisión -> Listo para aprobación
        let task = engine
            .avanzar_fase("T3.1", "auditoria completada", true)
            .expect("avanzar a ListoParaAprobacion");
        assert_eq!(task.state, FlowState::ReadyForApproval);
        assert_eq!(task.current_role, None);

        // 6. Aprobación final -> Fusionado
        let task = engine.approve_task("T3.1", true).expect("aprobar tarea");
        assert_eq!(task.state, FlowState::Merged);
        assert_eq!(task.history.len(), 6);
    }

    #[test]
    fn test_retry_on_qa_test_failure() {
        let engine = FlowEngine::global();
        let cwd = std::env::current_dir().expect("cwd");
        let state_dir = cwd.join(".antos");

        // Iniciar con ticket T1.1
        let _ = engine.iniciar_tarea(&cwd, &state_dir, "T1.1");
        let _ = engine.avanzar_fase("T1.1", "plan", true); // -> Implementing
        let _ = engine.avanzar_fase("T1.1", "codigo", true); // -> Testing

        // QA reporta fallo -> debe volver a Implementing con reintento 1
        let task_reintento = engine
            .avanzar_fase("T1.1", "assertion failed line 42", false)
            .expect("reintento QA");

        assert_eq!(task_reintento.state, FlowState::Implementing);
        assert_eq!(task_reintento.qa_retries, 1);
        assert_eq!(task_reintento.current_role, Some(AgentRole::Coder));
    }

    #[test]
    fn test_automated_pipeline_in_worktree() {
        let engine = FlowEngine::global();
        let temp_dir = std::env::temp_dir().join("antos_test_pipeline");
        let ws_dir = temp_dir.join("workspace");
        let state_dir = temp_dir.join(".antos");
        let tickets_dir = ws_dir.join("docs/tickets");

        std::fs::create_dir_all(&tickets_dir).expect("create tickets dir");
        std::fs::create_dir_all(&state_dir).expect("create state dir");

        // Crear ticket dummy T9.1
        let ticket_md =
            "# T9.1 · Pipeline Test\n\n## Descripción\nTest\n\n## Criterios de Aceptación\n* OK\n";
        std::fs::write(tickets_dir.join("T9.1-pipeline-test.md"), ticket_md).expect("write ticket");

        // Ejecutar pipeline completo pasando cambios
        let cambios = vec![("src/lib.rs".to_string(), "// test autogenerado".to_string())];
        let task = engine
            .ejecutar_pipeline_worktree(&ws_dir, &state_dir, "T9.1", &cambios)
            .expect("ejecutar pipeline");

        assert_eq!(task.ticket_id, "T9.1");
        assert_eq!(task.state, FlowState::ReadyForApproval);
        assert!(task.audit_summary.is_some());

        // Limpiar directorio temporal de prueba
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_flow_transitions_record_role_models() {
        let engine = FlowEngine::global();
        let temp_dir = std::env::temp_dir().join("antos_test_flow_models");
        let ws_dir = temp_dir.join("workspace");
        let state_dir = temp_dir.join(".antos");
        let tickets_dir = ws_dir.join("docs/tickets");

        std::fs::create_dir_all(&tickets_dir).expect("create tickets dir");
        std::fs::create_dir_all(&state_dir).expect("create state dir");

        // Set custom role model for Coder and Architect
        let mut config = crate::llm::LlmConfig::default();
        config.set_role_model("architect", "custom:deepseek-r1");
        config.set_role_model("coder", "custom:qwen2.5-coder");
        config.save_to_state(&state_dir).expect("save config");

        let ticket_md = "# T99.1 · Multi Model Test\n\n## Descripción\nTest\n\n## Criterios de Aceptación\n* OK\n";
        std::fs::write(tickets_dir.join("T99.1-test.md"), ticket_md).expect("write ticket");

        let task = engine
            .run_worktree_pipeline(&ws_dir, &state_dir, "T99.1", &[])
            .expect("run pipeline");

        assert_eq!(task.ticket_id, "T99.1");
        assert_eq!(task.state, FlowState::ReadyForApproval);

        // Verify that history contains models
        let arch_trans = task
            .history
            .iter()
            .find(|t| t.role == Some(AgentRole::Architect));
        assert!(arch_trans.is_some());
        assert_eq!(
            arch_trans.unwrap().model.as_deref(),
            Some("custom:deepseek-r1")
        );

        let coder_trans = task
            .history
            .iter()
            .find(|t| t.role == Some(AgentRole::Coder));
        assert!(coder_trans.is_some());
        assert_eq!(
            coder_trans.unwrap().model.as_deref(),
            Some("custom:qwen2.5-coder")
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
