#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn test_roles_and_system_prompts() {
    let roles = [
        AgentRole::Architect,
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
        .start_task(&cwd, &state_dir, "T3.1")
        .expect("iniciar tarea T3.1");

    assert_eq!(task.ticket_id, "T3.1");
    assert_eq!(task.state, FlowState::Planning);
    assert_eq!(task.current_role, Some(AgentRole::Architect));

    // 2. Arquitecto termina plan -> Coder
    let task = engine
        .advance_phase("T3.1", "arquitectura validada", true)
        .expect("avanzar a Coder");
    assert_eq!(task.state, FlowState::Implementing);
    assert_eq!(task.current_role, Some(AgentRole::Coder));

    // 3. Coder termina código -> QA
    let task = engine
        .advance_phase("T3.1", "codigo generado", true)
        .expect("avanzar a QA");
    assert_eq!(task.state, FlowState::Testing);
    assert_eq!(task.current_role, Some(AgentRole::QA));

    // 4. QA pasa tests -> Auditor
    let task = engine
        .advance_phase("T3.1", "tests pasaron en verde", true)
        .expect("avanzar a Auditor");
    assert_eq!(task.state, FlowState::Reviewing);
    assert_eq!(task.current_role, Some(AgentRole::Auditor));

    // 5. Auditor termina revisión -> Listo para aprobación
    let task = engine
        .advance_phase("T3.1", "auditoria completada", true)
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
    let _ = engine.start_task(&cwd, &state_dir, "T1.1");
    let _ = engine.advance_phase("T1.1", "plan", true); // -> Implementing
    let _ = engine.advance_phase("T1.1", "codigo", true); // -> Testing

    // QA reporta fallo -> debe volver a Implementing con reintento 1
    let task_reintento = engine
        .advance_phase("T1.1", "assertion failed line 42", false)
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
        .run_worktree_pipeline(&ws_dir, &state_dir, "T9.1", &cambios)
        .expect("ejecutar pipeline");

    assert_eq!(task.ticket_id, "T9.1");
    assert_eq!(task.state, FlowState::ReadyForApproval);
    assert!(task.audit_summary.is_some());
    // T33.1: el pipeline de hoy es una simulación y lo dice.
    assert_eq!(task.backend, FlowBackend::Simulated);
    assert!(
        task.history.iter().all(|t| t.simulated),
        "ninguna transición del pipeline simulado puede presentarse como obra de un modelo"
    );

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

    let ticket_md =
        "# T99.1 · Multi Model Test\n\n## Descripción\nTest\n\n## Criterios de Aceptación\n* OK\n";
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
