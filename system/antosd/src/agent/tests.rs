#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::fake::{FakeProvider, ScriptedCall, ScriptedTurn};
use super::*;
use antos_protocol::AgentStopReason;
use serde_json::json;
use std::path::PathBuf;

/// Un `Ctx` aislado: catálogo real (el binario de test corre dentro del árbol
/// de antOS), workspace y estado en un temporal propio de cada test.
fn test_ctx(name: &str) -> (Ctx, PathBuf) {
    let discovered = Ctx::discover().expect("Ctx::discover dentro del árbol de antOS");
    let temp = std::env::temp_dir().join(format!("antos_agent_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    let ws = temp.join("workspace");
    let state = temp.join("state");
    std::fs::create_dir_all(&ws).unwrap();
    std::fs::create_dir_all(&state).unwrap();
    let ctx = Ctx {
        workspace: ws,
        state,
        current_project: None,
        ..discovered
    };
    (ctx, temp)
}

/// Un crate mínimo con un test en rojo: `sum` devuelve el producto.
fn fixture_crate(ws: &std::path::Path) {
    std::fs::write(
        ws.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::create_dir_all(ws.join("src")).unwrap();
    std::fs::write(
        ws.join("src/lib.rs"),
        "pub fn sum(a: i32, b: i32) -> i32 {\n    a * b\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn sums() {\n        assert_eq!(super::sum(2, 3), 5);\n    }\n}\n",
    )
    .unwrap();
}

#[derive(Default)]
struct RecordingHandler {
    steps: Vec<AgentStepEvent>,
    notes: Vec<String>,
    confirms: Vec<String>,
    approve: bool,
    done: Option<AgentReport>,
}

impl AgentHandler for RecordingHandler {
    fn on_step(&mut self, event: &AgentStepEvent) -> Result<()> {
        self.steps.push(event.clone());
        Ok(())
    }
    fn on_confirm(&mut self, proposal: &Proposal) -> Result<bool> {
        self.confirms
            .push(proposal.plan.steps[0].capability.clone());
        Ok(self.approve)
    }
    fn on_note(&mut self, text: &str) -> Result<()> {
        self.notes.push(text.to_string());
        Ok(())
    }
    fn on_done(&mut self, report: &AgentReport) -> Result<()> {
        self.done = Some(report.clone());
        Ok(())
    }
}

fn call(tool: &str, input: serde_json::Value) -> ScriptedCall {
    ScriptedCall {
        tool: tool.into(),
        input,
    }
}

fn in_process(goal: &str) -> RunConfig {
    let mut cfg = RunConfig::new(goal);
    cfg.executor = Executor::InProcess;
    cfg
}

#[test]
fn fake_agent_reads_patches_tests_and_finishes_with_undoable_snapshot() {
    let (ctx, temp) = test_ctx("happy");
    fixture_crate(&ctx.workspace);
    let catalog = Catalog::load(&ctx.caps_dir).unwrap();

    let script = vec![
        ScriptedTurn {
            text: "Miro el código.".into(),
            calls: vec![call("fs.read", json!({"path": "src/lib.rs"}))],
            tokens: 100,
        },
        ScriptedTurn {
            text: "Corrijo la suma.".into(),
            calls: vec![call(
                "fs.patch",
                json!({"path": "src/lib.rs", "old": "    a * b\n", "new": "    a + b\n"}),
            )],
            tokens: 100,
        },
        ScriptedTurn {
            text: "Verifico.".into(),
            calls: vec![call("test.run", json!({"path": "."}))],
            tokens: 100,
        },
        ScriptedTurn {
            text: String::new(),
            calls: vec![call(
                FINISH_TOOL,
                json!({"resumen": "sum corregida y tests en verde"}),
            )],
            tokens: 50,
        },
    ];
    let mut provider = FakeProvider::new(script);
    let mut handler = RecordingHandler {
        approve: true,
        ..Default::default()
    };
    let cfg = in_process("haz que pase el test sums");
    let report = run(&ctx, &catalog, &mut provider, &cfg, &mut handler).unwrap();

    assert_eq!(report.stop_reason, AgentStopReason::Finished);
    assert_eq!(report.summary, "sum corregida y tests en verde");
    assert_eq!(report.steps, 4);
    assert_eq!(report.tokens_used, 350);
    assert_eq!(report.files_written, vec!["src/lib.rs".to_string()]);
    assert!(report.snapshot_id.is_some());
    assert_eq!(handler.confirms, vec!["fs.patch".to_string()]);

    // El modelo vio el fichero, el parche aplicado y los tests en verde.
    assert!(provider.received[0].content.contains("a * b"));
    assert!(provider.received[1].content.contains("parche aplicado"));
    assert!(
        provider.received[2].content.starts_with("TESTS EN VERDE"),
        "{}",
        provider.received[2].content
    );
    assert!(std::fs::read_to_string(ctx.workspace.join("src/lib.rs"))
        .unwrap()
        .contains("a + b"));

    // Un registro de journal por run, con instantánea; `undo` lo deshace.
    let records = journal::read_all(&ctx.journal_path()).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].outcome, Outcome::Executed);
    assert_eq!(records[0].plan.steps.len(), 3); // read, patch, test.run
    let snap = snapshot::load(records[0].snapshot.as_ref().unwrap(), &ctx.snapshots_dir()).unwrap();
    snapshot::restore(&snap).unwrap();
    assert!(std::fs::read_to_string(ctx.workspace.join("src/lib.rs"))
        .unwrap()
        .contains("a * b"));

    let outcomes: Vec<_> = handler.steps.iter().map(|s| s.outcome.clone()).collect();
    assert_eq!(
        outcomes,
        vec![
            AgentStepOutcome::Executed,
            AgentStepOutcome::Executed,
            AgentStepOutcome::Executed,
            AgentStepOutcome::Finished
        ]
    );
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn rejected_tools_go_back_to_the_model_as_errors_and_the_run_continues() {
    let (ctx, temp) = test_ctx("reject");
    fixture_crate(&ctx.workspace);
    let catalog = Catalog::load(&ctx.caps_dir).unwrap();

    let script = vec![
        ScriptedTurn {
            calls: vec![
                // Fuera del toolset (no existe para el modelo).
                call("fs.delete", json!({"path": "src/lib.rs"})),
                // Argumento inválido: falta `path`.
                call("fs.read", json!({})),
                // Escape del workspace.
                call("fs.read", json!({"path": "/etc/passwd"})),
            ],
            ..Default::default()
        },
        ScriptedTurn {
            calls: vec![call(FINISH_TOOL, json!({"resumen": "nada que hacer"}))],
            ..Default::default()
        },
    ];
    let mut provider = FakeProvider::new(script);
    let mut handler = RecordingHandler {
        approve: true,
        ..Default::default()
    };
    let cfg = in_process("intenta cosas prohibidas");
    let report = run(&ctx, &catalog, &mut provider, &cfg, &mut handler).unwrap();

    assert_eq!(report.stop_reason, AgentStopReason::Finished);
    assert_eq!(provider.received.len(), 3);
    assert!(provider.received.iter().all(|r| r.is_error));
    assert!(provider.received[0].content.contains("no disponible"));
    assert!(provider.received[1].content.contains("catálogo"));
    // El escape lo corta ya el catálogo (`within = $WORKSPACE`), antes que
    // el radio de impacto; cualquiera de los dos es un rechazo válido.
    assert!(
        provider.received[2].content.contains("catálogo")
            || provider.received[2].content.contains("fuera del espacio")
    );
    assert!(ctx.workspace.join("src/lib.rs").exists());
    assert!(handler
        .steps
        .iter()
        .take(3)
        .all(|s| s.outcome == AgentStepOutcome::Rejected));
    // Nada ejecutado → nada que registrar ni fotografiar.
    assert!(
        !ctx.journal_path().exists() || journal::read_all(&ctx.journal_path()).unwrap().is_empty()
    );
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn a_declined_confirmation_aborts_the_run_without_touching_disk() {
    let (ctx, temp) = test_ctx("declined");
    fixture_crate(&ctx.workspace);
    let catalog = Catalog::load(&ctx.caps_dir).unwrap();
    let script = vec![
        ScriptedTurn {
            calls: vec![call(
                "fs.patch",
                json!({"path": "src/lib.rs", "old": "a * b", "new": "a + b"}),
            )],
            ..Default::default()
        },
        ScriptedTurn {
            calls: vec![call("fs.read", json!({"path": "src/lib.rs"}))],
            ..Default::default()
        },
    ];
    let mut provider = FakeProvider::new(script);
    let mut handler = RecordingHandler {
        approve: false,
        ..Default::default()
    };
    let cfg = in_process("cambia sum");
    let report = run(&ctx, &catalog, &mut provider, &cfg, &mut handler).unwrap();

    assert_eq!(report.stop_reason, AgentStopReason::Declined);
    assert_eq!(report.steps, 1);
    assert!(report.snapshot_id.is_none());
    assert!(report.files_written.is_empty());
    assert!(std::fs::read_to_string(ctx.workspace.join("src/lib.rs"))
        .unwrap()
        .contains("a * b"));
    // El segundo turno del guion nunca se pidió.
    assert!(provider.received.is_empty());
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn budget_exhaustion_ends_the_run_cleanly() {
    let (ctx, temp) = test_ctx("budget");
    fixture_crate(&ctx.workspace);
    let catalog = Catalog::load(&ctx.caps_dir).unwrap();
    let script = vec![
        ScriptedTurn {
            calls: vec![call("fs.read", json!({"path": "src/lib.rs"}))],
            ..Default::default()
        },
        ScriptedTurn {
            calls: vec![call("fs.read", json!({"path": "Cargo.toml"}))],
            ..Default::default()
        },
        ScriptedTurn {
            calls: vec![call(FINISH_TOOL, json!({"resumen": "no debería llegar"}))],
            ..Default::default()
        },
    ];
    let mut provider = FakeProvider::new(script);
    let mut handler = RecordingHandler {
        approve: true,
        ..Default::default()
    };
    let mut cfg = in_process("lee todo");
    cfg.budget.max_steps = 1;
    let report = run(&ctx, &catalog, &mut provider, &cfg, &mut handler).unwrap();
    assert_eq!(report.stop_reason, AgentStopReason::BudgetExhausted);
    assert_eq!(report.steps, 1);
    assert!(report.summary.contains("1 pasos"));
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn dry_run_shows_but_does_not_write() {
    let (ctx, temp) = test_ctx("dry");
    fixture_crate(&ctx.workspace);
    let catalog = Catalog::load(&ctx.caps_dir).unwrap();
    let script = vec![
        ScriptedTurn {
            calls: vec![call(
                "fs.patch",
                json!({"path": "src/lib.rs", "old": "a * b", "new": "a + b"}),
            )],
            ..Default::default()
        },
        ScriptedTurn {
            calls: vec![call(FINISH_TOOL, json!({"resumen": "hecho"}))],
            ..Default::default()
        },
    ];
    let mut provider = FakeProvider::new(script);
    let mut handler = RecordingHandler {
        approve: true,
        ..Default::default()
    };
    let mut cfg = in_process("cambia sum");
    cfg.dry_run = true;
    let report = run(&ctx, &catalog, &mut provider, &cfg, &mut handler).unwrap();
    assert_eq!(report.stop_reason, AgentStopReason::Finished);
    assert!(provider.received[0].content.contains("dry-run"));
    assert!(std::fs::read_to_string(ctx.workspace.join("src/lib.rs"))
        .unwrap()
        .contains("a * b"));
    assert!(
        !ctx.journal_path().exists() || journal::read_all(&ctx.journal_path()).unwrap().is_empty()
    );
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn model_that_stops_talking_ends_the_run_as_model_stopped() {
    let (ctx, temp) = test_ctx("stopped");
    let catalog = Catalog::load(&ctx.caps_dir).unwrap();
    let mut provider = FakeProvider::new(vec![ScriptedTurn {
        text: "No sé qué hacer.".into(),
        ..Default::default()
    }]);
    let mut handler = RecordingHandler::default();
    let cfg = in_process("algo");
    let report = run(&ctx, &catalog, &mut provider, &cfg, &mut handler).unwrap();
    assert_eq!(report.stop_reason, AgentStopReason::ModelStopped);
    assert_eq!(report.summary, "No sé qué hacer.");
    assert_eq!(handler.notes, vec!["No sé qué hacer.".to_string()]);
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn unknown_toolset_entries_are_configuration_errors() {
    let (ctx, temp) = test_ctx("toolset");
    let catalog = Catalog::load(&ctx.caps_dir).unwrap();
    let mut provider = FakeProvider::new(vec![]);
    let mut handler = RecordingHandler::default();
    let mut cfg = in_process("x");
    cfg.toolset = vec!["shell.run".into()];
    let err = run(&ctx, &catalog, &mut provider, &cfg, &mut handler).unwrap_err();
    assert!(err.to_string().contains("capacidad desconocida"));
    let _ = std::fs::remove_dir_all(temp);
}

/// T33.4: «detener» desde la interfaz corta el run antes de la siguiente
/// herramienta; lo ya ejecutado queda en el journal con su instantánea.
#[test]
fn a_stop_request_ends_the_run_before_the_next_tool() {
    struct StopAfter {
        inner: RecordingHandler,
        after: usize,
    }
    impl AgentHandler for StopAfter {
        fn on_step(&mut self, e: &AgentStepEvent) -> Result<()> {
            self.inner.on_step(e)
        }
        fn on_confirm(&mut self, p: &Proposal) -> Result<bool> {
            self.inner.on_confirm(p)
        }
        fn on_note(&mut self, t: &str) -> Result<()> {
            self.inner.on_note(t)
        }
        fn on_done(&mut self, r: &AgentReport) -> Result<()> {
            self.inner.on_done(r)
        }
        fn should_stop(&mut self) -> bool {
            self.inner.steps.len() >= self.after
        }
    }
    let (ctx, temp) = test_ctx("stop");
    fixture_crate(&ctx.workspace);
    let catalog = Catalog::load(&ctx.caps_dir).unwrap();
    let script = vec![
        ScriptedTurn {
            calls: vec![call("fs.read", json!({"path": "src/lib.rs"}))],
            ..Default::default()
        },
        ScriptedTurn {
            calls: vec![call(
                "fs.patch",
                json!({"path": "src/lib.rs", "old": "a * b", "new": "a + b"}),
            )],
            ..Default::default()
        },
        ScriptedTurn {
            calls: vec![call(FINISH_TOOL, json!({"resumen": "no debería llegar"}))],
            ..Default::default()
        },
    ];
    let mut provider = FakeProvider::new(script);
    let mut handler = StopAfter {
        inner: RecordingHandler {
            approve: true,
            ..Default::default()
        },
        after: 1,
    };
    let cfg = in_process("cambia sum");
    let report = run(&ctx, &catalog, &mut provider, &cfg, &mut handler).unwrap();
    assert_eq!(report.stop_reason, AgentStopReason::Stopped);
    assert_eq!(report.steps, 1);
    // El parche nunca se ejecutó.
    assert!(std::fs::read_to_string(ctx.workspace.join("src/lib.rs"))
        .unwrap()
        .contains("a * b"));
    let _ = std::fs::remove_dir_all(temp);
}
