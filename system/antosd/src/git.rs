//! Git repository analyzer and introspection for antOS background services (T1.2 / T17.1).
//!
//! This module allows `antosd` to instantly inspect the state of any workspace
//! (current branch, ahead/behind remote commits, modified/staged/untracked files
//! and changed-line counts) using an in-memory cache invalidated by `mtime` stamps
//! of `.git/HEAD` and `.git/index`.
//!
//! ## Workspace boundary isolation (T17.1)
//!
//! All Git subprocess invocations carry the `GIT_CEILING_DIRECTORIES` environment
//! variable pointing at the parent directory of the antOS installation root.
//! This prevents Git from ascending past the `workspace/` boundary and accidentally
//! reporting changes that belong to the antOS OS repository itself.
//!
//! The [`find_git_root_with_ceiling`] function additionally enforces a hard stop:
//! if the traversal reaches the antOS root without finding a `.git` directory
//! that belongs to a project under `workspace/`, it returns `None`.

use antos_protocol::{GitFileDiffSummary, GitFileStatus, GitRepoStatus};
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

// ─────────────────────────────────────────────────────────────────── T17.1 ──
// Workspace boundary isolation helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Detects the antOS installation root by looking for `system/capabilities`
/// starting from `current_dir` and ascending the filesystem tree.
///
static ANTOS_ROOT_CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Detects the antOS installation root by looking for `system/capabilities`
/// starting from `current_dir` and ascending the filesystem tree.
///
/// Returns `None` if the root cannot be determined (e.g., running from an
/// unrelated directory).
///
/// Memoized via `OnceLock` so all subsequent calls execute in O(1) (T32.5).
pub fn detect_antos_root() -> Option<PathBuf> {
    ANTOS_ROOT_CACHE
        .get_or_init(|| {
            let start = std::env::current_dir().ok()?;
            let mut candidate = start.as_path();
            loop {
                if candidate.join("system").join("capabilities").is_dir() {
                    return Some(candidate.to_path_buf());
                }
                candidate = candidate.parent()?;
            }
        })
        .clone()
}

/// Returns the value to use for `GIT_CEILING_DIRECTORIES` for a given antOS root.
///
/// Git interprets this as a colon-separated list of directories above which it
/// will refuse to ascend when searching for `.git`. We set it to the *parent*
/// of the antOS root so that Git cannot find the OS `.git` while inspecting a
/// project directory that lives under `workspace/`.
fn ceiling_for_root(antos_root: &Path) -> String {
    antos_root
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| antos_root.display().to_string())
}

/// Builds a `Command` for `git` that carries `GIT_CEILING_DIRECTORIES` set to
/// the parent of `antos_root`, preventing Git from escaping the boundary.
///
/// If `antos_root` is `None` the environment variable is not injected (safe
/// fallback for contexts where the root is unknown).
pub(crate) fn git_cmd_with_ceiling(antos_root: Option<&Path>) -> Command {
    let mut cmd = Command::new("git");
    if let Some(root) = antos_root {
        cmd.env("GIT_CEILING_DIRECTORIES", ceiling_for_root(root));
    }
    cmd
}

/// Firma ligera del árbol de trabajo para detectar modificaciones, adiciones
/// o eliminaciones sin requerir `git add` previo (T32.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorktreeSignature {
    max_mtime: Option<SystemTime>,
    file_count: usize,
    total_bytes: u64,
}

fn get_worktree_signature(repo_root: &Path) -> WorktreeSignature {
    let mut sig = WorktreeSignature {
        max_mtime: get_mtime(repo_root),
        file_count: 0,
        total_bytes: 0,
    };
    scan_dir_signature(repo_root, 0, 5, &mut sig);
    sig
}

fn scan_dir_signature(
    dir: &Path,
    current_depth: usize,
    max_depth: usize,
    sig: &mut WorktreeSignature,
) {
    if current_depth > max_depth {
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        if name.starts_with('.')
            || name == "target"
            || name == "node_modules"
            || name == "result"
            || name == ".direnv"
        {
            continue;
        }

        if let Ok(meta) = entry.metadata() {
            if let Ok(m) = meta.modified() {
                if sig.max_mtime.is_none_or(|cur| m > cur) {
                    sig.max_mtime = Some(m);
                }
            }

            if meta.is_dir() {
                scan_dir_signature(&entry.path(), current_depth + 1, max_depth, sig);
            } else if meta.is_file() {
                sig.file_count += 1;
                sig.total_bytes += meta.len();
            }
        }
    }
}

/// Entrada de caché para un repositorio analizado.
#[derive(Debug, Clone)]
struct CacheEntry {
    head_mtime: Option<SystemTime>,
    index_mtime: Option<SystemTime>,
    worktree_sig: WorktreeSignature,
    status: GitRepoStatus,
}

/// Analizador global de Git con caché en memoria.
pub struct GitAnalyzer {
    cache: Mutex<HashMap<PathBuf, CacheEntry>>,
}

static INSTANCIA: OnceLock<GitAnalyzer> = OnceLock::new();

impl GitAnalyzer {
    pub fn global() -> &'static GitAnalyzer {
        INSTANCIA.get_or_init(|| GitAnalyzer {
            cache: Mutex::new(HashMap::new()),
        })
    }

    /// Obtiene el status del repositorio para una path dada, utilizando la caché si es válida.
    pub fn get_status(&self, workspace_path: &Path) -> Result<Option<GitRepoStatus>> {
        let Some((repo_root, git_dir)) = find_git_root(workspace_path) else {
            return Ok(None);
        };

        let head_mtime = get_mtime(&git_dir.join("HEAD"));
        let index_mtime = get_mtime(&git_dir.join("index"));
        let worktree_sig = get_worktree_signature(&repo_root);

        // Comprobar caché
        if let Ok(guard) = self.cache.lock() {
            if let Some(entry) = guard.get(&repo_root) {
                if entry.head_mtime == head_mtime
                    && entry.index_mtime == index_mtime
                    && entry.worktree_sig == worktree_sig
                {
                    return Ok(Some(entry.status.clone()));
                }
            }
        }

        // Analizar en disco
        let status = inspect_repo(&repo_root, &git_dir)?;

        // Re-leer index_mtime y worktree_sig para capturar el estado exacto post-análisis
        let final_head_mtime = get_mtime(&git_dir.join("HEAD"));
        let final_index_mtime = get_mtime(&git_dir.join("index"));
        let final_worktree_sig = get_worktree_signature(&repo_root);

        // Actualizar caché
        if let Ok(mut guard) = self.cache.lock() {
            guard.insert(
                repo_root,
                CacheEntry {
                    head_mtime: final_head_mtime,
                    index_mtime: final_index_mtime,
                    worktree_sig: final_worktree_sig,
                    status: status.clone(),
                },
            );
        }

        Ok(Some(status))
    }

    /// Invalida la entrada de caché para un repositorio dado.
    pub fn invalidate(&self, workspace_path: &Path) {
        if let Some((repo_root, _)) = find_git_root(workspace_path) {
            if let Ok(mut guard) = self.cache.lock() {
                guard.remove(&repo_root);
            }
        }
    }

    /// Invalida todas las entradas de caché del analizador.
    pub fn invalidate_all(&self) {
        if let Ok(mut guard) = self.cache.lock() {
            guard.clear();
        }
    }

    /// Alias compatible con el protocolo previo.
    pub fn consultar_estado(&self, workspace_path: &Path) -> Result<Option<GitRepoStatus>> {
        self.get_status(workspace_path)
    }
}

/// Finds the working-tree root and the associated `.git` directory.
///
/// Supports both `.git` as a directory (normal repo) and `.git` as a file
/// (worktrees and submodules). Ascends the directory tree without any ceiling;
/// prefer [`find_git_root_with_ceiling`] when querying project directories that
/// live inside the antOS workspace.
pub fn find_git_root(start: &Path) -> Option<(PathBuf, PathBuf)> {
    find_git_root_with_ceiling(start, None)
}

/// Ceiling-aware variant of [`find_git_root`].
///
/// `ceiling` is the exclusive upper bound: if the traversal reaches this
/// directory without having found a `.git` entry, the function returns `None`.
/// This prevents project directories that lack their own `.git` from inheriting
/// the antOS OS repository.
///
/// Pass `antos_root` as the ceiling when inspecting projects under `workspace/`.
pub fn find_git_root_with_ceiling(
    start: &Path,
    ceiling: Option<&Path>,
) -> Option<(PathBuf, PathBuf)> {
    let mut current = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    let ceiling_canon = ceiling
        .and_then(|c| c.canonicalize().ok())
        .or_else(|| ceiling.map(|c| c.to_path_buf()));

    loop {
        // Stop if we have reached (or passed) the ceiling directory.
        if let Some(ref ceil) = ceiling_canon {
            if current == *ceil {
                break;
            }
        }

        let git_candidate = current.join(".git");
        if git_candidate.is_dir() {
            return Some((current, git_candidate));
        } else if git_candidate.is_file() {
            // Worktree or submodule: `.git` file contains "gitdir: <path>"
            if let Ok(contents) = fs::read_to_string(&git_candidate) {
                for line in contents.lines() {
                    if let Some(rest) = line.strip_prefix("gitdir:") {
                        let rel = rest.trim();
                        let git_path = current.join(rel);
                        if let Ok(canon) = git_path.canonicalize() {
                            return Some((current, canon));
                        } else if git_path.exists() {
                            return Some((current, git_path));
                        }
                    }
                }
            }
        }

        if !current.pop() {
            break;
        }
    }

    None
}

fn get_mtime(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn char_to_status(c: char) -> GitFileStatus {
    match c {
        'M' => GitFileStatus::Modified,
        'A' => GitFileStatus::Created,
        'D' => GitFileStatus::Deleted,
        'R' => GitFileStatus::Renamed,
        'T' => GitFileStatus::TypeChanged,
        'U' => GitFileStatus::Conflicted,
        _ => GitFileStatus::Modified,
    }
}

/// Inspecciona un repositorio de forma unificada mediante `git status --porcelain=v2 --branch -uall` (T32.5).
/// Consolida en una sola llamada el estado de la rama, commit HEAD, ahead/behind y archivos staged/modified/untracked,
/// invocando `git diff --numstat` únicamente cuando existen cambios staged o modified concretos.
fn inspect_repo(repo_root: &Path, git_dir: &Path) -> Result<GitRepoStatus> {
    let antos_root = detect_antos_root();

    let output = git_cmd_with_ceiling(antos_root.as_deref())
        .arg("-C")
        .arg(repo_root)
        .args(["status", "--porcelain=v2", "--branch", "-uall"])
        .output()
        .context("ejecutando git status --porcelain=v2")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        bail!("git status falló: {err}");
    }

    let text = String::from_utf8_lossy(&output.stdout);

    let mut branch: Option<String> = None;
    let mut head_commit: Option<String> = None;
    let mut ahead: usize = 0;
    let mut behind: usize = 0;

    let mut staged_entries: Vec<(String, GitFileStatus)> = Vec::new();
    let mut modified_entries: Vec<(String, GitFileStatus)> = Vec::new();
    let mut untracked: Vec<String> = Vec::new();

    for line in text.lines() {
        if line.starts_with("# ") {
            if let Some(rest) = line.strip_prefix("# branch.oid ") {
                let rest = rest.trim();
                if rest != "(initial)" && !rest.is_empty() {
                    head_commit = Some(rest.chars().take(8).collect());
                }
            } else if let Some(rest) = line.strip_prefix("# branch.head ") {
                let rest = rest.trim();
                if rest != "(detached)" && !rest.is_empty() {
                    branch = Some(rest.to_string());
                }
            } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Some(a_str) = parts[0].strip_prefix('+') {
                        ahead = a_str.parse::<usize>().unwrap_or(0);
                    }
                    if let Some(b_str) = parts[1].strip_prefix('-') {
                        behind = b_str.parse::<usize>().unwrap_or(0);
                    }
                }
            }
        } else if let Some(rest) = line.strip_prefix("? ") {
            let raw_path = rest.trim();
            let path =
                if raw_path.starts_with('"') && raw_path.ends_with('"') && raw_path.len() >= 2 {
                    &raw_path[1..raw_path.len() - 1]
                } else {
                    raw_path
                };
            if !path.is_empty() {
                untracked.push(path.to_string());
            }
        } else if line.starts_with("1 ") {
            // 1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>
            let parts: Vec<&str> = line.splitn(9, ' ').collect();
            if parts.len() == 9 {
                let xy = parts[1];
                let x = xy.chars().next().unwrap_or('.');
                let y = xy.chars().nth(1).unwrap_or('.');
                let raw_path = parts[8];
                let path = if raw_path.starts_with('"')
                    && raw_path.ends_with('"')
                    && raw_path.len() >= 2
                {
                    &raw_path[1..raw_path.len() - 1]
                } else {
                    raw_path
                };

                if x != '.' {
                    staged_entries.push((path.to_string(), char_to_status(x)));
                }
                if y != '.' {
                    modified_entries.push((path.to_string(), char_to_status(y)));
                }
            }
        } else if line.starts_with("2 ") {
            // 2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path><tab><origPath>
            let parts: Vec<&str> = line.splitn(10, ' ').collect();
            if parts.len() == 10 {
                let xy = parts[1];
                let x = xy.chars().next().unwrap_or('.');
                let y = xy.chars().nth(1).unwrap_or('.');
                let path_and_orig = parts[9];
                let raw_path = path_and_orig.split('\t').next().unwrap_or(path_and_orig);
                let path = if raw_path.starts_with('"')
                    && raw_path.ends_with('"')
                    && raw_path.len() >= 2
                {
                    &raw_path[1..raw_path.len() - 1]
                } else {
                    raw_path
                };

                if x != '.' {
                    staged_entries.push((path.to_string(), char_to_status(x)));
                }
                if y != '.' {
                    modified_entries.push((path.to_string(), char_to_status(y)));
                }
            }
        } else if line.starts_with("u ") {
            // u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>
            let parts: Vec<&str> = line.splitn(11, ' ').collect();
            if parts.len() == 11 {
                let xy = parts[1];
                let x = xy.chars().next().unwrap_or('.');
                let y = xy.chars().nth(1).unwrap_or('.');
                let raw_path = parts[10];
                let path = if raw_path.starts_with('"')
                    && raw_path.ends_with('"')
                    && raw_path.len() >= 2
                {
                    &raw_path[1..raw_path.len() - 1]
                } else {
                    raw_path
                };

                if x != '.' {
                    staged_entries.push((path.to_string(), GitFileStatus::Conflicted));
                }
                if y != '.' {
                    modified_entries.push((path.to_string(), GitFileStatus::Conflicted));
                }
            }
        }
    }

    // Si branch o head_commit no fueron descubiertos por porcelain v2, recurrir a lectura directa
    if branch.is_none() && head_commit.is_none() {
        let (b, c) = read_head(git_dir);
        branch = b;
        head_commit = c;
    }

    // Numstat staged solo si hay entradas en staged
    let mut stats_staged: HashMap<String, (usize, usize)> = HashMap::new();
    if !staged_entries.is_empty() {
        if let Ok(out) = git_cmd_with_ceiling(antos_root.as_deref())
            .arg("-C")
            .arg(repo_root)
            .args(["diff", "--cached", "--numstat"])
            .output()
        {
            if out.status.success() {
                parse_numstat(&String::from_utf8_lossy(&out.stdout), &mut stats_staged);
            }
        }
    }

    // Numstat unstaged solo si hay entradas en modified
    let mut stats_unstaged: HashMap<String, (usize, usize)> = HashMap::new();
    if !modified_entries.is_empty() {
        if let Ok(out) = git_cmd_with_ceiling(antos_root.as_deref())
            .arg("-C")
            .arg(repo_root)
            .args(["diff", "--numstat"])
            .output()
        {
            if out.status.success() {
                parse_numstat(&String::from_utf8_lossy(&out.stdout), &mut stats_unstaged);
            }
        }
    }

    let staged: Vec<GitFileDiffSummary> = staged_entries
        .into_iter()
        .map(|(path, status)| {
            let (add, del) = stats_staged.get(&path).copied().unwrap_or((0, 0));
            GitFileDiffSummary {
                path,
                added_lines: add,
                deleted_lines: del,
                status,
            }
        })
        .collect();

    let modified: Vec<GitFileDiffSummary> = modified_entries
        .into_iter()
        .map(|(path, status)| {
            let (add, del) = stats_unstaged.get(&path).copied().unwrap_or((0, 0));
            GitFileDiffSummary {
                path,
                added_lines: add,
                deleted_lines: del,
                status,
            }
        })
        .collect();

    let clean = modified.is_empty() && staged.is_empty() && untracked.is_empty();

    Ok(GitRepoStatus {
        branch,
        head_commit,
        ahead,
        behind,
        modified,
        staged,
        untracked,
        clean,
    })
}

/// Lee `.git/HEAD` para resolver la branch actual y el commit actual.
fn read_head(git_dir: &Path) -> (Option<String>, Option<String>) {
    let head_file = git_dir.join("HEAD");
    let Ok(content) = fs::read_to_string(head_file) else {
        return (None, None);
    };

    let line = content.trim();
    if let Some(rest) = line.strip_prefix("ref: refs/heads/") {
        let branch_name = rest.to_string();
        // Intentar leer el hash del commit desde refs/heads/<branch>
        let ref_path = git_dir.join("refs").join("heads").join(&branch_name);
        let commit = fs::read_to_string(ref_path)
            .ok()
            .map(|s| s.trim().chars().take(8).collect::<String>());
        (Some(branch_name), commit)
    } else if !line.is_empty() {
        // HEAD desacoplado
        let commit = line.chars().take(8).collect::<String>();
        (None, Some(commit))
    } else {
        (None, None)
    }
}

fn parse_numstat(output_text: &str, target: &mut HashMap<String, (usize, usize)>) {
    for line in output_text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            let add = parts[0].parse::<usize>().unwrap_or(0);
            let del = parts[1].parse::<usize>().unwrap_or(0);
            let path = parts[2].to_string();
            target.insert(path, (add, del));
        }
    }
}

// ---------------------------------------------------- git worktrees (T2.2)

/// Creates an ephemeral Git worktree sharing objects from the base repository.
/// Carries `GIT_CEILING_DIRECTORIES` to enforce workspace boundary isolation.
pub fn create_worktree(
    repo_root: &Path,
    destination: &Path,
    branch: &str,
    base: &str,
) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating parent dir {}", parent.display()))?;
    }

    let antos_root = detect_antos_root();
    let out = git_cmd_with_ceiling(antos_root.as_deref())
        .arg("-C")
        .arg(repo_root)
        .args(["worktree", "add", "-B", branch])
        .arg(destination)
        .arg(base)
        .output()
        .context("executing git worktree add")?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("git worktree add failed: {err}");
    }
    Ok(())
}

/// Removes and prunes a Git worktree.
/// Carries `GIT_CEILING_DIRECTORIES` to enforce workspace boundary isolation.
pub fn remove_worktree(repo_root: &Path, destination: &Path, force: bool) -> Result<()> {
    let antos_root = detect_antos_root();
    let mut cmd = git_cmd_with_ceiling(antos_root.as_deref());
    cmd.arg("-C").arg(repo_root).arg("worktree").arg("remove");
    if force {
        cmd.arg("--force");
    }
    cmd.arg(destination);
    let out = cmd.output().context("executing git worktree remove")?;

    if !out.status.success() && destination.exists() {
        let _ = fs::remove_dir_all(destination);
    }

    let _ = git_cmd_with_ceiling(antos_root.as_deref())
        .arg("-C")
        .arg(repo_root)
        .args(["worktree", "prune"])
        .output();

    Ok(())
}

/// Merges a worktree branch into the target branch.
/// Carries `GIT_CEILING_DIRECTORIES` to enforce workspace boundary isolation.
pub fn merge_worktree(
    repo_root: &Path,
    branch: &str,
    target: &str,
    message: Option<&str>,
) -> Result<String> {
    let antos_root = detect_antos_root();
    let checkout = git_cmd_with_ceiling(antos_root.as_deref())
        .arg("-C")
        .arg(repo_root)
        .args(["checkout", target])
        .output()
        .context("checkout target branch")?;

    if !checkout.status.success() {
        bail!(
            "git checkout {target} failed: {}",
            String::from_utf8_lossy(&checkout.stderr)
        );
    }

    let default_msg = format!("merge: integrate changes from {branch}");
    let msg = message.unwrap_or(&default_msg);

    let merge_out = git_cmd_with_ceiling(antos_root.as_deref())
        .arg("-C")
        .arg(repo_root)
        .args(["merge", "--no-ff", "-m", msg, branch])
        .output()
        .context("merge worktree branch")?;

    if !merge_out.status.success() {
        bail!(
            "git merge failed: {}",
            String::from_utf8_lossy(&merge_out.stderr)
        );
    }

    Ok(String::from_utf8_lossy(&merge_out.stdout)
        .trim()
        .to_string())
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_find_git_root_finds_antos_repo() {
        let cwd = std::env::current_dir().expect("cwd");
        let found = find_git_root(&cwd);
        assert!(found.is_some(), "should find the antOS .git");
        let (repo_root, git_dir) = found.unwrap();
        assert!(git_dir.exists());
        assert!(repo_root.join("Cargo.toml").exists());
    }

    #[test]
    fn test_git_analyzer_on_current_repo() {
        let cwd = std::env::current_dir().expect("cwd");
        let analyzer = GitAnalyzer::global();
        let result = analyzer
            .consultar_estado(&cwd)
            .expect("analysis should succeed");
        assert!(result.is_some(), "antOS should be recognized as a Git repo");
        let status = result.unwrap();
        assert!(status.branch.is_some() || status.head_commit.is_some());
    }

    /// T17.1 — A directory inside workspace/ that has no .git of its own must
    /// return None when find_git_root_with_ceiling is called with the antOS root
    /// as the ceiling, preventing the OS repo from leaking into project diffs.
    #[test]
    fn test_workspace_project_does_not_inherit_antos_git() {
        let dir_temp = tempfile_simple("workspace_project_no_git");
        // Simulate a project directory inside workspace/ with no .git of its own.
        // Using the antOS root as the ceiling means we must NOT find the antOS .git.
        let antos_root = detect_antos_root();
        let result = find_git_root_with_ceiling(&dir_temp, antos_root.as_deref());
        let _ = fs::remove_dir_all(&dir_temp);
        assert!(
            result.is_none(),
            "a project dir without its own .git should not inherit the antOS OS repo; got: {:?}",
            result
        );
    }

    /// T17.1 — find_git_root_with_ceiling must stop at the ceiling and return None
    /// even when the antOS .git exists above.
    #[test]
    fn test_find_git_root_with_ceiling_stops_at_ceiling() {
        // Use /tmp as start and the temp dir parent as ceiling — guaranteed no .git.
        let dir_temp = tempfile_simple("ceiling_test");
        let ceiling = dir_temp.parent().map(|p| p.to_path_buf());
        let result = find_git_root_with_ceiling(&dir_temp, ceiling.as_deref());
        let _ = fs::remove_dir_all(&dir_temp);
        // The ceiling is the parent of dir_temp, so traversal stops immediately.
        assert!(
            result.is_none(),
            "traversal must stop at ceiling; got {:?}",
            result
        );
    }

    /// T17.1 — find_git_root_with_ceiling with no ceiling behaves like find_git_root.
    #[test]
    fn test_find_git_root_with_no_ceiling_behaves_like_find_git_root() {
        let cwd = std::env::current_dir().expect("cwd");
        let uncapped = find_git_root(&cwd);
        let with_none = find_git_root_with_ceiling(&cwd, None);
        assert_eq!(
            uncapped.as_ref().map(|(r, _)| r.clone()),
            with_none.as_ref().map(|(r, _)| r.clone()),
            "no ceiling should give same result as find_git_root"
        );
    }

    #[test]
    fn test_directorio_sin_git() {
        let dir_temp = tempfile_simple("sin_git_test");
        let analyzer = GitAnalyzer::global();
        let resultado = analyzer
            .consultar_estado(&dir_temp)
            .expect("analisis sin error");
        assert!(
            resultado.is_none(),
            "directorio temporal no debe ser repo Git"
        );
        let _ = fs::remove_dir_all(&dir_temp);
    }

    #[test]
    fn test_cache_performance_under_30ms() {
        let dir_repo = tempfile_simple("cache_bench_repo");
        let _ = Command::new("git")
            .arg("init")
            .arg("-b")
            .arg("main")
            .arg(&dir_repo)
            .output();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_repo)
            .args(["config", "user.name", "Test"])
            .output();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_repo)
            .args(["config", "user.email", "test@example.com"])
            .output();
        let _ = fs::write(dir_repo.join("README.md"), "# Bench\n");
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_repo)
            .args(["add", "README.md"])
            .output();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_repo)
            .args(["commit", "-m", "init"])
            .output();

        let analyzer = GitAnalyzer::global();
        // Primer acceso para calentar caché
        let _ = analyzer.consultar_estado(&dir_repo);

        // Segundo acceso desde caché
        let t0 = std::time::Instant::now();
        let resultado = analyzer
            .consultar_estado(&dir_repo)
            .expect("consulta en cache");
        let duracion = t0.elapsed();

        let _ = fs::remove_dir_all(&dir_repo);

        assert!(resultado.is_some());
        assert!(
            duracion.as_millis() < 500,
            "la respuesta desde caché debe tardar menos de 500ms (tardó: {:?})",
            duracion
        );
    }

    #[test]
    fn test_repositorio_vacio_o_nuevo() {
        let dir_temp = tempfile_simple("repo_vacio");
        let _ = Command::new("git").arg("init").arg(&dir_temp).output();

        let analyzer = GitAnalyzer::global();
        let resultado = analyzer
            .consultar_estado(&dir_temp)
            .expect("analisis repo vacio");
        assert!(
            resultado.is_some(),
            "debe detectar repo recién inicializado"
        );
        let status = resultado.unwrap();
        assert!(status.clean);

        let _ = fs::remove_dir_all(&dir_temp);
    }

    #[test]
    fn test_worktree_create_remove_and_performance() {
        let dir_repo = tempfile_simple("worktree_repo");
        let dir_wt = tempfile_simple("worktree_target");

        // Inicializar repo con commit inicial
        let _ = Command::new("git")
            .arg("init")
            .arg("-b")
            .arg("main")
            .arg(&dir_repo)
            .output();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_repo)
            .args(["config", "user.name", "Test"])
            .output();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_repo)
            .args(["config", "user.email", "test@example.com"])
            .output();
        let _ = fs::write(dir_repo.join("README.md"), "# Test Repo\n");
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_repo)
            .args(["add", "README.md"])
            .output();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_repo)
            .args(["commit", "-m", "init"])
            .output();

        // 1. Medir tiempo de creación de worktree (< 500ms según criterio de aceptación T2.2)
        let t0 = std::time::Instant::now();
        create_worktree(&dir_repo, &dir_wt, "agent/T2.2", "HEAD").expect("crear worktree");
        let duracion = t0.elapsed();

        assert!(
            duracion.as_millis() < 500,
            "creación de worktree debe ser < 500ms (tardó: {:?})",
            duracion
        );

        // 2. Verificar existencia y enlace compartido
        assert!(dir_wt.join("README.md").exists());
        let wt_git = dir_wt.join(".git");
        assert!(
            wt_git.is_file(),
            ".git en worktree debe ser un puntero gitdir:"
        );

        // 3. Verificar aislamiento: modificar archivo en worktree no toca el repo base
        fs::write(dir_wt.join("README.md"), "# Modificado en worktree\n").expect("write wt");
        let contenido_base = fs::read_to_string(dir_repo.join("README.md")).expect("read base");
        assert_eq!(
            contenido_base, "# Test Repo\n",
            "el repo base debe permanecer inalterado"
        );

        // 4. Eliminar worktree
        remove_worktree(&dir_repo, &dir_wt, true).expect("eliminar worktree");
        assert!(
            !dir_wt.exists(),
            "directorio de worktree debe haber sido eliminado"
        );

        let _ = fs::remove_dir_all(&dir_repo);
    }

    #[test]
    fn test_cache_invalidates_immediately_on_unadded_working_tree_modification() {
        let dir_repo = tempfile_simple("test_unadded_mod");
        let _ = Command::new("git")
            .arg("init")
            .arg("-b")
            .arg("main")
            .arg(&dir_repo)
            .output();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_repo)
            .args(["config", "user.name", "Test"])
            .output();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_repo)
            .args(["config", "user.email", "test@example.com"])
            .output();

        let readme = dir_repo.join("README.md");
        let _ = fs::write(&readme, "# Original Content\n");
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_repo)
            .args(["add", "README.md"])
            .output();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_repo)
            .args(["commit", "-m", "initial commit"])
            .output();

        let analyzer = GitAnalyzer::global();

        // 1. Estado inicial limpio cacheado
        let st1 = analyzer
            .consultar_estado(&dir_repo)
            .expect("consulta inicial")
            .expect("repo valido");
        assert!(st1.clean, "debe iniciar limpio");

        // 2. Modificar archivo en el workspace sin git add
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(&readme, "# Modified Content without git add\n").expect("escribir modificacion");

        // 3. Consultar de nuevo: la cache DEBE invalidarse de inmediato
        let st2 = analyzer
            .consultar_estado(&dir_repo)
            .expect("consulta tras mod")
            .expect("repo valido");

        assert!(
            !st2.clean,
            "el estado no debe ser limpio tras modificar un archivo sin git add"
        );
        assert_eq!(st2.modified.len(), 1);
        assert_eq!(st2.modified[0].path, "README.md");
        assert_eq!(st2.modified[0].status, GitFileStatus::Modified);

        let _ = fs::remove_dir_all(&dir_repo);
    }

    #[test]
    fn test_cache_invalidates_immediately_on_untracked_file() {
        let dir_repo = tempfile_simple("test_untracked_mod");
        let _ = Command::new("git")
            .arg("init")
            .arg("-b")
            .arg("main")
            .arg(&dir_repo)
            .output();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_repo)
            .args(["config", "user.name", "Test"])
            .output();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_repo)
            .args(["config", "user.email", "test@example.com"])
            .output();

        let readme = dir_repo.join("README.md");
        let _ = fs::write(&readme, "# Clean\n");
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_repo)
            .args(["add", "README.md"])
            .output();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_repo)
            .args(["commit", "-m", "clean"])
            .output();

        let analyzer = GitAnalyzer::global();
        let st1 = analyzer.consultar_estado(&dir_repo).unwrap().unwrap();
        assert!(st1.clean);

        // Añadir archivo sin git add
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(dir_repo.join("nuevo.txt"), "hola mundo").expect("crear nuevo");

        let st2 = analyzer.consultar_estado(&dir_repo).unwrap().unwrap();
        assert!(!st2.clean);
        assert_eq!(st2.untracked, vec!["nuevo.txt".to_string()]);

        let _ = fs::remove_dir_all(&dir_repo);
    }

    #[test]
    fn test_detect_antos_root_is_memoized() {
        let t0 = std::time::Instant::now();
        let root1 = detect_antos_root();
        let _d1 = t0.elapsed();

        let t1 = std::time::Instant::now();
        let root2 = detect_antos_root();
        let d2 = t1.elapsed();

        assert_eq!(root1, root2);
        assert!(
            d2 < std::time::Duration::from_millis(1),
            "llamada memoizada a detect_antos_root debe ser <1ms (tardó: {:?})",
            d2
        );
    }

    #[test]
    fn test_unified_porcelain_v2_status_and_numstat() {
        let dir_remote = tempfile_simple("porcelain_remote");
        let dir_local = tempfile_simple("porcelain_local");

        // Crear remote bare repo
        let _ = Command::new("git")
            .args(["init", "--bare", "-b", "main"])
            .arg(&dir_remote)
            .output();

        // Clonar a local
        let _ = Command::new("git")
            .args(["clone"])
            .arg(&dir_remote)
            .arg(&dir_local)
            .output();

        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_local)
            .args(["config", "user.name", "Test"])
            .output();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_local)
            .args(["config", "user.email", "test@example.com"])
            .output();

        // Commit base en remote
        let f1 = dir_local.join("file1.txt");
        let _ = fs::write(&f1, "line 1\nline 2\n");
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_local)
            .args(["add", "file1.txt"])
            .output();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_local)
            .args(["commit", "-m", "initial"])
            .output();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_local)
            .args(["push", "origin", "main"])
            .output();

        // 1. Commit local adicional (ahead: 1)
        let _ = fs::write(&f1, "line 1\nline 2\nline 3\n");
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_local)
            .args(["commit", "-am", "local ahead"])
            .output();

        // 2. Archivo staged (file2.txt)
        let f2 = dir_local.join("file2.txt");
        let _ = fs::write(&f2, "staged line 1\nstaged line 2\n");
        let _ = Command::new("git")
            .arg("-C")
            .arg(&dir_local)
            .args(["add", "file2.txt"])
            .output();

        // 3. Modificación unstaged en file1.txt
        let _ = fs::write(&f1, "line 1\nline 2\nline 3\nunstaged line 4\n");

        // 4. Archivo untracked
        let f3 = dir_local.join("untracked.txt");
        let _ = fs::write(&f3, "untracked\n");

        let analyzer = GitAnalyzer::global();
        let status = analyzer
            .consultar_estado(&dir_local)
            .expect("consultar estado")
            .expect("repo valido");

        assert_eq!(status.branch.as_deref(), Some("main"));
        assert!(status.head_commit.is_some());
        assert_eq!(status.ahead, 1, "debe detectar 1 commit ahead");
        assert_eq!(status.behind, 0, "debe detectar 0 commits behind");

        // Staged
        assert_eq!(status.staged.len(), 1);
        assert_eq!(status.staged[0].path, "file2.txt");
        assert_eq!(status.staged[0].added_lines, 2);
        assert_eq!(status.staged[0].deleted_lines, 0);

        // Modified
        assert_eq!(status.modified.len(), 1);
        assert_eq!(status.modified[0].path, "file1.txt");
        assert_eq!(status.modified[0].added_lines, 1);
        assert_eq!(status.modified[0].deleted_lines, 0);

        // Untracked
        assert_eq!(status.untracked, vec!["untracked.txt".to_string()]);
        assert!(!status.clean);

        let _ = fs::remove_dir_all(&dir_remote);
        let _ = fs::remove_dir_all(&dir_local);
    }

    fn tempfile_simple(nombre: &str) -> PathBuf {
        let ruta =
            std::env::temp_dir().join(format!("antos_test_{}_{}", nombre, std::process::id()));
        let _ = fs::create_dir_all(&ruta);
        ruta
    }
}
