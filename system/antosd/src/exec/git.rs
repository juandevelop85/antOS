//! Git and worktree execution capabilities and helpers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use anyhow::{bail, Context, Result};
use crate::ctx::Ctx;
use super::Change;

pub fn changes_for(
    cap: &str,
    a: &BTreeMap<String, String>,
    ctx: &Ctx,
) -> Result<Option<Vec<Change>>> {
    match cap {
        "git.status" => {
            let root = a.get("path").map(|p| abs(ctx, p)).unwrap_or_else(|| ctx.workspace.clone());
            Ok(Some(vec![Change::GitStatus { repo_root: root }]))
        }

        "git.commit_semantic" => {
            let root = a.get("path").map(|p| abs(ctx, p)).unwrap_or_else(|| ctx.workspace.clone());
            let tipo = a.get("type").cloned().unwrap_or_else(|| "feat".into());
            let msg = a.get("message").cloned().unwrap_or_default();
            let commit_msg = if let Some(scope) = a.get("scope").filter(|s| !s.is_empty()) {
                format!("{tipo}({scope}): {msg}")
            } else {
                format!("{tipo}: {msg}")
            };
            Ok(Some(vec![Change::GitCommit { repo_root: root, commit_msg }]))
        }

        "git.smart_branch" => {
            let root = a.get("path").map(|p| abs(ctx, p)).unwrap_or_else(|| ctx.workspace.clone());
            let name = a.get("name").cloned().unwrap_or_default();
            let base = a.get("base").cloned();
            let branch_name = if let Some(ticket) = a.get("ticket_id").filter(|t| !t.is_empty()) {
                format!("{}/{}", ticket.to_lowercase(), name)
            } else {
                name
            };
            Ok(Some(vec![Change::GitBranch { repo_root: root, branch_name, base }]))
        }

        "git.worktree_create" => {
            let root = a.get("path").map(|p| abs(ctx, p)).unwrap_or_else(|| ctx.workspace.clone());
            let ticket = a.get("ticket_id").cloned().unwrap_or_else(|| "task".into());
            let branch = a.get("branch").cloned().unwrap_or_else(|| format!("agent/{ticket}"));
            let base = a.get("base").cloned().unwrap_or_else(|| "HEAD".into());
            let target_path = ctx.state.join("worktrees").join(&ticket);
            Ok(Some(vec![Change::GitWorktreeCreate {
                repo_root: root,
                target_path,
                branch_name: branch,
                base,
            }]))
        }

        "git.worktree_cleanup" => {
            let root = a.get("path").map(|p| abs(ctx, p)).unwrap_or_else(|| ctx.workspace.clone());
            let ticket = a.get("ticket_id").cloned().unwrap_or_else(|| "task".into());
            let force = a.get("force").map(|f| f == "true").unwrap_or(false);
            let target_path = ctx.state.join("worktrees").join(&ticket);
            Ok(Some(vec![Change::GitWorktreeCleanup {
                repo_root: root,
                target_path,
                force,
            }]))
        }

        "git.worktree_merge" => {
            let root = a.get("path").map(|p| abs(ctx, p)).unwrap_or_else(|| ctx.workspace.clone());
            let ticket = a.get("ticket_id").cloned().unwrap_or_else(|| "task".into());
            let branch = format!("agent/{ticket}");
            let target = a.get("target").cloned().unwrap_or_else(|| "main".into());
            let message = a.get("message").cloned();
            Ok(Some(vec![Change::GitWorktreeMerge {
                repo_root: root,
                branch_name: branch,
                target_branch: target,
                message,
            }]))
        }

        "project.git_init" => {
            let project_name = a.get("project").cloned().unwrap_or_default();
            let branch = a.get("branch").cloned().unwrap_or_else(|| "main".into());
            let language_hint = a.get("language").filter(|l| *l != "auto").cloned();
            let project_dir = {
                let candidate = std::path::Path::new(&project_name);
                if candidate.is_absolute() {
                    candidate.to_path_buf()
                } else {
                    ctx.workspace.join(&project_name)
                }
            };
            Ok(Some(vec![Change::ProjectGitInit { project_dir, branch, language_hint }]))
        }

        _ => Ok(None),
    }
}

pub fn apply(change: &Change) -> Result<Option<String>> {
    match change {
        Change::GitStatus { repo_root } => {
            if let Some(status) = crate::git::GitAnalyzer::global().consultar_estado(repo_root)? {
                let lineas = vec![
                    format!("rama: {}", status.branch.unwrap_or_else(|| "HEAD desacoplado".into())),
                    format!("commits: +{} / -{}", status.ahead, status.behind),
                    format!("modificados: {}", status.modified.len()),
                    format!("staged: {}", status.staged.len()),
                    format!("sin seguimiento: {}", status.untracked.len()),
                ];
                Ok(Some(lineas.join("\n")))
            } else {
                Ok(Some("no es un repositorio Git".into()))
            }
        }
        Change::GitCommit { repo_root, commit_msg } => {
            let add_out = std::process::Command::new("git")
                .arg("-C")
                .arg(repo_root)
                .args(["add", "-A"])
                .output()
                .context("ejecutando git add")?;
            if !add_out.status.success() {
                bail!("git add falló: {}", String::from_utf8_lossy(&add_out.stderr));
            }

            let commit_out = std::process::Command::new("git")
                .arg("-C")
                .arg(repo_root)
                .args(["commit", "-m", commit_msg])
                .output()
                .context("ejecutando git commit")?;
            if !commit_out.status.success() {
                let err = String::from_utf8_lossy(&commit_out.stderr);
                let out_str = String::from_utf8_lossy(&commit_out.stdout);
                if out_str.contains("nothing to commit") || err.contains("nothing to commit") {
                    Ok(Some("nada que commitear (árbol limpio)".into()))
                } else {
                    bail!("git commit falló: {err}\n{out_str}");
                }
            } else {
                let resultado = String::from_utf8_lossy(&commit_out.stdout).trim().to_string();
                Ok(Some(format!("commit creado: {resultado}")))
            }
        }
        Change::GitBranch { repo_root, branch_name, base } => {
            let mut cmd = std::process::Command::new("git");
            cmd.arg("-C").arg(repo_root);
            if let Some(b) = base {
                cmd.args(["checkout", "-B", branch_name, b]);
            } else {
                cmd.args(["checkout", "-B", branch_name]);
            }
            let out = cmd.output().context("ejecutando git checkout")?;
            if !out.status.success() {
                bail!("git checkout falló: {}", String::from_utf8_lossy(&out.stderr));
            }
            Ok(Some(format!("rama activa: {branch_name}")))
        }
        Change::GitWorktreeCreate { repo_root, target_path, branch_name, base } => {
            crate::git::create_worktree(repo_root, target_path, branch_name, base)?;
            Ok(Some(format!(
                "worktree creado en: {} (rama: {})",
                target_path.display(),
                branch_name
            )))
        }
        Change::GitWorktreeCleanup { repo_root, target_path, force } => {
            crate::git::remove_worktree(repo_root, target_path, *force)?;
            Ok(Some(format!("worktree eliminado: {}", target_path.display())))
        }
        Change::GitWorktreeMerge { repo_root, branch_name, target_branch, message } => {
            let res = crate::git::merge_worktree(
                repo_root,
                branch_name,
                target_branch,
                message.as_deref(),
            )?;
            Ok(Some(format!("merge completado: {res}")))
        }
        Change::ProjectGitInit { project_dir, branch, language_hint } => {
            let msg = init_project_git_repo(project_dir, branch, language_hint.as_deref())?;
            Ok(Some(msg))
        }
        _ => Ok(None),
    }
}

fn abs(ctx: &Ctx, raw: &str) -> PathBuf {
    let expanded = crate::blast::expand(raw, &BTreeMap::new(), &ctx.workspace);
    let p = PathBuf::from(expanded);
    if p.is_absolute() { p } else { ctx.workspace.join(p) }
}

/// Returns the language-specific .gitignore template for a project.
pub fn gitignore_template(language: &str) -> &'static str {
    match language {
        "rust" => "\
# Generated by antOS (T17.3) for Rust
/target/
**/*.rs.bk
Cargo.lock
*.pdb
",
        "typescript" | "javascript" | "node" => "\
# Generated by antOS (T17.3) for Node / TypeScript
node_modules/
dist/
build/
.env
*.log
.npm
",
        "python" => "\
# Generated by antOS (T17.3) for Python
__pycache__/
*.py[cod]
*$py.class
.venv/
venv/
ENV/
env/
dist/
build/
*.egg-info/
.pytest_cache/
",
        "go" => "\
# Generated by antOS (T17.3) for Go
/bin/
/dist/
*.exe
*.test
vendor/
",
        _ => "\
# Generated by antOS (T17.3)
.DS_Store
Thumbs.db
*.log
.env
",
    }
}

/// Initializes an isolated Git repository in `project_dir` with the given branch (default `main`)
/// and generates a tech-stack adapted `.gitignore` if not already present.
///
/// Uses `GIT_CEILING_DIRECTORIES` so that Git cannot ascend into the antOS OS repository.
pub fn init_project_git_repo(
    project_dir: &Path,
    branch: &str,
    language_hint: Option<&str>,
) -> Result<String> {
    std::fs::create_dir_all(project_dir)
        .with_context(|| format!("creando directorio de proyecto {}", project_dir.display()))?;

    // 1. Generate .gitignore if missing
    let gitignore_path = project_dir.join(".gitignore");
    let mut gitignore_created = false;
    if !gitignore_path.exists() {
        let lang = language_hint
            .map(|l| l.to_lowercase())
            .unwrap_or_else(|| super::fs::detect_project_language(project_dir));
        let content = gitignore_template(&lang);
        std::fs::write(&gitignore_path, content)
            .with_context(|| format!("escribiendo {}", gitignore_path.display()))?;
        gitignore_created = true;
    }

    // 2. Initialize Git if .git is missing
    let git_dir = project_dir.join(".git");
    let antos_root = crate::git::detect_antos_root();
    let already_git = git_dir.exists();

    if !already_git {
        // Run git init -b <branch>
        let mut init_cmd = crate::git::git_cmd_with_ceiling(antos_root.as_deref());
        init_cmd.current_dir(project_dir).args(["init", "-b", branch]);
        let mut init_res = init_cmd.output();

        // Fallback for older git versions where -b might not be supported
        if let Ok(ref out) = init_res {
            if !out.status.success() {
                let mut fallback_cmd = crate::git::git_cmd_with_ceiling(antos_root.as_deref());
                fallback_cmd.current_dir(project_dir).arg("init");
                init_res = fallback_cmd.output();

                if let Ok(ref out2) = init_res {
                    if out2.status.success() {
                        let mut checkout_cmd = crate::git::git_cmd_with_ceiling(antos_root.as_deref());
                        checkout_cmd.current_dir(project_dir).args(["checkout", "-b", branch]);
                        let _ = checkout_cmd.output();
                    }
                }
            }
        }

        let out = init_res.context("ejecutando git init")?;
        if !out.status.success() {
            bail!("git init falló: {}", String::from_utf8_lossy(&out.stderr));
        }

        // Configure default user.name and user.email if not set locally
        let mut cfg_name = crate::git::git_cmd_with_ceiling(antos_root.as_deref());
        cfg_name.current_dir(project_dir).args(["config", "--local", "user.name"]);
        if let Ok(out) = cfg_name.output() {
            if out.stdout.is_empty() {
                let mut set_name = crate::git::git_cmd_with_ceiling(antos_root.as_deref());
                set_name.current_dir(project_dir).args(["config", "--local", "user.name", "antOS Developer"]);
                let _ = set_name.output();
            }
        }
        let mut cfg_email = crate::git::git_cmd_with_ceiling(antos_root.as_deref());
        cfg_email.current_dir(project_dir).args(["config", "--local", "user.email"]);
        if let Ok(out) = cfg_email.output() {
            if out.stdout.is_empty() {
                let mut set_email = crate::git::git_cmd_with_ceiling(antos_root.as_deref());
                set_email.current_dir(project_dir).args(["config", "--local", "user.email", "developer@antos.local"]);
                let _ = set_email.output();
            }
        }
    }

    let proj_name = project_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| project_dir.display().to_string());

    let status_str = if already_git {
        format!("proyecto «{proj_name}»: repositorio Git ya existía en {}", project_dir.display())
    } else {
        format!("proyecto «{proj_name}»: repositorio Git inicializado en {} (rama: {branch})", project_dir.display())
    };

    let gi_str = if gitignore_created {
        " con plantilla .gitignore generada"
    } else {
        ""
    };

    Ok(format!("{status_str}{gi_str}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gitignore_template_patterns() {
        assert!(gitignore_template("rust").contains("/target/"));
        assert!(gitignore_template("rust").contains("Cargo.lock"));

        assert!(gitignore_template("typescript").contains("node_modules/"));
        assert!(gitignore_template("node").contains("dist/"));

        assert!(gitignore_template("python").contains("__pycache__/"));
        assert!(gitignore_template("python").contains(".venv/"));

        assert!(gitignore_template("go").contains("/bin/"));
    }

    #[test]
    fn test_init_project_git_repo_creates_repo_and_gitignore() {
        let tmp = std::env::temp_dir().join(format!("antos_git_init_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("Cargo.toml"), "[package]\nname=\"sample\"").unwrap();

        let msg = init_project_git_repo(&tmp, "main", None).expect("init_project_git_repo");
        assert!(msg.contains("inicializado"));
        assert!(tmp.join(".git").exists(), ".git directory must exist");
        assert!(tmp.join(".gitignore").exists(), ".gitignore file must exist");

        let gi_content = std::fs::read_to_string(tmp.join(".gitignore")).unwrap();
        assert!(gi_content.contains("/target/"), ".gitignore must contain rust patterns");

        let out = std::process::Command::new("git")
            .current_dir(&tmp)
            .args(["branch", "--show-current"])
            .output();
        if let Ok(o) = out {
            let branch = String::from_utf8_lossy(&o.stdout).trim().to_string();
            assert_eq!(branch, "main", "initial branch must be main");
        }

        let msg2 = init_project_git_repo(&tmp, "main", None).expect("idempotent init");
        assert!(msg2.contains("ya existía"));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
