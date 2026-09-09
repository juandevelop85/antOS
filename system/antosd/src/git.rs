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
/// Returns `None` if the root cannot be determined (e.g., running from an
/// unrelated directory).
pub fn detect_antos_root() -> Option<PathBuf> {
    let start = std::env::current_dir().ok()?;
    let mut candidate = start.as_path();
    loop {
        if candidate.join("system").join("capabilities").is_dir() {
            return Some(candidate.to_path_buf());
        }
        match candidate.parent() {
            Some(p) => candidate = p,
            None => return None,
        }
    }
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

/// Entrada de caché para un repositorio analizado.
#[derive(Debug, Clone)]
struct CacheEntry {
    head_mtime: Option<SystemTime>,
    index_mtime: Option<SystemTime>,
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

    /// Obtiene el estado del repositorio para una ruta dada, utilizando la caché si es válida.
    pub fn get_status(&self, workspace_path: &Path) -> Result<Option<GitRepoStatus>> {
        let Some((repo_root, git_dir)) = find_git_root(workspace_path) else {
            return Ok(None);
        };

        let head_mtime = get_mtime(&git_dir.join("HEAD"));
        let index_mtime = get_mtime(&git_dir.join("index"));

        // Comprobar caché
        if let Ok(guard) = self.cache.lock() {
            if let Some(entry) = guard.get(&repo_root) {
                if entry.head_mtime == head_mtime && entry.index_mtime == index_mtime {
                    return Ok(Some(entry.status.clone()));
                }
            }
        }

        // Analizar en disco
        let status = inspect_repo(&repo_root, &git_dir)?;

        // Actualizar caché
        if let Ok(mut guard) = self.cache.lock() {
            guard.insert(
                repo_root,
                CacheEntry {
                    head_mtime,
                    index_mtime,
                    status: status.clone(),
                },
            );
        }

        Ok(Some(status))
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

#[deprecated(note = "use find_git_root")]
pub fn encontrar_raiz_git(inicio: &Path) -> Option<(PathBuf, PathBuf)> {
    find_git_root(inicio)
}

fn get_mtime(ruta: &Path) -> Option<SystemTime> {
    fs::metadata(ruta).and_then(|m| m.modified()).ok()
}

/// Inspecciona un repositorio de forma optimizada.
fn inspect_repo(repo_root: &Path, git_dir: &Path) -> Result<GitRepoStatus> {
    // 1. Obtener HEAD y rama desde el sistema de archivos
    let (rama, head_commit) = leer_head(git_dir);

    // 2. Obtener upstream y commits delante/detrás
    let (delante, detras) = calcular_delante_detras(repo_root);

    // 3. Obtener estado de archivos y conteo de líneas
    let (modificados, staged, sin_seguimiento) = get_files_and_diffs(repo_root)?;

    let clean = modificados.is_empty() && staged.is_empty() && sin_seguimiento.is_empty();

    Ok(GitRepoStatus {
        branch: rama,
        head_commit,
        ahead: delante,
        behind: detras,
        modified: modificados,
        staged,
        untracked: sin_seguimiento,
        clean,
    })
}

/// Lee `.git/HEAD` para resolver la rama actual y el commit actual.
fn leer_head(git_dir: &Path) -> (Option<String>, Option<String>) {
    let head_file = git_dir.join("HEAD");
    let Ok(contenido) = fs::read_to_string(head_file) else {
        return (None, None);
    };

    let linea = contenido.trim();
    if let Some(resto) = linea.strip_prefix("ref: refs/heads/") {
        let nombre_rama = resto.to_string();
        // Intentar leer el hash del commit desde refs/heads/<rama>
        let ref_path = git_dir.join("refs").join("heads").join(&nombre_rama);
        let commit = fs::read_to_string(ref_path)
            .ok()
            .map(|s| s.trim().chars().take(8).collect::<String>());
        (Some(nombre_rama), commit)
    } else if !linea.is_empty() {
        // HEAD desacoplado
        let commit = linea.chars().take(8).collect::<String>();
        (None, Some(commit))
    } else {
        (None, None)
    }
}

/// Calculates ahead/behind commits against the upstream using `git rev-list`.
/// Injects `GIT_CEILING_DIRECTORIES` so Git cannot escape the workspace boundary.
fn calcular_delante_detras(repo_root: &Path) -> (usize, usize) {
    let antos_root = detect_antos_root();
    let output = git_cmd_with_ceiling(antos_root.as_deref())
        .arg("-C")
        .arg(repo_root)
        .args(["rev-list", "--left-right", "--count", "@{upstream}...HEAD"])
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            let parts: Vec<&str> = text.trim().split_whitespace().collect();
            if parts.len() == 2 {
                let behind = parts[0].parse::<usize>().unwrap_or(0);
                let ahead = parts[1].parse::<usize>().unwrap_or(0);
                return (ahead, behind);
            }
        }
    }

    (0, 0)
}

/// Returns lists of modified, staged and untracked files with line-diff statistics.
/// All Git subprocesses carry `GIT_CEILING_DIRECTORIES` to enforce workspace isolation.
fn get_files_and_diffs(
    repo_root: &Path,
) -> Result<(
    Vec<GitFileDiffSummary>,
    Vec<GitFileDiffSummary>,
    Vec<String>,
)> {
    let mut modificados = Vec::new();
    let mut staged = Vec::new();
    let mut sin_seguimiento = Vec::new();

    let antos_root = detect_antos_root();

    // Staged numstat
    let mut stats_staged: HashMap<String, (usize, usize)> = HashMap::new();
    if let Ok(out) = git_cmd_with_ceiling(antos_root.as_deref())
        .arg("-C")
        .arg(repo_root)
        .args(["diff", "--cached", "--numstat"])
        .output()
    {
        if out.status.success() {
            parsear_numstat(&String::from_utf8_lossy(&out.stdout), &mut stats_staged);
        }
    }

    // Unstaged numstat
    let mut stats_unstaged: HashMap<String, (usize, usize)> = HashMap::new();
    if let Ok(out) = git_cmd_with_ceiling(antos_root.as_deref())
        .arg("-C")
        .arg(repo_root)
        .args(["diff", "--numstat"])
        .output()
    {
        if out.status.success() {
            parsear_numstat(&String::from_utf8_lossy(&out.stdout), &mut stats_unstaged);
        }
    }

    // Porcelain status v1
    let output = git_cmd_with_ceiling(antos_root.as_deref())
        .arg("-C")
        .arg(repo_root)
        .args(["status", "--porcelain=v1", "-uall"])
        .output()
        .context("failed to execute git status")?;

    if !output.status.success() {
        return Ok((modificados, staged, sin_seguimiento));
    }

    let texto = String::from_utf8_lossy(&output.stdout);
    for linea in texto.lines() {
        if linea.len() < 3 {
            continue;
        }

        let index_stat = linea.as_bytes()[0] as char;
        let work_stat = linea.as_bytes()[1] as char;
        let ruta_raw = linea[3..].trim();
        let ruta = ruta_raw
            .split(" -> ")
            .last()
            .unwrap_or(ruta_raw)
            .to_string();

        if index_stat == '?' && work_stat == '?' {
            sin_seguimiento.push(ruta);
            continue;
        }

        // Staged
        if index_stat != ' ' && index_stat != '?' {
            let estado = match index_stat {
                'M' => GitFileStatus::Modified,
                'A' => GitFileStatus::Created,
                'D' => GitFileStatus::Deleted,
                'R' => GitFileStatus::Renamed,
                'T' => GitFileStatus::TypeChanged,
                'U' => GitFileStatus::Conflicted,
                _ => GitFileStatus::Modified,
            };
            let (add, del) = stats_staged.get(&ruta).copied().unwrap_or((0, 0));
            staged.push(GitFileDiffSummary {
                path: ruta.clone(),
                added_lines: add,
                deleted_lines: del,
                status: estado,
            });
        }

        // Unstaged / Modificados en workspace
        if work_stat != ' ' && work_stat != '?' {
            let estado = match work_stat {
                'M' => GitFileStatus::Modified,
                'A' => GitFileStatus::Created,
                'D' => GitFileStatus::Deleted,
                'R' => GitFileStatus::Renamed,
                'T' => GitFileStatus::TypeChanged,
                'U' => GitFileStatus::Conflicted,
                _ => GitFileStatus::Modified,
            };
            let (add, del) = stats_unstaged.get(&ruta).copied().unwrap_or((0, 0));
            modificados.push(GitFileDiffSummary {
                path: ruta,
                added_lines: add,
                deleted_lines: del,
                status: estado,
            });
        }
    }

    Ok((modificados, staged, sin_seguimiento))
}

fn parsear_numstat(salida: &str, destino: &mut HashMap<String, (usize, usize)>) {
    for linea in salida.lines() {
        let partes: Vec<&str> = linea.split('\t').collect();
        if partes.len() >= 3 {
            let add = partes[0].parse::<usize>().unwrap_or(0);
            let del = partes[1].parse::<usize>().unwrap_or(0);
            let ruta = partes[2].to_string();
            destino.insert(ruta, (add, del));
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

#[deprecated(note = "use create_worktree")]
pub fn crear_worktree(repo_root: &Path, destino: &Path, branch: &str, base: &str) -> Result<()> {
    create_worktree(repo_root, destino, branch, base)
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

    if !out.status.success() {
        if destination.exists() {
            let _ = fs::remove_dir_all(destination);
        }
    }

    let _ = git_cmd_with_ceiling(antos_root.as_deref())
        .arg("-C")
        .arg(repo_root)
        .args(["worktree", "prune"])
        .output();

    Ok(())
}

#[deprecated(note = "use remove_worktree")]
pub fn eliminar_worktree(repo_root: &Path, destino: &Path, force: bool) -> Result<()> {
    remove_worktree(repo_root, destino, force)
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

    fn tempfile_simple(nombre: &str) -> PathBuf {
        let ruta =
            std::env::temp_dir().join(format!("antos_test_{}_{}", nombre, std::process::id()));
        let _ = fs::create_dir_all(&ruta);
        ruta
    }
}
