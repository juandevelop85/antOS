//! antOS Autonomous Sentinel & Continuous Autopilot Daemon (T16.3).
//!
//! Vigila el espacio de trabajo en busca de ficheros fuente con errores de
//! sintaxis y propone una corrección que el desarrollador aprueba o rechaza
//! desde la bandeja de notificaciones (T8.2).
//!
//! ## Estado de implementación (T33.1)
//!
//! Real:
//! - `start`/`stop`/`status` y la persistencia de la configuración e
//!   incidentes en `.antos/`;
//! - `scan_workspace`: recorre los fuentes (`rs`, `json`, `toml`, `ts`,
//!   `js`, `py`) y los valida con el VFS Guard (`vfs_guard::validate_content`);
//! - una notificación por incidente con acciones Aprobar/Rechazar, y
//!   `resolve_incident` que escribe la corrección aprobada en el fichero.
//!
//! Desde T33.3 no hay corrección inventada: al detectar un incidente,
//! Autopilot lo notifica; **aprobarlo lanza un run de Coder real**
//! (`agent::run` con el modelo del rol `coder`, objetivo = el error,
//! presupuesto corto, sobre el workspace; la aprobación del incidente vale
//! como aprobación de los pasos `confirm` del run, nunca de los `grant`).
//! Sin proveedor configurado, aprobar no hace nada más que decirlo
//! (estado `manual`).
//!
//! Lo que sigue sin hacer: detectar builds rotos o tests en rojo (solo
//! sintaxis vía VFS Guard) y aislar la corrección en un worktree
//! (`worktree_branch` es un nombre, no una rama creada).

use crate::util::unix_now;
use antos_protocol::{
    AutopilotConfig, AutopilotIncident, AutopilotStatus, NotificationAction, NotificationItem,
    NotificationKind,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredAutopilotState {
    pub active: bool,
    pub poll_interval_secs: u64,
    pub last_scan_timestamp: Option<String>,
}

pub struct AutopilotEngine;

impl AutopilotEngine {
    /// Returns the path to the autopilot state directory inside `$ANTOS_STATE/autopilot/`.
    pub fn autopilot_dir(state_dir: &Path) -> PathBuf {
        state_dir.join("autopilot")
    }

    /// Path to config file.
    pub fn config_path(state_dir: &Path) -> PathBuf {
        Self::autopilot_dir(state_dir).join("config.json")
    }

    /// Path to state file.
    pub fn state_path(state_dir: &Path) -> PathBuf {
        Self::autopilot_dir(state_dir).join("status.json")
    }

    /// Path to incidents database.
    pub fn incidents_path(state_dir: &Path) -> PathBuf {
        Self::autopilot_dir(state_dir).join("incidents.json")
    }

    /// Starts the Autopilot daemon monitoring.
    pub fn start(
        state_dir: &Path,
        workspace_dir: &Path,
        config: AutopilotConfig,
    ) -> Result<AutopilotStatus> {
        let dir = Self::autopilot_dir(state_dir);
        fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create autopilot dir {}", dir.display()))?;

        fs::write(
            Self::config_path(state_dir),
            serde_json::to_string_pretty(&config)?,
        )?;

        let now = chrono::Local::now().to_rfc3339();
        let st = StoredAutopilotState {
            active: true,
            poll_interval_secs: config.poll_interval_secs,
            last_scan_timestamp: Some(now),
        };
        fs::write(
            Self::state_path(state_dir),
            serde_json::to_string_pretty(&st)?,
        )?;

        // Run initial scan
        let _ = Self::scan_workspace(state_dir, workspace_dir);

        Self::status(state_dir, workspace_dir)
    }

    /// Stops the Autopilot daemon monitoring.
    pub fn stop(state_dir: &Path, workspace_dir: &Path) -> Result<AutopilotStatus> {
        let dir = Self::autopilot_dir(state_dir);
        fs::create_dir_all(&dir)?;

        let mut st = Self::load_stored_state(state_dir);
        st.active = false;
        fs::write(
            Self::state_path(state_dir),
            serde_json::to_string_pretty(&st)?,
        )?;

        Self::status(state_dir, workspace_dir)
    }

    /// Queries real-time status and metrics of the Autopilot daemon.
    pub fn status(state_dir: &Path, workspace_dir: &Path) -> Result<AutopilotStatus> {
        let st = Self::load_stored_state(state_dir);
        let incidents = Self::list_incidents(state_dir)?;

        let active_count = incidents
            .iter()
            .filter(|i| {
                !matches!(
                    i.status.as_str(),
                    "resolved" | "dismissed" | "manual" | "failed"
                )
            })
            .count();
        let resolved_count = incidents.iter().filter(|i| i.status == "resolved").count();

        Ok(AutopilotStatus {
            active: st.active,
            workspace_path: workspace_dir.display().to_string(),
            poll_interval_secs: st.poll_interval_secs,
            active_incidents_count: active_count,
            resolved_incidents_count: resolved_count,
            last_scan_timestamp: st.last_scan_timestamp,
        })
    }

    /// Lists all tracked incidents.
    pub fn list_incidents(state_dir: &Path) -> Result<Vec<AutopilotIncident>> {
        let path = Self::incidents_path(state_dir);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&path)?;
        if content.trim().is_empty() {
            return Ok(Vec::new());
        }
        let list: Vec<AutopilotIncident> = serde_json::from_str(&content).unwrap_or_default();
        Ok(list)
    }

    /// Scans workspace files for errors, broken tests, or syntax flaws.
    /// When an issue is discovered, triggers the antFlow agent roles and creates a fix proposal.
    pub fn scan_workspace(
        state_dir: &Path,
        workspace_dir: &Path,
    ) -> Result<Vec<AutopilotIncident>> {
        let mut new_incidents = Vec::new();
        let guard = crate::vfs_guard::VfsGuardEngine::global();
        let mut existing = Self::list_incidents(state_dir)?;

        let files = Self::collect_source_files(workspace_dir)?;
        for file in files {
            let rel_path = file
                .strip_prefix(workspace_dir)
                .unwrap_or(&file)
                .display()
                .to_string();

            // Read file content and validate with VFS Guard
            if let Ok(content) = fs::read_to_string(&file) {
                let validation = guard.validate_content(&rel_path, &content);
                if !validation.is_valid {
                    // Check if already tracking this file
                    let already_tracked = existing.iter().any(|i| {
                        i.file_path == rel_path && i.status != "resolved" && i.status != "dismissed"
                    });
                    if already_tracked {
                        continue;
                    }

                    let err_msg = validation
                        .errors
                        .first()
                        .map(|e| e.message.clone())
                        .unwrap_or_else(|| "Syntax error".to_string());
                    let incident_id =
                        format!("inc-{}", chrono::Local::now().format("%Y%m%d%H%M%S%3f"));
                    let branch = format!("autopilot/{incident_id}");

                    // T33.3: sin corrección inventada. Aprobar lanza un run
                    // de Coder real si hay proveedor; si no, solo se notifica.
                    let coder_available = Self::coder_provider(state_dir).is_ok();
                    let incident = AutopilotIncident {
                        id: incident_id.clone(),
                        timestamp: chrono::Local::now().to_rfc3339(),
                        incident_type: "SyntaxError".to_string(),
                        severity: "high".to_string(),
                        file_path: rel_path.clone(),
                        error_message: err_msg.clone(),
                        status: "ready_for_approval".to_string(),
                        worktree_branch: Some(branch),
                        fix_proposal: None,
                    };

                    // Send notification to notification engine
                    let notif_item = NotificationItem {
                        id: format!("notif-{incident_id}"),
                        ticket_id: incident_id.clone(),
                        title: format!("antOS Autopilot · Incidente en {rel_path}"),
                        body: if coder_available {
                            format!("Detectado error sintáctico: {err_msg}. Aprobar lanza un agente Coder con este error como objetivo (T33.3).")
                        } else {
                            format!("Detectado error sintáctico: {err_msg}. Sin proveedor de modelo configurado (antos llm use …): solo notificación.")
                        },
                        kind: NotificationKind::System,
                        created_at: unix_now()?,
                        read: false,
                        actions: vec![
                            NotificationAction::Approve,
                            NotificationAction::Reject,
                            NotificationAction::ViewDiff,
                        ],
                    };
                    let _ = crate::notification::NotificationEngine::global()
                        .notify(workspace_dir, notif_item);

                    existing.push(incident.clone());
                    new_incidents.push(incident);
                }
            }
        }

        // Save updated incidents list
        let dir = Self::autopilot_dir(state_dir);
        fs::create_dir_all(&dir)?;
        fs::write(
            Self::incidents_path(state_dir),
            serde_json::to_string_pretty(&existing)?,
        )?;

        // Update last scan timestamp
        let mut st = Self::load_stored_state(state_dir);
        st.last_scan_timestamp = Some(chrono::Local::now().to_rfc3339());
        if let Ok(json) = serde_json::to_string_pretty(&st) {
            let _ = fs::write(Self::state_path(state_dir), json);
        }

        Ok(new_incidents)
    }

    /// Resolves an incident: either approves & applies the fix or dismisses it.
    pub fn resolve_incident(
        state_dir: &Path,
        workspace_dir: &Path,
        incident_id: &str,
        approve_and_merge: bool,
    ) -> Result<AutopilotIncident> {
        let mut incidents = Self::list_incidents(state_dir)?;
        let mut target = None;

        for inc in &mut incidents {
            if inc.id == incident_id {
                if approve_and_merge {
                    // T33.3: la corrección la hace un Coder real, o nadie.
                    match Self::run_coder_fix(state_dir, workspace_dir, inc) {
                        Ok(Some(report)) => {
                            inc.status = if report.stop_reason
                                == antos_protocol::AgentStopReason::Finished
                            {
                                "resolved".to_string()
                            } else {
                                "failed".to_string()
                            };
                            inc.error_message = format!(
                                "{} · Coder [{}:{}] {:?}: {}",
                                inc.error_message,
                                report.provider,
                                report.model,
                                report.stop_reason,
                                report.summary
                            );
                        }
                        Ok(None) => {
                            inc.status = "manual".to_string();
                            inc.error_message = format!(
                                "{} · sin proveedor de modelo configurado: corrección manual",
                                inc.error_message
                            );
                        }
                        Err(e) => {
                            inc.status = "failed".to_string();
                            inc.error_message =
                                format!("{} · el run del Coder falló: {e:#}", inc.error_message);
                        }
                    }
                } else {
                    inc.status = "dismissed".to_string();
                }
                target = Some(inc.clone());
                break;
            }
        }

        let incident =
            target.ok_or_else(|| anyhow::anyhow!("Incident «{incident_id}» not found"))?;

        // Persist updated list
        fs::write(
            Self::incidents_path(state_dir),
            serde_json::to_string_pretty(&incidents)?,
        )?;

        Ok(incident)
    }

    // ------------------------------------------------------------- private helpers

    fn load_stored_state(state_dir: &Path) -> StoredAutopilotState {
        let path = Self::state_path(state_dir);
        if path.exists() {
            if let Ok(c) = fs::read_to_string(&path) {
                if let Ok(st) = serde_json::from_str::<StoredAutopilotState>(&c) {
                    return st;
                }
            }
        }
        StoredAutopilotState {
            active: false,
            poll_interval_secs: 5,
            last_scan_timestamp: None,
        }
    }

    fn collect_source_files(dir: &Path) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        if !dir.exists() {
            return Ok(files);
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }

            if path.is_dir() {
                let mut sub = Self::collect_source_files(&path)?;
                files.append(&mut sub);
            } else if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ["rs", "json", "toml", "ts", "js", "py"].contains(&ext) {
                    files.push(path);
                }
            }
        }
        Ok(files)
    }

    /// El proveedor del rol `coder` según `llm_config.json`; error si no
    /// hay ninguno configurado o resoluble (sin clave, etc.).
    fn coder_provider(state_dir: &Path) -> Result<Box<dyn crate::agent::providers::AgentProvider>> {
        let config = crate::llm::LlmConfig::load_from_state(state_dir);
        if config.active_provider == "auto" || config.active_provider == "local" {
            anyhow::bail!("sin proveedor activo (antos llm use <proveedor>)");
        }
        let spec = config.get_role_model("coder");
        crate::agent::providers::resolve(state_dir, Some(&spec))
    }

    /// Run de Coder sobre el workspace con el error como objetivo (T33.3).
    /// `Ok(None)`: no hay proveedor, no se hizo nada.
    fn run_coder_fix(
        state_dir: &Path,
        workspace_dir: &Path,
        inc: &AutopilotIncident,
    ) -> Result<Option<antos_protocol::AgentReport>> {
        let Ok(mut provider) = Self::coder_provider(state_dir) else {
            return Ok(None);
        };
        let discovered = crate::ctx::Ctx::discover()?;
        let ctx = crate::ctx::Ctx {
            workspace: workspace_dir.to_path_buf(),
            state: state_dir.to_path_buf(),
            current_project: None,
            ..discovered
        };
        let catalog = crate::capability::Catalog::load(&ctx.caps_dir)?;
        let coder = crate::agent::roles::spec_for(antos_protocol::AgentRole::Coder, 10);
        let mut cfg = crate::agent::RunConfig::new(format!(
            "El fichero {} tiene un error de sintaxis: {}. Corrígelo con el mínimo cambio y, si el proyecto tiene suite, verifica con test.run.",
            inc.file_path, inc.error_message
        ));
        cfg.toolset = coder.toolset;
        cfg.toolset_compact = Some(coder.toolset_compact);
        cfg.system_prompt = Some(coder.system_prompt);
        cfg.budget = coder.budget;
        cfg.finish = Some(coder.finish);
        // La aprobación del incidente es la aprobación del run: los pasos
        // `confirm` pasan; los `grant` siguen exigiendo su concesión.
        let mut handler = ApprovedIncidentHandler;
        let report = crate::agent::run(&ctx, &catalog, &mut *provider, &cfg, &mut handler)?;
        Ok(Some(report))
    }
}

/// Observador de un run lanzado desde una notificación ya aprobada: no hay
/// terminal ni barra delante, así que los pasos `confirm` se aceptan (el
/// usuario aprobó el incidente) y el resto se registra en el journal.
struct ApprovedIncidentHandler;

impl crate::agent::AgentHandler for ApprovedIncidentHandler {
    fn on_step(&mut self, _event: &antos_protocol::AgentStepEvent) -> Result<()> {
        Ok(())
    }
    fn on_confirm(&mut self, _proposal: &antos_protocol::Proposal) -> Result<bool> {
        Ok(true)
    }
    fn on_note(&mut self, _text: &str) -> Result<()> {
        Ok(())
    }
    fn on_done(&mut self, _report: &antos_protocol::AgentReport) -> Result<()> {
        Ok(())
    }
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_autopilot_lifecycle_start_scan_and_resolve() {
        let temp_dir = std::env::temp_dir().join("antos_test_autopilot");
        let _ = fs::remove_dir_all(&temp_dir);
        let state_dir = temp_dir.join(".antos");
        let ws_dir = temp_dir.join("workspace");
        fs::create_dir_all(&state_dir).unwrap();
        fs::create_dir_all(&ws_dir).unwrap();

        // 1. Initial status
        let initial = AutopilotEngine::status(&state_dir, &ws_dir).unwrap();
        assert!(!initial.active);
        assert_eq!(initial.active_incidents_count, 0);

        // 2. Create a broken source file with unclosed brace in workspace
        let broken_file = ws_dir.join("broken.rs");
        fs::write(&broken_file, "fn broken() {\n    let x = 42;\n").unwrap();

        // 3. Start autopilot
        let config = AutopilotConfig {
            enabled: true,
            poll_interval_secs: 5,
            watch_paths: vec!["workspace".into()],
            auto_merge: false,
            target_branch: "master".into(),
        };
        let status = AutopilotEngine::start(&state_dir, &ws_dir, config).unwrap();
        assert!(status.active);
        assert_eq!(status.active_incidents_count, 1);

        // 4. Inspect incident details
        let incidents = AutopilotEngine::list_incidents(&state_dir).unwrap();
        assert_eq!(incidents.len(), 1);
        let inc = &incidents[0];
        assert_eq!(inc.incident_type, "SyntaxError");
        assert_eq!(inc.file_path, "broken.rs");
        // T33.3: ya no hay una corrección inventada adjunta al incidente.
        assert!(inc.fix_proposal.is_none());

        // 5. Approve without a configured provider → nothing is fabricated:
        //    the incident goes to `manual` and the file is untouched.
        let resolved =
            AutopilotEngine::resolve_incident(&state_dir, &ws_dir, &inc.id, true).unwrap();
        assert_eq!(resolved.status, "manual");
        assert!(resolved.error_message.contains("sin proveedor"));
        let content = fs::read_to_string(&broken_file).unwrap();
        assert_eq!(content, "fn broken() {\n    let x = 42;\n");

        // 6. Stop autopilot: `manual` no cuenta como resuelto ni como activo.
        let stopped = AutopilotEngine::stop(&state_dir, &ws_dir).unwrap();
        assert!(!stopped.active);
        assert_eq!(stopped.resolved_incidents_count, 0);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
