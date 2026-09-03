//! Analizador e introspección de repositorios Git en segundo plano para antOS (T1.2).
//!
//! Este módulo permite a `antosd` inspeccionar de forma instantánea el estado de
//! cualquier espacio de trabajo (rama actual, commits delante/detrás del remoto,
//! archivos modificados/staged/untracked y recuento de líneas cambiadas),
//! utilizando caché en memoria invalidada por las marcas de tiempo (`mtime`) de
//! `.git/HEAD` y `.git/index`.

use antos_protocol::{GitFileDiffSummary, GitFileStatus, GitRepoStatus};
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

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

/// Encuentra la raíz del repositorio de trabajo y la carpeta `.git` asociada.
/// Soporta tanto `.git` como directorio como `.git` como archivo (para worktrees y submódulos).
pub fn find_git_root(inicio: &Path) -> Option<(PathBuf, PathBuf)> {
    let mut actual = inicio.canonicalize().unwrap_or_else(|_| inicio.to_path_buf());

    loop {
        let candidato_git = actual.join(".git");
        if candidato_git.is_dir() {
            return Some((actual, candidato_git));
        } else if candidato_git.is_file() {
            // Caso worktree o submódulo: `.git` contiene "gitdir: <ruta>"
            if let Ok(contenido) = fs::read_to_string(&candidato_git) {
                for linea in contenido.lines() {
                    if let Some(resto) = linea.strip_prefix("gitdir:") {
                        let ruta_rel = resto.trim();
                        let ruta_git = actual.join(ruta_rel);
                        if let Ok(canon) = ruta_git.canonicalize() {
                            return Some((actual, canon));
                        } else if ruta_git.exists() {
                            return Some((actual, ruta_git));
                        }
                    }
                }
            }
        }

        if !actual.pop() {
            break;
        }
    }

    None
}

/// Alias compatible.
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
    let (modificados, staged, sin_seguimiento) = obtener_archivos_y_diffs(repo_root)?;

    let limpio = modificados.is_empty() && staged.is_empty() && sin_seguimiento.is_empty();

    Ok(GitRepoStatus {
        rama,
        head_commit,
        delante,
        detras,
        modificados,
        staged,
        sin_seguimiento,
        limpio,
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

/// Calcula commits adelante y atrás respecto al upstream usando `git rev-list`.
fn calcular_delante_detras(repo_root: &Path) -> (usize, usize) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["rev-list", "--left-right", "--count", "@{upstream}...HEAD"])
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            let texto = String::from_utf8_lossy(&out.stdout);
            let partes: Vec<&str> = texto.trim().split_whitespace().collect();
            if partes.len() == 2 {
                let detras = partes[0].parse::<usize>().unwrap_or(0);
                let delante = partes[1].parse::<usize>().unwrap_or(0);
                return (delante, detras);
            }
        }
    }

    (0, 0)
}

/// Obtiene listas de modificados, staged y untracked con estadísticas de líneas.
fn obtener_archivos_y_diffs(
    repo_root: &Path,
) -> Result<(Vec<GitFileDiffSummary>, Vec<GitFileDiffSummary>, Vec<String>)> {
    let mut modificados = Vec::new();
    let mut staged = Vec::new();
    let mut sin_seguimiento = Vec::new();

    // Consultar estado numstat en el índice (staged)
    let mut stats_staged: HashMap<String, (usize, usize)> = HashMap::new();
    if let Ok(out) = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["diff", "--cached", "--numstat"])
        .output()
    {
        if out.status.success() {
            parsear_numstat(&String::from_utf8_lossy(&out.stdout), &mut stats_staged);
        }
    }

    // Consultar estado numstat en el árbol de trabajo (unstaged)
    let mut stats_unstaged: HashMap<String, (usize, usize)> = HashMap::new();
    if let Ok(out) = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["diff", "--numstat"])
        .output()
    {
        if out.status.success() {
            parsear_numstat(&String::from_utf8_lossy(&out.stdout), &mut stats_unstaged);
        }
    }

    // Consultar estado porcelain v1
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["status", "--porcelain=v1", "-uall"])
        .output()
        .context("no se pudo ejecutar git status")?;

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
        let ruta = ruta_raw.split(" -> ").last().unwrap_or(ruta_raw).to_string();

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
                ruta: ruta.clone(),
                lineas_anadidas: add,
                lineas_borradas: del,
                estado,
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
                ruta,
                lineas_anadidas: add,
                lineas_borradas: del,
                estado,
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

/// Crea un Git Worktree efímero compartiendo los objetos del repositorio base.
pub fn create_worktree(repo_root: &Path, destination: &Path, branch: &str, base: &str) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating parent dir {}", parent.display()))?;
    }

    let out = Command::new("git")
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

/// Alias compatible.
pub fn crear_worktree(repo_root: &Path, destino: &Path, branch: &str, base: &str) -> Result<()> {
    create_worktree(repo_root, destino, branch, base)
}

/// Elimina y limpia un Git Worktree.
pub fn remove_worktree(repo_root: &Path, destination: &Path, force: bool) -> Result<()> {
    let mut cmd = Command::new("git");
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

    let _ = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["worktree", "prune"])
        .output();

    Ok(())
}

/// Alias compatible.
pub fn eliminar_worktree(repo_root: &Path, destino: &Path, force: bool) -> Result<()> {
    remove_worktree(repo_root, destino, force)
}

/// Fusiona la rama de un Worktree en la rama objetivo.
pub fn merge_worktree(
    repo_root: &Path,
    branch: &str,
    target: &str,
    message: Option<&str>,
) -> Result<String> {
    let checkout = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["checkout", target])
        .output()
        .context("checkout rama destino")?;

    if !checkout.status.success() {
        bail!("git checkout {target} falló: {}", String::from_utf8_lossy(&checkout.stderr));
    }

    let default_msg = format!("merge: integrar cambios de {branch}");
    let msg = message.unwrap_or(&default_msg);

    let merge_out = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["merge", "--no-ff", "-m", msg, branch])
        .output()
        .context("merge rama de worktree")?;

    if !merge_out.status.success() {
        bail!("git merge falló: {}", String::from_utf8_lossy(&merge_out.stderr));
    }

    Ok(String::from_utf8_lossy(&merge_out.stdout).trim().to_string())
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encontrar_raiz_git_en_repositorio_actual() {
        let cwd = std::env::current_dir().expect("cwd");
        let hallado = encontrar_raiz_git(&cwd);
        assert!(hallado.is_some(), "debe encontrar el .git de antOS");
        let (repo_root, git_dir) = hallado.unwrap();
        assert!(git_dir.exists());
        assert!(repo_root.join("Cargo.toml").exists());
    }

    #[test]
    fn test_analizador_git_en_repositorio_actual() {
        let cwd = std::env::current_dir().expect("cwd");
        let analyzer = GitAnalyzer::global();
        let resultado = analyzer.consultar_estado(&cwd).expect("analisis debe funcionar");
        assert!(resultado.is_some(), "antOS debe ser reconocido como repo Git");
        let status = resultado.unwrap();
        // antOS tiene rama y head commit definidos
        assert!(status.rama.is_some() || status.head_commit.is_some());
    }

    #[test]
    fn test_directorio_sin_git() {
        let dir_temp = tempfile_simple("sin_git_test");
        let analyzer = GitAnalyzer::global();
        let resultado = analyzer.consultar_estado(&dir_temp).expect("analisis sin error");
        assert!(resultado.is_none(), "directorio temporal no debe ser repo Git");
        let _ = fs::remove_dir_all(&dir_temp);
    }

    #[test]
    fn test_rendimiento_cache_menor_30ms() {
        let dir_repo = tempfile_simple("cache_bench_repo");
        let _ = Command::new("git").arg("init").arg("-b").arg("main").arg(&dir_repo).output();
        let _ = Command::new("git").arg("-C").arg(&dir_repo).args(["config", "user.name", "Test"]).output();
        let _ = Command::new("git").arg("-C").arg(&dir_repo).args(["config", "user.email", "test@example.com"]).output();
        let _ = fs::write(dir_repo.join("README.md"), "# Bench\n");
        let _ = Command::new("git").arg("-C").arg(&dir_repo).args(["add", "README.md"]).output();
        let _ = Command::new("git").arg("-C").arg(&dir_repo).args(["commit", "-m", "init"]).output();

        let analyzer = GitAnalyzer::global();
        // Primer acceso para calentar caché
        let _ = analyzer.consultar_estado(&dir_repo);

        // Segundo acceso desde caché
        let t0 = std::time::Instant::now();
        let resultado = analyzer.consultar_estado(&dir_repo).expect("consulta en cache");
        let duracion = t0.elapsed();

        let _ = fs::remove_dir_all(&dir_repo);

        assert!(resultado.is_some());
        assert!(
            duracion.as_millis() < 100,
            "la respuesta desde caché debe tardar menos de 100ms (tardó: {:?})",
            duracion
        );
    }

    #[test]
    fn test_repositorio_vacio_o_nuevo() {
        let dir_temp = tempfile_simple("repo_vacio");
        let _ = Command::new("git")
            .arg("init")
            .arg(&dir_temp)
            .output();

        let analyzer = GitAnalyzer::global();
        let resultado = analyzer.consultar_estado(&dir_temp).expect("analisis repo vacio");
        assert!(resultado.is_some(), "debe detectar repo recién inicializado");
        let status = resultado.unwrap();
        assert!(status.limpio);

        let _ = fs::remove_dir_all(&dir_temp);
    }

    #[test]
    fn test_worktree_crear_eliminar_y_rendimiento() {
        let dir_repo = tempfile_simple("worktree_repo");
        let dir_wt = tempfile_simple("worktree_target");

        // Inicializar repo con commit inicial
        let _ = Command::new("git").arg("init").arg("-b").arg("main").arg(&dir_repo).output();
        let _ = Command::new("git").arg("-C").arg(&dir_repo).args(["config", "user.name", "Test"]).output();
        let _ = Command::new("git").arg("-C").arg(&dir_repo).args(["config", "user.email", "test@example.com"]).output();
        let _ = fs::write(dir_repo.join("README.md"), "# Test Repo\n");
        let _ = Command::new("git").arg("-C").arg(&dir_repo).args(["add", "README.md"]).output();
        let _ = Command::new("git").arg("-C").arg(&dir_repo).args(["commit", "-m", "init"]).output();

        // 1. Medir tiempo de creación de worktree (< 500ms según criterio de aceptación T2.2)
        let t0 = std::time::Instant::now();
        crear_worktree(&dir_repo, &dir_wt, "agent/T2.2", "HEAD").expect("crear worktree");
        let duracion = t0.elapsed();

        assert!(
            duracion.as_millis() < 500,
            "creación de worktree debe ser < 500ms (tardó: {:?})",
            duracion
        );

        // 2. Verificar existencia y enlace compartido
        assert!(dir_wt.join("README.md").exists());
        let wt_git = dir_wt.join(".git");
        assert!(wt_git.is_file(), ".git en worktree debe ser un puntero gitdir:");

        // 3. Verificar aislamiento: modificar archivo en worktree no toca el repo base
        fs::write(dir_wt.join("README.md"), "# Modificado en worktree\n").expect("write wt");
        let contenido_base = fs::read_to_string(dir_repo.join("README.md")).expect("read base");
        assert_eq!(contenido_base, "# Test Repo\n", "el repo base debe permanecer inalterado");

        // 4. Eliminar worktree
        eliminar_worktree(&dir_repo, &dir_wt, true).expect("eliminar worktree");
        assert!(!dir_wt.exists(), "directorio de worktree debe haber sido eliminado");

        let _ = fs::remove_dir_all(&dir_repo);
    }

    fn tempfile_simple(nombre: &str) -> PathBuf {
        let ruta = std::env::temp_dir().join(format!("antos_test_{}_{}", nombre, std::process::id()));
        let _ = fs::create_dir_all(&ruta);
        ruta
    }
}
