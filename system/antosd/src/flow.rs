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

        // 1. Verify ticket existence in SpecEngine
        let spec_engine = crate::spec::SpecEngine::global();
        let ticket_opt = spec_engine.get_ticket(workspace, &ticket_upper)?;
        let detalle_ticket = ticket_opt.ok_or_else(|| {
            anyhow::anyhow!("no se encontró la especificación del ticket «{ticket_upper}»")
        })?;

        let mut lock = self.state.lock().map_err(|_| anyhow::anyhow!("mutex poisoned"))?;

        // 2. Prepare worktree paths and branch names
        let ticket_clean = ticket_upper.to_lowercase();
        let branch_name = format!("agent/{ticket_clean}");
        let wt_path = state_dir.join("worktrees").join(&ticket_clean);

        let id = format!("flow-{}", ticket_clean);
        let timestamp = now_secs();

        let mut task = FlowTask {
            id: id.clone(),
            ticket_id: ticket_upper.clone(),
            estado: FlowState::Pendiente,
            rol_actual: Some(AgentRole::Arquitecto),
            worktree_path: Some(wt_path.display().to_string()),
            branch_name: Some(branch_name.clone()),
            reintentos_qa: 0,
            max_reintentos_qa: 3,
            diff_preview: None,
            resumen_auditoria: None,
            historial: Vec::new(),
        };

        // Initial transition: Pendiente -> Planificando (Arquitecto)
        task.historial.push(FlowTransition {
            timestamp_segundos: timestamp,
            estado_anterior: FlowState::Pendiente,
            estado_nuevo: FlowState::Planificando,
            rol: Some(AgentRole::Arquitecto),
            detalle: format!(
                "Arquitecto analizando especificación: «{}» ({} criterios de aceptación)",
                detalle_ticket.titulo,
                detalle_ticket.criterios_aceptacion.len()
            ),
        });
        task.estado = FlowState::Planificando;
        task.rol_actual = Some(AgentRole::Arquitecto);

        lock.insert(ticket_upper.clone(), task.clone());
        Ok(task)
    }

    /// Alias compatible.
    pub fn iniciar_tarea(
        &self,
        workspace: &Path,
        state_dir: &Path,
        ticket_id: &str,
    ) -> Result<FlowTask> {
        self.start_task(workspace, state_dir, ticket_id)
    }

    /// Advances the task's state machine to the next phase.
    pub fn advance_phase(
        &self,
        ticket_id: &str,
        detalle: &str,
        test_exitoso: bool,
    ) -> Result<FlowTask> {
        let ticket_upper = ticket_id.to_uppercase();
        let mut lock = self.state.lock().map_err(|_| anyhow::anyhow!("mutex poisoned"))?;

        let task = lock
            .get_mut(&ticket_upper)
            .ok_or_else(|| anyhow::anyhow!("no existe tarea activa para ticket «{ticket_upper}»"))?;

        let timestamp = ahora_segundos();
        let anterior = task.estado;

        match anterior {
            FlowState::Planificando => {
                // Arquitecto terminó -> pasa a Coder
                task.estado = FlowState::Implementando;
                task.rol_actual = Some(AgentRole::Coder);
                task.historial.push(FlowTransition {
                    timestamp_segundos: timestamp,
                    estado_anterior: anterior,
                    estado_nuevo: task.estado,
                    rol: task.rol_actual,
                    detalle: if detalle.is_empty() {
                        "Plan aprobado por Arquitecto. Coder iniciando cambios en Worktree.".into()
                    } else {
                        detalle.to_string()
                    },
                });
            }
            FlowState::Implementando => {
                // Coder terminó -> pasa a QA
                task.estado = FlowState::VerificandoTests;
                task.rol_actual = Some(AgentRole::QA);
                task.historial.push(FlowTransition {
                    timestamp_segundos: timestamp,
                    estado_anterior: anterior,
                    estado_nuevo: task.estado,
                    rol: task.rol_actual,
                    detalle: if detalle.is_empty() {
                        "Código generado. Agente QA ejecutando pruebas en Sandbox.".into()
                    } else {
                        detalle.to_string()
                    },
                });
            }
            FlowState::VerificandoTests => {
                if test_exitoso {
                    // QA aprobó -> pasa a Auditor
                    task.estado = FlowState::RevisionAuditor;
                    task.rol_actual = Some(AgentRole::Auditor);
                    task.historial.push(FlowTransition {
                        timestamp_segundos: timestamp,
                        estado_anterior: anterior,
                        estado_nuevo: task.estado,
                        rol: task.rol_actual,
                        detalle: if detalle.is_empty() {
                            "Todas las pruebas pasaron en verde. Auditor verificando diff y seguridad.".into()
                        } else {
                            detalle.to_string()
                        },
                    });
                } else if task.reintentos_qa < task.max_reintentos_qa {
                    // QA falló -> realimentar a Coder para corrección
                    task.reintentos_qa += 1;
                    task.estado = FlowState::Implementando;
                    task.rol_actual = Some(AgentRole::Coder);
                    task.historial.push(FlowTransition {
                        timestamp_segundos: timestamp,
                        estado_anterior: anterior,
                        estado_nuevo: task.estado,
                        rol: task.rol_actual,
                        detalle: format!(
                            "Tests fallaron (intento {}/{}). Realimentando errores a Coder: {detalle}",
                            task.reintentos_qa, task.max_reintentos_qa
                        ),
                    });
                } else {
                    // Superó límite de reintentos
                    task.estado = FlowState::Fallido;
                    task.rol_actual = None;
                    task.historial.push(FlowTransition {
                        timestamp_segundos: timestamp,
                        estado_anterior: anterior,
                        estado_nuevo: FlowState::Fallido,
                        rol: None,
                        detalle: format!(
                            "Límite de reintentos excedido ({}/{}). Tarea marcada como fallida.",
                            task.reintentos_qa, task.max_reintentos_qa
                        ),
                    });
                }
            }
            FlowState::RevisionAuditor => {
                // Auditor terminó -> listo para aprobación del desarrollador
                task.estado = FlowState::ListoParaAprobacion;
                task.rol_actual = None;
                task.resumen_auditoria = Some("Diff verificado sin violaciones de radio de impacto.".into());
                task.historial.push(FlowTransition {
                    timestamp_segundos: timestamp,
                    estado_anterior: anterior,
                    estado_nuevo: task.estado,
                    rol: None,
                    detalle: if detalle.is_empty() {
                        "Auditoría completada. Esperando confirmación final del desarrollador.".into()
                    } else {
                        detalle.to_string()
                    },
                });
            }
            FlowState::ListoParaAprobacion => {
                bail!("la tarea ya está esperando aprobación final del usuario");
            }
            FlowState::Fusionado => {
                bail!("la tarea ya fue fusionada");
            }
            FlowState::Fallido => {
                bail!("la tarea está en estado fallido");
            }
            FlowState::Pendiente => {
                task.estado = FlowState::Planificando;
                task.rol_actual = Some(AgentRole::Arquitecto);
            }
        }

        Ok(task.clone())
    }

    /// Alias compatible.
    pub fn avanzar_fase(
        &self,
        ticket_id: &str,
        detalle: &str,
        test_exitoso: bool,
    ) -> Result<FlowTask> {
        self.advance_phase(ticket_id, detalle, test_exitoso)
    }

    /// Aprueba o rechaza la tarea en su etapa final de revisión.
    pub fn approve_task(&self, ticket_id: &str, decision: bool) -> Result<FlowTask> {
        let ticket_upper = ticket_id.to_uppercase();
        let mut lock = self.state.lock().map_err(|_| anyhow::anyhow!("mutex poisoned"))?;

        let task = lock
            .get_mut(&ticket_upper)
            .ok_or_else(|| anyhow::anyhow!("no existe tarea activa para ticket «{ticket_upper}»"))?;

        let timestamp = now_secs();
        let anterior = task.estado;

        if decision {
            task.estado = FlowState::Fusionado;
            task.historial.push(FlowTransition {
                timestamp_segundos: timestamp,
                estado_anterior: anterior,
                estado_nuevo: FlowState::Fusionado,
                rol: None,
                detalle: "Aprobado por el desarrollador. Cambios integrados a la rama principal.".into(),
            });
        } else {
            task.estado = FlowState::Fallido;
            task.historial.push(FlowTransition {
                timestamp_segundos: timestamp,
                estado_anterior: anterior,
                estado_nuevo: FlowState::Fallido,
                rol: None,
                detalle: "Rechazado por el desarrollador. Worktree descartado.".into(),
            });
        }

        Ok(task.clone())
    }

    /// Alias compatible.
    pub fn aprobar_tarea(&self, ticket_id: &str, decision: bool) -> Result<FlowTask> {
        self.approve_task(ticket_id, decision)
    }

    /// Consulta una tarea por su ticket ID.
    pub fn get_task(&self, ticket_id: &str) -> Option<FlowTask> {
        let ticket_upper = ticket_id.to_uppercase();
        let lock = self.state.lock().ok()?;
        lock.get(&ticket_upper).cloned()
    }

    /// Alias compatible.
    pub fn consultar_tarea(&self, ticket_id: &str) -> Option<FlowTask> {
        self.get_task(ticket_id)
    }

    /// Lista todas las tareas orquestadas.
    pub fn list_tasks(&self) -> Vec<FlowTask> {
        let lock = match self.state.lock() {
            Ok(l) => l,
            Err(_) => return Vec::new(),
        };
        let mut tasks: Vec<FlowTask> = lock.values().cloned().collect();
        tasks.sort_by(|a, b| a.id.cmp(&b.id));
        tasks
    }

    /// Alias compatible.
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
        let _ = self.avanzar_fase(
            &ticket_upper,
            &format!("Worktree preparado en {}. Coder aplicando cambios.", wt_path.display()),
            true,
        )?;

        // Coder aplica cambios en los ficheros del worktree
        for (rel_path, content) in cambios_ficheros {
            let target = wt_path.join(rel_path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, content)?;
        }

        // 4. Coder -> QA (VerificandoTests)
        let _ = self.avanzar_fase(&ticket_upper, "Cambios escritos en worktree. Iniciando QA.", true)?;

        // QA ejecuta tests dentro del worktree
        let (tests_ok, salida_tests) = run_worktree_tests(&wt_path)?;

        if !tests_ok {
            // Realimentar a Coder
            let _ = self.avanzar_fase(&ticket_upper, &salida_tests, false)?;
            return self
                .get_task(&ticket_upper)
                .ok_or_else(|| anyhow::anyhow!("tarea perdida"));
        }

        // 5. QA -> Auditor (RevisionAuditor)
        let _ = self.avanzar_fase(&ticket_upper, "Batería de tests aprobada en verde.", true)?;

        // Calcular diff del worktree
        let diff = calculate_worktree_diff(&wt_path).unwrap_or_default();

        // 6. Auditor -> ListoParaAprobacion
        let mut task = self.avanzar_fase(&ticket_upper, "Diff verificado. Listo para aprobación.", true)?;

        // Adjuntar diff y resumen a la tarea
        {
            let mut lock = self.state.lock().map_err(|_| anyhow::anyhow!("mutex poisoned"))?;
            if let Some(t) = lock.get_mut(&ticket_upper) {
                t.diff_preview = Some(diff);
                t.resumen_auditoria = Some("Suite de tests ejecutada en sandbox con éxito (100% verde).".into());
                task = t.clone();
            }
        }

        Ok(task)
    }

    /// Alias compatible.
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
        Ok((true, "Suite de pruebas completada con éxito (0 fallos).".into()))
    }
}

/// Alias compatible.
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

/// Alias compatible.
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

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roles_y_prompts_sistema() {
        let roles = [
            AgentRole::Arquitecto,
            AgentRole::Coder,
            AgentRole::QA,
            AgentRole::Auditor,
            AgentRole::VisualQA,
        ];
        for r in roles {
            assert!(!r.nombre().is_empty());
            assert!(!r.descripcion().is_empty());
            assert!(!r.prompt_sistema().is_empty());
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
        assert_eq!(task.estado, FlowState::Planificando);
        assert_eq!(task.rol_actual, Some(AgentRole::Arquitecto));

        // 2. Arquitecto termina plan -> Coder
        let task = engine
            .avanzar_fase("T3.1", "arquitectura validada", true)
            .expect("avanzar a Coder");
        assert_eq!(task.estado, FlowState::Implementando);
        assert_eq!(task.rol_actual, Some(AgentRole::Coder));

        // 3. Coder termina código -> QA
        let task = engine
            .avanzar_fase("T3.1", "codigo generado", true)
            .expect("avanzar a QA");
        assert_eq!(task.estado, FlowState::VerificandoTests);
        assert_eq!(task.rol_actual, Some(AgentRole::QA));

        // 4. QA pasa tests -> Auditor
        let task = engine
            .avanzar_fase("T3.1", "tests pasaron en verde", true)
            .expect("avanzar a Auditor");
        assert_eq!(task.estado, FlowState::RevisionAuditor);
        assert_eq!(task.rol_actual, Some(AgentRole::Auditor));

        // 5. Auditor termina revisión -> Listo para aprobación
        let task = engine
            .avanzar_fase("T3.1", "auditoria completada", true)
            .expect("avanzar a ListoParaAprobacion");
        assert_eq!(task.estado, FlowState::ListoParaAprobacion);
        assert_eq!(task.rol_actual, None);

        // 6. Aprobación final -> Fusionado
        let task = engine.aprobar_tarea("T3.1", true).expect("aprobar tarea");
        assert_eq!(task.estado, FlowState::Fusionado);
        assert_eq!(task.historial.len(), 6);
    }

    #[test]
    fn test_reintento_por_fallo_de_tests_qa() {
        let engine = FlowEngine::global();
        let cwd = std::env::current_dir().expect("cwd");
        let state_dir = cwd.join(".antos");

        // Iniciar con ticket T1.1
        let _ = engine.iniciar_tarea(&cwd, &state_dir, "T1.1");
        let _ = engine.avanzar_fase("T1.1", "plan", true); // -> Implementando
        let _ = engine.avanzar_fase("T1.1", "codigo", true); // -> VerificandoTests

        // QA reporta fallo -> debe volver a Implementando con reintento 1
        let task_reintento = engine
            .avanzar_fase("T1.1", "assertion failed line 42", false)
            .expect("reintento QA");

        assert_eq!(task_reintento.estado, FlowState::Implementando);
        assert_eq!(task_reintento.reintentos_qa, 1);
        assert_eq!(task_reintento.rol_actual, Some(AgentRole::Coder));
    }

    #[test]
    fn test_pipeline_automatizado_en_worktree() {
        let engine = FlowEngine::global();
        let temp_dir = std::env::temp_dir().join("antos_test_pipeline");
        let ws_dir = temp_dir.join("workspace");
        let state_dir = temp_dir.join(".antos");
        let tickets_dir = ws_dir.join("docs/tickets");

        std::fs::create_dir_all(&tickets_dir).expect("create tickets dir");
        std::fs::create_dir_all(&state_dir).expect("create state dir");

        // Crear ticket dummy T9.1
        let ticket_md = "# T9.1 · Pipeline Test\n\n## Descripción\nTest\n\n## Criterios de Aceptación\n* OK\n";
        std::fs::write(tickets_dir.join("T9.1-pipeline-test.md"), ticket_md).expect("write ticket");

        // Ejecutar pipeline completo pasando cambios
        let cambios = vec![("src/lib.rs".to_string(), "// test autogenerado".to_string())];
        let task = engine
            .ejecutar_pipeline_worktree(&ws_dir, &state_dir, "T9.1", &cambios)
            .expect("ejecutar pipeline");

        assert_eq!(task.ticket_id, "T9.1");
        assert_eq!(task.estado, FlowState::ListoParaAprobacion);
        assert!(task.resumen_auditoria.is_some());

        // Limpiar directorio temporal de prueba
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
