//! Evaluación reproducible de agentes (T33.5).
//!
//! Casos declarativos en `evals/agent/*.toml` (ver su README): un fixture,
//! un objetivo, opcionalmente un guion del proveedor `fake` y lo esperado.
//! Dos modos:
//! - **determinista** (`fake`): verifica el contrato del runtime — orden de
//!   herramientas, rechazos, motivo de parada, tests en verde, ficheros
//!   permitidos. Corre en `cargo test` y en CI;
//! - **`--live`**: los mismos casos contra el proveedor configurado, con
//!   métricas (pasos, tokens, segundos) guardadas en `.antos/evals/` y
//!   `diff` contra la ejecución anterior para detectar regresiones. Nunca
//!   en CI: cuesta dinero.
//!
//! Sin evaluación, un cambio de prompt, de modelo o de toolset se degrada
//! sin que nadie lo note.

use super::fake::FakeProvider;
use super::providers::AgentProvider;
use super::{AgentHandler, Executor, RunConfig};
use crate::capability::Catalog;
use crate::ctx::Ctx;
use antos_protocol::{AgentReport, AgentStepEvent, AgentStepOutcome, AgentStopReason, Proposal};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Un caso tal como está en el `.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct EvalCase {
    pub name: String,
    pub fixture: String,
    pub goal: String,
    #[serde(default = "default_budget")]
    pub budget_steps: u32,
    /// Binarios que el caso necesita en el PATH; si falta alguno se omite.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Guion JSON del proveedor `fake` (turnos). Obligatorio para el modo
    /// determinista; en `--live` se ignora.
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub toolset: Option<Vec<String>>,
    /// `false`: el caso prueba el contrato del runtime (rechazos,
    /// presupuesto…) y solo tiene sentido con el guion `fake`; en `--live`
    /// se omite.
    #[serde(default = "default_true")]
    pub live: bool,
    pub expect: Expectation,
}

fn default_true() -> bool {
    true
}

fn default_budget() -> u32 {
    10
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Expectation {
    /// Orden exacto de herramientas (solo se exige en modo determinista).
    #[serde(default)]
    pub tools: Vec<String>,
    /// Número de pasos rechazados por el catálogo/toolset.
    #[serde(default)]
    pub rejected: Option<u32>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    /// Si `true`, al terminar se ejecuta la suite del fixture y debe estar
    /// en verde; si `false`, no se ejecuta.
    #[serde(default)]
    pub tests_green: bool,
    /// Rutas relativas que el run puede escribir; cualquier otra es fallo.
    #[serde(default)]
    pub files_allowed: Vec<String>,
}

/// Resultado de un caso: métricas y veredicto.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaseResult {
    pub name: String,
    pub skipped: Option<String>,
    pub passed: bool,
    pub failures: Vec<String>,
    pub tools: Vec<String>,
    pub rejected: u32,
    pub stop_reason: String,
    pub steps: u32,
    pub tokens: u64,
    pub seconds: u64,
    pub tests_green: Option<bool>,
    pub files_written: Vec<String>,
}

/// Una ejecución completa, tal como se guarda en `.antos/evals/`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvalRun {
    pub at: String,
    pub provider: String,
    pub live: bool,
    pub cases: Vec<CaseResult>,
}

impl EvalRun {
    pub fn passed(&self) -> bool {
        self.cases.iter().all(|c| c.passed || c.skipped.is_some())
    }
}

/// Carga todos los casos de un directorio, ordenados por nombre de fichero.
pub fn load_cases(dir: &Path) -> Result<Vec<EvalCase>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("leyendo los casos en {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "toml").unwrap_or(false))
        .collect();
    paths.sort();
    let mut cases = Vec::new();
    for p in paths {
        let raw = std::fs::read_to_string(&p)?;
        let case: EvalCase =
            toml::from_str(&raw).with_context(|| format!("caso inválido: {}", p.display()))?;
        cases.push(case);
    }
    if cases.is_empty() {
        bail!("no hay casos *.toml en {}", dir.display());
    }
    Ok(cases)
}

/// Directorio de casos: `evals/agent` bajo la raíz de antOS.
pub fn default_cases_dir(ctx: &Ctx) -> Result<PathBuf> {
    let root = ctx
        .antos_root
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no encuentro la raíz de antOS (evals/agent)"))?;
    Ok(root.join("evals").join("agent"))
}

struct Recorder {
    tools: Vec<String>,
    rejected: u32,
}

impl AgentHandler for Recorder {
    fn on_step(&mut self, e: &AgentStepEvent) -> Result<()> {
        self.tools.push(e.tool.clone());
        if e.outcome == AgentStepOutcome::Rejected {
            self.rejected += 1;
        }
        Ok(())
    }
    fn on_confirm(&mut self, _p: &Proposal) -> Result<bool> {
        // La evaluación aprueba los pasos `confirm`: lo que se mide es el
        // agente, no al operador. Los `grant` siguen exigiendo concesión.
        Ok(true)
    }
    fn on_note(&mut self, _t: &str) -> Result<()> {
        Ok(())
    }
    fn on_done(&mut self, _r: &AgentReport) -> Result<()> {
        Ok(())
    }
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn missing_requirement(case: &EvalCase) -> Option<String> {
    case.requires.iter().find(|bin| !in_path(bin)).cloned()
}

fn in_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p).any(|d| {
                let f = d.join(bin);
                f.is_file()
            })
        })
        .unwrap_or(false)
}

/// Ejecuta un caso. `provider_for(case)` decide el proveedor (fake con el
/// guion del caso, o el real en `--live`).
pub(crate) fn run_case(
    base_ctx: &Ctx,
    catalog: &Catalog,
    fixtures_dir: &Path,
    case: &EvalCase,
    live: bool,
    provider_for: &mut dyn FnMut(&EvalCase) -> Result<Box<dyn AgentProvider>>,
    executor: Executor,
) -> Result<CaseResult> {
    let mut result = CaseResult {
        name: case.name.clone(),
        skipped: None,
        passed: false,
        failures: Vec::new(),
        tools: Vec::new(),
        rejected: 0,
        stop_reason: String::new(),
        steps: 0,
        tokens: 0,
        seconds: 0,
        tests_green: None,
        files_written: Vec::new(),
    };
    if let Some(bin) = missing_requirement(case) {
        result.skipped = Some(format!("falta `{bin}` en el PATH"));
        return Ok(result);
    }
    if live && !case.live {
        result.skipped = Some("caso de contrato del runtime, solo determinista".into());
        return Ok(result);
    }

    // Workspace temporal propio, con el fixture copiado.
    let temp = std::env::temp_dir().join(format!(
        "antos_eval_{}_{}_{}",
        case.name,
        std::process::id(),
        chrono::Local::now().format("%H%M%S%3f")
    ));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp)?;
    // Ruta canónica: en macOS `temp_dir()` es `/var/folders/…`, un enlace a
    // `/private/var/…`, y el recinto (Seatbelt) compara rutas reales — con
    // la no canónica, cada escritura del agente sería «Operation not
    // permitted» aunque estuviera declarada.
    let temp = temp.canonicalize().unwrap_or(temp);
    let ws = temp.join("workspace");
    copy_tree(&fixtures_dir.join(&case.fixture), &ws)
        .with_context(|| format!("copiando el fixture «{}»", case.fixture))?;
    let state = temp.join("state");
    std::fs::create_dir_all(&state)?;
    let ctx = Ctx {
        workspace: ws.clone(),
        state,
        current_project: None,
        ..base_ctx.clone()
    };

    let mut cfg = RunConfig::new(case.goal.clone());
    cfg.budget.max_steps = case.budget_steps;
    if let Some(t) = &case.toolset {
        cfg.toolset = t.clone();
    }
    cfg.executor = executor;
    let mut provider = provider_for(case)?;
    let mut recorder = Recorder {
        tools: Vec::new(),
        rejected: 0,
    };
    let report = super::run(&ctx, catalog, &mut *provider, &cfg, &mut recorder)?;

    result.tools = recorder.tools;
    result.rejected = recorder.rejected;
    result.stop_reason = stop_reason_name(&report.stop_reason).to_string();
    result.steps = report.steps;
    result.tokens = report.tokens_used;
    result.seconds = report.seconds;
    result.files_written = report.files_written.clone();

    // Veredicto.
    let e = &case.expect;
    if !live && !e.tools.is_empty() && result.tools != e.tools {
        result.failures.push(format!(
            "herramientas: esperado {:?}, obtenido {:?}",
            e.tools, result.tools
        ));
    }
    if let Some(r) = e.rejected {
        if result.rejected != r {
            result.failures.push(format!(
                "rechazos: esperado {r}, obtenido {}",
                result.rejected
            ));
        }
    }
    if let Some(sr) = &e.stop_reason {
        if &result.stop_reason != sr {
            result.failures.push(format!(
                "motivo de parada: esperado {sr}, obtenido {}",
                result.stop_reason
            ));
        }
    }
    for f in &result.files_written {
        if !e.files_allowed.iter().any(|a| a == f) {
            result
                .failures
                .push(format!("escritura fuera de lo permitido: {f}"));
        }
    }
    if e.tests_green {
        let outcome = crate::exec::fs::run_tests(&ws, None);
        let green = outcome
            .as_ref()
            .map(|out| out.starts_with("TESTS EN VERDE"))
            .unwrap_or(false);
        result.tests_green = Some(green);
        if !green {
            let detail = match &outcome {
                Ok(out) => out.lines().rev().take(6).collect::<Vec<_>>().join(" | "),
                Err(err) => format!("{err:#}"),
            };
            result.failures.push(format!(
                "la suite del fixture no está en verde al terminar: {detail}"
            ));
        }
    }
    result.passed = result.failures.is_empty();
    let _ = std::fs::remove_dir_all(&temp);
    Ok(result)
}

fn stop_reason_name(r: &AgentStopReason) -> &'static str {
    match r {
        AgentStopReason::Finished => "finished",
        AgentStopReason::BudgetExhausted => "budget_exhausted",
        AgentStopReason::Declined => "declined",
        AgentStopReason::ModelStopped => "model_stopped",
        AgentStopReason::Stopped => "stopped",
        AgentStopReason::Error => "error",
    }
}

/// Ejecuta todos los casos (o uno) y devuelve la ejecución.
pub(crate) fn run_all(
    base_ctx: &Ctx,
    catalog: &Catalog,
    cases_dir: &Path,
    only: Option<&str>,
    live: bool,
    provider_spec: Option<&str>,
    executor: Executor,
) -> Result<EvalRun> {
    let cases = load_cases(cases_dir)?;
    let fixtures = cases_dir.join("fixtures");
    let provider_name = if live {
        provider_spec.unwrap_or("(activo)").to_string()
    } else {
        "fake".into()
    };
    let state = base_ctx.state.clone();
    let mut provider_for = |case: &EvalCase| -> Result<Box<dyn AgentProvider>> {
        if live {
            super::providers::resolve(&state, provider_spec)
        } else {
            let script = case
                .script
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("el caso «{}» no tiene guion `fake`", case.name))?;
            Ok(Box::new(FakeProvider::from_json(script)?))
        }
    };
    let mut results = Vec::new();
    for case in cases.iter().filter(|c| only.is_none_or(|o| o == c.name)) {
        results.push(run_case(
            base_ctx,
            catalog,
            &fixtures,
            case,
            live,
            &mut provider_for,
            executor,
        )?);
    }
    if results.is_empty() {
        bail!("ningún caso coincide con «{}»", only.unwrap_or(""));
    }
    Ok(EvalRun {
        at: chrono::Local::now().to_rfc3339(),
        provider: provider_name,
        live,
        cases: results,
    })
}

/// Guarda la ejecución en `<state>/evals/<fecha>.json` y devuelve la ruta.
pub fn save(state_dir: &Path, run: &EvalRun) -> Result<PathBuf> {
    let dir = state_dir.join("evals");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!(
        "{}.json",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    ));
    std::fs::write(&path, serde_json::to_vec_pretty(run)?)?;
    Ok(path)
}

/// Las dos ejecuciones guardadas más recientes (anterior, última).
pub fn last_two(state_dir: &Path) -> Result<(EvalRun, EvalRun)> {
    let dir = state_dir.join("evals");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .with_context(|| format!("no hay ejecuciones guardadas en {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
        .collect();
    paths.sort();
    if paths.len() < 2 {
        bail!("hacen falta al menos dos ejecuciones guardadas para comparar");
    }
    let load =
        |p: &Path| -> Result<EvalRun> { Ok(serde_json::from_str(&std::fs::read_to_string(p)?)?) };
    Ok((
        load(&paths[paths.len() - 2])?,
        load(&paths[paths.len() - 1])?,
    ))
}

/// Una regresión detectada entre dos ejecuciones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Regression {
    pub case: String,
    pub what: String,
}

/// Compara dos ejecuciones: tests que pasaban y ya no, casos que
/// terminaban y ya no, y +30 % de pasos o de tokens.
pub fn diff(before: &EvalRun, after: &EvalRun) -> Vec<Regression> {
    let mut out = Vec::new();
    for b in &before.cases {
        let Some(a) = after.cases.iter().find(|c| c.name == b.name) else {
            out.push(Regression {
                case: b.name.clone(),
                what: "el caso ya no se ejecuta".into(),
            });
            continue;
        };
        if a.skipped.is_some() || b.skipped.is_some() {
            continue;
        }
        if b.passed && !a.passed {
            out.push(Regression {
                case: b.name.clone(),
                what: format!("pasaba y ahora falla: {}", a.failures.join("; ")),
            });
        }
        if b.tests_green == Some(true) && a.tests_green == Some(false) {
            out.push(Regression {
                case: b.name.clone(),
                what: "los tests pasaban y ahora están en rojo".into(),
            });
        }
        if b.stop_reason == "finished" && a.stop_reason != "finished" {
            out.push(Regression {
                case: b.name.clone(),
                what: format!("terminaba con `finished` y ahora `{}`", a.stop_reason),
            });
        }
        if grew_30_percent(b.steps as u64, a.steps as u64) {
            out.push(Regression {
                case: b.name.clone(),
                what: format!("pasos: {} → {} (+30 % o más)", b.steps, a.steps),
            });
        }
        if grew_30_percent(b.tokens, a.tokens) {
            out.push(Regression {
                case: b.name.clone(),
                what: format!("tokens: {} → {} (+30 % o más)", b.tokens, a.tokens),
            });
        }
    }
    out
}

fn grew_30_percent(before: u64, after: u64) -> bool {
    before > 0 && after * 10 >= before * 13
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn repo_cases_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../evals/agent")
            .canonicalize()
            .unwrap()
    }

    fn test_ctx(name: &str) -> (Ctx, PathBuf) {
        let discovered = Ctx::discover().unwrap();
        let temp =
            std::env::temp_dir().join(format!("antos_evaltest_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        let ctx = Ctx {
            workspace: temp.join("ws"),
            state: temp.join("state"),
            current_project: None,
            ..discovered
        };
        (ctx, temp)
    }

    /// Criterio de T33.5: los casos del repo pasan con `fake`.
    #[test]
    fn repository_cases_pass_deterministically() {
        let (ctx, temp) = test_ctx("repo");
        let catalog = Catalog::load(&ctx.caps_dir).unwrap();
        let run = run_all(
            &ctx,
            &catalog,
            &repo_cases_dir(),
            None,
            false,
            None,
            Executor::InProcess,
        )
        .unwrap();
        for c in &run.cases {
            assert!(
                c.passed || c.skipped.is_some(),
                "{}: {:?} (herramientas {:?})",
                c.name,
                c.failures,
                c.tools
            );
        }
        assert!(run.passed());
        let rust = run
            .cases
            .iter()
            .find(|c| c.name == "rust-fix-failing-test")
            .unwrap();
        assert_eq!(rust.tests_green, Some(true));
        assert_eq!(rust.files_written, vec!["src/lib.rs".to_string()]);
        let _ = std::fs::remove_dir_all(temp);
    }

    /// Un caso que espera otro orden de herramientas debe fallar: la
    /// evaluación detecta que el contrato cambió.
    #[test]
    fn a_mutated_expectation_fails_the_case() {
        let (ctx, temp) = test_ctx("mutated");
        let catalog = Catalog::load(&ctx.caps_dir).unwrap();
        let mut cases = load_cases(&repo_cases_dir()).unwrap();
        let case = cases
            .iter_mut()
            .find(|c| c.name == "budget-stops-the-run")
            .unwrap();
        case.expect.tools = vec!["fs.read".into()]; // falta el segundo
        case.expect.stop_reason = Some("finished".into()); // y el motivo
        let mut provider_for = |c: &EvalCase| -> Result<Box<dyn AgentProvider>> {
            Ok(Box::new(FakeProvider::from_json(
                c.script.as_deref().unwrap(),
            )?))
        };
        let result = run_case(
            &ctx,
            &catalog,
            &repo_cases_dir().join("fixtures"),
            case,
            false,
            &mut provider_for,
            Executor::InProcess,
        )
        .unwrap();
        assert!(!result.passed);
        assert_eq!(result.failures.len(), 2, "{:?}", result.failures);
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn diff_flags_regressions_and_ignores_noise() {
        let base = CaseResult {
            name: "c".into(),
            skipped: None,
            passed: true,
            failures: vec![],
            tools: vec![],
            rejected: 0,
            stop_reason: "finished".into(),
            steps: 10,
            tokens: 1000,
            seconds: 5,
            tests_green: Some(true),
            files_written: vec![],
        };
        let before = EvalRun {
            at: "a".into(),
            provider: "x".into(),
            live: true,
            cases: vec![base.clone()],
        };
        // Sin cambios relevantes: +20 % de pasos no es regresión.
        let mut same = base.clone();
        same.steps = 12;
        let after_ok = EvalRun {
            cases: vec![same],
            ..before.clone()
        };
        assert!(diff(&before, &after_ok).is_empty());
        // Regresión: tests en rojo, no termina, +50 % tokens.
        let mut worse = base.clone();
        worse.passed = false;
        worse.failures = vec!["rojo".into()];
        worse.tests_green = Some(false);
        worse.stop_reason = "budget_exhausted".into();
        worse.tokens = 1500;
        let after_bad = EvalRun {
            cases: vec![worse],
            ..before.clone()
        };
        let regressions = diff(&before, &after_bad);
        assert_eq!(regressions.len(), 4, "{regressions:?}");
        // Caso desaparecido.
        let gone = EvalRun {
            cases: vec![],
            ..before.clone()
        };
        assert_eq!(diff(&before, &gone).len(), 1);
    }

    #[test]
    fn save_and_last_two_round_trip() {
        let (ctx, temp) = test_ctx("save");
        std::fs::create_dir_all(&ctx.state).unwrap();
        let run = EvalRun {
            at: "t".into(),
            provider: "fake".into(),
            live: false,
            cases: vec![],
        };
        save(&ctx.state, &run).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        save(&ctx.state, &run).unwrap();
        let (a, b) = last_two(&ctx.state).unwrap();
        assert_eq!(a, run);
        assert_eq!(b, run);
        let _ = std::fs::remove_dir_all(temp);
    }
}
