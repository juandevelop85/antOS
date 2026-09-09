//! Semantic Memory and Context Graph for antOS (Ticket T6.2).
//!
//! Indexes workspace source files, documentation, tickets, and code symbols into
//! a local vector store with cosine similarity retrieval and a relationship context graph.
//! Operates 100% offline with zero external cloud dependencies.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Type of semantic chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkKind {
    CodeFunction,
    CodeStruct,
    CodeModule,
    TicketSpec,
    Documentation,
    Configuration,
    General,
}

/// A chunk of text with metadata and a normalized vector embedding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticChunk {
    pub id: String,
    pub path: String,
    pub title: String,
    pub kind: ChunkKind,
    pub content: String,
    pub line_start: usize,
    pub line_end: usize,
    /// Term frequency or dense vector representation.
    pub vector: HashMap<String, f32>,
}

/// Relationship edge in the project context graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// File or module defines a symbol / struct / function.
    Defines,
    /// Code or doc references another file or symbol.
    References,
    /// Code or commit relates to a ticket (e.g. T1.1).
    ImplementsTicket,
    /// Module depends on or imports another module.
    Imports,
}

/// A node in the context graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub path: Option<String>,
}

/// An edge connecting two nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub kind: EdgeKind,
}

/// Project Context Graph mapping relationships between files, symbols, and tickets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextGraph {
    pub nodes: BTreeMap<String, GraphNode>,
    pub edges: Vec<GraphEdge>,
}

impl ContextGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, id: &str, label: &str, kind: &str, path: Option<&str>) {
        if !self.nodes.contains_key(id) {
            self.nodes.insert(
                id.to_string(),
                GraphNode {
                    id: id.to_string(),
                    label: label.to_string(),
                    kind: kind.to_string(),
                    path: path.map(String::from),
                },
            );
        }
    }

    pub fn add_edge(&mut self, source: &str, target: &str, kind: EdgeKind) {
        let exists = self.edges.iter().any(|e| e.source == source && e.target == target && e.kind == kind);
        if !exists {
            self.edges.push(GraphEdge {
                source: source.to_string(),
                target: target.to_string(),
                kind,
            });
        }
    }

    pub fn related_to(&self, query_id: &str) -> Vec<(&GraphNode, &EdgeKind)> {
        let mut results = Vec::new();
        for edge in &self.edges {
            if edge.source == query_id {
                if let Some(node) = self.nodes.get(&edge.target) {
                    results.push((node, &edge.kind));
                }
            } else if edge.target == query_id {
                if let Some(node) = self.nodes.get(&edge.source) {
                    results.push((node, &edge.kind));
                }
            }
        }
        results
    }
}

/// Search result item with relevance score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub score: f32,
    pub chunk_id: String,
    pub path: String,
    pub title: String,
    pub kind: ChunkKind,
    pub snippet: String,
    pub line_start: usize,
    pub line_end: usize,
}

/// Persistent memory store holding chunks and graph.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryStore {
    pub version: u32,
    pub last_indexed: u64,
    pub chunks: Vec<SemanticChunk>,
    pub graph: ContextGraph,
}

/// Core engine for semantic indexing and vector search.
pub struct MemoryEngine;

impl MemoryEngine {
    /// Loads the memory store from disk or creates an empty one.
    pub fn load(db_path: &Path) -> Result<MemoryStore> {
        if db_path.exists() {
            let content = std::fs::read_to_string(db_path)
                .with_context(|| format!("failed to read memory store from {}", db_path.display()))?;
            let store: MemoryStore = serde_json::from_str(&content)
                .with_context(|| "failed to parse memory store JSON")?;
            Ok(store)
        } else {
            Ok(MemoryStore {
                version: 1,
                last_indexed: 0,
                chunks: Vec::new(),
                graph: ContextGraph::new(),
            })
        }
    }

    /// Saves the memory store to disk atomically.
    pub fn save(store: &MemoryStore, db_path: &Path) -> Result<()> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
        let serialized = serde_json::to_string_pretty(store)
            .context("failed to serialize memory store")?;
        let tmp_path = db_path.with_extension("tmp");
        std::fs::write(&tmp_path, serialized)
            .with_context(|| format!("failed to write temporary file {}", tmp_path.display()))?;
        std::fs::rename(&tmp_path, db_path)
            .with_context(|| format!("failed to rename to {}", db_path.display()))?;
        Ok(())
    }

    /// Discovers the memory database path for a workspace.
    pub fn default_db_path(workspace: &Path) -> PathBuf {
        workspace.join(".antos").join("memory.json")
    }

    /// Indexes the workspace files, parses code blocks, and builds the context graph.
    pub fn index_workspace(workspace: &Path) -> Result<MemoryStore> {
        let mut chunks = Vec::new();
        let mut graph = ContextGraph::new();

        let files = scan_workspace_files(workspace)?;

        for file_path in files {
            let rel_path = match file_path.strip_prefix(workspace) {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(_) => file_path.to_string_lossy().to_string(),
            };

            // Register file node in context graph
            let file_node_id = format!("file:{}", rel_path);
            graph.add_node(&file_node_id, &rel_path, "file", Some(&rel_path));

            let content = match std::fs::read_to_string(&file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let ext = file_path.extension().and_then(|s| s.to_str()).unwrap_or_default();
            let parsed_chunks = parse_file_chunks(&rel_path, &content, ext);

            for chunk in parsed_chunks {
                // Link chunk symbols to file in graph
                if chunk.kind == ChunkKind::CodeFunction || chunk.kind == ChunkKind::CodeStruct {
                    let sym_id = format!("symbol:{}:{}", rel_path, chunk.title);
                    graph.add_node(&sym_id, &chunk.title, "symbol", Some(&rel_path));
                    graph.add_edge(&file_node_id, &sym_id, EdgeKind::Defines);
                } else if chunk.kind == ChunkKind::TicketSpec {
                    let ticket_id = chunk.title.split('·').next().unwrap_or(&chunk.title).trim();
                    let ticket_node_id = format!("ticket:{}", ticket_id);
                    graph.add_node(&ticket_node_id, ticket_id, "ticket", Some(&rel_path));
                    graph.add_edge(&file_node_id, &ticket_node_id, EdgeKind::Defines);
                }

                // Check for ticket references like T1.1, T2.2 in content
                for word in chunk.content.split_whitespace() {
                    let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.');
                    if clean.starts_with('T') && clean.contains('.') && clean.len() >= 4 && clean.len() <= 6 {
                        let ticket_node_id = format!("ticket:{}", clean);
                        graph.add_node(&ticket_node_id, clean, "ticket", None);
                        graph.add_edge(&file_node_id, &ticket_node_id, EdgeKind::ImplementsTicket);
                    }
                }

                chunks.push(chunk);
            }
        }

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Ok(MemoryStore {
            version: 1,
            last_indexed: now,
            chunks,
            graph,
        })
    }

    /// Performs semantic vector search using cosine similarity on term vectors.
    pub fn search(store: &MemoryStore, query: &str, limit: usize) -> Vec<SearchHit> {
        let query_vector = compute_term_vector(query);
        if query_vector.is_empty() {
            return Vec::new();
        }

        let mut scored_hits = Vec::new();

        for chunk in &store.chunks {
            let score = cosine_similarity(&query_vector, &chunk.vector);
            if score > 0.05 {
                let snippet = make_snippet(&chunk.content, 200);
                scored_hits.push(SearchHit {
                    score,
                    chunk_id: chunk.id.clone(),
                    path: chunk.path.clone(),
                    title: chunk.title.clone(),
                    kind: chunk.kind,
                    snippet,
                    line_start: chunk.line_start,
                    line_end: chunk.line_end,
                });
            }
        }

        // Sort descending by score
        scored_hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored_hits.truncate(limit);
        scored_hits
    }
}

/// Recursively scans workspace files ignoring build outputs and version control.
fn scan_workspace_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let ignored_names: BTreeSet<&str> = [
        "target", ".git", "node_modules", ".antos", ".idea", ".vscode", "dist", "build",
    ]
    .into_iter()
    .collect();

    let mut stack = vec![dir.to_path_buf()];

    while let Some(current_dir) = stack.pop() {
        let read_dir = match std::fs::read_dir(&current_dir) {
            Ok(rd) => rd,
            Err(_) => continue,
        };

        for entry in read_dir.flatten() {
            let path = entry.path();
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };

            if ignored_names.contains(name) || name.starts_with('.') {
                continue;
            }

            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
                if matches!(ext, "rs" | "toml" | "md" | "json" | "sh" | "c" | "h" | "nix") {
                    files.push(path);
                }
            }
        }
    }

    files.sort();
    Ok(files)
}

/// Parses a file into semantic chunks based on syntax or structure.
fn parse_file_chunks(rel_path: &str, content: &str, ext: &str) -> Vec<SemanticChunk> {
    let mut chunks = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() {
        return chunks;
    }

    if ext == "md" {
        // Markdown: chunk by headings
        let mut current_title = rel_path.to_string();
        let mut current_lines = Vec::new();
        let mut start_idx = 1;
        let is_ticket = rel_path.contains("tickets/");

        for (i, line) in lines.iter().enumerate() {
            if line.starts_with("# ") || line.starts_with("## ") {
                if !current_lines.is_empty() {
                    let chunk_text = current_lines.join("\n");
                    let kind = if is_ticket { ChunkKind::TicketSpec } else { ChunkKind::Documentation };
                    let vector = compute_term_vector(&chunk_text);
                    chunks.push(SemanticChunk {
                        id: format!("{}:{}-{}", rel_path, start_idx, i),
                        path: rel_path.to_string(),
                        title: current_title.clone(),
                        kind,
                        content: chunk_text,
                        line_start: start_idx,
                        line_end: i,
                        vector,
                    });
                    current_lines.clear();
                }
                current_title = line.trim_start_matches('#').trim().to_string();
                start_idx = i + 1;
            }
            current_lines.push(*line);
        }

        if !current_lines.is_empty() {
            let chunk_text = current_lines.join("\n");
            let kind = if is_ticket { ChunkKind::TicketSpec } else { ChunkKind::Documentation };
            let vector = compute_term_vector(&chunk_text);
            chunks.push(SemanticChunk {
                id: format!("{}:{}-{}", rel_path, start_idx, lines.len()),
                path: rel_path.to_string(),
                title: current_title,
                kind,
                content: chunk_text,
                line_start: start_idx,
                line_end: lines.len(),
                vector,
            });
        }
    } else if ext == "rs" {
        // Rust code: chunk by functions, structs, enums or modules
        let mut current_title = format!("module {}", rel_path);
        let mut current_lines = Vec::new();
        let mut current_kind = ChunkKind::CodeModule;
        let mut start_idx = 1;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            let is_symbol_decl = trimmed.starts_with("pub fn ")
                || trimmed.starts_with("fn ")
                || trimmed.starts_with("pub struct ")
                || trimmed.starts_with("struct ")
                || trimmed.starts_with("pub enum ")
                || trimmed.starts_with("enum ");

            if is_symbol_decl && !current_lines.is_empty() && current_lines.len() >= 5 {
                let chunk_text = current_lines.join("\n");
                let vector = compute_term_vector(&chunk_text);
                chunks.push(SemanticChunk {
                    id: format!("{}:{}-{}", rel_path, start_idx, i),
                    path: rel_path.to_string(),
                    title: current_title.clone(),
                    kind: current_kind,
                    content: chunk_text,
                    line_start: start_idx,
                    line_end: i,
                    vector,
                });
                current_lines.clear();
                start_idx = i + 1;
            }

            if is_symbol_decl {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if let Some(pos) = parts.iter().position(|p| *p == "fn" || *p == "struct" || *p == "enum") {
                    if let Some(name) = parts.get(pos + 1) {
                        let clean_name = name.split('(').next().unwrap_or(name).trim();
                        current_title = clean_name.to_string();
                        current_kind = if parts[pos] == "fn" {
                            ChunkKind::CodeFunction
                        } else {
                            ChunkKind::CodeStruct
                        };
                    }
                }
            }

            current_lines.push(*line);
        }

        if !current_lines.is_empty() {
            let chunk_text = current_lines.join("\n");
            let vector = compute_term_vector(&chunk_text);
            chunks.push(SemanticChunk {
                id: format!("{}:{}-{}", rel_path, start_idx, lines.len()),
                path: rel_path.to_string(),
                title: current_title,
                kind: current_kind,
                content: chunk_text,
                line_start: start_idx,
                line_end: lines.len(),
                vector,
            });
        }
    } else {
        // General chunking in blocks of ~60 lines
        for (chunk_idx, window) in lines.chunks(60).enumerate() {
            let start = chunk_idx * 60 + 1;
            let end = start + window.len() - 1;
            let chunk_text = window.join("\n");
            let vector = compute_term_vector(&chunk_text);
            let kind = if ext == "toml" || ext == "json" {
                ChunkKind::Configuration
            } else {
                ChunkKind::General
            };
            chunks.push(SemanticChunk {
                id: format!("{}:{}-{}", rel_path, start, end),
                path: rel_path.to_string(),
                title: format!("{} ({}-{})", rel_path, start, end),
                kind,
                content: chunk_text,
                line_start: start,
                line_end: end,
                vector,
            });
        }
    }

    chunks
}

/// Computes normalized term frequencies for cosine similarity.
pub fn compute_term_vector(text: &str) -> HashMap<String, f32> {
    let mut counts: HashMap<String, f32> = HashMap::new();
    let mut total = 0.0;

    for word in tokenize(text) {
        *counts.entry(word).or_insert(0.0) += 1.0;
        total += 1.0;
    }

    if total == 0.0 {
        return counts;
    }

    // Term frequency normalized by Euclidean L2 norm
    let mut sum_sq = 0.0;
    for val in counts.values_mut() {
        *val /= total;
        sum_sq += (*val) * (*val);
    }

    let norm = sum_sq.sqrt();
    if norm > 0.0 {
        for val in counts.values_mut() {
            *val /= norm;
        }
    }

    counts
}

/// Cosine similarity between two normalized vectors.
pub fn cosine_similarity(v1: &HashMap<String, f32>, v2: &HashMap<String, f32>) -> f32 {
    if v1.is_empty() || v2.is_empty() {
        return 0.0;
    }

    let (smaller, larger) = if v1.len() < v2.len() { (v1, v2) } else { (v2, v1) };
    let mut dot = 0.0;
    for (k, val1) in smaller {
        if let Some(val2) = larger.get(k) {
            dot += val1 * val2;
        }
    }
    dot
}

/// Tokenizer splitting words, camelCase and snake_case into searchable terms.
fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for raw in text.split(|c: char| !c.is_alphanumeric() && c != '_') {
        let trimmed = raw.trim();
        if trimmed.len() < 2 {
            continue;
        }

        // Add whole token in lowercase
        tokens.push(trimmed.to_lowercase());

        // Split snake_case
        if trimmed.contains('_') {
            for sub in trimmed.split('_') {
                if sub.len() >= 2 {
                    tokens.push(sub.to_lowercase());
                }
            }
        }

        // Split camelCase
        let mut current = String::new();
        for ch in trimmed.chars() {
            if ch.is_uppercase() && !current.is_empty() {
                if current.len() >= 2 {
                    tokens.push(current.to_lowercase());
                }
                current.clear();
            }
            current.push(ch);
        }
        if current.len() >= 2 {
            tokens.push(current.to_lowercase());
        }
    }
    tokens
}

fn make_snippet(content: &str, max_chars: usize) -> String {
    let clean: String = content.lines().take(4).collect::<Vec<_>>().join(" ");
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.len() <= max_chars {
        clean
    } else {
        format!("{}…", &clean[..max_chars])
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_compute_vector_and_similarity() {
        let v1 = compute_term_vector("fn create_worktree(repo: &str) -> Result<PathBuf>");
        let v2 = compute_term_vector("worktree ephemeral git repository path");
        let v3 = compute_term_vector("postgres database ephemeral service port 5432");

        let sim_12 = cosine_similarity(&v1, &v2);
        let sim_13 = cosine_similarity(&v1, &v3);

        assert!(sim_12 > 0.1, "worktree terms should have positive similarity");
        assert!(sim_12 > sim_13, "worktree should be more similar than postgres");
    }

    #[test]
    fn test_context_graph_relationships() {
        let mut graph = ContextGraph::new();
        graph.add_node("file:src/git.rs", "src/git.rs", "file", Some("src/git.rs"));
        graph.add_node("symbol:src/git.rs:create_worktree", "create_worktree", "symbol", Some("src/git.rs"));
        graph.add_node("ticket:T2.2", "T2.2", "ticket", None);

        graph.add_edge("file:src/git.rs", "symbol:src/git.rs:create_worktree", EdgeKind::Defines);
        graph.add_edge("file:src/git.rs", "ticket:T2.2", EdgeKind::ImplementsTicket);

        let related = graph.related_to("file:src/git.rs");
        assert_eq!(related.len(), 2);
    }

    #[test]
    fn test_index_and_search_mock() {
        let temp_dir = std::env::temp_dir().join(format!("antos-mem-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("src")).expect("create dir");

        let code = r#"
pub fn kill_port_listener(port: u16) -> Result<()> {
    // Diagnostico y liberacion de puertos en antOS
    Ok(())
}
"#;
        std::fs::write(temp_dir.join("src/net.rs"), code).expect("write code");

        let store = MemoryEngine::index_workspace(&temp_dir).expect("index");
        assert!(!store.chunks.is_empty());

        let hits = MemoryEngine::search(&store, "liberar puerto listener", 5);
        assert!(!hits.is_empty(), "debe encontrar la funcion de liberacion de puertos");
        assert_eq!(hits[0].path, "src/net.rs");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
