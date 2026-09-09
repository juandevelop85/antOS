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

        if lower.contains("depend") || (lower.contains("paquete") && (lower.contains("proyecto") || lower.contains("project"))) {
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
            && !lower.contains("perfil")
            && !lower.contains("profile")
            && !lower.contains("hotspot")
            && !lower.contains("lsp")
            && !lower.contains("pair")
            && !lower.contains("collab")
            && !lower.contains("dap")
            && !lower.contains("depura")
            && !lower.contains("desktop")
            && !lower.contains("escritorio")
            && !lower.contains("atajo")
            && !lower.contains("hotkey")
            && !lower.contains("teclado")
            && !lower.contains("barra")
            && !lower.contains("boot")
            && !lower.contains("qemu")
            && !lower.contains("kernel")
            && !lower.contains("plugin")
            && !lower.contains("wasm")
            && !lower.contains("pkg")
            && !lower.contains("paquete")
            && !lower.contains("package")
            && !lower.contains("autopilot")
            && !lower.contains("centinela")
            && !lower.contains("web")
            && !lower.contains("websocket")
            && !lower.contains("bench")
            && !lower.contains("rendimiento")
            && !lower.contains("snapshot")
            && !lower.contains("instantanea")
            && !lower.contains("instantánea")
            && !lower.contains("issue")
            && !lower.contains("pull")
            && !lower.contains("pr")
            && !lower.contains("doc")
            && !lower.contains("diagrama")
            && !lower.contains("arquitectura")
            && !lower.contains("mermaid")
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
        if lower.contains("commit") && !lower.contains("hook") && !lower.contains("pre-commit") {
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

        if (lower.contains("rama") && !lower.contains("diagrama")) || lower.contains("branch") {
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

        if (lower.contains("puerto") || (lower.contains("port") && !lower.contains("import")))
            && !lower.contains("web")
            && !lower.contains("websocket")
        {
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
        if lower.contains("entorno") || lower.contains("toolchain") || lower.contains("devbox") || lower.contains("flake") || (lower.contains("perfil") && !lower.contains("perfila") && !lower.contains("profil") && !lower.contains("rendimiento") && !lower.contains("hotspot")) {
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

        if (lower.contains("terminal") || lower.contains("consola") || lower.contains("vte")) && !lower.contains("web") {
            return Ok(Propuesta::solo(vec![step("ui.terminal", &[])]));
        }

        // Intenciones de bandeja de notificaciones y aprobaciones asíncronas (T8.2)
        if (lower.contains("notifica") || lower.contains("alerta") || lower.contains("bandeja")) && !lower.contains("barra") {
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

        // Intenciones de Profiler Continuo y Rendimiento (T11.2)
        if lower.contains("profil") || lower.contains("perfila") || lower.contains("hotspot") || lower.contains("cuello")
            || (lower.contains("rendimiento") && !lower.contains("git") && !lower.contains("bench") && !lower.contains("compara") && !lower.contains("contra") && !lower.contains("diff") && !lower.contains("historial")) {
            if lower.contains("analiz") || lower.contains("sugerencia") || lower.contains("top") || lower.contains("hotspot") || lower.contains("cuello") {
                return Ok(Propuesta::solo(vec![step("profile.analyze", &[])]));
            }
            let cmd = after(&words, &["ejecuta", "run", "comando", "el", "con"]).unwrap_or_else(|| "cargo test".into());
            return Ok(Propuesta::solo(vec![step("profile.run", &[("command", &cmd)])]));
        }

        // Intenciones de Servidor LSP Unificado (T12.1)
        if lower.contains("lsp") || lower.contains("language server") {
            if lower.contains("inicia") || lower.contains("start") || lower.contains("arranca") || lower.contains("ejecuta") {
                let mode = after(&words, &["sobre", "en", "modo", "mode"]).unwrap_or_else(|| "stdio".into());
                return Ok(Propuesta::solo(vec![step("lsp.start", &[("mode", &mode)])]));
            }
            return Ok(Propuesta::solo(vec![step("lsp.status", &[])]));
        }

        // Intenciones de Co-Edición Colaborativa CRDT (T12.2)
        if lower.contains("pair") || lower.contains("collab") || lower.contains("co-edici") || (lower.contains("programa") && lower.contains("pareja")) {
            let file = words.iter().find(|w| w.ends_with(".rs") || w.ends_with(".toml") || w.ends_with(".md")).cloned().unwrap_or_else(|| "src/main.rs".into());
            let ticket = words.iter().find(|w| w.starts_with('T') && w.chars().nth(1).map(|c| c.is_ascii_digit()).unwrap_or(false)).cloned();
            let mut args = vec![("file", file.as_str())];
            if let Some(ref t) = ticket {
                args.push(("ticket", t.as_str()));
            }
            return Ok(Propuesta::solo(vec![step("collab.session", &args)]));
        }

        // Intenciones de Depuración Supervisada DAP (T12.2)
        if lower.contains("dap") || lower.contains("depura") || lower.contains("debugger") {
            let cmd = after(&words, &["comando", "el", "con", "a"]).unwrap_or_else(|| "cargo test".into());
            return Ok(Propuesta::solo(vec![step("dap.attach", &[("command", &cmd)])]));
        }

        // Intenciones de Reproducción Autónoma de Bugs TDD (T20.2)
        if lower.contains("reproduce") || lower.contains("reproducir") || lower.contains("reproductor") {
            let error_text = if let Some((_, rest)) = intent.split_once(':') {
                rest.trim().to_string()
            } else if let Some((_, rest)) = intent.split_once("bug") {
                rest.trim().to_string()
            } else if let Some((_, rest)) = intent.split_once("error") {
                rest.trim().to_string()
            } else {
                intent.to_string()
            };
            return Ok(Propuesta::solo(vec![step("test.reproduce", &[("error", &error_text)])]));
        }

        // Intenciones de Generación de Tests (T20.2)
        if (lower.contains("genera test") || lower.contains("generar test") || lower.contains("testgen") || lower.contains("crea test"))
            && !lower.contains("reproduce") {
            let target = words.iter().find(|w| w.ends_with(".rs") || w.ends_with(".py") || w.ends_with(".ts") || w.ends_with(".js"))
                .cloned()
                .unwrap_or_else(|| "src/lib.rs".into());
            return Ok(Propuesta::solo(vec![step("test.gen", &[("target", &target), ("suite_type", "unit")])]));
        }

        // Intenciones de CI / CD Local Paralelo (T20.3)
        let is_ci_intent = words.iter().any(|w| w == "ci")
            || lower.contains("integracion continua")
            || lower.contains("integración continua")
            || lower.contains("pipeline local");

        if is_ci_intent {
            if lower.contains("estado") || lower.contains("status") || lower.contains("reporte") {
                return Ok(Propuesta::solo(vec![step("ci.status", &[])]));
            }
            let stage = if lower.contains("lint") {
                Some("lint")
            } else if lower.contains("test") {
                Some("test")
            } else if lower.contains("security") || lower.contains("seguridad") || lower.contains("secretos") {
                Some("security")
            } else if lower.contains("format") {
                Some("format")
            } else {
                None
            };
            let fast = if lower.contains("fast") || lower.contains("rapido") || lower.contains("rápido") { "true" } else { "false" };
            let mut args = Vec::new();
            if let Some(st) = stage {
                args.push(("stage", st));
            }
            args.push(("fast", fast));
            return Ok(Propuesta::solo(vec![step("ci.run", &args)]));
        }

        // Intenciones de Git Hooks Inteligentes (T20.3)
        if lower.contains("hook") || lower.contains("pre-commit") || lower.contains("pre-push") {
            let action = if lower.contains("instal") || lower.contains("activa") {
                "install"
            } else if lower.contains("desinstal") || lower.contains("remue") || lower.contains("elimina") || lower.contains("desactiva") {
                "uninstall"
            } else if lower.contains("check") || lower.contains("audita") || lower.contains("valida") || lower.contains("revisa") {
                "check"
            } else {
                "status"
            };
            return Ok(Propuesta::solo(vec![step("git.hook", &[("action", action)])]));
        }

        // Intenciones de Snapshots Atómicos y Time Machine (T20.4)
        if lower.contains("snapshot") || lower.contains("instantanea") || lower.contains("instantánea") || lower.contains("time machine") {
            if lower.contains("crea") || lower.contains("guarda") || lower.contains("toma") || lower.contains("nuevo") || lower.contains("hacer") {
                let label = after(&words, &["etiqueta", "llamado", "llamada", "nombre", "como"]);
                let mut args = Vec::new();
                if let Some(ref l) = label {
                    args.push(("label", l.as_str()));
                }
                return Ok(Propuesta::solo(vec![step("snapshot.create", &args)]));
            } else if lower.contains("restaura") || lower.contains("recupera") || lower.contains("revert") || lower.contains("volver") {
                let id = after(&words, &["a", "al", "snapshot", "instantanea", "instantánea"])
                    .unwrap_or_else(|| "latest".into());
                return Ok(Propuesta::solo(vec![step("snapshot.restore", &[("id", &id)])]));
            } else if lower.contains("elimina") || lower.contains("borra") || lower.contains("remueve") {
                let id = after(&words, &["snapshot", "instantanea", "instantánea"])
                    .unwrap_or_else(|| "latest".into());
                return Ok(Propuesta::solo(vec![step("snapshot.delete", &[("id", &id)])]));
            } else {
                return Ok(Propuesta::solo(vec![step("snapshot.list", &[])]));
            }
        }

        // Intenciones de Benchmarking Continuo y Perf Diff (T21.1)
        if lower.contains("bench") || lower.contains("rendimiento") {
            if lower.contains("diff") || lower.contains("compara") || lower.contains("regresion") || lower.contains("regresión") || lower.contains("contra") {
                let against = after(&words, &["contra", "con", "rama", "base"])
                    .unwrap_or_else(|| "master".into());
                return Ok(Propuesta::solo(vec![step("bench.diff", &[("against", &against)])]));
            } else if lower.contains("historial") || lower.contains("evolucion") || lower.contains("evolución") || lower.contains("history") {
                return Ok(Propuesta::solo(vec![step("bench.history", &[])]));
            } else {
                let target = after(&words, &["suite", "benchmark", "de", "en"]);
                let mut args = Vec::new();
                if let Some(ref t) = target {
                    args.push(("target", t.as_str()));
                }
                return Ok(Propuesta::solo(vec![step("bench.run", &args)]));
            }
        }

        // Intenciones de Forjas Git: Issues y Pull Requests (T21.2)
        if lower.contains("issue") {
            if lower.contains("import") || lower.contains("descarga") || lower.contains("trae") || lower.contains("sincroniza") {
                let id = after(&words, &["issue", "el", "#", "de", "numero", "número"])
                    .unwrap_or_else(|| "42".into());
                return Ok(Propuesta::solo(vec![step("issue.import", &[("id", &id)])]));
            } else {
                return Ok(Propuesta::solo(vec![step("issue.list", &[])]));
            }
        }

        if lower.contains("pull request") || lower.contains("merge request") || lower.contains(" pr ") || lower.ends_with(" pr") || lower.starts_with("pr ") {
            if lower.contains("estado") || lower.contains("status") || lower.contains("ci") || lower.contains("revisa") {
                let num = after(&words, &["pr", "request", "numero", "número", "#"]);
                let mut args = Vec::new();
                if let Some(ref n) = num {
                    args.push(("number", n.as_str()));
                }
                return Ok(Propuesta::solo(vec![step("pr.status", &args)]));
            } else {
                let draft = if lower.contains("draft") || lower.contains("borrador") { "true" } else { "false" };
                let title = after(&words, &["titulo", "título", "con", "llamado"]);
                let mut args = vec![("draft", draft)];
                if let Some(ref t) = title {
                    args.push(("title", t.as_str()));
                }
                return Ok(Propuesta::solo(vec![step("pr.create", &args)]));
            }
        }

        // Intenciones de Documentación Viva y Diagramas Mermaid (T21.3)
        if lower.contains("doc") || lower.contains("documentacion") || lower.contains("documentación") || lower.contains("diagrama") || lower.contains("arquitectura") || lower.contains("mermaid") {
            if lower.contains("check") || lower.contains("verifica") || lower.contains("comprueba") || lower.contains("valida") {
                let target = after(&words, &["fichero", "archivo", "doc", "en", "para"]);
                let mut args = Vec::new();
                if let Some(ref t) = target {
                    args.push(("target", t.as_str()));
                }
                return Ok(Propuesta::solo(vec![step("doc.check", &args)]));
            } else if lower.contains("sync") || lower.contains("sincroniza") || lower.contains("actualiza") || lower.contains("incrusta") {
                let target = after(&words, &["fichero", "archivo", "doc", "en", "para"]);
                let mut args = Vec::new();
                if let Some(ref t) = target {
                    args.push(("target", t.as_str()));
                }
                return Ok(Propuesta::solo(vec![step("doc.sync", &args)]));
            } else if lower.contains("arch") || lower.contains("diagrama") || lower.contains("arquitectura") || lower.contains("mermaid") || lower.contains("c4") || lower.contains("topologia") || lower.contains("topología") {
                let kind = if lower.contains("componentes") || lower.contains("c4") || lower.contains("topologia") || lower.contains("topología") {
                    "components"
                } else if lower.contains("flow") || lower.contains("flujo") || lower.contains("ipc") {
                    "flow"
                } else if lower.contains("antflow") || lower.contains("estado") || lower.contains("ciclo") {
                    "antflow"
                } else {
                    "full"
                };
                return Ok(Propuesta::solo(vec![step("doc.arch", &[("kind", kind)])]));
            }
        }

        // Intenciones de Espacio de Trabajo Integrado Dev TUI (T20.1)
        if lower.contains("espacio de trabajo") || lower.contains("dev tui") || lower.contains("modo dev") || (lower.contains("workspace") && (lower.contains("inicia") || lower.contains("abre") || lower.contains("tui"))) {
            let project = after(&words, &["proyecto", "en", "para"]);
            let mut args = Vec::new();
            if let Some(ref p) = project {
                args.push(("project", p.as_str()));
            }
            return Ok(Propuesta::solo(vec![step("dev.workspace", &args)]));
        }

        // Intenciones de Editor de Texto (Neovim por defecto)
        if (lower.contains("editor") || lower.contains("neovim") || lower.contains("nvim") || lower.contains("editar"))
            && !lower.contains("co-edici") && !lower.contains("pair") {
            let file = words.iter().find(|w| w.ends_with(".rs") || w.ends_with(".toml") || w.ends_with(".md") || w.ends_with(".json") || w.ends_with(".sh")).cloned().unwrap_or_else(|| "src/main.rs".into());
            return Ok(Propuesta::solo(vec![step("ui.terminal", &[("command", &format!("nvim {file}"))])]));
        }

        // Intenciones de Escritorio Wayland y Atajos (T13.0)
        if lower.contains("desktop") || lower.contains("escritorio") || lower.contains("wayland") || lower.contains("atajos") || lower.contains("hotkeys") {
            if lower.contains("atajo") || lower.contains("hotkey") || lower.contains("teclado") {
                return Ok(Propuesta::solo(vec![step("desktop.keys", &[])]));
            }
            if lower.contains("inicia") || lower.contains("arranca") || lower.contains("start") {
                return Ok(Propuesta::solo(vec![step("desktop.session", &[("action", "start")])]));
            }
            return Ok(Propuesta::solo(vec![step("desktop.session", &[])]));
        }

        // Intenciones de Telemetría y Alertas de la Barra (T13.1)
        if lower.contains("barra") && (lower.contains("telemetr") || lower.contains("estado") || lower.contains("status") || lower.contains("alerta") || lower.contains("notifica")) {
            if lower.contains("alerta") || lower.contains("notifica") {
                let msg = intent.split_once(':').map(|(_, c)| c.trim().to_string()).unwrap_or_else(|| "Alerta de sistema para la barra".into());
                let urgent = if lower.contains("urgente") { "true" } else { "false" };
                return Ok(Propuesta::solo(vec![step("barra.notify", &[("category", "alerta"), ("message", &msg), ("urgent", urgent)])]));
            }
            return Ok(Propuesta::solo(vec![step("barra.status", &[])]));
        }

        // Intenciones de Pipeline de Arranque Bare Metal y QEMU (T13.2)
        if (!lower.contains("instala") && !lower.contains("deploy") && !lower.contains("bootloader") && !lower.contains("uefi") && !lower.contains("sondea") && !lower.contains("dual"))
            && (lower.contains("boot") || lower.contains("arranque") || lower.contains("qemu") || (lower.contains("kernel") && (lower.contains("compila") || lower.contains("construye") || lower.contains("prueba") || lower.contains("test")))) {
            if lower.contains("test") || lower.contains("prueba") {
                return Ok(Propuesta::solo(vec![step("boot.pipeline", &[("action", "test")])]));
            }
            if lower.contains("compila") || lower.contains("build") || lower.contains("construye") || lower.contains("imagen") {
                return Ok(Propuesta::solo(vec![step("boot.pipeline", &[("action", "build")])]));
            }
            if lower.contains("qemu") || lower.contains("inicia") || lower.contains("arranca") {
                return Ok(Propuesta::solo(vec![step("boot.pipeline", &[("action", "qemu")])]));
            }
            return Ok(Propuesta::solo(vec![step("boot.pipeline", &[("action", "status")])]));
        }

        // Intenciones de Plugins WebAssembly (WASM) (T14.1)
        if lower.contains("plugin") || lower.contains("wasm") {
            if lower.contains("lista") || lower.contains("list") || lower.contains("instalados") || lower.contains("instaladas") {
                return Ok(Propuesta::solo(vec![step("plugin.list", &[])]));
            }
            if lower.contains("instala") || lower.contains("install") {
                let path = after(&words, &["instala", "install", "plugin", "desde", "en"]).unwrap_or_else(|| ".".into());
                return Ok(Propuesta::solo(vec![step("plugin.install", &[("path", &path)])]));
            }
            if lower.contains("ejecuta") || lower.contains("run") {
                let plugin = after(&words, &["ejecuta", "run", "plugin"]).unwrap_or_default();
                let action = after(&words, &["con", "accion", "acción"]).unwrap_or_else(|| "run".into());
                return Ok(Propuesta::solo(vec![step("plugin.run", &[("plugin", &plugin), ("action", &action)])]));
            }
            return Ok(Propuesta::solo(vec![step("plugin.list", &[])]));
        }

        // Intenciones de Captura de Pantalla e Inspección Visual QA (T14.2)
        if lower.contains("captura") || lower.contains("screenshot") || lower.contains("screencopy") {
            let target = after(&words, &["de", "ventana", "target"]).unwrap_or_else(|| "desktop".into());
            let path = after(&words, &["guardando", "hacia", "path"]);
            let mut args = vec![("target", target.as_str())];
            let p_str;
            if let Some(ref p) = path {
                p_str = p.as_str();
                args.push(("path", p_str));
            }
            return Ok(Propuesta::solo(vec![step("ui.screenshot", &args)]));
        }
        if lower.contains("visual") || lower.contains("qa visual") {
            let target = after(&words, &["de", "sobre", "en", "target"]).unwrap_or_else(|| "desktop".into());
            return Ok(Propuesta::solo(vec![step("ui.inspect_visual", &[("target", &target)])]));
        }

        // Intenciones de Almacenamiento y Particionamiento (T15.1)
        if (!lower.contains("instala") && !lower.contains("deploy"))
            && (lower.contains("disco") || lower.contains("particion") || lower.contains("partición") || lower.contains("almacenamiento") || lower.contains("storage") || lower.contains("disk"))
        {
            if lower.contains("particiona") || lower.contains("partition") || lower.contains("formatea") {
                let dev = after(&words, &["disco", "en", "sobre", "dispositivo", "target"]).unwrap_or_else(|| "/dev/nvme0n1".into());
                let clean = if lower.contains("limpio") || lower.contains("clean") || lower.contains("completo") { "true" } else { "false" };
                return Ok(Propuesta::solo(vec![step("disk.partition", &[("device", &dev), ("clean", clean)])]));
            }
            if lower.contains("inspecciona") || lower.contains("inspect") || lower.contains("info") || lower.contains("detalle") {
                let dev = after(&words, &["disco", "dispositivo", "de", "en"]).unwrap_or_else(|| "/dev/nvme0n1".into());
                return Ok(Propuesta::solo(vec![step("disk.inspect", &[("device", &dev)])]));
            }
            return Ok(Propuesta::solo(vec![step("disk.list", &[])]));
        }

        // Intenciones de Instalación y Despliegue de antOS (T15.2)
        if (lower.contains("instala") || lower.contains("instalacion") || lower.contains("instalación") || lower.contains("installer") || lower.contains("deploy"))
            && !lower.contains("plugin")
            && !lower.contains("wasm")
            && !lower.contains("bootloader")
            && !lower.contains("uefi")
            && !lower.contains("gestor")
            && !lower.contains("cargador")
            && !lower.contains("paquete")
            && !lower.contains("package")
            && !lower.contains("pkg")
        {
            let dev = after(&words, &["disco", "dispositivo", "target", "sobre"]).unwrap_or_else(|| "/dev/nvme0n1".into());
            let clean = if lower.contains("limpio") || lower.contains("clean") || lower.contains("completo") || lower.contains("principal") { "true" } else { "false" };
            let dry = if lower.contains("apply") || lower.contains("real") || lower.contains("definitivo") { "false" } else { "true" };
            return Ok(Propuesta::solo(vec![step("install.deploy", &[
                ("target_device", &dev),
                ("clean", clean),
                ("dry_run", dry),
            ])]));
        }

        // Intenciones de Gestor de Arranque UEFI y Dual Boot (T15.3)
        if lower.contains("bootloader")
            || lower.contains("uefi")
            || lower.contains("cargador")
            || lower.contains("efibootmgr")
            || lower.contains("dual boot")
            || lower.contains("dual-boot")
            || (lower.contains("arranque") && (lower.contains("gestor") || lower.contains("dual") || lower.contains("sondea") || lower.contains("detecta")))
            || (lower.contains("sondea") && lower.contains("sistemas"))
        {
            if lower.contains("sondea") || lower.contains("probe") || lower.contains("detecta") || lower.contains("busca") || lower.contains("lista") {
                let esp = after(&words, &["en", "esp", "particion", "partición"]);
                let mut args = Vec::new();
                let esp_s;
                if let Some(ref e) = esp {
                    esp_s = e.as_str();
                    args.push(("esp_path", esp_s));
                }
                return Ok(Propuesta::solo(vec![step("bootloader.probe", &args)]));
            }

            let dev = after(&words, &["disco", "dispositivo", "target", "sobre"]).unwrap_or_else(|| "/dev/nvme0n1".into());
            let esp = after(&words, &["esp", "en"]).unwrap_or_else(|| "/boot/efi".into());
            let dry = if lower.contains("apply") || lower.contains("real") || lower.contains("definitivo") { "false" } else { "true" };
            return Ok(Propuesta::solo(vec![step("bootloader.install", &[
                ("target_device", &dev),
                ("esp_path", &esp),
                ("dry_run", dry),
            ])]));
        }

        // Intenciones de MicroVMs y Aislamiento por Hipervisor (T16.1)
        if lower.contains("microvm")
            || lower.contains("kvm")
            || lower.contains("cloud-hypervisor")
            || lower.contains("firecracker")
            || (lower.contains("vm") && (lower.contains("arranca") || lower.contains("spawn") || lower.contains("ejecuta") || lower.contains("exec") || lower.contains("mata") || lower.contains("kill") || lower.contains("deten") || lower.contains("detén") || lower.contains("destruye")))
        {
            if lower.contains("ejecuta") || lower.contains("exec") {
                let vm_id = after(&words, &["microvm", "vm"]).unwrap_or_else(|| "vm-default".into());
                let cmd = after(&words, &["comando", "cmd", "exec", "ejecuta"]).unwrap_or_else(|| "echo test".into());
                return Ok(Propuesta::solo(vec![step("microvm.exec", &[
                    ("vm_id", &vm_id),
                    ("command", &cmd),
                ])]));
            } else if lower.contains("mata") || lower.contains("kill") || lower.contains("destruye") || lower.contains("deten") || lower.contains("detén") || lower.contains("stop") {
                let vm_id = after(&words, &["microvm", "vm", "id"]).unwrap_or_else(|| "vm-default".into());
                return Ok(Propuesta::solo(vec![step("microvm.destroy", &[
                    ("vm_id", &vm_id),
                ])]));
            } else {
                let vm_id = after(&words, &["id", "nombre", "llamada"]).unwrap_or_else(|| format!("vm-{}", chrono::Local::now().format("%Y%m%d%H%M%S")));
                let cpus = before_or_after(&words, &["cpus", "cpu", "cores"]).unwrap_or_else(|| "2".into());
                let memory = before_or_after(&words, &["memoria", "ram", "mb"]).unwrap_or_else(|| "512".into());
                return Ok(Propuesta::solo(vec![step("microvm.spawn", &[
                    ("vm_id", &vm_id),
                    ("cpus", &cpus),
                    ("memory", &memory),
                ])]));
            }
        }

        // Intenciones de paquetes inmutables antpkg (T16.2)
        if lower.contains("antpkg") || lower.contains("paquete") || lower.contains("package") || lower.contains("pkg") {
            let extract_pkg = || {
                if let Some(p) = after(&words, &["paquete", "package", "pkg"]) {
                    if p != "el" && p != "un" && p != "la" && p != "de" && p != "the" && p != "a" {
                        return p;
                    }
                }
                words.last().cloned().unwrap_or_else(|| "ripgrep".into())
            };

            if lower.contains("desinstala") || lower.contains("remove") || lower.contains("uninstall") || lower.contains("borra") || lower.contains("elimina") {
                let pkg_name = extract_pkg();
                return Ok(Propuesta::solo(vec![step("pkg.remove", &[
                    ("package", &pkg_name),
                ])]));
            } else if lower.contains("rollback") || lower.contains("revierte") || lower.contains("revert") {
                let gen = after(&words, &["generacion", "generación", "generation", "gen"]);
                if let Some(g) = gen {
                    return Ok(Propuesta::solo(vec![step("pkg.rollback", &[("generation", &g)])]));
                } else {
                    return Ok(Propuesta::solo(vec![step("pkg.rollback", &[])]));
                }
            } else if lower.contains("lista") || lower.contains("list") {
                return Ok(Propuesta::solo(vec![step("pkg.list", &[])]));
            } else if lower.contains("verifica") || lower.contains("verify") || lower.contains("check") {
                return Ok(Propuesta::solo(vec![step("pkg.verify", &[])]));
            } else if lower.contains("instala") || lower.contains("install") || lower.contains("agrega") || lower.contains("add") {
                let pkg_name = extract_pkg();
                let dry_run = if lower.contains("dry-run") || lower.contains("simula") { "true" } else { "false" };
                return Ok(Propuesta::solo(vec![step("pkg.install", &[
                    ("package", &pkg_name),
                    ("dry_run", dry_run),
                ])]));
            }
        }

        // Intenciones de Autopilot Daemon (T16.3)
        if lower.contains("autopilot") || lower.contains("centinela") || lower.contains("sentinel") || lower.contains("incidente") || lower.contains("incident") {
            if lower.contains("stop") || lower.contains("deten") || lower.contains("para") || lower.contains("cancela") {
                return Ok(Propuesta::solo(vec![step("autopilot.stop", &[])]));
            } else if lower.contains("status") || lower.contains("estado") || lower.contains("metricas") || lower.contains("métricas") {
                return Ok(Propuesta::solo(vec![step("autopilot.status", &[])]));
            } else if lower.contains("scan") || lower.contains("escanea") || lower.contains("revisa") || lower.contains("busca fallos") {
                return Ok(Propuesta::solo(vec![step("autopilot.scan", &[])]));
            } else if lower.contains("aprueba") || lower.contains("approve") || lower.contains("merge") || lower.contains("resuelve") || lower.contains("descarta") || lower.contains("reject") {
                let inc_id = after(&words, &["incidente", "incident"])
                    .or_else(|| {
                        let candidate = after(&words, &["aprueba", "approve", "descarta", "reject", "resuelve"])?;
                        if candidate == "la" || candidate == "el" || candidate == "propuesta" || candidate == "proposal" || candidate == "del" {
                            words.last().cloned()
                        } else {
                            Some(candidate)
                        }
                    })
                    .unwrap_or_else(|| "inc-1".into());
                let approve = if lower.contains("descarta") || lower.contains("reject") { "false" } else { "true" };
                return Ok(Propuesta::solo(vec![step("autopilot.resolve", &[
                    ("incident_id", &inc_id),
                    ("approve", approve),
                ])]));
            } else {
                let interval = after(&words, &["intervalo", "interval", "cada", "every"]).unwrap_or_else(|| "5".into());
                return Ok(Propuesta::solo(vec![step("autopilot.start", &[
                    ("interval", &interval),
                ])]));
            }
        }

        // Intenciones de Consola Web Remota (T16.4)
        if lower.contains("web") || lower.contains("websocket") {
            if lower.contains("token") || lower.contains("enlace") || lower.contains("acceso") {
                let label = after(&words, &["para", "cliente", "label", "dispositivo"]).unwrap_or_else(|| "admin".into());
                let ttl = after(&words, &["ttl", "expira", "tiempo"]).unwrap_or_else(|| "86400".into());
                return Ok(Propuesta::solo(vec![step("web.token", &[
                    ("label", &label),
                    ("ttl", &ttl),
                ])]));
            } else if lower.contains("stop") || lower.contains("deten") || lower.contains("apaga") || lower.contains("cierra") || words.first().map(String::as_str) == Some("para") {
                return Ok(Propuesta::solo(vec![step("web.stop", &[])]));
            } else if lower.contains("status") || lower.contains("estado") || lower.contains("metricas") || lower.contains("métricas") {
                return Ok(Propuesta::solo(vec![step("web.status", &[])]));
            } else {
                let port = after(&words, &["puerto", "port"])
                    .or_else(|| {
                        words.iter().find(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_digit())).cloned()
                    })
                    .unwrap_or_else(|| "8088".into());
                let bind = after(&words, &["ip", "bind", "host"]).unwrap_or_else(|| "127.0.0.1".into());
                return Ok(Propuesta::solo(vec![step("web.start", &[
                    ("port", &port),
                    ("bind", &bind),
                ])]));
            }
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

/// Devuelve la palabra anterior o posterior (priorizando dígitos) para `keys`.
fn before_or_after(words: &[String], keys: &[&str]) -> Option<String> {
    for (i, w) in words.iter().enumerate() {
        if keys.contains(&w.as_str()) {
            if i > 0 {
                if let Some(prev) = words.get(i - 1) {
                    if prev.chars().all(|c| c.is_ascii_digit()) {
                        return Some(prev.clone());
                    }
                }
            }
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
    #![allow(clippy::unwrap_used, clippy::expect_used)]

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
    fn test_plan_worktree_create_and_cleanup() {
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
    fn test_plan_port_diagnose_and_release() {
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
    fn test_plan_secrets_and_grants() {
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
    fn test_plan_memory_and_graph() {
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
    fn test_plan_env_profile_and_sync() {
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
    fn test_plan_quota_status_and_set() {
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
    fn test_plan_diff_viewer_and_terminal() {
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
    fn test_plan_notifications_and_approvals() {
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

        let p_prof_run = planner
            .plan("perfila el comando cargo test", &catalog)
            .expect("plan profile run");
        assert_eq!(p_prof_run.steps.len(), 1);
        assert_eq!(p_prof_run.steps[0].capability, "profile.run");

        let p_prof_an = planner
            .plan("analiza los hotspots de rendimiento y cuellos de botella", &catalog)
            .expect("plan profile analyze");
        assert_eq!(p_prof_an.steps.len(), 1);
        assert_eq!(p_prof_an.steps[0].capability, "profile.analyze");

        let p_lsp_start = planner
            .plan("inicia el servidor lsp sobre stdio", &catalog)
            .expect("plan lsp start");
        assert_eq!(p_lsp_start.steps.len(), 1);
        assert_eq!(p_lsp_start.steps[0].capability, "lsp.start");

        let p_lsp_status = planner
            .plan("muestra el estado del servidor lsp", &catalog)
            .expect("plan lsp status");
        assert_eq!(p_lsp_status.steps.len(), 1);
        assert_eq!(p_lsp_status.steps[0].capability, "lsp.status");

        let p_collab = planner
            .plan("inicia pair programming con coder en src/main.rs para T12.2", &catalog)
            .expect("plan collab session");
        assert_eq!(p_collab.steps.len(), 1);
        assert_eq!(p_collab.steps[0].capability, "collab.session");

        let p_dap = planner
            .plan("depura con dap el comando cargo test", &catalog)
            .expect("plan dap attach");
        assert_eq!(p_dap.steps.len(), 1);
        assert_eq!(p_dap.steps[0].capability, "dap.attach");

        let p_desk_status = planner
            .plan("consulta el estado del escritorio wayland", &catalog)
            .expect("plan desktop session status");
        assert_eq!(p_desk_status.steps.len(), 1);
        assert_eq!(p_desk_status.steps[0].capability, "desktop.session");

        let p_desk_start = planner
            .plan("inicia el escritorio wayland", &catalog)
            .expect("plan desktop session start");
        assert_eq!(p_desk_start.steps.len(), 1);
        assert_eq!(p_desk_start.steps[0].capability, "desktop.session");
        assert_eq!(p_desk_start.steps[0].args.get("action").map(|s| s.as_str()), Some("start"));

        let p_desk_keys = planner
            .plan("muestra los atajos de teclado del escritorio", &catalog)
            .expect("plan desktop keys");
        assert_eq!(p_desk_keys.steps.len(), 1);
        assert_eq!(p_desk_keys.steps[0].capability, "desktop.keys");

        let p_barra_status = planner
            .plan("consulta la telemetría de la barra", &catalog)
            .expect("plan barra status");
        assert_eq!(p_barra_status.steps.len(), 1);
        assert_eq!(p_barra_status.steps[0].capability, "barra.status");

        let p_barra_notif = planner
            .plan("envia una alerta a la barra: Violacion detectada", &catalog)
            .expect("plan barra notify");
        assert_eq!(p_barra_notif.steps.len(), 1);
        assert_eq!(p_barra_notif.steps[0].capability, "barra.notify");

        let p_boot_test = planner
            .plan("prueba el arranque del kernel en qemu", &catalog)
            .expect("plan boot test");
        assert_eq!(p_boot_test.steps.len(), 1);
        assert_eq!(p_boot_test.steps[0].capability, "boot.pipeline");
        assert_eq!(p_boot_test.steps[0].args.get("action").map(|s| s.as_str()), Some("test"));

        let p_boot_build = planner
            .plan("compila el kernel y genera la imagen", &catalog)
            .expect("plan boot build");
        assert_eq!(p_boot_build.steps.len(), 1);
        assert_eq!(p_boot_build.steps[0].capability, "boot.pipeline");
        assert_eq!(p_boot_build.steps[0].args.get("action").map(|s| s.as_str()), Some("build"));

        let p_plugin_list = planner
            .plan("lista los plugins wasm instalados", &catalog)
            .expect("plan plugin list");
        assert_eq!(p_plugin_list.steps.len(), 1);
        assert_eq!(p_plugin_list.steps[0].capability, "plugin.list");

        let p_plugin_run = planner
            .plan("ejecuta el plugin json-parser con accion format", &catalog)
            .expect("plan plugin run");
        assert_eq!(p_plugin_run.steps.len(), 1);
        assert_eq!(p_plugin_run.steps[0].capability, "plugin.run");

        let p_screenshot = planner
            .plan("captura la pantalla de la ventana barra guardando en /tmp/bar.png", &catalog)
            .expect("plan screenshot");
        assert_eq!(p_screenshot.steps.len(), 1);
        assert_eq!(p_screenshot.steps[0].capability, "ui.screenshot");

        let p_visual = planner
            .plan("inspecciona el diseño visual de la ventana antos-barra", &catalog)
            .expect("plan visual inspection");
        assert_eq!(p_visual.steps.len(), 1);
        assert_eq!(p_visual.steps[0].capability, "ui.inspect_visual");

        let p_disks = planner
            .plan("lista los discos del sistema", &catalog)
            .expect("plan disk list");
        assert_eq!(p_disks.steps.len(), 1);
        assert_eq!(p_disks.steps[0].capability, "disk.list");

        let p_inspect = planner
            .plan("inspecciona el disco /dev/nvme0n1", &catalog)
            .expect("plan disk inspect");
        assert_eq!(p_inspect.steps.len(), 1);
        assert_eq!(p_inspect.steps[0].capability, "disk.inspect");
        assert_eq!(p_inspect.steps[0].args.get("device").map(|s| s.as_str()), Some("/dev/nvme0n1"));

        let p_part = planner
            .plan("particiona el disco /dev/sda en modo limpio", &catalog)
            .expect("plan disk partition");
        assert_eq!(p_part.steps.len(), 1);
        assert_eq!(p_part.steps[0].capability, "disk.partition");
        assert_eq!(p_part.steps[0].args.get("device").map(|s| s.as_str()), Some("/dev/sda"));
        assert_eq!(p_part.steps[0].args.get("clean").map(|s| s.as_str()), Some("true"));

        let p_install = planner
            .plan("instala antos en el disco /dev/nvme0n1 en modo dual boot", &catalog)
            .expect("plan install deploy");
        assert_eq!(p_install.steps.len(), 1);
        assert_eq!(p_install.steps[0].capability, "install.deploy");
        assert_eq!(p_install.steps[0].args.get("target_device").map(|s| s.as_str()), Some("/dev/nvme0n1"));
        assert_eq!(p_install.steps[0].args.get("clean").map(|s| s.as_str()), Some("false"));

        let p_probe = planner
            .plan("sondea los sistemas operativos en /boot/efi para dual boot", &catalog)
            .expect("plan bootloader probe");
        assert_eq!(p_probe.steps.len(), 1);
        assert_eq!(p_probe.steps[0].capability, "bootloader.probe");

        let p_bootloader = planner
            .plan("instala el gestor de arranque uefi en el disco /dev/nvme0n1", &catalog)
            .expect("plan bootloader install");
        assert_eq!(p_bootloader.steps.len(), 1);
        assert_eq!(p_bootloader.steps[0].capability, "bootloader.install");
        assert_eq!(p_bootloader.steps[0].args.get("target_device").map(|s| s.as_str()), Some("/dev/nvme0n1"));

        let p_vm_spawn = planner
            .plan("arranca una microvm aislada con 4 cpus y 1024 memoria", &catalog)
            .expect("plan vm spawn");
        assert_eq!(p_vm_spawn.steps.len(), 1);
        assert_eq!(p_vm_spawn.steps[0].capability, "microvm.spawn");
        assert_eq!(p_vm_spawn.steps[0].args.get("cpus").map(|s| s.as_str()), Some("4"));
        assert_eq!(p_vm_spawn.steps[0].args.get("memory").map(|s| s.as_str()), Some("1024"));

        let p_vm_exec = planner
            .plan("ejecuta en la microvm vm-test el comando uname -a", &catalog)
            .expect("plan vm exec");
        assert_eq!(p_vm_exec.steps.len(), 1);
        assert_eq!(p_vm_exec.steps[0].capability, "microvm.exec");
        assert_eq!(p_vm_exec.steps[0].args.get("vm_id").map(|s| s.as_str()), Some("vm-test"));

        let p_vm_kill = planner
            .plan("destruye la microvm vm-test", &catalog)
            .expect("plan vm destroy");
        assert_eq!(p_vm_kill.steps.len(), 1);
        assert_eq!(p_vm_kill.steps[0].capability, "microvm.destroy");
        assert_eq!(p_vm_kill.steps[0].args.get("vm_id").map(|s| s.as_str()), Some("vm-test"));

        // antpkg planning tests (T16.2)
        let p_pkg_inst = planner
            .plan("instala el paquete ripgrep", &catalog)
            .expect("plan pkg install");
        assert_eq!(p_pkg_inst.steps.len(), 1);
        assert_eq!(p_pkg_inst.steps[0].capability, "pkg.install");
        assert_eq!(p_pkg_inst.steps[0].args.get("package").map(|s| s.as_str()), Some("ripgrep"));

        let p_pkg_rm = planner
            .plan("desinstala el paquete curl", &catalog)
            .expect("plan pkg remove");
        assert_eq!(p_pkg_rm.steps.len(), 1);
        assert_eq!(p_pkg_rm.steps[0].capability, "pkg.remove");
        assert_eq!(p_pkg_rm.steps[0].args.get("package").map(|s| s.as_str()), Some("curl"));

        let p_pkg_rb = planner
            .plan("haz rollback de paquetes a la generacion 2", &catalog)
            .expect("plan pkg rollback");
        assert_eq!(p_pkg_rb.steps.len(), 1);
        assert_eq!(p_pkg_rb.steps[0].capability, "pkg.rollback");
        assert_eq!(p_pkg_rb.steps[0].args.get("generation").map(|s| s.as_str()), Some("2"));

        let p_pkg_ls = planner
            .plan("lista los paquetes instalados", &catalog)
            .expect("plan pkg list");
        assert_eq!(p_pkg_ls.steps.len(), 1);
        assert_eq!(p_pkg_ls.steps[0].capability, "pkg.list");

        let p_pkg_vf = planner
            .plan("verifica los paquetes antpkg", &catalog)
            .expect("plan pkg verify");
        assert_eq!(p_pkg_vf.steps.len(), 1);
        assert_eq!(p_pkg_vf.steps[0].capability, "pkg.verify");
    }

    #[test]
    fn test_plan_autopilot_sentinela() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_start = planner
            .plan("inicia el modo autopilot continuo con intervalo 10", &catalog)
            .expect("plan autopilot start");
        assert_eq!(p_start.steps.len(), 1);
        assert_eq!(p_start.steps[0].capability, "autopilot.start");
        assert_eq!(p_start.steps[0].args.get("interval").map(|s| s.as_str()), Some("10"));

        let p_status = planner
            .plan("muestra el estado de autopilot", &catalog)
            .expect("plan autopilot status");
        assert_eq!(p_status.steps.len(), 1);
        assert_eq!(p_status.steps[0].capability, "autopilot.status");

        let p_scan = planner
            .plan("escanea el workspace con el centinela", &catalog)
            .expect("plan autopilot scan");
        assert_eq!(p_scan.steps.len(), 1);
        assert_eq!(p_scan.steps[0].capability, "autopilot.scan");

        let p_resolve = planner
            .plan("aprueba la propuesta del incidente inc-42", &catalog)
            .expect("plan autopilot resolve");
        assert_eq!(p_resolve.steps.len(), 1);
        assert_eq!(p_resolve.steps[0].capability, "autopilot.resolve");
        assert_eq!(p_resolve.steps[0].args.get("incident_id").map(|s| s.as_str()), Some("inc-42"));
        assert_eq!(p_resolve.steps[0].args.get("approve").map(|s| s.as_str()), Some("true"));

        let p_stop = planner
            .plan("deten el modo autopilot", &catalog)
            .expect("plan autopilot stop");
        assert_eq!(p_stop.steps.len(), 1);
        assert_eq!(p_stop.steps[0].capability, "autopilot.stop");
    }

    #[test]
    fn test_plan_web_console() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_start = planner
            .plan("inicia el servidor web en puerto 9000", &catalog)
            .expect("plan web start");
        assert_eq!(p_start.steps.len(), 1);
        assert_eq!(p_start.steps[0].capability, "web.start");
        assert_eq!(p_start.steps[0].args.get("port").map(|s| s.as_str()), Some("9000"));

        let p_status = planner
            .plan("muestra el estado del servidor web", &catalog)
            .expect("plan web status");
        assert_eq!(p_status.steps.len(), 1);
        assert_eq!(p_status.steps[0].capability, "web.status");

        let p_token = planner
            .plan("genera un token web para laptop", &catalog)
            .expect("plan web token");
        assert_eq!(p_token.steps.len(), 1);
        assert_eq!(p_token.steps[0].capability, "web.token");
        assert_eq!(p_token.steps[0].args.get("label").map(|s| s.as_str()), Some("laptop"));

        let p_stop = planner
            .plan("deten la consola web", &catalog)
            .expect("plan web stop");
        assert_eq!(p_stop.steps.len(), 1);
        assert_eq!(p_stop.steps[0].capability, "web.stop");
    }

    #[test]
    fn test_plan_open_default_editor_neovim() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_edit = planner
            .plan("abre el editor para src/lib.rs", &catalog)
            .expect("plan editor");
        assert_eq!(p_edit.steps.len(), 1);
        assert_eq!(p_edit.steps[0].capability, "ui.terminal");
        assert!(p_edit.steps[0].args.get("command").unwrap().contains("nvim src/lib.rs"));

        let p_nvim = planner
            .plan("editar en neovim", &catalog)
            .expect("plan nvim");
        assert_eq!(p_nvim.steps.len(), 1);
        assert_eq!(p_nvim.steps[0].capability, "ui.terminal");
        assert!(p_nvim.steps[0].args.get("command").unwrap().contains("nvim"));
    }

    #[test]
    fn test_plan_dev_workspace() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_dev = planner
            .plan("inicia el espacio de trabajo para api-service", &catalog)
            .expect("plan dev workspace");
        assert_eq!(p_dev.steps.len(), 1);
        assert_eq!(p_dev.steps[0].capability, "dev.workspace");
        assert_eq!(p_dev.steps[0].args.get("project").map(|s| s.as_str()), Some("api-service"));

        let p_tui = planner
            .plan("abrir dev tui", &catalog)
            .expect("plan dev tui");
        assert_eq!(p_tui.steps.len(), 1);
        assert_eq!(p_tui.steps[0].capability, "dev.workspace");
    }

    #[test]
    fn test_plan_reproduce_and_testgen() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_rep = planner
            .plan("reproduce el error: thread 'main' panicked at 'index out of bounds', src/lib.rs:12:4", &catalog)
            .expect("plan reproduce");
        assert_eq!(p_rep.steps.len(), 1);
        assert_eq!(p_rep.steps[0].capability, "test.reproduce");
        assert!(p_rep.steps[0].args.get("error").unwrap().contains("panicked at"));

        let p_gen = planner
            .plan("genera tests para src/service.rs", &catalog)
            .expect("plan testgen");
        assert_eq!(p_gen.steps.len(), 1);
        assert_eq!(p_gen.steps[0].capability, "test.gen");
        assert_eq!(p_gen.steps[0].args.get("target").map(|s| s.as_str()), Some("src/service.rs"));
    }

    #[test]
    fn test_plan_ci_and_git_hooks() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_ci = planner
            .plan("ejecuta ci rápido", &catalog)
            .expect("plan ci");
        assert_eq!(p_ci.steps.len(), 1);
        assert_eq!(p_ci.steps[0].capability, "ci.run");
        assert_eq!(p_ci.steps[0].args.get("fast").map(|s| s.as_str()), Some("true"));

        let p_hook = planner
            .plan("instala pre-commit hook", &catalog)
            .expect("plan hook");
        assert_eq!(p_hook.steps.len(), 1);
        assert_eq!(p_hook.steps[0].capability, "git.hook");
        assert_eq!(p_hook.steps[0].args.get("action").map(|s| s.as_str()), Some("install"));
    }

    #[test]
    fn test_plan_dev_snapshots() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_create = planner
            .plan("crea snapshot llamado pre-refactor", &catalog)
            .expect("plan snapshot create");
        assert_eq!(p_create.steps.len(), 1);
        assert_eq!(p_create.steps[0].capability, "snapshot.create");
        assert_eq!(p_create.steps[0].args.get("label").map(|s| s.as_str()), Some("pre-refactor"));

        let p_list = planner
            .plan("lista snapshots del time machine", &catalog)
            .expect("plan snapshot list");
        assert_eq!(p_list.steps.len(), 1);
        assert_eq!(p_list.steps[0].capability, "snapshot.list");

        let p_restore = planner
            .plan("restaura snapshot pre-refactor", &catalog)
            .expect("plan snapshot restore");
        assert_eq!(p_restore.steps.len(), 1);
        assert_eq!(p_restore.steps[0].capability, "snapshot.restore");
        assert_eq!(p_restore.steps[0].args.get("id").map(|s| s.as_str()), Some("pre-refactor"));
    }

    #[test]
    fn test_plan_benchmarking() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_run = planner
            .plan("ejecuta benchmarks del proyecto", &catalog)
            .expect("plan bench run");
        assert_eq!(p_run.steps.len(), 1);
        assert_eq!(p_run.steps[0].capability, "bench.run");

        let p_diff = planner
            .plan("compara rendimiento contra master", &catalog)
            .expect("plan bench diff");
        assert_eq!(p_diff.steps.len(), 1);
        assert_eq!(p_diff.steps[0].capability, "bench.diff");
        assert_eq!(p_diff.steps[0].args.get("against").map(|s| s.as_str()), Some("master"));

        let p_hist = planner
            .plan("muestra historial de rendimiento", &catalog)
            .expect("plan bench history");
        assert_eq!(p_hist.steps.len(), 1);
        assert_eq!(p_hist.steps[0].capability, "bench.history");
    }

    #[test]
    fn test_plan_forge_issues_and_prs() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_list = planner
            .plan("lista issues remotos", &catalog)
            .expect("plan issue list");
        assert_eq!(p_list.steps.len(), 1);
        assert_eq!(p_list.steps[0].capability, "issue.list");

        let p_import = planner
            .plan("importa issue 42", &catalog)
            .expect("plan issue import");
        assert_eq!(p_import.steps.len(), 1);
        assert_eq!(p_import.steps[0].capability, "issue.import");
        assert_eq!(p_import.steps[0].args.get("id").map(|s| s.as_str()), Some("42"));

        let p_pr = planner
            .plan("crea pull request", &catalog)
            .expect("plan pr create");
        assert_eq!(p_pr.steps.len(), 1);
        assert_eq!(p_pr.steps[0].capability, "pr.create");

        let p_status = planner
            .plan("revisa estado del pull request", &catalog)
            .expect("plan pr status");
        assert_eq!(p_status.steps.len(), 1);
        assert_eq!(p_status.steps[0].capability, "pr.status");
    }

    #[test]
    fn test_plan_doc_arch_and_sync() {
        let ctx = Ctx::discover().expect("ctx");
        let catalog = Catalog::load(&ctx.caps_dir).expect("catalog");
        let planner = LocalPlanner;

        let p_arch = planner
            .plan("genera diagrama de arquitectura", &catalog)
            .expect("plan doc arch");
        assert_eq!(p_arch.steps.len(), 1);
        assert_eq!(p_arch.steps[0].capability, "doc.arch");

        let p_sync = planner
            .plan("sincroniza documentacion de arquitectura", &catalog)
            .expect("plan doc sync");
        assert_eq!(p_sync.steps.len(), 1);
        assert_eq!(p_sync.steps[0].capability, "doc.sync");

        let p_check = planner
            .plan("verifica documentacion de arquitectura", &catalog)
            .expect("plan doc check");
        assert_eq!(p_check.steps.len(), 1);
        assert_eq!(p_check.steps[0].capability, "doc.check");
    }
}
