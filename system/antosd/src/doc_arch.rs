//! antOS Live Architecture & Mermaid Documentation Engine (Ticket T21.3).
//!
//! Analyzes workspace AST, module topology, IPC contracts, and capabilities,
//! generating live Mermaid diagrams and synchronizing them into project documentation.

use antos_protocol::{ArchDiagramKind, ArchDiagramReport, DocSyncReport};
use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct DocArchEngine;

#[derive(Debug, Clone, Default)]
pub struct WorkspaceTopology {
    pub crates: Vec<String>,
    pub antosd_modules: Vec<String>,
    pub ipc_requests_count: usize,
    pub ipc_events_count: usize,
    pub capabilities_count: usize,
    pub auto_caps_count: usize,
    pub confirm_caps_count: usize,
    pub grant_caps_count: usize,
}

impl DocArchEngine {
    /// Analyzes the workspace to extract crates, modules, IPC types, and capabilities.
    pub fn analyze_topology(workspace: &Path) -> WorkspaceTopology {
        let mut topo = WorkspaceTopology::default();

        let effective_root = if workspace.join("system").join("antosd").exists() {
            workspace.to_path_buf()
        } else if let Some(root) = crate::git::detect_antos_root() {
            root
        } else {
            workspace.to_path_buf()
        };

        // 1. Detect crates in workspace
        let known_crates = [
            ("kernel", "kernel bare-metal no_std"),
            ("builder", "generador de imágenes UEFI"),
            ("system/protocolo", "protocolo IPC tipado serde"),
            ("system/antosd", "demonio principal y runtime antOS"),
            (
                "system/capabilities",
                "catálogo de capacidades declarativas",
            ),
            ("system/barra", "shell de escritorio Wayland GTK4"),
        ];

        for (rel, _desc) in known_crates {
            if effective_root.join(rel).exists() {
                topo.crates.push(rel.to_string());
            }
        }

        // 2. Scan internal modules of antosd
        let main_rs = effective_root
            .join("system")
            .join("antosd")
            .join("src")
            .join("main.rs");
        if let Ok(content) = fs::read_to_string(&main_rs) {
            for line in content.lines() {
                let trimmed = line.trim();
                if (trimmed.starts_with("pub mod ") || trimmed.starts_with("mod "))
                    && trimmed.ends_with(';')
                {
                    let mod_name = trimmed
                        .trim_start_matches("pub mod ")
                        .trim_start_matches("mod ")
                        .trim_end_matches(';')
                        .trim()
                        .to_string();
                    if !mod_name.is_empty() && !topo.antosd_modules.contains(&mod_name) {
                        topo.antosd_modules.push(mod_name);
                    }
                }
            }
        }

        // 3. Scan IPC requests and events in system/protocolo/src/ipc.rs (or lib.rs)
        let proto_ipc = effective_root
            .join("system")
            .join("protocolo")
            .join("src")
            .join("ipc.rs");
        let proto_rs = if proto_ipc.exists() {
            proto_ipc
        } else {
            effective_root
                .join("system")
                .join("protocolo")
                .join("src")
                .join("lib.rs")
        };
        if let Ok(content) = fs::read_to_string(&proto_rs) {
            if let Some(req_start) = content.find("pub enum Request {") {
                if let Some(req_end) = content[req_start..].find("\n}") {
                    let req_body = &content[req_start..req_start + req_end];
                    topo.ipc_requests_count = req_body.matches("#[serde(alias").count();
                }
            }
            if let Some(ev_start) = content.find("pub enum Event {") {
                if let Some(ev_end) = content[ev_start..].find("\n}") {
                    let ev_body = &content[ev_start..ev_start + ev_end];
                    topo.ipc_events_count = ev_body.matches("#[serde(alias").count();
                }
            }
        }

        // 4. Scan capabilities in system/capabilities
        let caps_dir = effective_root.join("system").join("capabilities");
        if let Ok(entries) = fs::read_dir(&caps_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                    topo.capabilities_count += 1;
                    if let Ok(content) = fs::read_to_string(&path) {
                        if content.contains("tier = \"auto\"") {
                            topo.auto_caps_count += 1;
                        } else if content.contains("tier = \"confirm\"") {
                            topo.confirm_caps_count += 1;
                        } else if content.contains("tier = \"grant\"") {
                            topo.grant_caps_count += 1;
                        }
                    }
                }
            }
        }

        topo
    }

    /// Generates C4 components and workspace topology diagram in Mermaid.
    pub fn generate_components_diagram(topo: &WorkspaceTopology) -> String {
        format!(
            "graph TD\n\
            \x20\x20subgraph UI[\"🖥️ Shell & Escritorio\"]\n\
            \x20\x20\x20\x20Barra[\"system/barra (Wayland GTK4 Shell)\"]\n\
            \x20\x20\x20\x20VTE[\"Consola Terminal VTE & HUD\"]\n\
            \x20\x20\x20\x20Kanban[\"Panel Kanban de Agentes (Super + A)\"]\n\
            \x20\x20\x20\x20DiffViewer[\"Visor de Diffs Sintácticos\"]\n\
            \x20\x20end\n\n\
            \x20\x20subgraph PROTO[\"⚡ Protocolo IPC Tipado\"]\n\
            \x20\x20\x20\x20Protocolo[\"system/protocolo ({reqs} Peticiones, {evs} Eventos)\"]\n\
            \x20\x20end\n\n\
            \x20\x20subgraph DAEMON[\"🐜 Demonio del Sistema (system/antosd - {mods} Módulos)\"]\n\
            \x20\x20\x20\x20subgraph MultiAgente[\"Orquestación antFlow\"]\n\
            \x20\x20\x20\x20\x20\x20Architect[\"Arquitecto (Specs & Tickets)\"]\n\
            \x20\x20\x20\x20\x20\x20Coder[\"Coder (Worktree Patch)\"]\n\
            \x20\x20\x20\x20\x20\x20QA[\"QA (TDD & Regresiones)\"]\n\
            \x20\x20\x20\x20\x20\x20Auditor[\"Auditor (Certificación & Perf Diff)\"]\n\
            \x20\x20\x20\x20end\n\n\
            \x20\x20\x20\x20subgraph Seguridad[\"Aislamiento & Blindaje\"]\n\
            \x20\x20\x20\x20\x20\x20Seatbelt[\"Seatbelt / Landlock LSM\"]\n\
            \x20\x20\x20\x20\x20\x20Vault[\"Bóveda de Secretos & Grants\"]\n\
            \x20\x20\x20\x20\x20\x20Quota[\"Watchdog & Cuotas de Memoria\"]\n\
            \x20\x20\x20\x20end\n\n\
            \x20\x20\x20\x20subgraph Motores[\"Subsistemas de Desarrollo\"]\n\
            \x20\x20\x20\x20\x20\x20TimeMachine[\"Time Machine (Snapshots Atómicos)\"]\n\
            \x20\x20\x20\x20\x20\x20BenchEngine[\"Benchmarking Continuo (Perf Diff)\"]\n\
            \x20\x20\x20\x20\x20\x20ForgeEngine[\"Sincronización Git Forge (GitHub/GitLab)\"]\n\
            \x20\x20\x20\x20\x20\x20DocArch[\"Documentación Viva & Mermaid\"]\n\
            \x20\x20\x20\x20end\n\
            \x20\x20end\n\n\
            \x20\x20subgraph CAPS[\"📋 Catálogo de Capacidades Declarativas\"]\n\
            \x20\x20\x20\x20Capabilities[\"system/capabilities ({caps} capacidades: {auto} auto, {confirm} confirm, {grant} grant)\"]\n\
            \x20\x20end\n\n\
            \x20\x20subgraph BAREMETAL[\"⚙️ Núcleo & Arranque Bare-Metal\"]\n\
            \x20\x20\x20\x20Kernel[\"kernel (Rust no_std Bare-Metal)\"]\n\
            \x20\x20\x20\x20Builder[\"builder (Generador de Imágenes UEFI)\"]\n\
            \x20\x20end\n\n\
            \x20\x20UI -->|Unix Stream IPC| Protocolo\n\
            \x20\x20Protocolo -->|Despacho Serde| DAEMON\n\
            \x20\x20DAEMON -->|Radio de Impacto| CAPS\n\
            \x20\x20DAEMON -->|Ejecución Enjaulada| Seguridad\n\
            \x20\x20DAEMON -->|Syscalls / Init| BAREMETAL",
            reqs = topo.ipc_requests_count,
            evs = topo.ipc_events_count,
            mods = topo.antosd_modules.len(),
            caps = topo.capabilities_count,
            auto = topo.auto_caps_count,
            confirm = topo.confirm_caps_count,
            grant = topo.grant_caps_count,
        )
    }

    /// Generates IPC intent and reactive event dataflow diagram in Mermaid.
    pub fn generate_ipc_flow_diagram() -> String {
        "sequenceDiagram\n\
        \x20\x20autonumber\n\
        \x20\x20actor User as Desarrollador / UI (Barra)\n\
        \x20\x20participant IPC as Socket IPC Unix (antos-protocolo)\n\
        \x20\x20participant Daemon as Demonio antosd\n\
        \x20\x20participant Planner as Planificador IA (Local/Ollama)\n\
        \x20\x20participant Blast as Blast Radius & Cuotas\n\
        \x20\x20participant Sandbox as Recinto Seatbelt/Landlock\n\
        \x20\x20participant TM as Time Machine / Journal\n\n\
        \x20\x20User->>IPC: Request::Intent { text }\n\
        \x20\x20IPC->>Daemon: Despacho asíncrono serializado\n\
        \x20\x20Daemon->>Planner: plan(text, catalog)\n\
        \x20\x20Planner-->>Daemon: Propuesta de Pasos y Capacidades\n\
        \x20\x20Daemon->>Blast: Evaluar radio de impacto (Auto/Confirm/Grant)\n\
        \x20\x20Daemon->>TM: Instantánea atómica pre-ejecución\n\
        \x20\x20Daemon->>Sandbox: Iniciar proceso enjaulado con cuota estricta\n\
        \x20\x20Sandbox-->>Daemon: Resultado verificado y salida\n\
        \x20\x20Daemon->>TM: Registrar en bitácora inmutable (journal.jsonl)\n\
        \x20\x20Daemon-->>IPC: Event::Done / Event::Milestone\n\
        \x20\x20IPC-->>User: Actualización reactiva en HUD/Barra"
            .to_string()
    }

    /// Generates antFlow multi-agent lifecycle state diagram in Mermaid.
    pub fn generate_antflow_diagram() -> String {
        "stateDiagram-v2\n\
        \x20\x20[*] --> IssueImportado: antos issue import / docs/tickets/\n\
        \x20\x20IssueImportado --> Arquitecto: Análisis y Criterios de Aceptación\n\
        \x20\x20Arquitecto --> Coder: Despacho a Worktree Efímero Aislado\n\
        \x20\x20Coder --> Tester: Código implementado (AST validado por VFS Guard)\n\
        \x20\x20Tester --> Auditor: TDD Green (0 Panics, Tests 100% pasando)\n\
        \x20\x20Auditor --> BenchDiff: Evaluación de Rendimiento Continuo\n\
        \x20\x20BenchDiff --> Coder: Regresión Detectada (>15% latencia o RSS)\n\
        \x20\x20BenchDiff --> PullRequest: Rendimiento y Calidad Certificados\n\
        \x20\x20PullRequest --> [*]: antos pr create / Publicado en GitHub o GitLab"
            .to_string()
    }

    /// Generates the requested Mermaid diagram based on kind.
    pub fn generate_diagram(workspace: &Path, kind: ArchDiagramKind) -> ArchDiagramReport {
        let topo = Self::analyze_topology(workspace);

        let mermaid_content = match kind {
            ArchDiagramKind::Components => Self::generate_components_diagram(&topo),
            ArchDiagramKind::IpcFlow => Self::generate_ipc_flow_diagram(),
            ArchDiagramKind::AntFlow => Self::generate_antflow_diagram(),
            ArchDiagramKind::Full => {
                format!(
                    "### 1. Topología de Componentes y Límites de Seguridad\n\n\
                    ```mermaid\n{}\n```\n\n\
                    ### 2. Flujo de Datos IPC y Ciclo de Ejecución de Intenciones\n\n\
                    ```mermaid\n{}\n```\n\n\
                    ### 3. Ciclo de Vida Multi-Agente antFlow\n\n\
                    ```mermaid\n{}\n```",
                    Self::generate_components_diagram(&topo),
                    Self::generate_ipc_flow_diagram(),
                    Self::generate_antflow_diagram(),
                )
            }
        };

        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        ArchDiagramReport {
            kind,
            mermaid_content,
            crates_count: topo.crates.len(),
            modules_count: topo.antosd_modules.len(),
            caps_count: topo.capabilities_count,
            generated_at_secs: now_secs,
        }
    }

    /// Delimiters for live architecture documentation blocks.
    const START_DELIM: &'static str = "<!-- ANTOS_ARCH_START -->";
    const END_DELIM: &'static str = "<!-- ANTOS_ARCH_END -->";

    /// Formats the live block content wrapped in delimiters.
    pub fn format_markdown_block(diagram_report: &ArchDiagramReport) -> String {
        let diagram_body = match diagram_report.kind {
            ArchDiagramKind::Full => diagram_report.mermaid_content.clone(),
            _ => format!("```mermaid\n{}\n```", diagram_report.mermaid_content),
        };

        format!(
            "{start}\n\
            > 📐 **antOS Living Architecture (T21.3)** · Generado automáticamente a partir del código fuente.\n\
            > *Crates: {crates} | Módulos Demonio: {mods} | Capacidades: {caps}*\n\n\
            {diagram_body}\n\
            {end}",
            start = Self::START_DELIM,
            crates = diagram_report.crates_count,
            mods = diagram_report.modules_count,
            caps = diagram_report.caps_count,
            diagram_body = diagram_body,
            end = Self::END_DELIM,
        )
    }

    /// In-place updates architecture block inside a markdown file.
    pub fn update_markdown_file(file_path: &Path, block_content: &str) -> Result<bool> {
        if !file_path.exists() {
            // If file does not exist, create it with appropriate header and block
            if let Some(parent) = file_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let initial = format!(
                "# Arquitectura de antOS\n\n\
                Este documento contiene los diagramas vivos de arquitectura sincronizados continuamente.\n\n\
                {}\n",
                block_content
            );
            fs::write(file_path, initial)?;
            return Ok(true);
        }

        let original = fs::read_to_string(file_path)?;

        if let (Some(start_idx), Some(end_idx)) = (
            original.find(Self::START_DELIM),
            original.find(Self::END_DELIM),
        ) {
            if end_idx >= start_idx {
                let actual_end = end_idx + Self::END_DELIM.len();
                let before = &original[..start_idx];
                let after = &original[actual_end..];
                let new_content = format!("{}{}{}", before, block_content, after);

                if new_content != original {
                    fs::write(file_path, new_content)?;
                    return Ok(true);
                } else {
                    return Ok(false); // Already in sync
                }
            }
        }

        // Delimiters not found in existing file: append the block cleanly
        let mut new_content = original.clone();
        if !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str("\n## 📐 Diagramas Vivos de Arquitectura (T21.3)\n\n");
        new_content.push_str(block_content);
        new_content.push('\n');

        fs::write(file_path, new_content)?;
        Ok(true)
    }

    /// Scans and synchronizes live architecture documentation.
    pub fn sync_docs(workspace: &Path, target_file: Option<&str>) -> Result<DocSyncReport> {
        let effective_root = if workspace.join("system").join("antosd").exists() {
            workspace.to_path_buf()
        } else if let Some(root) = crate::git::detect_antos_root() {
            root
        } else {
            workspace.to_path_buf()
        };

        let targets = if let Some(t) = target_file {
            vec![PathBuf::from(t)]
        } else {
            vec![
                effective_root.join("docs").join("arquitectura.md"),
                effective_root.join("README.md"),
            ]
        };

        let report = Self::generate_diagram(workspace, ArchDiagramKind::Full);
        let block_content = Self::format_markdown_block(&report);

        let mut scanned = 0;
        let mut updated = 0;
        let mut updated_paths = Vec::new();

        for target in targets {
            let path = if target.is_absolute() {
                target
            } else if effective_root.join(&target).exists() || !workspace.join(&target).exists() {
                effective_root.join(&target)
            } else {
                workspace.join(&target)
            };
            scanned += 1;
            match Self::update_markdown_file(&path, &block_content) {
                Ok(changed) => {
                    if changed {
                        updated += 1;
                        updated_paths.push(path.display().to_string());
                    }
                }
                Err(e) => {
                    eprintln!("advertencia al sincronizar {}: {}", path.display(), e);
                }
            }
        }

        Ok(DocSyncReport {
            files_scanned: scanned,
            files_updated: updated,
            in_sync: true,
            updated_paths,
            message: format!(
                "Sincronización completada: {} archivo(s) actualizados de {} escaneados.",
                updated, scanned
            ),
        })
    }

    /// Checks if documentation files are in sync with workspace code.
    pub fn check_docs(workspace: &Path, target_file: Option<&str>) -> Result<DocSyncReport> {
        let effective_root = if workspace.join("system").join("antosd").exists() {
            workspace.to_path_buf()
        } else if let Some(root) = crate::git::detect_antos_root() {
            root
        } else {
            workspace.to_path_buf()
        };

        let targets = if let Some(t) = target_file {
            vec![PathBuf::from(t)]
        } else {
            vec![
                effective_root.join("docs").join("arquitectura.md"),
                effective_root.join("README.md"),
            ]
        };

        let report = Self::generate_diagram(workspace, ArchDiagramKind::Full);
        let expected_block = Self::format_markdown_block(&report);

        let mut scanned = 0;
        let mut out_of_sync_paths = Vec::new();

        for target in targets {
            let path = if target.is_absolute() {
                target
            } else if effective_root.join(&target).exists() || !workspace.join(&target).exists() {
                effective_root.join(&target)
            } else {
                workspace.join(&target)
            };
            if path.exists() {
                scanned += 1;
                let content = fs::read_to_string(&path)?;
                if let (Some(start_idx), Some(end_idx)) = (
                    content.find(Self::START_DELIM),
                    content.find(Self::END_DELIM),
                ) {
                    if end_idx >= start_idx {
                        let actual_end = end_idx + Self::END_DELIM.len();
                        let current_block = &content[start_idx..actual_end];
                        if current_block != expected_block {
                            out_of_sync_paths.push(path.display().to_string());
                        }
                    } else {
                        out_of_sync_paths.push(path.display().to_string());
                    }
                } else {
                    out_of_sync_paths.push(path.display().to_string());
                }
            }
        }

        let in_sync = out_of_sync_paths.is_empty();
        let message = if in_sync {
            "✅ La documentación de arquitectura está 100% sincronizada con el código fuente."
                .to_string()
        } else {
            format!(
                "⚠️ Divergencia detectada: {} archivo(s) desactualizados.",
                out_of_sync_paths.len()
            )
        };

        if !in_sync {
            bail!(
                "documentación de arquitectura desactualizada en: {:?}",
                out_of_sync_paths
            );
        }

        Ok(DocSyncReport {
            files_scanned: scanned,
            files_updated: 0,
            in_sync,
            updated_paths: out_of_sync_paths,
            message,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_topology_analysis_on_workspace() {
        let antos_root = crate::git::detect_antos_root().expect("antos root");
        let topo = DocArchEngine::analyze_topology(&antos_root);

        assert!(!topo.crates.is_empty(), "crates must not be empty");
        assert!(topo.crates.iter().any(|c| c.contains("system/antosd")));
        assert!(topo.crates.iter().any(|c| c.contains("system/protocolo")));
        assert!(
            !topo.antosd_modules.is_empty(),
            "antosd modules must not be empty"
        );
        assert!(
            topo.capabilities_count > 0,
            "must find capabilities in catalog"
        );
        assert!(topo.ipc_requests_count > 0, "must find IPC requests");
        assert!(topo.ipc_events_count > 0, "must find IPC events");
    }

    #[test]
    fn test_generate_mermaid_diagrams() {
        let antos_root = crate::git::detect_antos_root().expect("antos root");

        let r_comp = DocArchEngine::generate_diagram(&antos_root, ArchDiagramKind::Components);
        assert!(r_comp.mermaid_content.contains("graph TD"));
        assert!(r_comp.mermaid_content.contains("system/barra"));
        assert!(r_comp.mermaid_content.contains("system/protocolo"));

        let r_flow = DocArchEngine::generate_diagram(&antos_root, ArchDiagramKind::IpcFlow);
        assert!(r_flow.mermaid_content.contains("sequenceDiagram"));
        assert!(r_flow.mermaid_content.contains("Request::Intent"));

        let r_antflow = DocArchEngine::generate_diagram(&antos_root, ArchDiagramKind::AntFlow);
        assert!(r_antflow.mermaid_content.contains("stateDiagram-v2"));
        assert!(r_antflow.mermaid_content.contains("Arquitecto"));
        assert!(r_antflow.mermaid_content.contains("Auditor"));
    }

    #[test]
    fn test_markdown_in_place_synchronization_and_check() {
        let temp_dir = std::env::temp_dir().join(format!("test_doc_arch_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let doc_file = temp_dir.join("arch_test.md");

        let initial_text = "# Mi Proyecto\n\nTexto previo que debe preservarse.\n\n<!-- ANTOS_ARCH_START -->\nViejo diagrama\n<!-- ANTOS_ARCH_END -->\n\nTexto posterior que debe preservarse.\n";
        fs::write(&doc_file, initial_text).expect("write doc_file");

        let report = ArchDiagramReport {
            kind: ArchDiagramKind::Components,
            mermaid_content: "graph TD\n  A --> B".into(),
            crates_count: 3,
            modules_count: 10,
            caps_count: 15,
            generated_at_secs: 1000,
        };
        let block = DocArchEngine::format_markdown_block(&report);

        // 1. Update in-place
        let updated = DocArchEngine::update_markdown_file(&doc_file, &block).expect("update");
        assert!(updated);

        let content_after = fs::read_to_string(&doc_file).expect("read");
        assert!(content_after.contains("Texto previo que debe preservarse."));
        assert!(content_after.contains("Texto posterior que debe preservarse."));
        assert!(content_after.contains("graph TD"));
        assert!(!content_after.contains("Viejo diagrama"));

        // 2. Second update without changes returns false (already in sync)
        let updated_again =
            DocArchEngine::update_markdown_file(&doc_file, &block).expect("update again");
        assert!(!updated_again);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
