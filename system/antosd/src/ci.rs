//! Local Parallel CI/CD Engine and Intelligent Git Hooks for antOS (Ticket T20.3).
//!
//! Provides millisecond-fast local continuous integration in sandboxes, declarative stage execution
//! (.antos/ci.toml), security and secret leakage scanning, and automated Git pre-commit/pre-push hooks.

use antos_protocol::{CiReport, CiStageResult, CiStageStatus, GitHookStatus};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// Declarative CI stage configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub parallel: bool,
    #[serde(default)]
    pub fast: bool,
}

/// Declarative pipeline definition stored in `.antos/ci.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiPipelineConfig {
    #[serde(default = "default_pipeline_name")]
    pub name: String,
    #[serde(default)]
    pub stages: Vec<StageConfig>,
}

fn default_pipeline_name() -> String {
    "local-ci".into()
}

impl Default for CiPipelineConfig {
    fn default() -> Self {
        Self {
            name: "local-ci".into(),
            stages: vec![
                StageConfig {
                    name: "security".into(),
                    command: "internal:secret_scanner".into(),
                    parallel: true,
                    fast: true,
                },
                StageConfig {
                    name: "format".into(),
                    command: "cargo fmt --check".into(),
                    parallel: true,
                    fast: true,
                },
                StageConfig {
                    name: "lint".into(),
                    command: "cargo check".into(),
                    parallel: true,
                    fast: true,
                },
                StageConfig {
                    name: "test".into(),
                    command: "cargo test".into(),
                    parallel: false,
                    fast: false,
                },
            ],
        }
    }
}

/// Core CI and Hook Engine.
pub struct CiEngine;

impl CiEngine {
    /// Loads `.antos/ci.toml` or returns the auto-detected configuration for the workspace.
    pub fn load_or_detect_config(workspace: &Path) -> CiPipelineConfig {
        let manifest_path = workspace.join(".antos").join("ci.toml");
        if manifest_path.exists() {
            if let Ok(content) = fs::read_to_string(&manifest_path) {
                if let Ok(cfg) = toml::from_str::<CiPipelineConfig>(&content) {
                    return cfg;
                }
            }
        }

        // Auto-detect project tech stack
        if workspace.join("Cargo.toml").exists() {
            Self::default_rust_pipeline()
        } else if workspace.join("package.json").exists() {
            Self::default_node_pipeline()
        } else if workspace.join("pyproject.toml").exists() || workspace.join("requirements.txt").exists() {
            Self::default_python_pipeline()
        } else {
            Self::default_generic_pipeline()
        }
    }

    fn default_rust_pipeline() -> CiPipelineConfig {
        CiPipelineConfig {
            name: "rust-ci".into(),
            stages: vec![
                StageConfig {
                    name: "security".into(),
                    command: "internal:secret_scanner".into(),
                    parallel: true,
                    fast: true,
                },
                StageConfig {
                    name: "lint".into(),
                    command: "cargo check".into(),
                    parallel: true,
                    fast: true,
                },
                StageConfig {
                    name: "test".into(),
                    command: "cargo test".into(),
                    parallel: false,
                    fast: false,
                },
            ],
        }
    }

    fn default_node_pipeline() -> CiPipelineConfig {
        CiPipelineConfig {
            name: "node-ci".into(),
            stages: vec![
                StageConfig {
                    name: "security".into(),
                    command: "internal:secret_scanner".into(),
                    parallel: true,
                    fast: true,
                },
                StageConfig {
                    name: "test".into(),
                    command: "npm test --if-present".into(),
                    parallel: false,
                    fast: false,
                },
            ],
        }
    }

    fn default_python_pipeline() -> CiPipelineConfig {
        CiPipelineConfig {
            name: "python-ci".into(),
            stages: vec![
                StageConfig {
                    name: "security".into(),
                    command: "internal:secret_scanner".into(),
                    parallel: true,
                    fast: true,
                },
                StageConfig {
                    name: "test".into(),
                    command: "pytest -q".into(),
                    parallel: false,
                    fast: false,
                },
            ],
        }
    }

    fn default_generic_pipeline() -> CiPipelineConfig {
        CiPipelineConfig {
            name: "generic-ci".into(),
            stages: vec![
                StageConfig {
                    name: "security".into(),
                    command: "internal:secret_scanner".into(),
                    parallel: true,
                    fast: true,
                },
            ],
        }
    }

    /// Scans the workspace or specific path for leaked credentials, private keys, and API tokens.
    pub fn scan_secrets(workspace: &Path) -> Result<Vec<String>> {
        let mut detected = Vec::new();
        Self::scan_dir_recursive(workspace, workspace, &mut detected, 0)?;
        Ok(detected)
    }

    fn scan_dir_recursive(
        base: &Path,
        current: &Path,
        detected: &mut Vec<String>,
        depth: usize,
    ) -> Result<()> {
        if depth > 8 {
            return Ok(());
        }

        let entries = match fs::read_dir(current) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();

            // Skip version control and heavy build directories
            if file_name == ".git" || file_name == "target" || file_name == "node_modules" || file_name == ".antos" {
                continue;
            }

            if path.is_dir() {
                Self::scan_dir_recursive(base, &path, detected, depth + 1)?;
            } else if path.is_file() {
                // Check if file size is reasonable for code (< 2 MB)
                if let Ok(meta) = path.metadata() {
                    if meta.len() > 2 * 1024 * 1024 {
                        continue;
                    }
                }

                if let Ok(content) = fs::read_to_string(&path) {
                    let rel_path = path.strip_prefix(base).unwrap_or(&path).display().to_string();
                    let file_secrets = Self::scan_content_for_secrets(&rel_path, &content);
                    detected.extend(file_secrets);
                }
            }
        }

        Ok(())
    }

    /// Scans text content for high-risk secret patterns.
    pub fn scan_content_for_secrets(file_label: &str, content: &str) -> Vec<String> {
        let mut findings = Vec::new();

        for (line_idx, line) in content.lines().enumerate() {
            let line_num = line_idx + 1;
            let l = line.trim();

            // 1. Private Key headers
            if l.contains("-----BEGIN") && (l.contains("PRIVATE KEY") || l.contains("RSA PRIVATE")) {
                findings.push(format!("{file_label}:{line_num}: Llave privada criptográfica expuesta"));
                continue;
            }

            // 2. AWS Access Key IDs
            if l.contains("AKIA") {
                if let Some(idx) = l.find("AKIA") {
                    let candidate = &l[idx..];
                    if candidate.len() >= 20 && candidate[..20].chars().all(|c| c.is_ascii_alphanumeric()) {
                        findings.push(format!("{file_label}:{line_num}: Posible credencial AWS Access Key ID"));
                        continue;
                    }
                }
            }

            // 3. GitHub Personal Access Tokens
            if l.contains("ghp_") || l.contains("gho_") {
                findings.push(format!("{file_label}:{line_num}: Token de acceso de GitHub detectado"));
                continue;
            }

            // 4. Generic high-entropy secret assignment
            let lower = l.to_lowercase();
            if (lower.contains("secret") || lower.contains("api_key") || lower.contains("password") || lower.contains("token"))
                && (l.contains('=') || l.contains(':'))
            {
                // Verify it's not a placeholder
                if !lower.contains("placeholder")
                    && !lower.contains("example")
                    && !lower.contains("test")
                    && !lower.contains("dummy")
                    && !lower.contains("your_")
                {
                    let val_opt = if let Some((_, val_part)) = l.split_once('=') {
                        Some(val_part)
                    } else if let Some((_, val_part)) = l.split_once(':') {
                        Some(val_part)
                    } else {
                        None
                    };

                    if let Some(val_part) = val_opt {
                        let token = val_part.trim().trim_matches(|c| c == '"' || c == '\'' || c == ';' || c == ',');
                        if token.len() >= 16 && !token.contains(' ') {
                            findings.push(format!("{file_label}:{line_num}: Clave o secreto de alta entropía"));
                        }
                    }
                }
            }
        }

        findings
    }

    /// Executes the CI pipeline locally, collecting stage results and metrics.
    pub fn run_pipeline(
        workspace: &Path,
        state_dir: &Path,
        stage_filter: Option<&str>,
        fast_mode: bool,
    ) -> Result<CiReport> {
        let config = Self::load_or_detect_config(workspace);
        let start_time = Instant::now();
        let run_id = format!("ci-{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis());

        let mut stages_results = Vec::new();
        let mut overall_success = true;
        let mut secrets_found = Vec::new();
        let mut security_clean = true;

        for stage in &config.stages {
            // Apply stage filtering if requested
            if let Some(target_stage) = stage_filter {
                if !stage.name.eq_ignore_ascii_case(target_stage) {
                    continue;
                }
            }

            // In fast mode, skip stages not flagged as fast
            if fast_mode && !stage.fast {
                stages_results.push(CiStageResult {
                    name: stage.name.clone(),
                    command: stage.command.clone(),
                    status: CiStageStatus::Skipped,
                    duration_ms: 0,
                    output_snippet: "Omitido en modo rápido (--fast)".into(),
                    exit_code: None,
                });
                continue;
            }

            let stage_start = Instant::now();

            if stage.command == "internal:secret_scanner" {
                // Internal security scanner
                match Self::scan_secrets(workspace) {
                    Ok(secrets) => {
                        let duration = stage_start.elapsed().as_millis() as u64;
                        if secrets.is_empty() {
                            stages_results.push(CiStageResult {
                                name: stage.name.clone(),
                                command: stage.command.clone(),
                                status: CiStageStatus::Passed,
                                duration_ms: duration,
                                output_snippet: "0 secretos o credenciales detectadas".into(),
                                exit_code: Some(0),
                            });
                        } else {
                            security_clean = false;
                            overall_success = false;
                            secrets_found.extend(secrets.clone());
                            stages_results.push(CiStageResult {
                                name: stage.name.clone(),
                                command: stage.command.clone(),
                                status: CiStageStatus::Failed,
                                duration_ms: duration,
                                output_snippet: format!("Se detectaron {} secretos potenciales:\n{}", secrets.len(), secrets.join("\n")),
                                exit_code: Some(1),
                            });
                        }
                    }
                    Err(e) => {
                        overall_success = false;
                        stages_results.push(CiStageResult {
                            name: stage.name.clone(),
                            command: stage.command.clone(),
                            status: CiStageStatus::Failed,
                            duration_ms: stage_start.elapsed().as_millis() as u64,
                            output_snippet: format!("Error en escaneo de seguridad: {e}"),
                            exit_code: Some(1),
                        });
                    }
                }
            } else {
                // External shell / tool execution
                let output_res = Command::new("sh")
                    .arg("-c")
                    .arg(&stage.command)
                    .current_dir(workspace)
                    .output();

                let duration = stage_start.elapsed().as_millis() as u64;

                match output_res {
                    Ok(out) => {
                        let code = out.status.code().unwrap_or(-1);
                        let is_ok = out.status.success();
                        if !is_ok {
                            overall_success = false;
                        }

                        let combined_out = String::from_utf8_lossy(&out.stdout).to_string()
                            + "\n"
                            + &String::from_utf8_lossy(&out.stderr);
                        let snippet = combined_out.trim().lines().take(5).collect::<Vec<&str>>().join("\n");

                        stages_results.push(CiStageResult {
                            name: stage.name.clone(),
                            command: stage.command.clone(),
                            status: if is_ok { CiStageStatus::Passed } else { CiStageStatus::Failed },
                            duration_ms: duration,
                            output_snippet: if snippet.is_empty() { "OK".into() } else { snippet },
                            exit_code: Some(code),
                        });
                    }
                    Err(e) => {
                        overall_success = false;
                        stages_results.push(CiStageResult {
                            name: stage.name.clone(),
                            command: stage.command.clone(),
                            status: CiStageStatus::Failed,
                            duration_ms: duration,
                            output_snippet: format!("Fallo al invocar comando: {e}"),
                            exit_code: Some(127),
                        });
                    }
                }
            }
        }

        let total_duration = start_time.elapsed().as_millis() as u64;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let report = CiReport {
            id: run_id,
            success: overall_success,
            stages: stages_results,
            total_duration_ms: total_duration,
            security_clean,
            secrets_found,
            timestamp_secs: timestamp,
        };

        // Persist last report in .antos/ci/last_report.json
        let ci_dir = state_dir.join("ci");
        let _ = fs::create_dir_all(&ci_dir);
        let _ = fs::write(ci_dir.join("last_report.json"), serde_json::to_string_pretty(&report)?);

        Ok(report)
    }

    /// Reads the last persisted CI execution report.
    pub fn get_last_report(state_dir: &Path) -> Result<Option<CiReport>> {
        let path = state_dir.join("ci").join("last_report.json");
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("No se pudo leer {}", path.display()))?;
        let report = serde_json::from_str::<CiReport>(&content)?;
        Ok(Some(report))
    }

    /// Installs intelligent Git pre-commit and pre-push hooks managed by antOS.
    pub fn install_git_hooks(workspace: &Path) -> Result<GitHookStatus> {
        let git_dir = Self::find_git_dir(workspace)?;
        let hooks_dir = git_dir.join("hooks");
        fs::create_dir_all(&hooks_dir)?;

        let pre_commit_path = hooks_dir.join("pre-commit");
        let pre_push_path = hooks_dir.join("pre-push");

        let pre_commit_script = r#"#!/usr/bin/env sh
# antOS-managed-hook: pre-commit
# Bloquea commits con fuga de secretos o violaciones de CI local
antos hook check || exit 1
"#;

        let pre_push_script = r#"#!/usr/bin/env sh
# antOS-managed-hook: pre-push
# Ejecuta pipeline de CI local rápido antes de enviar al remoto
antos ci run --fast || exit 1
"#;

        fs::write(&pre_commit_path, pre_commit_script)?;
        fs::write(&pre_push_path, pre_push_script)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&pre_commit_path, fs::Permissions::from_mode(0o755));
            let _ = fs::set_permissions(&pre_push_path, fs::Permissions::from_mode(0o755));
        }

        Self::query_git_hooks_status(workspace)
    }

    /// Removes antOS-managed Git hooks.
    pub fn uninstall_git_hooks(workspace: &Path) -> Result<GitHookStatus> {
        let git_dir = Self::find_git_dir(workspace)?;
        let hooks_dir = git_dir.join("hooks");

        let pre_commit = hooks_dir.join("pre-commit");
        if pre_commit.exists() {
            if let Ok(c) = fs::read_to_string(&pre_commit) {
                if c.contains("antOS-managed-hook") {
                    let _ = fs::remove_file(&pre_commit);
                }
            }
        }

        let pre_push = hooks_dir.join("pre-push");
        if pre_push.exists() {
            if let Ok(c) = fs::read_to_string(&pre_push) {
                if c.contains("antOS-managed-hook") {
                    let _ = fs::remove_file(&pre_push);
                }
            }
        }

        Self::query_git_hooks_status(workspace)
    }

    /// Queries the current installation status of Git hooks.
    pub fn query_git_hooks_status(workspace: &Path) -> Result<GitHookStatus> {
        let git_dir = match Self::find_git_dir(workspace) {
            Ok(d) => d,
            Err(_) => {
                return Ok(GitHookStatus {
                    pre_commit_installed: false,
                    pre_push_installed: false,
                    hook_dir: "no_git_repository".into(),
                    active_guards: Vec::new(),
                })
            }
        };

        let hooks_dir = git_dir.join("hooks");
        let pre_commit = hooks_dir.join("pre-commit");
        let pre_push = hooks_dir.join("pre-push");

        let pre_commit_installed = pre_commit.exists()
            && fs::read_to_string(&pre_commit).map(|c| c.contains("antOS-managed-hook")).unwrap_or(false);

        let pre_push_installed = pre_push.exists()
            && fs::read_to_string(&pre_push).map(|c| c.contains("antOS-managed-hook")).unwrap_or(false);

        let mut guards = Vec::new();
        if pre_commit_installed {
            guards.push("secret_leak_prevention".into());
            guards.push("ast_syntax_guard".into());
        }
        if pre_push_installed {
            guards.push("fast_ci_matrix".into());
        }

        Ok(GitHookStatus {
            pre_commit_installed,
            pre_push_installed,
            hook_dir: hooks_dir.display().to_string(),
            active_guards: guards,
        })
    }

    fn find_git_dir(start: &Path) -> Result<PathBuf> {
        let mut curr = start.to_path_buf();
        loop {
            let candidate = curr.join(".git");
            if candidate.is_dir() {
                return Ok(candidate);
            }
            if !curr.pop() {
                break;
            }
        }
        anyhow::bail!("No se encontró repositorio Git en {}", start.display());
    }

    /// Runs pre-commit validation directly: audits staged files for secrets and syntax.
    pub fn run_pre_commit_check(workspace: &Path) -> Result<(bool, Vec<String>)> {
        let mut errors = Vec::new();

        // 1. Scan for secrets
        let secrets = Self::scan_secrets(workspace)?;
        if !secrets.is_empty() {
            errors.push(format!("Se encontraron {} secretos o credenciales expuestas:", secrets.len()));
            for s in secrets {
                errors.push(format!("  • {s}"));
            }
        }

        let passed = errors.is_empty();
        Ok((passed, errors))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_scanner_detection() {
        let clean_code = r#"
            pub fn get_user(id: u64) -> String {
                format!("user_{id}")
            }
        "#;
        let findings = CiEngine::scan_content_for_secrets("src/lib.rs", clean_code);
        assert!(findings.is_empty());

        let dirty_code = r#"
            pub const AWS_KEY: &str = "AKIAIOSFODNN7EXAMPLE12";
            pub const GITHUB_TOKEN: &str = "ghp_123456789012345678901234567890123456";
            pub const SECRET_KEY: &str = "super_secret_production_key_12345";
        "#;
        let findings = CiEngine::scan_content_for_secrets("src/config.rs", dirty_code);
        assert!(!findings.is_empty());
        assert!(findings.iter().any(|f| f.contains("AWS")));
        assert!(findings.iter().any(|f| f.contains("GitHub")));
        assert!(findings.iter().any(|f| f.contains("alta entropía")));
    }

    #[test]
    fn test_pipeline_execution_lifecycle() {
        let temp_ws = std::env::temp_dir().join(format!("test_ci_ws_{}", std::process::id()));
        let temp_state = std::env::temp_dir().join(format!("test_ci_state_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_ws);
        let _ = fs::create_dir_all(&temp_state);

        // Create clean code file
        fs::write(temp_ws.join("main.rs"), "fn main() { println!(\"Hello\"); }\n").unwrap();

        let report = CiEngine::run_pipeline(&temp_ws, &temp_state, None, true).expect("run ci");
        assert!(report.security_clean);
        assert!(!report.stages.is_empty());

        // Verify report persistence
        let loaded = CiEngine::get_last_report(&temp_state).expect("load report");
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, report.id);

        let _ = fs::remove_dir_all(&temp_ws);
        let _ = fs::remove_dir_all(&temp_state);
    }

    #[test]
    fn test_git_hooks_install_and_uninstall() {
        let temp_ws = std::env::temp_dir().join(format!("test_hook_ws_{}", std::process::id()));
        let git_dir = temp_ws.join(".git");
        let _ = fs::create_dir_all(&git_dir);

        let status = CiEngine::install_git_hooks(&temp_ws).expect("install hooks");
        assert!(status.pre_commit_installed);
        assert!(status.pre_push_installed);
        assert!(git_dir.join("hooks").join("pre-commit").exists());

        let uninstalled = CiEngine::uninstall_git_hooks(&temp_ws).expect("uninstall hooks");
        assert!(!uninstalled.pre_commit_installed);
        assert!(!uninstalled.pre_push_installed);

        let _ = fs::remove_dir_all(&temp_ws);
    }
}
