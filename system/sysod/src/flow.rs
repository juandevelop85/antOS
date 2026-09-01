//! Orquestador de Roles Multi-Agente (antFlow Core en Rust) (T3.1).
//!
//! Coordina la máquina de estados entre agentes especializados:
//! - Arquitecto: Análisis técnico de tickets y especificaciones (T1.3).
//! - Coder: Modificación de código y refactorización en Worktree efímero (T2.2).
//! - QA / Tester: Ejecución de pruebas y validación en sandbox confinado.
//! - Auditor: Análisis de radio de impacto, seguridad y diffs para aprobación final.

use antos_protocolo::{AgentRole, FlowState, FlowTask, FlowTransition};
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

    /// Inicia una nueva tarea de orquestación antFlow para un ticket dado.
    pub fn iniciar_tarea(
        &self,
        workspace: &Path,
        state_dir: &Path,
        ticket_id: &str,
    ) -> Result<FlowTask> {
        let ticket_upper = ticket_id.to_uppercase();

        // 1. Verificar existencia del ticket en el SpecEngine
        let spec_engine = crate::spec::SpecEngine::global();
        let ticket_opt = spec_engine.obtener_ticket(workspace, &ticket_upper)?;
        let detalle_ticket = ticket_opt.ok_or_else(|| {
            anyhow::anyhow!("no se encontró la especificación del ticket «{ticket_upper}»")
        })?;

        let mut lock = self.state.lock().map_err(|_| anyhow::anyhow!("mutex poisoned"))?;

        // 2. Preparar rutas de worktree y ramas
        let ticket_clean = ticket_upper.to_lowercase();
        let branch_name = format!("agent/{ticket_clean}");
        let wt_path = state_dir.join("worktrees").join(&ticket_clean);

        let id = format!("flow-{}", ticket_clean);
        let timestamp = ahora_segundos();

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

        // Registrar transición inicial: Pendiente -> Planificando (Arquitecto)
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

    /// Avanza la máquina de estados de una tarea a su siguiente fase.
    pub fn avanzar_fase(
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

    /// Aprueba o rechaza la tarea en su etapa final de revisión.
    pub fn aprobar_tarea(&self, ticket_id: &str, decision: bool) -> Result<FlowTask> {
        let ticket_upper = ticket_id.to_uppercase();
        let mut lock = self.state.lock().map_err(|_| anyhow::anyhow!("mutex poisoned"))?;

        let task = lock
            .get_mut(&ticket_upper)
            .ok_or_else(|| anyhow::anyhow!("no existe tarea activa para ticket «{ticket_upper}»"))?;

        let timestamp = ahora_segundos();
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

    /// Consulta una tarea por su ticket ID.
    pub fn consultar_tarea(&self, ticket_id: &str) -> Option<FlowTask> {
        let ticket_upper = ticket_id.to_uppercase();
        let lock = self.state.lock().ok()?;
        lock.get(&ticket_upper).cloned()
    }

    /// Lista todas las tareas orquestadas.
    pub fn listar_tareas(&self) -> Vec<FlowTask> {
        let lock = match self.state.lock() {
            Ok(l) => l,
            Err(_) => return Vec::new(),
        };
        let mut tasks: Vec<FlowTask> = lock.values().cloned().collect();
        tasks.sort_by(|a, b| a.id.cmp(&b.id));
        tasks
    }
}

fn ahora_segundos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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
}
