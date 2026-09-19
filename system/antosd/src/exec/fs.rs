//! Filesystem operations, project scaffolding, declarations, and syntax guard interception.

use super::{Change, PendingChanges};
use crate::ctx::Ctx;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn changes_for(
    cap: &str,
    a: &BTreeMap<String, String>,
    ctx: &Ctx,
    pending: &PendingChanges,
) -> Result<Option<Vec<Change>>> {
    match cap {
        "fs.read" => Ok(Some(vec![Change::Read {
            path: abs(ctx, &a["path"]),
        }])),

        "fs.write" => Ok(Some(vec![Change::Write {
            path: abs(ctx, &a["path"]),
            content: a["content"].clone(),
        }])),

        "fs.patch" => Ok(Some(vec![Change::Patch {
            path: abs(ctx, &a["path"]),
            old: a["old"].clone(),
            new: a["new"].clone(),
        }])),

        "fs.list" => Ok(Some(vec![Change::ListDir {
            path: abs(ctx, &a["path"]),
            depth: a
                .get("depth")
                .and_then(|d| d.parse().ok())
                .unwrap_or(2)
                .clamp(1, 6),
            limit: a
                .get("limit")
                .and_then(|l| l.parse().ok())
                .unwrap_or(200)
                .clamp(1, 2000),
        }])),

        "test.run" => Ok(Some(vec![Change::TestRun {
            workspace: abs(ctx, &a["path"]),
            filter: a.get("filter").filter(|f| !f.is_empty()).cloned(),
        }])),

        "fs.delete" => Ok(Some(vec![Change::Delete {
            path: abs(ctx, &a["path"]),
        }])),

        "fs.mkdir" => Ok(Some(vec![Change::Mkdir {
            path: abs(ctx, &a["path"]),
        }])),

        "project.scaffold" => {
            let name = &a["name"];
            let language = &a["language"];
            let framework = a.get("framework").map(String::as_str).unwrap_or("");
            let root = ctx.workspace.join(name);
            // T35.1: el stack viene del catálogo (`system/stacks/*.toml`),
            // no de un `match`. Un framework desconocido es un error que
            // nombra los disponibles, no un proyecto a medias.
            let catalog = crate::stacks::StackCatalog::load(ctx.antos_root.as_deref())?;
            let stack = catalog.find(language, framework).ok_or_else(|| {
                let available = catalog.frameworks_for(language);
                if framework.is_empty() {
                    anyhow::anyhow!(
                        "no hay stack base para el lenguaje «{language}»; los conocidos son: {}",
                        catalog.languages().join(", ")
                    )
                } else if available.is_empty() {
                    anyhow::anyhow!(
                        "no conozco ningún framework para «{language}» (pedido: «{framework}»)"
                    )
                } else {
                    anyhow::anyhow!(
                        "no conozco el framework «{framework}» para «{language}»; disponibles: {}",
                        available.join(", ")
                    )
                }
            })?;
            let mut changes: Vec<Change> = stack
                .render(name)
                .into_iter()
                .map(|(rel, content)| Change::Write {
                    path: root.join(rel),
                    content,
                })
                .collect();
            // La fuente de verdad del stack para `test.run`, `ci` y los
            // agentes (T35.3): qué es este proyecto y cómo se prueba.
            changes.push(Change::Write {
                path: root.join(".antos").join("project.toml"),
                content: stack.project_manifest(name)?,
            });
            // T35.2: el perfil de entorno del stack (lo que `env.init` haría
            // a mano), con el toolchain que el stack declara. Con `nix`, es
            // lo que trae node/cargo/uv aunque la imagen no los lleve.
            if !stack.toolchain.nix.is_empty() {
                let label = stack.id.as_str();
                changes.push(Change::Write {
                    path: root.join("flake.nix"),
                    content: crate::env::render_flake_nix(label, &stack.toolchain.nix),
                });
                changes.push(Change::Write {
                    path: root.join("devbox.json"),
                    content: crate::env::render_devbox_json(label, &stack.toolchain.nix)?,
                });
                changes.push(Change::Write {
                    path: root.join(".antos").join("env.toml"),
                    content: crate::env::render_env_toml(label, &stack.toolchain.nix)?,
                });
            }
            // T17.3: project.scaffold now also emits a ProjectGitInit change so every
            // newly scaffolded project starts with a clean, isolated Git repository.
            changes.push(Change::ProjectGitInit {
                project_dir: root,
                branch: "main".into(),
                language_hint: Some(language.clone()),
            });
            Ok(Some(changes))
        }

        "pkg.declare" => {
            let proj = ctx.workspace.join(&a["project"]);
            let path = proj.join("antos.packages.toml");
            // Migración de una sola vez (T31.11): un proyecto con el fichero
            // todavía llamado `syso.packages.toml` de antes del renombrado
            // se migra a `antos.packages.toml` exactamente una vez.
            let _ = crate::util::migrate_legacy_path(&proj.join("syso.packages.toml"), &path);
            let previo = read_with_pending(&path, pending);
            Ok(Some(vec![Change::Write {
                content: declare_package(&previo, &a["package"], &a["version"])?,
                path,
            }]))
        }

        "system.declare" => {
            let path = ctx.system_config.join("antos-paquetes.nix");
            // Migración de una sola vez (T31.11): mismo caso que arriba,
            // para el módulo Nix declarativo del sistema.
            let _ = crate::util::migrate_legacy_path(
                &ctx.system_config.join("syso-paquetes.nix"),
                &path,
            );
            let previo = read_with_pending(&path, pending);
            Ok(Some(vec![Change::Write {
                content: declare_system_package(&previo, &a["package"])?,
                path,
            }]))
        }

        _ => Ok(None),
    }
}

pub fn apply(change: &Change) -> Result<Option<String>> {
    match change {
        Change::Write { path, content } => {
            write_guarded(path, content)?;
            Ok(Some(String::new()))
        }
        Change::Patch { path, old, new } => {
            let current = std::fs::read_to_string(path)
                .with_context(|| format!("leyendo {} para parchear", path.display()))?;
            let patched = apply_patch(&current, old, new)
                .with_context(|| format!("parcheando {}", path.display()))?;
            write_guarded(path, &patched)?;
            Ok(Some(format!(
                "parche aplicado en {} ({} → {} líneas)",
                path.display(),
                current.lines().count(),
                patched.lines().count()
            )))
        }
        Change::ListDir { path, depth, limit } => Ok(Some(list_dir(path, *depth, *limit)?)),
        Change::TestRun { workspace, filter } => Ok(Some(run_tests(workspace, filter.as_deref())?)),
        Change::Mkdir { path } => {
            std::fs::create_dir_all(path)
                .with_context(|| format!("creando el directorio {}", path.display()))?;
            Ok(Some(String::new()))
        }
        Change::Delete { path } => {
            if path.is_dir() {
                std::fs::remove_dir_all(path)?;
            } else if path.exists() {
                std::fs::remove_file(path)?;
            } else {
                bail!("no existe: {}", path.display());
            }
            Ok(Some(String::new()))
        }
        Change::Read { path } => {
            let content = std::fs::read_to_string(path)?;
            Ok(Some(content))
        }
        _ => Ok(None),
    }
}

/// Escritura con el interceptor sintáctico del VFS Guard (T10.2): compartida
/// por `Write` y `Patch` para que un parche no pueda colar lo que una
/// escritura completa rechazaría.
fn write_guarded(path: &Path, content: &str) -> Result<()> {
    let guard = crate::vfs_guard::VfsGuardEngine::global();
    let val = guard.intercept_write(&path.display().to_string(), content)?;
    if !val.is_valid {
        let err_msgs: Vec<String> = val
            .errors
            .iter()
            .map(|e| format!("  • L{}:{}: {}", e.line, e.column, e.message))
            .collect();
        bail!(
            "escritura rechazada por el interceptor sintáctico VFS ({}):\n{}",
            path.display(),
            err_msgs.join("\n")
        );
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creando el directorio {}", parent.display()))?;
    }
    std::fs::write(path, content).with_context(|| format!("escribiendo {}", path.display()))
}

/// Sustitución exacta: `old` tiene que aparecer exactamente una vez. Cero
/// apariciones o más de una son errores, nunca una elección silenciosa —
/// es la garantía que hace que un modelo no pueda «parchear» a ciegas.
pub fn apply_patch(current: &str, old: &str, new: &str) -> Result<String> {
    if old.is_empty() {
        bail!("el bloque a sustituir («old») no puede estar vacío");
    }
    let occurrences = current.matches(old).count();
    match occurrences {
        0 => {
            bail!("el bloque a sustituir no aparece en el fichero (¿espacios o sangría distintos?)")
        }
        1 => Ok(current.replacen(old, new, 1)),
        n => {
            bail!("el bloque a sustituir aparece {n} veces; amplía el contexto para que sea único")
        }
    }
}

/// Listado acotado, ordenado, sin `.git` ni `target`/`node_modules` (ruido que
/// un agente no necesita y que dispara el límite antes de ver el código).
fn list_dir(root: &Path, depth: usize, limit: usize) -> Result<String> {
    const SKIP: &[&str] = &[".git", "target", "node_modules", ".antos", "__pycache__"];
    fn walk(dir: &Path, root: &Path, depth: usize, limit: usize, out: &mut Vec<String>) {
        if depth == 0 || out.len() >= limit {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            if out.len() >= limit {
                return;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if SKIP.contains(&name.as_str()) {
                continue;
            }
            let p = e.path();
            let rel = p.strip_prefix(root).unwrap_or(&p).display().to_string();
            if p.is_dir() {
                out.push(format!("{rel}/"));
                walk(&p, root, depth - 1, limit, out);
            } else {
                out.push(rel);
            }
        }
    }
    if !root.is_dir() {
        bail!("no es un directorio: {}", root.display());
    }
    let mut out = Vec::new();
    walk(root, root, depth, limit, &mut out);
    if out.len() >= limit {
        out.push(format!("… (límite de {limit} entradas alcanzado)"));
    }
    Ok(out.join("\n"))
}

/// Suite de tests del proyecto. El intérprete NUNCA es `sh -c` (T31.4): se
/// invoca el binario del gestor con argumentos separados. La salida se
/// acota a las últimas líneas para que quepa en el contexto de un modelo.
pub(crate) fn run_tests(workspace: &Path, filter: Option<&str>) -> Result<String> {
    const TAIL_LINES: usize = 120;
    let (program, args): (&str, Vec<String>) = if workspace.join("Cargo.toml").exists() {
        let mut a = vec!["test".to_string(), "--quiet".to_string()];
        if let Some(f) = filter {
            a.push(f.to_string());
        }
        ("cargo", a)
    } else if workspace.join("package.json").exists() {
        let mut a = vec!["test".to_string(), "--silent".to_string()];
        if let Some(f) = filter {
            a.push("--".to_string());
            a.push(f.to_string());
        }
        ("npm", a)
    } else if workspace.join("pyproject.toml").exists()
        || workspace.join("pytest.ini").exists()
        || workspace.join("setup.py").exists()
    {
        // Sin caché ni bytecode: así pytest no necesita escribir nada fuera
        // de lo que el recinto permite.
        let mut a = vec![
            "-q".to_string(),
            "-p".to_string(),
            "no:cacheprovider".to_string(),
        ];
        if let Some(f) = filter {
            a.push("-k".to_string());
            a.push(f.to_string());
        }
        ("pytest", a)
    } else {
        bail!(
            "no reconozco el proyecto en {} (busco Cargo.toml, package.json o pyproject.toml)",
            workspace.display()
        );
    };
    // El recinto solo deja escribir lo declarado, y `$TMPDIR` no lo está:
    // los temporales de rustc/pytest van dentro del artefacto de
    // construcción (`target/`, declarado como `scratch`).
    let tmp = workspace.join("target").join("antos-tmp");
    std::fs::create_dir_all(&tmp)
        .with_context(|| format!("creando el temporal de tests {}", tmp.display()))?;
    let out = std::process::Command::new(program)
        .args(&args)
        .current_dir(workspace)
        .env("TMPDIR", &tmp)
        .env("TMP", &tmp)
        .env("TEMP", &tmp)
        .env("CARGO_TERM_COLOR", "never")
        .env("NO_COLOR", "1")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .with_context(|| format!("ejecutando {program} {}", args.join(" ")))?;
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    let lines: Vec<&str> = text.lines().collect();
    let tail: Vec<&str> = lines[lines.len().saturating_sub(TAIL_LINES)..].to_vec();
    let verdict = if out.status.success() {
        "TESTS EN VERDE"
    } else {
        "TESTS EN ROJO"
    };
    Ok(format!(
        "{verdict} ({program} {}, código {}){}\n{}",
        args.join(" "),
        out.status.code().unwrap_or(-1),
        if lines.len() > TAIL_LINES {
            format!(" — últimas {TAIL_LINES} de {} líneas", lines.len())
        } else {
            String::new()
        },
        tail.join("\n")
    ))
}

pub fn abs(ctx: &Ctx, raw: &str) -> PathBuf {
    let expanded = crate::blast::expand(raw, &BTreeMap::new(), &ctx.workspace);
    let p = PathBuf::from(expanded);
    if p.is_absolute() {
        p
    } else {
        ctx.workspace.join(p)
    }
}

fn read_with_pending(path: &Path, pending: &PendingChanges) -> String {
    pending
        .read(path)
        .unwrap_or_else(|| std::fs::read_to_string(path).unwrap_or_default())
}

const NIX_HEADER: &str = "\
# Paquetes del sistema, declarados por antOS.
#
# Esto NO instala nada: describe qué debe tener la máquina. Aplicarlo es un
# paso aparte, explícito y tuyo:
#
#     sudo nixos-rebuild switch
#
# Editarlo a mano es correcto: antOS respeta lo que encuentre aquí.
{ pkgs, ... }:
{
  environment.systemPackages = with pkgs; [
";

const NIX_FOOTER: &str = "  ];\n}\n";

pub fn declare_system_package(previo: &str, package: &str) -> Result<String> {
    let mut packages: BTreeMap<String, ()> = BTreeMap::new();

    {
        let existing = previo;
        let mut inside = false;
        for line in existing.lines() {
            let trimmed = line.trim();
            if trimmed.ends_with('[') {
                inside = true;
                continue;
            }
            if trimmed.starts_with(']') {
                inside = false;
                continue;
            }
            if inside && !trimmed.is_empty() && !trimmed.starts_with('#') {
                packages.insert(trimmed.to_string(), ());
            }
        }
    }
    packages.insert(package.to_string(), ());

    let cuerpo: String = packages
        .keys()
        .map(|name| format!("    {name}\n"))
        .collect();
    Ok(format!("{NIX_HEADER}{cuerpo}{NIX_FOOTER}"))
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PackagesFile {
    #[serde(default)]
    packages: BTreeMap<String, String>,
}

const PACKAGES_HEADER: &str = "\
# Dependencias declaradas por antOS.
#
# Declarar no es instalar: este fichero dice qué necesita el proyecto, y la
# instalación es un paso aparte y explícito. Así lo que apruebas es un diff
# legible, y deshacerlo es volver a la declaración anterior.
";

pub fn declare_package(previo: &str, package: &str, version: &str) -> Result<String> {
    let mut file: PackagesFile = toml::from_str(previo).unwrap_or_default();
    file.packages
        .insert(package.to_string(), version.to_string());
    Ok(format!("{PACKAGES_HEADER}\n{}", toml::to_string(&file)?))
}

/// Collects relative file paths in `dir` up to `limit` entries, skipping hidden
/// directories (`.git`, `.cargo`, `target`, `node_modules`, etc.).
pub fn collect_project_files(dir: &std::path::Path, limit: usize) -> Vec<String> {
    let mut results = Vec::new();
    let skip_dirs: &[&str] = &[
        ".git",
        ".cargo",
        "target",
        "node_modules",
        "__pycache__",
        ".venv",
        "venv",
        "dist",
        "build",
        ".idea",
        ".vscode",
    ];
    collect_files_recursive(dir, dir, &mut results, limit, skip_dirs);
    results
}

fn collect_files_recursive(
    root: &std::path::Path,
    dir: &std::path::Path,
    out: &mut Vec<String>,
    limit: usize,
    skip_dirs: &[&str],
) {
    if out.len() >= limit {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if out.len() >= limit {
            break;
        }
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if path.is_dir() {
            if skip_dirs.contains(&name_str.as_ref()) {
                continue;
            }
            collect_files_recursive(root, &path, out, limit, skip_dirs);
        } else if path.is_file() {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            out.push(rel.display().to_string());
        }
    }
}

/// Scans `workspace` for immediate subdirectories (developer projects) and
/// returns their paths. Directories whose names start with `.` are excluded.
pub fn scan_workspace_projects(workspace: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(workspace) else {
        return Vec::new();
    };
    let skip_dirs = &["target", "node_modules", "dist", "build", ".git", ".antos"];
    let mut projects: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name_str = name.to_string_lossy();
            e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                && !name_str.starts_with('.')
                && !skip_dirs.contains(&name_str.as_ref())
        })
        .map(|e| e.path())
        .collect();
    projects.sort();
    projects
}

/// Heuristically detects the technology stack of a project directory based on its files.
pub fn detect_project_language(dir: &Path) -> String {
    if dir.join("Cargo.toml").exists() {
        "rust".into()
    } else if dir.join("package.json").exists() || dir.join("tsconfig.json").exists() {
        "typescript".into()
    } else if dir.join("pyproject.toml").exists()
        || dir.join("requirements.txt").exists()
        || dir.join("main.py").exists()
    {
        "python".into()
    } else if dir.join("go.mod").exists() {
        "go".into()
    } else {
        "default".into()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_scan_workspace_projects_empty() {
        let tmp = std::env::temp_dir().join(format!("antos_ws_empty_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let projects = scan_workspace_projects(&tmp);
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(
            projects.is_empty(),
            "empty workspace should return no projects"
        );
    }

    #[test]
    fn test_scan_workspace_projects_detects_projects() {
        let tmp = std::env::temp_dir().join(format!("antos_ws_scan_{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("api-service")).unwrap();
        std::fs::create_dir_all(tmp.join("frontend")).unwrap();
        std::fs::create_dir_all(tmp.join(".hidden")).unwrap();
        let projects = scan_workspace_projects(&tmp);
        let _ = std::fs::remove_dir_all(&tmp);
        let names: Vec<String> = projects
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.contains(&"api-service".to_string()),
            "api-service must be detected"
        );
        assert!(
            names.contains(&"frontend".to_string()),
            "frontend must be detected"
        );
        assert!(
            !names.contains(&".hidden".to_string()),
            "hidden dirs must be excluded"
        );
    }

    #[test]
    fn test_collect_project_files_lists_relative_paths() {
        let tmp = std::env::temp_dir().join(format!("antos_collect_{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("src")).unwrap();
        std::fs::create_dir_all(tmp.join("target/debug")).unwrap();
        std::fs::write(tmp.join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(tmp.join("src/main.rs"), "fn main(){}").unwrap();
        std::fs::write(tmp.join("target/debug/binary"), "bin").unwrap();
        let files = collect_project_files(&tmp, 50);
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(
            files.iter().any(|f| f.contains("Cargo.toml")),
            "Cargo.toml must be listed"
        );
        assert!(
            files.iter().any(|f| f.contains("main.rs")),
            "src/main.rs must be listed"
        );
        assert!(
            !files.iter().any(|f| f.contains("target")),
            "target/ must be excluded"
        );
    }

    #[test]
    fn test_collect_project_files_respects_limit() {
        let tmp = std::env::temp_dir().join(format!("antos_limit_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        for i in 0..20 {
            std::fs::write(tmp.join(format!("file{i}.txt")), "x").unwrap();
        }
        let files = collect_project_files(&tmp, 5);
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(
            files.len() <= 5,
            "collect_project_files must not exceed the limit; got {}",
            files.len()
        );
    }

    #[test]
    fn test_detect_project_language() {
        let tmp = std::env::temp_dir().join(format!("antos_lang_detect_{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("rust_p")).unwrap();
        std::fs::write(tmp.join("rust_p/Cargo.toml"), "[package]").unwrap();
        assert_eq!(detect_project_language(&tmp.join("rust_p")), "rust");

        std::fs::create_dir_all(tmp.join("ts_p")).unwrap();
        std::fs::write(tmp.join("ts_p/package.json"), "{}").unwrap();
        assert_eq!(detect_project_language(&tmp.join("ts_p")), "typescript");

        std::fs::create_dir_all(tmp.join("py_p")).unwrap();
        std::fs::write(tmp.join("py_p/pyproject.toml"), "").unwrap();
        assert_eq!(detect_project_language(&tmp.join("py_p")), "python");

        std::fs::create_dir_all(tmp.join("other_p")).unwrap();
        assert_eq!(detect_project_language(&tmp.join("other_p")), "default");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn un_paso_ve_lo_que_decidio_el_anterior() {
        let mut pending = PendingChanges::default();
        let ruta = PathBuf::from("/ws/paquetes.toml");

        assert_eq!(pending.read(&ruta), None, "de partida, manda el disco");

        pending.apply(&Change::Write {
            path: ruta.clone(),
            content: "express".into(),
        });
        assert_eq!(
            pending.read(&ruta).as_deref(),
            Some("express"),
            "el paso siguiente debe ver lo que este escribió, no el disco"
        );
    }

    #[test]
    fn escribir_despues_de_borrar_parte_de_cero() {
        let mut pending = PendingChanges::default();
        let ruta = PathBuf::from("/ws/notas.txt");

        pending.apply(&Change::Delete { path: ruta.clone() });
        assert_eq!(
            pending.read(&ruta).as_deref(),
            Some(""),
            "un fichero borrado por un paso anterior está vacío, no como en el disco"
        );

        pending.apply(&Change::Write {
            path: ruta.clone(),
            content: "nuevo".into(),
        });
        assert_eq!(pending.read(&ruta).as_deref(), Some("nuevo"));
    }
}
