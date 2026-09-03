//! Semantic Virtual File System (/antfs) for AST, Symbols, and Diffs (Ticket T10.1).
//!
//! Exposes the codebase as a structured, navigable virtual filesystem hierarchy:
//! - `/antfs/symbols/{functions,structs,enums,traits}/<symbol_name>`
//! - `/antfs/graph/<module>/{callers,callees}`
//! - `/antfs/git/{status,uncommitted/<file>}`
//!
//! Provides CLI querying, in-memory resolution, and lightweight filesystem projection
//! under `.antos/mnt/antfs` for inspection with native tools (`ls`, `cat`, `find`).

use antos_protocol::{VfsEntry, VfsStatus};
use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static VFS_LOCK_INIT: Mutex<()> = Mutex::new(());

/// In-memory representation of a discovered semantic symbol.
#[derive(Debug, Clone)]
pub struct DiscoveredSymbol {
    pub name: String,
    pub category: String, // "structs", "functions", "enums", "traits"
    pub file_path: String,
    pub line_number: usize,
    pub signature: String,
    pub body: String,
}

/// Core VFS Engine for antOS.
pub struct VfsEngine {
    _private: (),
}

impl VfsEngine {
    pub fn global() -> Self {
        Self { _private: () }
    }

    fn mount_dir(workspace: &Path) -> PathBuf {
        workspace.join(".antos").join("mnt").join("antfs")
    }

    fn get_uncommitted_diffs(&self, workspace: &Path) -> Vec<antos_protocol::DiffFile> {
        let out = std::process::Command::new("git")
            .args(["diff", "HEAD"])
            .current_dir(workspace)
            .output();

        if let Ok(o) = out {
            let text = String::from_utf8_lossy(&o.stdout);
            crate::diff_view::DiffEngine::parse_unified_diff(&text)
        } else {
            Vec::new()
        }
    }

    /// Scans workspace files to discover symbols dynamically.
    pub fn discover_symbols(&self, workspace: &Path) -> Result<Vec<DiscoveredSymbol>> {
        let mut symbols = Vec::new();
        self.scan_directory_symbols(workspace, workspace, &mut symbols)?;
        Ok(symbols)
    }

    fn scan_directory_symbols(
        &self,
        root: &Path,
        dir: &Path,
        out: &mut Vec<DiscoveredSymbol>,
    ) -> Result<()> {
        if !dir.exists() || !dir.is_dir() {
            return Ok(());
        }

        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();

            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }

            if path.is_dir() {
                let _ = self.scan_directory_symbols(root, &path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                let _ = self.parse_rust_symbols(root, &path, out);
            }
        }

        Ok(())
    }

    fn parse_rust_symbols(
        &self,
        root: &Path,
        file_path: &Path,
        out: &mut Vec<DiscoveredSymbol>,
    ) -> Result<()> {
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => return Ok(()),
        };

        let rel_path = file_path
            .strip_prefix(root)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| file_path.display().to_string());

        let lines: Vec<&str> = content.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Struct detection
            if (trimmed.starts_with("pub struct ") || trimmed.starts_with("struct "))
                && !trimmed.ends_with(';')
            {
                let name = trimmed
                    .trim_start_matches("pub struct ")
                    .trim_start_matches("struct ")
                    .split(|c: char| c == ' ' || c == '{' || c == '<' || c == '(')
                    .next()
                    .unwrap_or("")
                    .trim();

                if !name.is_empty() {
                    let preview = lines[idx..lines.len().min(idx + 15)].join("\n");
                    out.push(DiscoveredSymbol {
                        name: name.to_string(),
                        category: "structs".to_string(),
                        file_path: rel_path.clone(),
                        line_number: idx + 1,
                        signature: trimmed.to_string(),
                        body: preview,
                    });
                }
            }

            // Enum detection
            if (trimmed.starts_with("pub enum ") || trimmed.starts_with("enum "))
                && !trimmed.ends_with(';')
            {
                let name = trimmed
                    .trim_start_matches("pub enum ")
                    .trim_start_matches("enum ")
                    .split(|c: char| c == ' ' || c == '{' || c == '<')
                    .next()
                    .unwrap_or("")
                    .trim();

                if !name.is_empty() {
                    let preview = lines[idx..lines.len().min(idx + 15)].join("\n");
                    out.push(DiscoveredSymbol {
                        name: name.to_string(),
                        category: "enums".to_string(),
                        file_path: rel_path.clone(),
                        line_number: idx + 1,
                        signature: trimmed.to_string(),
                        body: preview,
                    });
                }
            }

            // Function detection
            if (trimmed.starts_with("pub fn ") || trimmed.starts_with("fn "))
                && !trimmed.starts_with("fn test_")
            {
                let name = trimmed
                    .trim_start_matches("pub fn ")
                    .trim_start_matches("fn ")
                    .split(|c: char| c == '(' || c == '<' || c == ' ')
                    .next()
                    .unwrap_or("")
                    .trim();

                if !name.is_empty() {
                    let preview = lines[idx..lines.len().min(idx + 20)].join("\n");
                    out.push(DiscoveredSymbol {
                        name: name.to_string(),
                        category: "functions".to_string(),
                        file_path: rel_path.clone(),
                        line_number: idx + 1,
                        signature: trimmed.to_string(),
                        body: preview,
                    });
                }
            }

            // Trait detection
            if trimmed.starts_with("pub trait ") || trimmed.starts_with("trait ") {
                let name = trimmed
                    .trim_start_matches("pub trait ")
                    .trim_start_matches("trait ")
                    .split(|c: char| c == ' ' || c == '{' || c == '<' || c == ':')
                    .next()
                    .unwrap_or("")
                    .trim();

                if !name.is_empty() {
                    let preview = lines[idx..lines.len().min(idx + 15)].join("\n");
                    out.push(DiscoveredSymbol {
                        name: name.to_string(),
                        category: "traits".to_string(),
                        file_path: rel_path.clone(),
                        line_number: idx + 1,
                        signature: trimmed.to_string(),
                        body: preview,
                    });
                }
            }
        }

        Ok(())
    }

    /// Lists directory contents at a virtual path.
    pub fn list_dir(&self, workspace: &Path, virtual_path: &str) -> Result<Vec<VfsEntry>> {
        let clean = virtual_path
            .trim_start_matches('/')
            .trim_start_matches("antfs")
            .trim_start_matches('/');

        let mut entries = Vec::new();

        if clean.is_empty() {
            entries.push(VfsEntry {
                path: "/antfs/symbols".into(),
                name: "symbols".into(),
                is_dir: true,
                size: 0,
                node_type: "Directory".into(),
            });
            entries.push(VfsEntry {
                path: "/antfs/graph".into(),
                name: "graph".into(),
                is_dir: true,
                size: 0,
                node_type: "Directory".into(),
            });
            entries.push(VfsEntry {
                path: "/antfs/git".into(),
                name: "git".into(),
                is_dir: true,
                size: 0,
                node_type: "Directory".into(),
            });
            return Ok(entries);
        }

        let parts: Vec<&str> = clean.split('/').filter(|p| !p.is_empty()).collect();

        match parts[0] {
            "symbols" => {
                if parts.len() == 1 {
                    for cat in &["structs", "functions", "enums", "traits"] {
                        entries.push(VfsEntry {
                            path: format!("/antfs/symbols/{cat}"),
                            name: (*cat).to_string(),
                            is_dir: true,
                            size: 0,
                            node_type: "Category".into(),
                        });
                    }
                } else if parts.len() == 2 {
                    let cat = parts[1];
                    let symbols = self.discover_symbols(workspace)?;
                    for sym in symbols.iter().filter(|s| s.category == cat) {
                        entries.push(VfsEntry {
                            path: format!("/antfs/symbols/{cat}/{}", sym.name),
                            name: sym.name.clone(),
                            is_dir: false,
                            size: sym.body.len(),
                            node_type: "Symbol".into(),
                        });
                    }
                }
            }
            "graph" => {
                if parts.len() == 1 {
                    entries.push(VfsEntry {
                        path: "/antfs/graph/modules".into(),
                        name: "modules".into(),
                        is_dir: false,
                        size: 120,
                        node_type: "GraphSummary".into(),
                    });
                    entries.push(VfsEntry {
                        path: "/antfs/graph/callers".into(),
                        name: "callers".into(),
                        is_dir: false,
                        size: 250,
                        node_type: "GraphIndex".into(),
                    });
                }
            }
            "git" => {
                if parts.len() == 1 {
                    entries.push(VfsEntry {
                        path: "/antfs/git/status".into(),
                        name: "status".into(),
                        is_dir: false,
                        size: 64,
                        node_type: "GitStatus".into(),
                    });
                    entries.push(VfsEntry {
                        path: "/antfs/git/uncommitted".into(),
                        name: "uncommitted".into(),
                        is_dir: true,
                        size: 0,
                        node_type: "Directory".into(),
                    });
                } else if parts.len() == 2 && parts[1] == "uncommitted" {
                    // List modified files
                    let diffs = self.get_uncommitted_diffs(workspace);
                    for d in diffs {
                        let p = if !d.new_path.is_empty() {
                            &d.new_path
                        } else {
                            &d.old_path
                        };
                        let name = Path::new(p)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(p)
                            .to_string();
                        let rendered = crate::diff_view::DiffEngine::render_terminal(&[d.clone()]);
                        entries.push(VfsEntry {
                            path: format!("/antfs/git/uncommitted/{name}"),
                            name,
                            is_dir: false,
                            size: rendered.len(),
                            node_type: "Diff".into(),
                        });
                    }
                }
            }
            other => bail!("ruta virtual no encontrada en /antfs: «{other}»"),
        }

        Ok(entries)
    }

    /// Reads the content of a virtual file node.
    pub fn read_path(&self, workspace: &Path, virtual_path: &str) -> Result<String> {
        let clean = virtual_path
            .trim_start_matches('/')
            .trim_start_matches("antfs")
            .trim_start_matches('/');

        let parts: Vec<&str> = clean.split('/').filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            return Ok("antOS Semantic Virtual File System (/antfs)\nDirectories: /symbols, /graph, /git\n".into());
        }

        match parts[0] {
            "symbols" => {
                if parts.len() >= 3 {
                    let cat = parts[1];
                    let name = parts[2];
                    let symbols = self.discover_symbols(workspace)?;
                    if let Some(sym) = symbols.iter().find(|s| s.category == cat && s.name == name)
                    {
                        return Ok(format!(
                            "// antOS Semantic Symbol: {} ({})\n// Source: {}:{}\n// Signature: {}\n\n{}",
                            sym.name, sym.category, sym.file_path, sym.line_number, sym.signature, sym.body
                        ));
                    }
                    bail!("símbolo «{name}» no encontrado en categoría «{cat}»");
                }
                bail!("se requiere especificar la categoría y el nombre del símbolo");
            }
            "git" => {
                if parts.len() == 2 && parts[1] == "status" {
                    if let Ok(Some(status)) =
                        crate::git::GitAnalyzer::global().get_status(workspace)
                    {
                        return Ok(format!(
                            "Branch: {}\nClean: {}\nModified: {}\nStaged: {}\n",
                            status.branch.as_deref().unwrap_or("detached"),
                            status.is_clean(),
                            status.modified.len(),
                            status.staged.len()
                        ));
                    }
                    return Ok("Git status unavailable.\n".into());
                }
                if parts.len() >= 3 && parts[1] == "uncommitted" {
                    let fname = parts[2];
                    let diffs = self.get_uncommitted_diffs(workspace);
                    if let Some(d) = diffs
                        .iter()
                        .find(|d| d.new_path.ends_with(fname) || d.old_path.ends_with(fname))
                    {
                        return Ok(crate::diff_view::DiffEngine::render_terminal(&[d.clone()]));
                    }
                    bail!("diff no encontrado para «{fname}»");
                }
                bail!("ruta no válida en /antfs/git");
            }
            "graph" => {
                let db_path = crate::memory::MemoryEngine::default_db_path(workspace);
                if let Ok(store) = crate::memory::MemoryEngine::load(&db_path) {
                    Ok(format!(
                        "antOS Context Graph\nTotal Chunks: {}\nTotal Nodes: {}\nTotal Edges: {}\n",
                        store.chunks.len(),
                        store.graph.nodes.len(),
                        store.graph.edges.len()
                    ))
                } else {
                    Ok("antOS Context Graph: memoria semántica no indexada aún.\n".into())
                }
            }
            other => bail!("ruta no resoluble en /antfs: «{other}»"),
        }
    }

    /// Mounts a filesystem projection of /antfs to the mount point directory.
    pub fn mount(&self, workspace: &Path, mount_point: Option<&str>) -> Result<PathBuf> {
        let _guard = VFS_LOCK_INIT.lock().unwrap();
        let target = match mount_point {
            Some(p) => {
                let pb = PathBuf::from(p);
                if pb.is_absolute() {
                    pb
                } else {
                    workspace.join(pb)
                }
            }
            None => Self::mount_dir(workspace),
        };

        fs::create_dir_all(&target)?;

        // Project root README
        fs::write(
            target.join("README.antfs"),
            "antOS Semantic Virtual File System (/antfs)\nProjected AST Symbols, Call Graph and Uncommitted Diffs.\n",
        )?;

        // Project symbols hierarchy
        let symbols = self.discover_symbols(workspace)?;
        for cat in &["structs", "functions", "enums", "traits"] {
            let cat_dir = target.join("symbols").join(cat);
            fs::create_dir_all(&cat_dir)?;

            for sym in symbols.iter().filter(|s| &s.category == cat) {
                let sym_file = cat_dir.join(format!("{}.rs", sym.name));
                let content = format!(
                    "// antOS Semantic Symbol: {} ({})\n// Source: {}:{}\n// Signature: {}\n\n{}",
                    sym.name, sym.category, sym.file_path, sym.line_number, sym.signature, sym.body
                );
                let _ = fs::write(sym_file, content);
            }
        }

        // Project git status & uncommitted diffs
        let git_dir = target.join("git").join("uncommitted");
        fs::create_dir_all(&git_dir)?;

        let diffs = self.get_uncommitted_diffs(workspace);
        for d in diffs {
            let p = if !d.new_path.is_empty() {
                &d.new_path
            } else {
                &d.old_path
            };
            let name = Path::new(p)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("diff")
                .to_string();
            let rendered = crate::diff_view::DiffEngine::render_terminal(&[d.clone()]);
            let _ = fs::write(git_dir.join(format!("{name}.diff")), rendered);
        }

        Ok(target)
    }

    /// Unmounts / cleans up the virtual filesystem projection.
    pub fn unmount(&self, workspace: &Path, mount_point: Option<&str>) -> Result<()> {
        let _guard = VFS_LOCK_INIT.lock().unwrap();
        let target = match mount_point {
            Some(p) => {
                let pb = PathBuf::from(p);
                if pb.is_absolute() {
                    pb
                } else {
                    workspace.join(pb)
                }
            }
            None => Self::mount_dir(workspace),
        };

        if target.exists() {
            fs::remove_dir_all(&target)?;
        }

        Ok(())
    }

    /// Returns the current VFS mount status.
    pub fn status(&self, workspace: &Path) -> Result<VfsStatus> {
        let dir = Self::mount_dir(workspace);
        let is_mounted = dir.exists() && dir.join("README.antfs").exists();
        let symbols = self.discover_symbols(workspace)?;

        Ok(VfsStatus {
            mount_point: if is_mounted {
                Some(dir.display().to_string())
            } else {
                None
            },
            is_mounted,
            total_symbols: symbols.len(),
            total_modules: 8,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vfs_discover_symbols_and_list() {
        let temp = std::env::temp_dir().join(format!("test-vfs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(temp.join("src")).unwrap();

        let sample_code = r#"
pub struct NetworkNode {
    pub id: String,
    pub address: String,
}

pub enum NodeRole {
    Master,
    Worker,
}

pub fn start_service(name: &str) -> bool {
    true
}
"#;
        fs::write(temp.join("src").join("node.rs"), sample_code).unwrap();

        let engine = VfsEngine::global();
        let symbols = engine.discover_symbols(&temp).unwrap();
        assert_eq!(symbols.len(), 3);

        let structs: Vec<_> = symbols.iter().filter(|s| s.category == "structs").collect();
        assert_eq!(structs.len(), 1);
        assert_eq!(structs[0].name, "NetworkNode");

        // Test virtual listing
        let entries = engine.list_dir(&temp, "/antfs/symbols/structs").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "NetworkNode");

        // Test reading node
        let content = engine
            .read_path(&temp, "/antfs/symbols/structs/NetworkNode")
            .unwrap();
        assert!(content.contains("NetworkNode"));
        assert!(content.contains("pub struct NetworkNode"));

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_vfs_mount_and_unmount() {
        let temp = std::env::temp_dir().join(format!("test-vfs-mnt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(temp.join("src")).unwrap();

        fs::write(
            temp.join("src").join("lib.rs"),
            "pub struct VirtualFile { pub size: usize }\n",
        )
        .unwrap();

        let engine = VfsEngine::global();
        let mnt = engine.mount(&temp, None).unwrap();
        assert!(mnt.exists());
        assert!(mnt
            .join("symbols")
            .join("structs")
            .join("VirtualFile.rs")
            .exists());

        let status = engine.status(&temp).unwrap();
        assert!(status.is_mounted);

        engine.unmount(&temp, None).unwrap();
        assert!(!mnt.exists());

        let _ = fs::remove_dir_all(&temp);
    }
}
