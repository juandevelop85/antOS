//! Planificador local determinista: reglas de palabras clave, sin modelo.
//!
//! Existe por dos razones. Una, poder ejercitar el recorrido completo sin
//! clave de API ni latencia de red. Dos, y más importante: obliga a que el
//! resto del sistema no dependa de que el planificador sea listo. Si el
//! aislamiento y el deshacer solo funcionan cuando el modelo acierta, no
//! funcionan.

use super::{Planner, Propuesta};
use crate::capability::Catalog;
use crate::plan::Step;
use anyhow::{bail, Result};
use std::collections::BTreeMap;

pub struct LocalPlanner;

impl Planner for LocalPlanner {
    fn name(&self) -> &'static str {
        "local"
    }

    fn plan(&self, intent: &str, _catalog: &Catalog) -> Result<Propuesta> {
        let lower = intent.to_lowercase();
        // El punto se conserva porque forma parte de nombres de fichero
        // (`main.rs`), pero un punto FINAL es puntuación de frase. Sin
        // quitarlo, dictar "crea un proyecto llamado demo." crea un
        // directorio que se llama literalmente `demo.`.
        let words: Vec<String> = lower
            .split_whitespace()
            .map(|w| {
                w.trim_matches(|c: char| {
                    !c.is_alphanumeric() && c != '.' && c != '/' && c != '-' && c != '_'
                })
                .trim_end_matches('.')
                .to_string()
            })
            .collect();

        if lower.contains("proyecto") && (lower.contains("cre") || lower.contains("nuev")) {
            let language = if lower.contains("typescript") || lower.contains(" ts ") {
                "typescript"
            } else if lower.contains("python") || lower.contains(" py ") {
                "python"
            } else {
                "rust"
            };
            let name = after(&words, &["llamado", "llamada", "nombre"])
                .unwrap_or_else(|| words.last().cloned().unwrap_or_default());
            return Ok(Propuesta::solo(vec![step("project.scaffold", &[("language", language), ("name", &name)])]));
        }

        // El sistema se comprueba ANTES que el proyecto: "declara htop en el
        // sistema" y "declara serde en el proyecto demo" empiezan igual.
        if lower.contains("sistema")
            && (lower.contains("declar")
                || lower.contains("paquete")
                || lower.contains("instal")
                || lower.contains("añad"))
        {
            let package = after(&words, &["declara", "instala", "añade", "paquete"])
                .ok_or_else(|| anyhow::anyhow!("no veo qué paquete quieres declarar"))?;
            return Ok(Propuesta::solo(vec![step("system.declare", &[("package", &package)])]));
        }

        if lower.contains("depend") || lower.contains("paquete") {
            let package = after(&words, &["dependencia", "dependencias", "paquete"])
                .ok_or_else(|| anyhow::anyhow!("no veo qué paquete quieres declarar"))?;
            let project = after(&words, &["proyecto"])
                .ok_or_else(|| anyhow::anyhow!("no veo en qué proyecto declararlo"))?;
            let version = after(&words, &["version", "versión", "v"]).unwrap_or_else(|| "*".into());
            return Ok(Propuesta::solo(vec![step(
                "pkg.declare",
                &[("project", &project), ("package", &package), ("version", &version)],
            )]));
        }

        if lower.contains("borra") || lower.contains("elimin") {
            let path = words.last().cloned().unwrap_or_default();
            return Ok(Propuesta::solo(vec![step("fs.delete", &[("path", &path)])]));
        }

        if (lower.contains("lee") || lower.contains("muestra") || lower.contains("enseña"))
            && !lower.contains("grafo")
            && !lower.contains("memoria")
            && !lower.contains("cuota")
            && !lower.contains("limite")
            && !lower.contains("límite")
            && !lower.contains("diff")
            && !lower.contains("terminal")
            && !lower.contains("notifica")
            && !lower.contains("alerta")
            && !lower.contains("bandeja")
            && !lower.contains("mesh")
            && !lower.contains("peer")
            && !lower.contains("p2p")
            && !lower.contains("malla")
            && !lower.contains("swarm")
            && !lower.contains("enjambre")
            && !lower.contains("vfs")
            && !lower.contains("antfs")
            && !lower.contains("ebpf")
            && !lower.contains("bpf")
        {
            let path = words.last().cloned().unwrap_or_default();
            return Ok(Propuesta::solo(vec![step("fs.read", &[("path", &path)])]));
        }

        if lower.contains("escribe") {
            let path = after(&words, &["en", "fichero", "archivo"])
                .ok_or_else(|| anyhow::anyhow!("no veo en qué fichero escribir"))?;
            let content = intent.split_once(':').map(|(_, c)| c.trim().to_string()).unwrap_or_default();

            return Ok(Propuesta::solo(vec![step("fs.write", &[("path", &path), ("content", &content)])]));
        }

        // Intenciones Git semánticas (T2.1)
        if lower.contains("commit") {
            let tipo = if lower.contains("fix") || lower.contains("arregl") || lower.contains("corrige") {
                "fix"
            } else if lower.contains("doc") {
                "docs"
            } else if lower.contains("refactor") {
                "refactor"
            } else if lower.contains("test") || lower.contains("prueba") {
                "test"
            } else {
                "feat"
            };

            let scope = if lower.contains("auth") || lower.contains("autenticacion") || lower.contains("autenticación") {
                Some("auth")
            } else if lower.contains("ipc") || lower.contains("protocolo") {
                Some("protocolo")
            } else if lower.contains("ui") || lower.contains("barra") {
                Some("barra")
            } else {
                None
            };

            // Extraer mensaje descriptivo
            let msg = if let Some((_, resto)) = intent.split_once("commit") {
                let m = resto.trim_start_matches(|c: char| c == ':' || c == '-' || c.is_whitespace()).trim();
                let m = m.trim_start_matches("con ").trim_start_matches("de ").trim();
                if m.is_empty() { intent.trim() } else { m }
            } else {
                intent.trim()
            };

            let mut params = vec![("type", tipo), ("message", msg)];
            if let Some(s) = scope {
                params.push(("scope", s));
            }

            return Ok(Propuesta::solo(vec![step("git.commit_semantic", &params)]));
        }

        if lower.contains("rama") || lower.contains("branch") {
            let name = after(&words, &["rama", "branch", "llamada", "nombre"])
                .unwrap_or_else(|| words.last().cloned().unwrap_or_else(|| "feature".into()));
            return Ok(Propuesta::solo(vec![step("git.smart_branch", &[("name", &name)])]));
        }

        if lower.contains("estado") && (lower.contains("git") || lower.contains("repo")) || lower == "git status" {
            return Ok(Propuesta::solo(vec![step("git.status", &[])]));
        }

        if lower.contains("worktree") {
            let ticket = after(&words, &["ticket", "para", "de"])
                .or_else(|| after(&words, &["worktree"]))
                .unwrap_or_else(|| "task".into());

            if lower.contains("limpia") || lower.contains("borra") || lower.contains("elimin") {
                return Ok(Propuesta::solo(vec![step(
                    "git.worktree_cleanup",
                    &[("ticket_id", &ticket)],
                )]));
            }

            if lower.contains("merge") || lower.contains("fusiona") || lower.contains("integra") {
                let target = after(&words, &["en", "a", "hacia"]).unwrap_or_else(|| "main".into());
                return Ok(Propuesta::solo(vec![step(
                    "git.worktree_merge",
                    &[("ticket_id", &ticket), ("target", &target)],
                )]));
            }

            return Ok(Propuesta::solo(vec![step(
                "git.worktree_create",
                &[("ticket_id", &ticket)],
            )]));
        }

        // Intenciones de servicios efímeros (T5.1)
        if lower.contains("servicio")
            || lower.contains("postgres")
            || lower.contains("postgresql")
            || lower.contains("redis")
            || lower.contains("mariadb")
            || lower.contains("mysql")
            || lower.contains("meilisearch")
            || lower.contains("rabbitmq")
            || lower.contains("base de datos")
            || words.iter().any(|w| w == "db")
        {
            let svc = if lower.contains("redis") {
                "redis"
            } else if lower.contains("mariadb") || lower.contains("mysql") {
                "mariadb"
            } else if lower.contains("meilisearch") {
                "meilisearch"
            } else if lower.contains("rabbitmq") {
                "rabbitmq"
            } else {
                "postgres"
            };

            if lower.contains("apaga")
                || lower.contains("deten")
                || lower.contains("detén")
                || lower.contains("stop")
                || lower.contains("down")
                || lower.contains("parar")
            {
                return Ok(Propuesta::solo(vec![step(
                    "env.service_down",
                    &[("service", svc)],
                )]));
            }

            if lower.contains("estado")
                || lower.contains("status")
                || lower.contains("lista")
                || lower.contains("info")
            {
                return Ok(Propuesta::solo(vec![step(
                    "env.service_status",
                    &[("service", svc)],
                )]));
            }

            let mut args = vec![("service", svc)];
            let port_str = words.iter().find_map(|w| {
                if let Ok(p) = w.parse::<u16>() {
                    if p > 1000 {
                        return Some(w.as_str());
                    }
                }
                None
            });
            if let Some(p) = port_str {
                args.push(("port", p));
            }

            return Ok(Propuesta::solo(vec![step("env.service_up", &args)]));
        }

        if lower.contains("puerto") || lower.contains("port") {
            let port_num = words.iter().find_map(|w| {
                let digitos: String = w.chars().filter(|c| c.is_ascii_digit()).collect();
                if !digitos.is_empty() {
                    Some(digitos)
                } else {
                    None
                }
            });

            if lower.contains("libera")
                || lower.contains("mata")
                || lower.contains("cierra")
                || lower.contains("kill")
            {
                let port =
                    port_num.ok_or_else(|| anyhow::anyhow!("no veo qué puerto quieres liberar"))?;
                return Ok(Propuesta::solo(vec![step(
                    "diag.port_kill",
                    &[("port", &port)],
                )]));
            }

            let mut params = Vec::new();
            if let Some(ref p) = port_num {
                params.push(("port", p.as_str()));
            }
            return Ok(Propuesta::solo(vec![step("diag.port_status", &params)]));
        }

        // Intenciones de perfiles de entorno y flakes (T7.1)
        if lower.contains("entorno") || lower.contains("toolchain") || lower.contains("devbox") || lower.contains("flake") || lower.contains("perfil") {
            if lower.contains("init") || lower.contains("configura") || lower.contains("crea") || lower.contains("inicializa") {
                let prof = words.iter().find_map(|w| {
                    match w.as_str() {
                        "rust" | "node" | "python" | "go" | "base" => Some(w.as_str()),
                        _ => None,
                    }
                }).unwrap_or("rust");
                return Ok(Propuesta::solo(vec![step("env.init", &[("profile", prof)])]));
            }

            if lower.contains("sync") || lower.contains("sincroniza") || lower.contains("verifica") {
                return Ok(Propuesta::solo(vec![step("env.sync", &[])]));
            }

            return Ok(Propuesta::solo(vec![step("env.profile_status", &[])]));
        }

        // Intenciones de cuotas y límites de recursos para sandboxes (T7.2)
        if lower.contains("cuota") || lower.contains("cuotas") || lower.contains("límite") || lower.contains("limite") || lower.contains("timeout") {
            if lower.contains("establece") || lower.contains("configura") || lower.contains("pon") || lower.contains("cambia") || lower.contains("limita") {
                let mut args = Vec::new();
                for (i, w) in words.iter().enumerate() {
                    if (w.contains("timeout") || w.contains("tiempo")) && words.len() > i + 1 {
                        let digits: String = words[i + 1].chars().filter(|c| c.is_ascii_digit()).collect();
                        if !digits.is_empty() {
                            args.push(("timeout", digits));
                        }
                    }
                    if (w.contains("memoria") || w.contains("ram")) && words.len() > i + 1 {
                        let digits: String = words[i + 1].chars().filter(|c| c.is_ascii_digit()).collect();
                        if !digits.is_empty() {
                            args.push(("memory", digits));
                        }
                    }
                    if w.contains("cpu") && words.len() > i + 1 {
                        let digits: String = words[i + 1].chars().filter(|c| c.is_ascii_digit()).collect();
                        if !digits.is_empty() {
                            args.push(("cpu", digits));
                        }
                    }
                }
                let arg_refs: Vec<(&str, &str)> = args.iter().map(|(k, v)| (*k, v.as_str())).collect();
                return Ok(Propuesta::solo(vec![step("quota.set", &arg_refs)]));
            }

            return Ok(Propuesta::solo(vec![step("quota.status", &[])]));
        }

        // Intenciones de visor de diffs y terminal interactiva (T8.1)
        if lower.contains("diff") || lower.contains("diferencias") || lower.contains("parche") {
            let target = words.iter().find(|w| w.starts_with('t') && w.chars().nth(1).map(|c| c.is_numeric()).unwrap_or(false))
                .or_else(|| words.iter().find(|w| w.starts_with("head")))
                .cloned();
            let mut params = Vec::new();
            if let Some(ref t) = target {
                params.push(("target", t.as_str()));
            }
            return Ok(Propuesta::solo(vec![step("ui.diff_viewer", &params)]));
        }

        if lower.contains("terminal") || lower.contains("consola") || lower.contains("vte") {
            return Ok(Propuesta::solo(vec![step("ui.terminal", &[])]));
        }

        // Intenciones de bandeja de notificaciones y aprobaciones asíncronas (T8.2)
        if lower.contains("notifica") || lower.contains("alerta") || lower.contains("bandeja") {
            if lower.contains("aprueba") || lower.contains("approve") {
                let id = words.iter().find(|w| w.starts_with("notif-")).cloned().unwrap_or_else(|| "notif-1".into());
                return Ok(Propuesta::solo(vec![step("notify.action", &[("id", &id), ("action", "approve")])]));
            }
            if lower.contains("rechaza") || lower.contains("reject") || lower.contains("rollback") {
                let id = words.iter().find(|w| w.starts_with("notif-")).cloned().unwrap_or_else(|| "notif-1".into());
                return Ok(Propuesta::solo(vec![step("notify.action", &[("id", &id), ("action", "reject")])]));
            }
            return Ok(Propuesta::solo(vec![step("notify.list", &[])]));
        }

        // Intenciones de red P2P y antMesh (T9.1)
        if lower.contains("mesh") || lower.contains("p2p") || lower.contains("peer") || lower.contains("malla") || lower.contains("empareja") {
            if lower.contains("conecta") || lower.contains("connect") || lower.contains("une") || lower.contains("unir") {
                let addr = words.iter().find(|w| w.contains(':') || (w.contains('.') && w.chars().any(|c| c.is_ascii_digit())))
                    .cloned()
                    .unwrap_or_else(|| "127.0.0.1:9042".into());
                return Ok(Propuesta::solo(vec![step("mesh.connect", &[("address", &addr)])]));
            }
            if lower.contains("pair") || lower.contains("token") || lower.contains("empareja") || lower.contains("invita") {
                return Ok(Propuesta::solo(vec![step("mesh.pair", &[])]));
            }
            return Ok(Propuesta::solo(vec![step("mesh.status", &[])]));
        }

        // Intenciones de Swarm y despacho distribuido de agentes (T9.2)
        if lower.contains("swarm") || lower.contains("enjambre") || lower.contains("despacha") || lower.contains("dispatch") || (lower.contains("agente") && (lower.contains("distribu") || lower.contains("remoto"))) {
            if lower.contains("despacha") || lower.contains("dispatch") || lower.contains("envia") || lower.contains("asigna") {
                let ticket_id = words.iter().find(|w| w.starts_with('t') && w.chars().nth(1).map(|c| c.is_numeric()).unwrap_or(false))
                    .cloned()
                    .unwrap_or_else(|| "T1.1".into())
                    .to_uppercase();

                let role = if lower.contains("qa") || lower.contains("test") {
                    "qa"
                } else if lower.contains("auditor") {
                    "auditor"
                } else {
                    "coder"
                };

                let node_target = after(&words, &["nodo", "node"]);
                let mut args = vec![("ticket_id", ticket_id.as_str()), ("role", role)];
                if let Some(ref n) = node_target {
                    args.push(("node", n.as_str()));
                }
                return Ok(Propuesta::solo(vec![step("flow.dispatch_remote", &args)]));
            }
            return Ok(Propuesta::solo(vec![step("flow.swarm_status", &[])]));
        }

        // Intenciones de Sistema de Ficheros Virtual Semántico /antfs (T10.1)
        if lower.contains("vfs") || lower.contains("antfs") || (lower.contains("sistema") && lower.contains("virtual")) {
            if lower.contains("desmonta") || lower.contains("unmount") || lower.contains("umount") {
                let mnt = after(&words, &["en", "de", "ruta", "mount"]);
                let mut args = Vec::new();
                if let Some(ref m) = mnt {
                    args.push(("mount_point", m.as_str()));
                }
                return Ok(Propuesta::solo(vec![step("vfs.unmount", &args)]));
            }
            if lower.contains("monta") || lower.contains("mount") {
                let mnt = after(&words, &["en", "a", "ruta", "mount"]);
                let mut args = Vec::new();
                if let Some(ref m) = mnt {
                    args.push(("mount_point", m.as_str()));
                }
                return Ok(Propuesta::solo(vec![step("vfs.mount", &args)]));
            }
            if lower.contains("valida") || lower.contains("validate") || lower.contains("check") || lower.contains("sintaxis") {
                let file = after(&words, &["archivo", "fichero", "de", "file", "valida"]).unwrap_or_else(|| "src/main.rs".into());
                return Ok(Propuesta::solo(vec![step("vfs.validate_write", &[("file_path", &file)])]));
            }
            if lower.contains("guard") || lower.contains("interceptor") || lower.contains("guardia") {
                return Ok(Propuesta::solo(vec![step("vfs.guard_status", &[])]));
            }
            let path = words.iter().find(|w| w.starts_with("/antfs") || w.starts_with("symbols/") || w.starts_with("/symbols")).cloned();
            let mut args = Vec::new();
            if let Some(ref p) = path {
                args.push(("path", p.as_str()));
            }
            return Ok(Propuesta::solo(vec![step("vfs.query", &args)]));
        }

        // Intenciones de Supervisor Kernel eBPF LSM (T11.1)
        if lower.contains("ebpf") || lower.contains("bpf") || (lower.contains("syscall") && lower.contains("kernel")) {
            if lower.contains("audit") || lower.contains("registro") || lower.contains("traza") || lower.contains("log") {
                let limit = after(&words, &["ultimos", "últimos", "limite", "limit", "de"]).unwrap_or_else(|| "20".into());
                let pid = after(&words, &["pid", "proceso"]);
                let mut args = vec![("limit", limit.as_str())];
                if let Some(ref p) = pid {
                    args.push(("pid", p.as_str()));
                }
                return Ok(Propuesta::solo(vec![step("ebpf.audit_log", &args)]));
            }
            return Ok(Propuesta::solo(vec![step("ebpf.status", &[])]));
        }

        // Intenciones de secretos y concesiones (T5.2)
        if (!lower.contains("busca") && !lower.contains("search"))
            && (lower.contains("secreto")
                || lower.contains("secret")
                || lower.contains("concede")
                || lower.contains("grant")
                || lower.contains("revoca")
                || lower.contains("revoke")
                || lower.contains("bóveda")
                || lower.contains("boveda"))
        {
            if lower.contains("revoca") || lower.contains("revoke") {
                let sec = after(&words, &["revoca", "revoke", "a", "de", "secreto"]).unwrap_or_else(|| "secret.env".into());
                let clean_sec = if sec.starts_with("secret.") { sec } else { format!("secret.{sec}") };
                return Ok(Propuesta::solo(vec![step(
                    "secret.revoke",
                    &[("secret", &clean_sec)],
                )]));
            }

            if lower.contains("concede") || lower.contains("grant") || lower.contains("permite") {
                let sec = after(&words, &["concede", "grant", "a", "para", "secreto"]).unwrap_or_else(|| "secret.env".into());
                let clean_sec = if sec.starts_with("secret.") { sec } else { format!("secret.{sec}") };
                return Ok(Propuesta::solo(vec![step(
                    "secret.grant",
                    &[("secret", &clean_sec), ("minutes", "10"), ("reason", "intención local")],
                )]));
            }

            if lower.contains("guarda") || lower.contains("set") || lower.contains("almacena") {
                let key = after(&words, &["guarda", "secreto", "clave", "set"]).unwrap_or_else(|| "API_KEY".into());
                let val = after(&words, &["valor", "val", "con"]).unwrap_or_else(|| "dummy_val".into());
                return Ok(Propuesta::solo(vec![step(
                    "secret.set",
                    &[("key", &key), ("value", &val)],
                )]));
            }

            return Ok(Propuesta::solo(vec![step("secret.list", &[])]));
        }

        // Intenciones de tickets y especificaciones
        if (lower.contains("ticket") || lower.contains("especificación") || lower.contains("especificacion"))
            && !lower.contains("grafo")
            && !lower.contains("despacha")
            && !lower.contains("dispatch")
            && !lower.contains("swarm")
        {
            if lower.contains("crea") || lower.contains("nuevo") || lower.contains("agrega") || lower.contains("new") {
                let id_cand = words.iter().find(|w| w.starts_with('t') && w.chars().nth(1).map(|c| c.is_numeric()).unwrap_or(false))
                    .cloned()
                    .unwrap_or_else(|| "T6.1".into())
                    .to_uppercase();
                
                // Extraer título después de "para" o "de" o "llamado"
                let title = if let Some(pos) = words.iter().position(|w| w == "para" || w == "de" || w == "llamado" || w == "titulado") {
                    words[pos + 1..].join(" ")
                } else {
                    format!("Nueva funcionalidad {id_cand}")
                };

                let phase = if id_cand.starts_with('T') && id_cand.contains('.') {
                    let p_num = id_cand.strip_prefix('T').and_then(|r| r.split('.').next()).unwrap_or("1");
                    format!("Fase {p_num}")
                } else {
                    "Fase Activa".to_string()
                };

                return Ok(Propuesta::solo(vec![step(
                    "spec.create_ticket",
                    &[
                        ("ticket_id", &id_cand),
                        ("title", &title),
                        ("phase", &phase),
                        ("description", &format!("Requerimiento técnico para {title}")),
                    ],
                )]));
            }

            if lower.contains("actualiza") || lower.contains("marca") || lower.contains("status") {
                let id_cand = words.iter().find(|w| w.starts_with('t') && w.chars().nth(1).map(|c| c.is_numeric()).unwrap_or(false))
                    .cloned()
                    .unwrap_or_else(|| "T1.1".into())
                    .to_uppercase();
                let status = if lower.contains("completado") || lower.contains("hecho") || lower.contains("done") {
                    "completado"
                } else if lower.contains("progreso") {
                    "progreso"
                } else if lower.contains("revisión") || lower.contains("revision") {
                    "revision"
                } else {
                    "pendiente"
                };

                return Ok(Propuesta::solo(vec![step(
                    "spec.update_ticket",
                    &[("ticket_id", &id_cand), ("status", status)],
                )]));
            }

            return Ok(Propuesta::solo(vec![step("spec.list_tickets", &[])]));
        }

        // Intenciones de memoria semántica y grafo de contexto
        if lower.contains("memoria") || lower.contains("grafo") || (lower.contains("busca") && (lower.contains("codigo") || lower.contains("código") || lower.contains("semántica") || lower.contains("semantica"))) {
            if lower.contains("indexa") || lower.contains("actualiza") || lower.contains("reindexa") {
                return Ok(Propuesta::solo(vec![step("memory.index", &[])]));
            }
            if lower.contains("grafo") {
                let target = after(&words, &["para", "de", "sobre", "grafo"]);
                let args = if let Some(ref t) = target {
                    vec![("target", t.as_str())]
                } else {
                    vec![]
                };
                return Ok(Propuesta::solo(vec![step("memory.graph", &args)]));
            }
            // Búsqueda semántica
            let query = if let Some(pos) = words.iter().position(|w| w == "sobre" || w == "de" || w == "para" || w == "busca") {
                words[pos + 1..].join(" ")
            } else {
                words.join(" ")
            };
            return Ok(Propuesta::solo(vec![step(
                "memory.search",
                &[("query", &query), ("limit", "5")],
            )]));
        }

        bail!(
            "el planificador local no sabe traducir esa intención.\n\
             Entiende: crear proyectos, declarar dependencias, leer, escribir, borrar, commits semánticos, ramas, worktrees Git, puertos de red, servicios efímeros (postgres, redis), secretos/concesiones, tickets y memoria semántica/grafo.\n\
             Para lenguaje libre usa: antos --planificador ollama \"…\" o antos --planificador claude \"…\""
        )
    }
}

/// Devuelve la palabra siguiente a la primera de `keys` que aparezca.
fn after(words: &[String], keys: &[&str]) -> Option<String> {
    for (i, w) in words.iter().enumerate() {
        if keys.contains(&w.as_str()) {
            if let Some(next) = words.get(i + 1) {
                if !next.is_empty() {
                    return Some(next.clone());
                }
            }
        }
    }
    None
}

fn step(capability: &str, args: &[(&str, &str)]) -> Step {
    Step {
        capability: capability.to_string(),
        args: args
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<BTreeMap<_, _>>(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::Ctx;

    #[test]
    fn test_plan_commit_semantico_auth() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let propuesta = planner
            .plan("haz commit con los cambios de auth", &catalog)
            .expect("debe planificar");

        assert_eq!(propuesta.steps.len(), 1);
        let s = &propuesta.steps[0];
        assert_eq!(s.capability, "git.commit_semantic");
        assert_eq!(s.args.get("type").map(String::as_str), Some("feat"));
        assert_eq!(s.args.get("scope").map(String::as_str), Some("auth"));
    }

    #[test]
    fn test_plan_crear_rama() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let propuesta = planner
            .plan("crea rama login-flow", &catalog)
            .expect("debe planificar");

        assert_eq!(propuesta.steps.len(), 1);
        let s = &propuesta.steps[0];
        assert_eq!(s.capability, "git.smart_branch");
        assert_eq!(s.args.get("name").map(String::as_str), Some("login-flow"));
    }

    #[test]
    fn test_plan_worktree_crear_y_limpiar() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_crear = planner
            .plan("crea worktree para T3.1", &catalog)
            .expect("debe planificar");
        assert_eq!(p_crear.steps.len(), 1);
        assert_eq!(p_crear.steps[0].capability, "git.worktree_create");
        assert_eq!(p_crear.steps[0].args.get("ticket_id").map(String::as_str), Some("t3.1"));

        let p_limpiar = planner
            .plan("limpia worktree de T3.1", &catalog)
            .expect("debe planificar");
        assert_eq!(p_limpiar.steps.len(), 1);
        assert_eq!(p_limpiar.steps[0].capability, "git.worktree_cleanup");
        assert_eq!(p_limpiar.steps[0].args.get("ticket_id").map(String::as_str), Some("t3.1"));
    }

    #[test]
    fn test_plan_puerto_diagnostico_y_liberacion() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_libera = planner
            .plan("libera el puerto 3000", &catalog)
            .expect("debe planificar");
        assert_eq!(p_libera.steps.len(), 1);
        assert_eq!(p_libera.steps[0].capability, "diag.port_kill");
        assert_eq!(p_libera.steps[0].args.get("port").map(String::as_str), Some("3000"));

        let p_estado = planner
            .plan("puertos", &catalog)
            .expect("debe planificar");
        assert_eq!(p_estado.steps.len(), 1);
        assert_eq!(p_estado.steps[0].capability, "diag.port_status");
    }

    #[test]
    fn test_plan_servicios_locales() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_up = planner
            .plan("levanta un postgres para este proyecto", &catalog)
            .expect("debe planificar postgres");
        assert_eq!(p_up.steps.len(), 1);
        assert_eq!(p_up.steps[0].capability, "env.service_up");
        assert_eq!(p_up.steps[0].args.get("service").map(String::as_str), Some("postgres"));

        let p_redis = planner
            .plan("inicia redis en el puerto 6379", &catalog)
            .expect("debe planificar redis");
        assert_eq!(p_redis.steps.len(), 1);
        assert_eq!(p_redis.steps[0].capability, "env.service_up");
        assert_eq!(p_redis.steps[0].args.get("service").map(String::as_str), Some("redis"));
        assert_eq!(p_redis.steps[0].args.get("port").map(String::as_str), Some("6379"));

        let p_down = planner
            .plan("apaga el servicio postgres", &catalog)
            .expect("debe planificar down");
        assert_eq!(p_down.steps.len(), 1);
        assert_eq!(p_down.steps[0].capability, "env.service_down");
        assert_eq!(p_down.steps[0].args.get("service").map(String::as_str), Some("postgres"));
    }

    #[test]
    fn test_plan_secretos_y_concesiones() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_grant = planner
            .plan("concede acceso a .env", &catalog)
            .expect("debe planificar grant");
        assert_eq!(p_grant.steps.len(), 1);
        assert_eq!(p_grant.steps[0].capability, "secret.grant");

        let p_revoke = planner
            .plan("revoca acceso a .env", &catalog)
            .expect("debe planificar revoke");
        assert_eq!(p_revoke.steps.len(), 1);
        assert_eq!(p_revoke.steps[0].capability, "secret.revoke");

        let p_list = planner
            .plan("lista los secretos de la bóveda", &catalog)
            .expect("debe planificar list");
        assert_eq!(p_list.steps.len(), 1);
        assert_eq!(p_list.steps[0].capability, "secret.list");
    }

    #[test]
    fn test_plan_tickets_dinamicos() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_create = planner
            .plan("crea un ticket T6.1 para motor de inferencia local", &catalog)
            .expect("plan create");
        assert_eq!(p_create.steps.len(), 1);
        assert_eq!(p_create.steps[0].capability, "spec.create_ticket");
        assert_eq!(p_create.steps[0].args.get("ticket_id").map(String::as_str), Some("T6.1"));

        let p_status = planner
            .plan("actualiza ticket T1.1 a completado", &catalog)
            .expect("plan status");
        assert_eq!(p_status.steps.len(), 1);
        assert_eq!(p_status.steps[0].capability, "spec.update_ticket");
        assert_eq!(p_status.steps[0].args.get("ticket_id").map(String::as_str), Some("T1.1"));
    }

    #[test]
    fn test_plan_memoria_y_grafo() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_index = planner
            .plan("indexa la memoria del proyecto", &catalog)
            .expect("plan index");
        assert_eq!(p_index.steps.len(), 1);
        assert_eq!(p_index.steps[0].capability, "memory.index");

        let p_search = planner
            .plan("busca codigo sobre gestion de secretos", &catalog)
            .expect("plan search");
        assert_eq!(p_search.steps.len(), 1);
        assert_eq!(p_search.steps[0].capability, "memory.search");

        let p_graph = planner
            .plan("muestra el grafo para ticket:T6.1", &catalog)
            .expect("plan graph");
        assert_eq!(p_graph.steps.len(), 1);
        assert_eq!(p_graph.steps[0].capability, "memory.graph");
    }

    #[test]
    fn test_plan_env_profile_y_sync() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_init = planner
            .plan("configura el entorno para python", &catalog)
            .expect("plan init");
        assert_eq!(p_init.steps.len(), 1);
        assert_eq!(p_init.steps[0].capability, "env.init");
        assert_eq!(p_init.steps[0].args.get("profile").map(String::as_str), Some("python"));

        let p_sync = planner
            .plan("sincroniza el entorno del proyecto", &catalog)
            .expect("plan sync");
        assert_eq!(p_sync.steps.len(), 1);
        assert_eq!(p_sync.steps[0].capability, "env.sync");
    }

    #[test]
    fn test_plan_quota_status_y_set() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_status = planner
            .plan("muestra las cuotas de recursos del sandbox", &catalog)
            .expect("plan status");
        assert_eq!(p_status.steps.len(), 1);
        assert_eq!(p_status.steps[0].capability, "quota.status");

        let p_set = planner
            .plan("establece timeout 60 y memoria 512", &catalog)
            .expect("plan set");
        assert_eq!(p_set.steps.len(), 1);
        assert_eq!(p_set.steps[0].capability, "quota.set");
        assert_eq!(p_set.steps[0].args.get("timeout").map(String::as_str), Some("60"));
        assert_eq!(p_set.steps[0].args.get("memory").map(String::as_str), Some("512"));
    }

    #[test]
    fn test_plan_diff_viewer_y_terminal() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_diff = planner
            .plan("muestra el diff para ticket:T8.1", &catalog)
            .expect("plan diff");
        assert_eq!(p_diff.steps.len(), 1);
        assert_eq!(p_diff.steps[0].capability, "ui.diff_viewer");

        let p_term = planner
            .plan("abre la consola terminal interactiva", &catalog)
            .expect("plan term");
        assert_eq!(p_term.steps.len(), 1);
        assert_eq!(p_term.steps[0].capability, "ui.terminal");
    }

    #[test]
    fn test_plan_notificaciones_y_aprobaciones() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_list = planner
            .plan("muestra la bandeja de notificaciones", &catalog)
            .expect("plan list notif");
        assert_eq!(p_list.steps.len(), 1);
        assert_eq!(p_list.steps[0].capability, "notify.list");

        let p_approve = planner
            .plan("aprueba la notificación notif-t82", &catalog)
            .expect("plan approve notif");
        assert_eq!(p_approve.steps.len(), 1);
        assert_eq!(p_approve.steps[0].capability, "notify.action");
        assert_eq!(p_approve.steps[0].args.get("action").map(String::as_str), Some("approve"));
    }

    #[test]
    fn test_plan_antmesh_p2p() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_status = planner
            .plan("muestra el estado de la red mesh", &catalog)
            .expect("plan mesh status");
        assert_eq!(p_status.steps.len(), 1);
        assert_eq!(p_status.steps[0].capability, "mesh.status");

        let p_conn = planner
            .plan("conecta al nodo peer 192.168.1.50:9042", &catalog)
            .expect("plan mesh connect");
        assert_eq!(p_conn.steps.len(), 1);
        assert_eq!(p_conn.steps[0].capability, "mesh.connect");
        assert_eq!(p_conn.steps[0].args.get("address").map(String::as_str), Some("192.168.1.50:9042"));

        let p_pair = planner
            .plan("genera un token para emparejar nuevo dispositivo", &catalog)
            .expect("plan mesh pair");
        assert_eq!(p_pair.steps.len(), 1);
        assert_eq!(p_pair.steps[0].capability, "mesh.pair");
    }

    #[test]
    fn test_plan_swarm_distribuido() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_status = planner
            .plan("muestra el estado del swarm", &catalog)
            .expect("plan swarm status");
        assert_eq!(p_status.steps.len(), 1);
        assert_eq!(p_status.steps[0].capability, "flow.swarm_status");

        let p_disp = planner
            .plan("despacha el rol coder del ticket T9.2 al nodo node-gpu-01", &catalog)
            .expect("plan swarm dispatch");
        assert_eq!(p_disp.steps.len(), 1);
        assert_eq!(p_disp.steps[0].capability, "flow.dispatch_remote");
        assert_eq!(p_disp.steps[0].args.get("ticket_id").map(String::as_str), Some("T9.2"));
        assert_eq!(p_disp.steps[0].args.get("role").map(String::as_str), Some("coder"));
        assert_eq!(p_disp.steps[0].args.get("node").map(String::as_str), Some("node-gpu-01"));
    }

    #[test]
    fn test_plan_vfs_semantico() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_mount = planner
            .plan("monta el sistema de ficheros virtual antfs", &catalog)
            .expect("plan vfs mount");
        assert_eq!(p_mount.steps.len(), 1);
        assert_eq!(p_mount.steps[0].capability, "vfs.mount");

        let p_unmount = planner
            .plan("desmonta el sistema virtual antfs", &catalog)
            .expect("plan vfs unmount");
        assert_eq!(p_unmount.steps.len(), 1);
        assert_eq!(p_unmount.steps[0].capability, "vfs.unmount");

        let p_query = planner
            .plan("consulta los símbolos en vfs /antfs/symbols/structs", &catalog)
            .expect("plan vfs query");
        assert_eq!(p_query.steps.len(), 1);
        assert_eq!(p_query.steps[0].capability, "vfs.query");

        let p_val = planner
            .plan("valida la sintaxis del archivo src/lib.rs en vfs", &catalog)
            .expect("plan vfs validate");
        assert_eq!(p_val.steps.len(), 1);
        assert_eq!(p_val.steps[0].capability, "vfs.validate_write");

        let p_guard = planner
            .plan("muestra el estado del guard vfs", &catalog)
            .expect("plan vfs guard status");
        assert_eq!(p_guard.steps.len(), 1);
        assert_eq!(p_guard.steps[0].capability, "vfs.guard_status");

        let p_ebpf = planner
            .plan("muestra el estado del supervisor ebpf", &catalog)
            .expect("plan ebpf status");
        assert_eq!(p_ebpf.steps.len(), 1);
        assert_eq!(p_ebpf.steps[0].capability, "ebpf.status");

        let p_audit = planner
            .plan("audita las trazas de syscalls de ebpf", &catalog)
            .expect("plan ebpf audit");
        assert_eq!(p_audit.steps.len(), 1);
        assert_eq!(p_audit.steps[0].capability, "ebpf.audit_log");
    }
}
