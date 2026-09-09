//! Declarative Environment and Toolchain Profile Manager (Ticket T7.1).
//!
//! Provisions Nix flakes and Devbox profiles for project workspaces.
//! Detects project tech stack (Rust, Node/TS, Python, Go) and produces reproducible,
//! isolated environment configurations (.antos/env.toml, devbox.json, flake.nix).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Supported declarative development environment profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvProfile {
    Rust,
    Node,
    Python,
    Go,
    Base,
}

impl EnvProfile {
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().trim() {
            "rust" | "rs" | "cargo" => Some(Self::Rust),
            "node" | "nodejs" | "ts" | "typescript" | "js" | "javascript" => Some(Self::Node),
            "python" | "py" | "django" | "fastapi" => Some(Self::Python),
            "go" | "golang" => Some(Self::Go),
            "base" | "general" | "minimal" => Some(Self::Base),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Node => "node",
            Self::Python => "python",
            Self::Go => "go",
            Self::Base => "base",
        }
    }

    pub fn default_packages(&self) -> Vec<&'static str> {
        match self {
            Self::Rust => vec!["rustc", "cargo", "clippy", "rustfmt", "rust-analyzer"],
            Self::Node => vec!["nodejs", "pnpm", "typescript", "prettier"],
            Self::Python => vec!["python3", "uv", "ruff", "pyright"],
            Self::Go => vec!["go", "gopls", "golangci-lint"],
            Self::Base => vec!["git", "ripgrep", "jq", "curl", "tree"],
        }
    }

    pub fn nixpkgs_names(&self) -> Vec<&'static str> {
        match self {
            Self::Rust => vec!["rustc", "cargo", "clippy", "rustfmt", "rust-analyzer"],
            Self::Node => vec!["nodejs_20", "nodePackages.pnpm", "typescript"],
            Self::Python => vec!["python311", "uv", "ruff"],
            Self::Go => vec!["go", "gopls", "golangci-lint"],
            Self::Base => vec!["git", "ripgrep", "jq", "curl"],
        }
    }
}

/// Status of an individual toolchain binary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolchainStatus {
    pub name: String,
    pub available: bool,
    pub path: Option<String>,
}

/// Environment summary created during initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitSummary {
    pub profile: String,
    pub created_files: Vec<String>,
    pub packages: Vec<String>,
}

/// Native antOS project environment declaration (.antos/env.toml).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEnvConfig {
    pub profile: String,
    pub packages: Vec<String>,
    #[serde(default)]
    pub env_vars: BTreeMap<String, String>,
}

/// Core engine for managing environment profiles and flakes.
pub struct EnvEngine;

impl EnvEngine {
    /// Detects project tech stack based on existing files in workspace.
    pub fn detect_stack(workspace: &Path) -> Option<EnvProfile> {
        if workspace.join("Cargo.toml").exists() {
            Some(EnvProfile::Rust)
        } else if workspace.join("package.json").exists() {
            Some(EnvProfile::Node)
        } else if workspace.join("pyproject.toml").exists() || workspace.join("requirements.txt").exists() {
            Some(EnvProfile::Python)
        } else if workspace.join("go.mod").exists() {
            Some(EnvProfile::Go)
        } else {
            None
        }
    }

    /// Initializes a declarative dev profile creating .antos/env.toml, devbox.json and flake.nix.
    pub fn init_profile(
        workspace: &Path,
        profile: EnvProfile,
        create_devbox: bool,
        create_flake: bool,
    ) -> Result<InitSummary> {
        let mut created = Vec::new();
        let packages: Vec<String> = profile.default_packages().into_iter().map(String::from).collect();

        // 1. Create .antos/env.toml
        let antos_dir = workspace.join(".antos");
        std::fs::create_dir_all(&antos_dir)
            .with_context(|| format!("failed to create {}", antos_dir.display()))?;

        let config = ProjectEnvConfig {
            profile: profile.as_str().to_string(),
            packages: packages.clone(),
            env_vars: BTreeMap::new(),
        };
        let env_toml_path = antos_dir.join("env.toml");
        let toml_str = toml::to_string_pretty(&config)
            .context("failed to serialize env config")?;
        std::fs::write(&env_toml_path, toml_str)
            .with_context(|| format!("failed to write {}", env_toml_path.display()))?;
        created.push(".antos/env.toml".to_string());

        // 2. Create devbox.json if requested
        if create_devbox {
            let devbox_path = workspace.join("devbox.json");
            let devbox_json = serde_json::json!({
                "$schema": "https://raw.githubusercontent.com/jetpack-io/devbox/0.12.0/.schema/devbox.schema.json",
                "packages": packages,
                "shell": {
                    "init_hook": [
                        format!("echo 'antOS · Perfil [{}] cargado correctamente.'", profile.as_str())
                    ]
                }
            });
            let formatted = serde_json::to_string_pretty(&devbox_json)
                .context("failed to serialize devbox.json")?;
            std::fs::write(&devbox_path, formatted)
                .with_context(|| format!("failed to write {}", devbox_path.display()))?;
            created.push("devbox.json".to_string());
        }

        // 3. Create flake.nix if requested
        if create_flake {
            let flake_path = workspace.join("flake.nix");
            let flake_content = generate_flake_nix(profile);
            std::fs::write(&flake_path, flake_content)
                .with_context(|| format!("failed to write {}", flake_path.display()))?;
            created.push("flake.nix".to_string());
        }

        Ok(InitSummary {
            profile: profile.as_str().to_string(),
            created_files: created,
            packages,
        })
    }

    /// Loads active project env config if present.
    pub fn load_config(workspace: &Path) -> Result<Option<ProjectEnvConfig>> {
        let env_toml = workspace.join(".antos/env.toml");
        if env_toml.exists() {
            let content = std::fs::read_to_string(&env_toml)?;
            let cfg: ProjectEnvConfig = toml::from_str(&content)?;
            return Ok(Some(cfg));
        }

        let devbox_json = workspace.join("devbox.json");
        if devbox_json.exists() {
            let content = std::fs::read_to_string(&devbox_json)?;
            let v: serde_json::Value = serde_json::from_str(&content)?;
            let pkgs = v["packages"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|s| s.as_str().map(String::from)).collect())
                .unwrap_or_default();
            return Ok(Some(ProjectEnvConfig {
                profile: "devbox".to_string(),
                packages: pkgs,
                env_vars: BTreeMap::new(),
            }));
        }

        Ok(None)
    }

    /// Checks availability of declared tools on the system PATH.
    pub fn check_toolchains(workspace: &Path) -> Result<Vec<ToolchainStatus>> {
        let config = Self::load_config(workspace)?
            .unwrap_or_else(|| {
                let detected = Self::detect_stack(workspace).unwrap_or(EnvProfile::Base);
                ProjectEnvConfig {
                    profile: detected.as_str().to_string(),
                    packages: detected.default_packages().into_iter().map(String::from).collect(),
                    env_vars: BTreeMap::new(),
                }
            });

        let mut results = Vec::new();
        for pkg in config.packages {
            let binary = pkg.split('@').next().unwrap_or(&pkg).trim();

            // T31.4: no shell involved at all — `binary` comes straight from
            // the project's own environment profile (`.antos/env.toml` or
            // `devbox.json`), which travels with a cloned repository. A
            // package declared as `foo; curl … | sh` must, at worst, be
            // reported as "not found"; it must never reach an interpreter.
            if !is_valid_binary_name(binary) {
                results.push(ToolchainStatus {
                    name: binary.to_string(),
                    available: false,
                    path: None,
                });
                continue;
            }

            match find_in_path(binary) {
                Some(found) => results.push(ToolchainStatus {
                    name: binary.to_string(),
                    available: true,
                    path: Some(found.display().to_string()),
                }),
                None => results.push(ToolchainStatus {
                    name: binary.to_string(),
                    available: false,
                    path: None,
                }),
            }
        }

        Ok(results)
    }
}

/// Characters allowed in a toolchain/package binary name (T31.4): plain
/// identifiers only. This is defense in depth on top of `find_in_path`
/// already never invoking a shell — it just means a malformed name is
/// rejected up front, with a clear reason, instead of silently failing to
/// resolve to any file on `PATH`.
fn is_valid_binary_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+'))
}

/// Searches `PATH` directly for an executable named `binary`, with no shell
/// involved (T31.4) — replaces the previous `sh -c "which <binary>"`, which
/// interpolated a value straight from the project's environment profile
/// into a shell command line.
fn find_in_path(binary: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(binary))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && (m.permissions().mode() & 0o111 != 0))
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Generates a valid Nix flake content for the given profile.
fn generate_flake_nix(profile: EnvProfile) -> String {
    let pkgs = profile.nixpkgs_names().join(" ");
    format!(
        r#"{{
  description = "antOS declarative development environment profile for {profile_name}";

  inputs = {{
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  }};

  outputs = {{ self, nixpkgs, flake-utils }}:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {{ inherit system; }};
      in
      {{
        devShells.default = pkgs.mkShell {{
          buildInputs = with pkgs; [
            {pkgs}
          ];

          shellHook = ''
            echo "antOS · Nix Flake [{profile_name}] activo."
          '';
        }};
      }}
    );
}}
"#,
        profile_name = profile.as_str(),
        pkgs = pkgs
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_parsing_and_defaults() {
        assert_eq!(EnvProfile::from_str_loose("rust"), Some(EnvProfile::Rust));
        assert_eq!(EnvProfile::from_str_loose("typescript"), Some(EnvProfile::Node));
        assert_eq!(EnvProfile::from_str_loose("python"), Some(EnvProfile::Python));
        assert_eq!(EnvProfile::from_str_loose("go"), Some(EnvProfile::Go));

        let rust_pkgs = EnvProfile::Rust.default_packages();
        assert!(rust_pkgs.contains(&"cargo"));
        assert!(rust_pkgs.contains(&"rustc"));
    }

    #[test]
    fn test_init_profile_in_tempdir() {
        let temp_dir = std::env::temp_dir().join(format!("antos-env-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).expect("create tempdir");

        let summary = EnvEngine::init_profile(&temp_dir, EnvProfile::Rust, true, true)
            .expect("init profile");

        assert_eq!(summary.profile, "rust");
        assert!(temp_dir.join(".antos/env.toml").exists());
        assert!(temp_dir.join("devbox.json").exists());
        assert!(temp_dir.join("flake.nix").exists());

        let cfg = EnvEngine::load_config(&temp_dir).expect("load config").expect("exists");
        assert_eq!(cfg.profile, "rust");
        assert!(cfg.packages.contains(&"cargo".to_string()));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // ---------------------------------------------------------------- T31.4

    #[test]
    fn test_check_toolchains_rejects_shell_metacharacters_in_package_name() {
        let temp_dir = std::env::temp_dir().join(format!("antos-env-injection-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join(".antos")).expect("create tempdir");

        let probe = std::env::temp_dir().join(format!("antos-injection-probe-{}", std::process::id()));
        let _ = std::fs::remove_file(&probe);

        let config = ProjectEnvConfig {
            profile: "base".to_string(),
            packages: vec![format!("foo; touch {}", probe.display())],
            env_vars: BTreeMap::new(),
        };
        let toml_str = toml::to_string_pretty(&config).expect("serialize config");
        std::fs::write(temp_dir.join(".antos/env.toml"), toml_str).expect("write env.toml");

        let results = EnvEngine::check_toolchains(&temp_dir).expect("check toolchains must not error");
        assert_eq!(results.len(), 1);
        assert!(!results[0].available, "a malicious package name must resolve to \"not found\", not execute");
        assert!(!probe.exists(), "a malicious package name must never reach a shell");

        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::remove_file(&probe);
    }

    #[test]
    fn test_is_valid_binary_name() {
        assert!(is_valid_binary_name("cargo"));
        assert!(is_valid_binary_name("rust-analyzer"));
        assert!(is_valid_binary_name("node_modules.bin"));
        assert!(is_valid_binary_name("g++"));

        assert!(!is_valid_binary_name("foo; touch /tmp/antos-injection-probe"));
        assert!(!is_valid_binary_name("foo | sh"));
        assert!(!is_valid_binary_name("$(whoami)"));
        assert!(!is_valid_binary_name("foo`whoami`"));
        assert!(!is_valid_binary_name("foo && rm -rf /"));
        assert!(!is_valid_binary_name(""));
    }

    #[test]
    fn test_find_in_path_locates_a_real_binary_without_a_shell() {
        // `sh` is guaranteed present on both supported hosts (macOS, Linux)
        // — exactly the kind of binary this must find without invoking one.
        assert!(find_in_path("sh").is_some());
        assert!(find_in_path("antos-definitely-not-a-real-binary-t31-4").is_none());
    }

    #[test]
    fn test_detect_stack() {
        let temp_dir = std::env::temp_dir().join(format!("antos-stack-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).expect("create tempdir");

        std::fs::write(temp_dir.join("Cargo.toml"), "[package]").expect("write cargo");
        assert_eq!(EnvEngine::detect_stack(&temp_dir), Some(EnvProfile::Rust));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
