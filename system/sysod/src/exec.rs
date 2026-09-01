//! Traduce pasos en cambios concretos, y aplica esos cambios.
//!
//! `changes_for` es la única fuente de verdad: la previsualización y la
//! ejecución llaman a la MISMA función. Si fueran dos caminos distintos, el
//! diff que apruebas y lo que ocurre podrían divergir — y ese es justo el
//! fallo que hace inaceptable un sistema gobernado por IA.

use crate::blast::expand;
use crate::capability::Capability;
use crate::ctx::Ctx;
use crate::plan::Step;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Serializable porque cruza la frontera de proceso: el broker decide los
/// cambios, el ejecutor confinado los aplica.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "lowercase")]
pub enum Change {
    Write { path: PathBuf, content: String },
    Mkdir { path: PathBuf },
    Delete { path: PathBuf },
    Read { path: PathBuf },
    GitStatus { repo_root: PathBuf },
    GitCommit { repo_root: PathBuf, commit_msg: String },
    GitBranch { repo_root: PathBuf, branch_name: String, base: Option<String> },
    GitWorktreeCreate {
        repo_root: PathBuf,
        target_path: PathBuf,
        branch_name: String,
        base: String,
    },
    GitWorktreeCleanup {
        repo_root: PathBuf,
        target_path: PathBuf,
        force: bool,
    },
    GitWorktreeMerge {
        repo_root: PathBuf,
        branch_name: String,
        target_branch: String,
        message: Option<String>,
    },
    PortStatus {
        port: Option<u16>,
    },
    PortKill {
        port: u16,
        force: bool,
    },
    ServiceUp {
        service: String,
        port: Option<u16>,
        db_name: Option<String>,
        state_dir: PathBuf,
        workspace: PathBuf,
    },
    ServiceDown {
        service: String,
        state_dir: PathBuf,
    },
    ServiceStatus {
        service: Option<String>,
        state_dir: PathBuf,
    },
}

/// Lo que el plan ya ha decidido escribir, antes de haberlo escrito.
///
/// Sin esto, dos pasos que tocan el mismo fichero se pisan: cada uno lo lee
/// del disco tal y como estaba ANTES del plan, y al ejecutar gana el último.
/// Declarar tres dependencias dejaba una.
///
/// Y lo grave no era perder dos líneas: era que el diff aprobado dejaba de
/// describir el resultado. Que lo que ves sea lo que pasa es la propiedad
/// que sostiene todo lo demás.
#[derive(Default)]
pub struct Pendiente {
    escrituras: BTreeMap<PathBuf, String>,
    borrados: std::collections::BTreeSet<PathBuf>,
}

impl Pendiente {
    /// El contenido que tendrá el fichero cuando llegue este paso.
    /// `None` significa «pregúntale al disco».
    pub fn leer(&self, path: &Path) -> Option<String> {
        if self.borrados.contains(path) {
            return Some(String::new());
        }
        self.escrituras.get(path).cloned()
    }

    pub fn aplicar(&mut self, change: &Change) {
        match change {
            Change::Write { path, content } => {
                self.borrados.remove(path);
                self.escrituras.insert(path.clone(), content.clone());
            }
            Change::Delete { path } => {
                self.escrituras.remove(path);
                self.borrados.insert(path.clone());
            }
            Change::Mkdir { .. }
            | Change::Read { .. }
            | Change::GitStatus { .. }
            | Change::GitCommit { .. }
            | Change::GitBranch { .. }
            | Change::GitWorktreeCreate { .. }
            | Change::GitWorktreeCleanup { .. }
            | Change::GitWorktreeMerge { .. }
            | Change::PortStatus { .. }
            | Change::PortKill { .. }
            | Change::ServiceUp { .. }
            | Change::ServiceDown { .. }
            | Change::ServiceStatus { .. } => {}
        }
    }
}

/// Lee un fichero teniendo en cuenta lo que el plan ya ha decidido.
fn leer_con_pendiente(path: &Path, pendiente: &Pendiente) -> String {
    pendiente
        .leer(path)
        .unwrap_or_else(|| std::fs::read_to_string(path).unwrap_or_default())
}

pub fn changes_for(
    step: &Step,
    cap: &Capability,
    ctx: &Ctx,
    pendiente: &Pendiente,
) -> Result<Vec<Change>> {
    let a = &step.args;
    match cap.name.as_str() {
        "fs.read" => Ok(vec![Change::Read { path: abs(ctx, &a["path"]) }]),

        "fs.write" => Ok(vec![Change::Write {
            path: abs(ctx, &a["path"]),
            content: a["content"].clone(),
        }]),

        "fs.delete" => Ok(vec![Change::Delete { path: abs(ctx, &a["path"]) }]),

        "fs.mkdir" => Ok(vec![Change::Mkdir { path: abs(ctx, &a["path"]) }]),

        "project.scaffold" => {
            let root = ctx.workspace.join(&a["name"]);
            Ok(scaffold(&a["language"], &a["name"])
                .into_iter()
                .map(|(rel, content)| Change::Write { path: root.join(rel), content })
                .collect())
        }

        "pkg.declare" => {
            let proj = ctx.workspace.join(&a["project"]);
            let path = if proj.join("syso.packages.toml").exists() {
                proj.join("syso.packages.toml")
            } else {
                proj.join("antos.packages.toml")
            };
            let previo = leer_con_pendiente(&path, pendiente);
            Ok(vec![Change::Write {
                content: declare_package(&previo, &a["package"], &a["version"])?,
                path,
            }])
        }

        "system.declare" => {
            let path = if ctx.system_config.join("syso-paquetes.nix").exists() {
                ctx.system_config.join("syso-paquetes.nix")
            } else {
                ctx.system_config.join("antos-paquetes.nix")
            };
            let previo = leer_con_pendiente(&path, pendiente);
            Ok(vec![Change::Write {
                content: declare_system_package(&previo, &a["package"])?,
                path,
            }])
        }

        "git.status" => {
            let root = a.get("path").map(|p| abs(ctx, p)).unwrap_or_else(|| ctx.workspace.clone());
            Ok(vec![Change::GitStatus { repo_root: root }])
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
            Ok(vec![Change::GitCommit { repo_root: root, commit_msg }])
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
            Ok(vec![Change::GitBranch { repo_root: root, branch_name, base }])
        }

        "git.worktree_create" => {
            let root = a.get("path").map(|p| abs(ctx, p)).unwrap_or_else(|| ctx.workspace.clone());
            let ticket = a.get("ticket_id").cloned().unwrap_or_else(|| "task".into());
            let branch = a.get("branch").cloned().unwrap_or_else(|| format!("agent/{ticket}"));
            let base = a.get("base").cloned().unwrap_or_else(|| "HEAD".into());
            let target_path = ctx.state.join("worktrees").join(&ticket);
            Ok(vec![Change::GitWorktreeCreate {
                repo_root: root,
                target_path,
                branch_name: branch,
                base,
            }])
        }

        "git.worktree_cleanup" => {
            let root = a.get("path").map(|p| abs(ctx, p)).unwrap_or_else(|| ctx.workspace.clone());
            let ticket = a.get("ticket_id").cloned().unwrap_or_else(|| "task".into());
            let force = a.get("force").map(|f| f == "true").unwrap_or(false);
            let target_path = ctx.state.join("worktrees").join(&ticket);
            Ok(vec![Change::GitWorktreeCleanup {
                repo_root: root,
                target_path,
                force,
            }])
        }

        "git.worktree_merge" => {
            let root = a.get("path").map(|p| abs(ctx, p)).unwrap_or_else(|| ctx.workspace.clone());
            let ticket = a.get("ticket_id").cloned().unwrap_or_else(|| "task".into());
            let branch = format!("agent/{ticket}");
            let target = a.get("target").cloned().unwrap_or_else(|| "main".into());
            let message = a.get("message").cloned();
            Ok(vec![Change::GitWorktreeMerge {
                repo_root: root,
                branch_name: branch,
                target_branch: target,
                message,
            }])
        }

        "diag.port_status" => {
            let port = a.get("port").and_then(|p| p.parse::<u16>().ok());
            Ok(vec![Change::PortStatus { port }])
        }

        "diag.port_kill" => {
            let port = a
                .get("port")
                .and_then(|p| p.parse::<u16>().ok())
                .ok_or_else(|| anyhow::anyhow!("debes especificar el puerto a liberar"))?;
            let force = a.get("force").map(|f| f == "true").unwrap_or(false);
            Ok(vec![Change::PortKill { port, force }])
        }

        "env.service_up" => {
            let service = a
                .get("service")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("debes especificar el nombre del servicio (ej. postgres, redis)"))?;
            let port = a.get("port").and_then(|p| p.parse::<u16>().ok());
            let db_name = a.get("db_name").cloned();
            Ok(vec![Change::ServiceUp {
                service,
                port,
                db_name,
                state_dir: ctx.state.clone(),
                workspace: ctx.workspace.clone(),
            }])
        }

        "env.service_down" => {
            let service = a
                .get("service")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("debes especificar el nombre del servicio a detener"))?;
            Ok(vec![Change::ServiceDown {
                service,
                state_dir: ctx.state.clone(),
            }])
        }

        "env.service_status" => {
            let service = a.get("service").cloned();
            Ok(vec![Change::ServiceStatus {
                service,
                state_dir: ctx.state.clone(),
            }])
        }

        other => bail!("no hay implementación para la capacidad «{other}»"),
    }
}

pub fn apply(changes: &[Change]) -> Result<Vec<String>> {
    let mut output = Vec::new();
    for change in changes {
        match change {
            Change::Write { path, content } => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("creando el directorio {}", parent.display()))?;
                }
                std::fs::write(path, content)
                    .with_context(|| format!("escribiendo {}", path.display()))?;
            }
            Change::Mkdir { path } => {
                std::fs::create_dir_all(path)
                    .with_context(|| format!("creando el directorio {}", path.display()))?;
            }
            Change::Delete { path } => {
                if path.is_dir() {
                    std::fs::remove_dir_all(path)?;
                } else if path.exists() {
                    std::fs::remove_file(path)?;
                } else {
                    bail!("no existe: {}", path.display());
                }
            }
            Change::Read { path } => {
                output.push(std::fs::read_to_string(path)?);
            }
            Change::GitStatus { repo_root } => {
                if let Some(status) = crate::git::GitAnalyzer::global().consultar_estado(repo_root)? {
                    let lineas = vec![
                        format!("rama: {}", status.rama.unwrap_or_else(|| "HEAD desacoplado".into())),
                        format!("commits: +{} / -{}", status.delante, status.detras),
                        format!("modificados: {}", status.modificados.len()),
                        format!("staged: {}", status.staged.len()),
                        format!("sin seguimiento: {}", status.sin_seguimiento.len()),
                    ];
                    output.push(lineas.join("\n"));
                } else {
                    output.push("no es un repositorio Git".into());
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
                        output.push("nada que commitear (árbol limpio)".into());
                    } else {
                        bail!("git commit falló: {err}\n{out_str}");
                    }
                } else {
                    let resultado = String::from_utf8_lossy(&commit_out.stdout).trim().to_string();
                    output.push(format!("commit creado: {resultado}"));
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
                output.push(format!("rama activa: {branch_name}"));
            }
            Change::GitWorktreeCreate { repo_root, target_path, branch_name, base } => {
                crate::git::crear_worktree(repo_root, target_path, branch_name, base)?;
                output.push(format!(
                    "worktree creado en: {} (rama: {})",
                    target_path.display(),
                    branch_name
                ));
            }
            Change::GitWorktreeCleanup { repo_root, target_path, force } => {
                crate::git::eliminar_worktree(repo_root, target_path, *force)?;
                output.push(format!("worktree eliminado: {}", target_path.display()));
            }
            Change::GitWorktreeMerge { repo_root, branch_name, target_branch, message } => {
                let res = crate::git::merge_worktree(
                    repo_root,
                    branch_name,
                    target_branch,
                    message.as_deref(),
                )?;
                output.push(format!("merge completado: {res}"));
            }
            Change::PortStatus { port } => {
                let puertos = crate::net::diagnosticar_puertos(*port)?;
                if puertos.is_empty() {
                    if let Some(p) = port {
                        output.push(format!("puerto {p} está libre"));
                    } else {
                        output.push("no hay puertos de desarrollo en escucha".into());
                    }
                } else {
                    let mut lineas = Vec::new();
                    for p in puertos {
                        let dir_info = p
                            .working_dir
                            .as_deref()
                            .map(|d| format!(" (en {d})"))
                            .unwrap_or_default();
                        lineas.push(format!(
                            "puerto {:<5} | PID {:<6} | {:<15} | {}{dir_info}",
                            p.port, p.pid, p.process_name, p.command
                        ));
                    }
                    output.push(lineas.join("\n"));
                }
            }
            Change::PortKill { port, force } => {
                let eliminados = crate::net::liberar_puerto(*port, *force)?;
                if eliminados.is_empty() {
                    output.push(format!("puerto {port} ya estaba libre"));
                } else {
                    let pids: Vec<String> = eliminados
                        .iter()
                        .map(|p| format!("PID {} ({})", p.pid, p.process_name))
                        .collect();
                    output.push(format!("puerto {port} liberado terminando {}", pids.join(", ")));
                }
            }
            Change::ServiceUp {
                service,
                port,
                db_name,
                state_dir,
                workspace,
            } => {
                let info = crate::service::start_service(
                    service,
                    *port,
                    db_name.as_deref(),
                    state_dir,
                    workspace,
                )?;
                output.push(format!(
                    "servicio «{}» arrancado en puerto {} | {}={}",
                    info.name, info.port, info.env_var_key, info.env_var_value
                ));
            }
            Change::ServiceDown {
                service,
                state_dir,
            } => {
                crate::service::stop_service(service, state_dir)?;
                output.push(format!("servicio «{service}» detenido y limpiado"));
            }
            Change::ServiceStatus {
                service,
                state_dir,
            } => {
                let services = crate::service::get_service_status(service.as_deref(), state_dir)?;
                if services.is_empty() {
                    output.push("no hay servicios efímeros aprovisionados".into());
                } else {
                    let mut lines = Vec::new();
                    for s in services {
                        lines.push(format!(
                            "servicio {:<12} | puerto {:<5} | estado {:<8} | {}={}",
                            s.name, s.port, s.status, s.env_var_key, s.env_var_value
                        ));
                    }
                    output.push(lines.join("\n"));
                }
            }
        }
    }
    Ok(output)
}

fn abs(ctx: &Ctx, raw: &str) -> PathBuf {
    let expanded = expand(raw, &BTreeMap::new(), &ctx.workspace);
    let p = PathBuf::from(expanded);
    if p.is_absolute() { p } else { ctx.workspace.join(p) }
}

/// Los ficheros que produce un proyecto nuevo, por lenguaje.
fn scaffold(language: &str, name: &str) -> Vec<(&'static str, String)> {
    match language {
        "rust" => vec![
            ("Cargo.toml", format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n"
            )),
            ("src/main.rs", format!(
                "fn main() {{\n    println!(\"{name} en marcha\");\n}}\n"
            )),
        ],
        "typescript" => vec![
            ("package.json", format!(
                "{{\n  \"name\": \"{name}\",\n  \"version\": \"0.1.0\",\n  \"type\": \"module\",\n  \"scripts\": {{\n    \"start\": \"node --experimental-strip-types src/index.ts\"\n  }}\n}}\n"
            )),
            ("tsconfig.json",
                "{\n  \"compilerOptions\": {\n    \"target\": \"es2022\",\n    \"module\": \"esnext\",\n    \"moduleResolution\": \"bundler\",\n    \"strict\": true\n  }\n}\n".to_string()),
            ("src/index.ts", format!("console.log(\"{name} en marcha\");\n")),
        ],
        "python" => vec![
            ("pyproject.toml", format!(
                "[project]\nname = \"{name}\"\nversion = \"0.1.0\"\nrequires-python = \">=3.11\"\ndependencies = []\n"
            )),
            ("main.py", format!("def main() -> None:\n    print(\"{name} en marcha\")\n\n\nif __name__ == \"__main__\":\n    main()\n")),
        ],
        _ => Vec::new(),
    }
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

/// Devuelve el fichero Nix COMPLETO tras añadir el paquete.
///
/// Se lee lo que hay y se vuelve a escribir entero, en vez de aplicar un
/// parche. Es lo que permite fotografiarlo, previsualizarlo y revertirlo con
/// el mismo código que cualquier otro fichero.
fn declare_system_package(previo: &str, package: &str) -> Result<String> {
    let mut packages: BTreeMap<String, ()> = BTreeMap::new();

    {
        let existing = previo;
        // Un análisis por líneas basta porque este fichero lo genera antOS.
        // Si alguien lo reescribe con Nix de verdad, lo peor que pasa es que
        // no reconozcamos sus paquetes — y eso se ve en el diff antes de
        // aprobar nada.
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

/// Devuelve el contenido COMPLETO que tendría el fichero de declaraciones
/// tras añadir el paquete. Devolver el fichero entero (y no un parche) es lo
/// que permite fotografiarlo y previsualizarlo con el mismo código.
fn declare_package(previo: &str, package: &str, version: &str) -> Result<String> {
    let mut file: PackagesFile = toml::from_str(previo).unwrap_or_default();
    file.packages.insert(package.to_string(), version.to_string());
    Ok(format!("{PACKAGES_HEADER}\n{}", toml::to_string(&file)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_paso_ve_lo_que_decidio_el_anterior() {
        let mut pendiente = Pendiente::default();
        let ruta = PathBuf::from("/ws/paquetes.toml");

        assert_eq!(pendiente.leer(&ruta), None, "de partida, manda el disco");

        pendiente.aplicar(&Change::Write {
            path: ruta.clone(),
            content: "express".into(),
        });
        assert_eq!(
            pendiente.leer(&ruta).as_deref(),
            Some("express"),
            "el paso siguiente debe ver lo que este escribió, no el disco"
        );
    }

    #[test]
    fn escribir_despues_de_borrar_parte_de_cero() {
        let mut pendiente = Pendiente::default();
        let ruta = PathBuf::from("/ws/notas.txt");

        pendiente.aplicar(&Change::Delete { path: ruta.clone() });
        assert_eq!(
            pendiente.leer(&ruta).as_deref(),
            Some(""),
            "un fichero borrado por un paso anterior está vacío, no como en el disco"
        );

        pendiente.aplicar(&Change::Write {
            path: ruta.clone(),
            content: "nuevo".into(),
        });
        assert_eq!(pendiente.leer(&ruta).as_deref(), Some("nuevo"));
    }

    #[test]
    fn test_changes_for_capacidades_git() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = crate::capability::Catalog::load(&ctx.caps_dir).expect("catalog");
        let pendiente = Pendiente::default();

        // 1. git.status
        let step_status = Step {
            capability: "git.status".into(),
            args: BTreeMap::new(),
        };
        let cap_status = catalog.get("git.status").expect("cap git.status");
        let changes_status = changes_for(&step_status, cap_status, &ctx, &pendiente).expect("changes");
        assert_eq!(changes_status.len(), 1);
        match &changes_status[0] {
            Change::GitStatus { repo_root } => assert_eq!(repo_root, &ctx.workspace),
            _ => panic!("debe ser GitStatus"),
        }

        // 2. git.commit_semantic
        let mut args_commit = BTreeMap::new();
        args_commit.insert("type".into(), "feat".into());
        args_commit.insert("scope".into(), "auth".into());
        args_commit.insert("message".into(), "soporte de tokens JWT".into());
        let step_commit = Step {
            capability: "git.commit_semantic".into(),
            args: args_commit,
        };
        let cap_commit = catalog.get("git.commit_semantic").expect("cap git.commit_semantic");
        let changes_commit = changes_for(&step_commit, cap_commit, &ctx, &pendiente).expect("changes");
        assert_eq!(changes_commit.len(), 1);
        match &changes_commit[0] {
            Change::GitCommit { commit_msg, .. } => {
                assert_eq!(commit_msg, "feat(auth): soporte de tokens JWT");
            }
            _ => panic!("debe ser GitCommit"),
        }

        // 3. git.smart_branch
        let mut args_branch = BTreeMap::new();
        args_branch.insert("name".into(), "login-oauth".into());
        args_branch.insert("ticket_id".into(), "T2.1".into());
        let step_branch = Step {
            capability: "git.smart_branch".into(),
            args: args_branch,
        };
        let cap_branch = catalog.get("git.smart_branch").expect("cap git.smart_branch");
        let changes_branch = changes_for(&step_branch, cap_branch, &ctx, &pendiente).expect("changes");
        assert_eq!(changes_branch.len(), 1);
        match &changes_branch[0] {
            Change::GitBranch { branch_name, .. } => {
                assert_eq!(branch_name, "t2.1/login-oauth");
            }
            _ => panic!("debe ser GitBranch"),
        }

        // 4. git.worktree_create
        let mut args_wt_create = BTreeMap::new();
        args_wt_create.insert("ticket_id".into(), "T2.2".into());
        let step_wt_create = Step {
            capability: "git.worktree_create".into(),
            args: args_wt_create,
        };
        let cap_wt_create = catalog.get("git.worktree_create").expect("cap git.worktree_create");
        let changes_wt_create = changes_for(&step_wt_create, cap_wt_create, &ctx, &pendiente).expect("changes");
        assert_eq!(changes_wt_create.len(), 1);
        match &changes_wt_create[0] {
            Change::GitWorktreeCreate { branch_name, target_path, .. } => {
                assert_eq!(branch_name, "agent/T2.2");
                assert_eq!(target_path, &ctx.state.join("worktrees/T2.2"));
            }
            _ => panic!("debe ser GitWorktreeCreate"),
        }

        // 5. git.worktree_cleanup
        let mut args_wt_clean = BTreeMap::new();
        args_wt_clean.insert("ticket_id".into(), "T2.2".into());
        let step_wt_clean = Step {
            capability: "git.worktree_cleanup".into(),
            args: args_wt_clean,
        };
        let cap_wt_clean = catalog.get("git.worktree_cleanup").expect("cap git.worktree_cleanup");
        let changes_wt_clean = changes_for(&step_wt_clean, cap_wt_clean, &ctx, &pendiente).expect("changes");
        assert_eq!(changes_wt_clean.len(), 1);
        match &changes_wt_clean[0] {
            Change::GitWorktreeCleanup { target_path, .. } => {
                assert_eq!(target_path, &ctx.state.join("worktrees/T2.2"));
            }
            _ => panic!("debe ser GitWorktreeCleanup"),
        }

        // 6. diag.port_status
        let mut args_port_st = BTreeMap::new();
        args_port_st.insert("port".into(), "3000".into());
        let step_port_st = Step {
            capability: "diag.port_status".into(),
            args: args_port_st,
        };
        let cap_port_st = catalog.get("diag.port_status").expect("cap diag.port_status");
        let changes_port_st = changes_for(&step_port_st, cap_port_st, &ctx, &pendiente).expect("changes");
        assert_eq!(changes_port_st.len(), 1);
        match &changes_port_st[0] {
            Change::PortStatus { port } => assert_eq!(*port, Some(3000)),
            _ => panic!("debe ser PortStatus"),
        }

        // 7. diag.port_kill
        let mut args_port_kill = BTreeMap::new();
        args_port_kill.insert("port".into(), "8080".into());
        args_port_kill.insert("force".into(), "true".into());
        let step_port_kill = Step {
            capability: "diag.port_kill".into(),
            args: args_port_kill,
        };
        let cap_port_kill = catalog.get("diag.port_kill").expect("cap diag.port_kill");
        let changes_port_kill = changes_for(&step_port_kill, cap_port_kill, &ctx, &pendiente).expect("changes");
        assert_eq!(changes_port_kill.len(), 1);
        match &changes_port_kill[0] {
            Change::PortKill { port, force } => {
                assert_eq!(*port, 8080);
                assert!(*force);
            }
            _ => panic!("debe ser PortKill"),
        }
    }
}
