//! Interactive Syntax-Highlighted Diff Viewer and Patch Engine (Ticket T8.1).
//!
//! Parses unified git diffs into structured file hunks, performs syntax highlighting,
//! renders side-by-side or unified views with line numbers, and allows granular patch applications.

use antos_protocol::{DiffFile, DiffHunk, DiffLine, DiffLineKind, SyntaxToken, SyntaxTokenType};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

/// Core engine for parsing and rendering interactive diffs.
pub struct DiffEngine;

impl DiffEngine {
    /// Parses a raw unified diff string into structured `DiffFile` instances.
    pub fn parse_unified_diff(raw: &str) -> Vec<DiffFile> {
        let mut files = Vec::new();
        let mut current_file: Option<DiffFile> = None;
        let mut current_hunk: Option<DiffHunk> = None;

        let mut old_line_counter = 0usize;
        let mut new_line_counter = 0usize;

        for line in raw.lines() {
            if line.starts_with("diff --git ") {
                if let Some(hunk) = current_hunk.take() {
                    if let Some(ref mut file) = current_file {
                        file.hunks.push(hunk);
                    }
                }
                if let Some(file) = current_file.take() {
                    files.push(file);
                }

                // Parse paths from "diff --git a/path b/path"
                let parts: Vec<&str> = line.split_whitespace().collect();
                let old_path = parts
                    .get(2)
                    .unwrap_or(&"a/unknown")
                    .trim_start_matches("a/")
                    .to_string();
                let new_path = parts
                    .get(3)
                    .unwrap_or(&"b/unknown")
                    .trim_start_matches("b/")
                    .to_string();

                current_file = Some(DiffFile {
                    old_path,
                    new_path,
                    additions: 0,
                    deletions: 0,
                    hunks: Vec::new(),
                });
                continue;
            }

            if line.starts_with("--- ") || line.starts_with("+++ ") {
                continue;
            }

            if line.starts_with("@@ ") {
                if let Some(hunk) = current_hunk.take() {
                    if let Some(ref mut file) = current_file {
                        file.hunks.push(hunk);
                    }
                }

                let (old_start, old_lines, new_start, new_lines) = parse_hunk_header(line);
                old_line_counter = old_start;
                new_line_counter = new_start;

                current_hunk = Some(DiffHunk {
                    header: line.to_string(),
                    old_start,
                    old_lines,
                    new_start,
                    new_lines,
                    lines: Vec::new(),
                });
                continue;
            }

            if let Some(ref mut hunk) = current_hunk {
                let ext = current_file
                    .as_ref()
                    .and_then(|f| Path::new(&f.new_path).extension())
                    .and_then(|e| e.to_str())
                    .unwrap_or("rs");

                if let Some(stripped) = line.strip_prefix('+') {
                    let tokens = highlight_syntax(stripped, ext);
                    hunk.lines.push(DiffLine {
                        kind: DiffLineKind::Addition,
                        old_line_num: None,
                        new_line_num: Some(new_line_counter),
                        content: stripped.to_string(),
                        tokens,
                    });
                    new_line_counter += 1;
                    if let Some(ref mut file) = current_file {
                        file.additions += 1;
                    }
                } else if let Some(stripped) = line.strip_prefix('-') {
                    let tokens = highlight_syntax(stripped, ext);
                    hunk.lines.push(DiffLine {
                        kind: DiffLineKind::Deletion,
                        old_line_num: Some(old_line_counter),
                        new_line_num: None,
                        content: stripped.to_string(),
                        tokens,
                    });
                    old_line_counter += 1;
                    if let Some(ref mut file) = current_file {
                        file.deletions += 1;
                    }
                } else {
                    let content = line.strip_prefix(' ').unwrap_or(line);
                    let tokens = highlight_syntax(content, ext);
                    hunk.lines.push(DiffLine {
                        kind: DiffLineKind::Context,
                        old_line_num: Some(old_line_counter),
                        new_line_num: Some(new_line_counter),
                        content: content.to_string(),
                        tokens,
                    });
                    old_line_counter += 1;
                    new_line_counter += 1;
                }
            }
        }

        if let Some(hunk) = current_hunk {
            if let Some(ref mut file) = current_file {
                file.hunks.push(hunk);
            }
        }
        if let Some(file) = current_file {
            files.push(file);
        }

        files
    }

    /// Renders structured diffs to formatted ANSI console output.
    pub fn render_terminal(files: &[DiffFile]) -> String {
        let mut out = String::new();

        for file in files {
            out.push_str(&format!(
                "\x1b[1m\x1b[36m📄 {} -> {} \x1b[32m(+{})\x1b[31m(-{})\x1b[0m\n",
                file.old_path, file.new_path, file.additions, file.deletions
            ));

            for hunk in &file.hunks {
                out.push_str(&format!("  \x1b[34m{}\x1b[0m\n", hunk.header));

                for line in &hunk.lines {
                    let old_str = line
                        .old_line_num
                        .map(|n| format!("{n:>4}"))
                        .unwrap_or_else(|| "    ".into());
                    let new_str = line
                        .new_line_num
                        .map(|n| format!("{n:>4}"))
                        .unwrap_or_else(|| "    ".into());

                    match line.kind {
                        DiffLineKind::Addition => {
                            out.push_str(&format!(
                                "  \x1b[32m+ {old_str} {new_str} │ {}\x1b[0m\n",
                                line.content
                            ));
                        }
                        DiffLineKind::Deletion => {
                            out.push_str(&format!(
                                "  \x1b[31m- {old_str} {new_str} │ {}\x1b[0m\n",
                                line.content
                            ));
                        }
                        DiffLineKind::Context => {
                            out.push_str(&format!(
                                "    {old_str} {new_str} │ \x1b[2m{}\x1b[0m\n",
                                line.content
                            ));
                        }
                        DiffLineKind::HunkHeader => {}
                    }
                }
            }
            out.push('\n');
        }

        out
    }

    /// Granular patch application for a single hunk to a file on disk.
    pub fn apply_hunk(file_path: &Path, hunk: &DiffHunk) -> Result<()> {
        if !file_path.exists() {
            bail!("archivo destino {} no existe", file_path.display());
        }
        let original = fs::read_to_string(file_path)
            .with_context(|| format!("error al leer {}", file_path.display()))?;
        let orig_lines: Vec<&str> = original.lines().collect();

        let mut result_lines = Vec::new();
        let target_idx = if hunk.old_start > 0 {
            hunk.old_start - 1
        } else {
            0
        };

        for (i, line) in orig_lines.iter().enumerate() {
            if i == target_idx {
                for dl in &hunk.lines {
                    match dl.kind {
                        DiffLineKind::Addition | DiffLineKind::Context => {
                            result_lines.push(dl.content.as_str());
                        }
                        DiffLineKind::Deletion => {
                            // Omit deleted lines
                        }
                        _ => {}
                    }
                }
            } else if i < target_idx || i >= target_idx + hunk.old_lines {
                result_lines.push(*line);
            }
        }

        let new_content = result_lines.join("\n") + "\n";
        fs::write(file_path, new_content)?;
        Ok(())
    }
}

/// Parses "@@ -old_start,old_lines +new_start,new_lines @@" into numbers.
fn parse_hunk_header(header: &str) -> (usize, usize, usize, usize) {
    let parts: Vec<&str> = header.split("@@").collect();
    if parts.len() < 2 {
        return (1, 0, 1, 0);
    }
    let nums_str = parts[1].trim();
    let sides: Vec<&str> = nums_str.split_whitespace().collect();

    let parse_side = |s: &str| -> (usize, usize) {
        let clean = s.trim_start_matches('-').trim_start_matches('+');
        if let Some((start, lines)) = clean.split_once(',') {
            (start.parse().unwrap_or(1), lines.parse().unwrap_or(1))
        } else {
            (clean.parse().unwrap_or(1), 1)
        }
    };

    let (old_start, old_lines) = sides.first().map(|s| parse_side(s)).unwrap_or((1, 0));
    let (new_start, new_lines) = sides.get(1).map(|s| parse_side(s)).unwrap_or((1, 0));

    (old_start, old_lines, new_start, new_lines)
}

static RS_KEYWORDS: &[&str] = &[
    "Self", "async", "await", "break", "const", "continue", "else", "enum", "fn", "for",
    "if", "impl", "let", "loop", "match", "mod", "mut", "pub", "return", "self",
    "static", "struct", "trait", "type", "unsafe", "use", "where", "while",
];

static PY_KEYWORDS: &[&str] = &[
    "as", "async", "await", "class", "def", "elif", "else", "except", "for",
    "from", "if", "import", "lambda", "pass", "return", "try", "while", "with", "yield",
];

static JS_KEYWORDS: &[&str] = &[
    "async", "await", "class", "const", "else", "export", "for", "function",
    "if", "import", "interface", "let", "new", "return", "type", "var", "while",
];

static DEFAULT_KEYWORDS: &[&str] = &[
    "const", "def", "fn", "func", "function", "let", "return", "var",
];

/// Tokenizes a single code line into syntax tokens with color classifications (T32.7).
/// Uses binary search on static sorted keyword tables and eliminates intermediate allocations.
pub fn highlight_syntax(line: &str, ext: &str) -> Vec<SyntaxToken> {
    let keywords: &[&str] = match ext {
        "rs" => RS_KEYWORDS,
        "py" => PY_KEYWORDS,
        "ts" | "js" => JS_KEYWORDS,
        _ => DEFAULT_KEYWORDS,
    };

    let trimmed = line.trim();

    if trimmed.starts_with("//") || trimmed.starts_with('#') {
        return vec![SyntaxToken {
            text: line.to_string(),
            token_type: SyntaxTokenType::Comment,
        }];
    }

    let mut tokens = Vec::with_capacity(line.len() / 6 + 1);

    for w in line.split_inclusive(|c: char| !c.is_alphanumeric() && c != '_') {
        let clean_word = w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
        if clean_word.is_empty() {
            tokens.push(SyntaxToken {
                text: w.to_string(),
                token_type: if w.contains('"') || w.contains('\'') {
                    SyntaxTokenType::StringLit
                } else {
                    SyntaxTokenType::Normal
                },
            });
        } else if keywords.binary_search(&clean_word).is_ok() {
            tokens.push(SyntaxToken {
                text: w.to_string(),
                token_type: SyntaxTokenType::Keyword,
            });
        } else if clean_word.starts_with(|c: char| c.is_uppercase()) {
            tokens.push(SyntaxToken {
                text: w.to_string(),
                token_type: SyntaxTokenType::Type,
            });
        } else if w.contains('"') || w.contains('\'') {
            tokens.push(SyntaxToken {
                text: w.to_string(),
                token_type: SyntaxTokenType::StringLit,
            });
        } else if clean_word.chars().all(|c| c.is_ascii_digit()) {
            tokens.push(SyntaxToken {
                text: w.to_string(),
                token_type: SyntaxTokenType::Number,
            });
        } else {
            tokens.push(SyntaxToken {
                text: w.to_string(),
                token_type: SyntaxTokenType::Normal,
            });
        }
    }

    if tokens.is_empty() {
        tokens.push(SyntaxToken {
            text: line.to_string(),
            token_type: SyntaxTokenType::Normal,
        });
    }

    tokens
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    const SAMPLE_DIFF: &str = r#"diff --git a/src/main.rs b/src/main.rs
index 1234567..89abcdef 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,4 +1,5 @@
 fn main() {
+    println!("antOS");
-    println!("old");
 }
"#;

    #[test]
    fn test_parse_unified_diff() {
        let files = DiffEngine::parse_unified_diff(SAMPLE_DIFF);
        assert_eq!(files.len(), 1);

        let file = &files[0];
        assert_eq!(file.old_path, "src/main.rs");
        assert_eq!(file.new_path, "src/main.rs");
        assert_eq!(file.additions, 1);
        assert_eq!(file.deletions, 1);
        assert_eq!(file.hunks.len(), 1);

        let hunk = &file.hunks[0];
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.lines.len(), 4);
        assert_eq!(hunk.lines[0].kind, DiffLineKind::Context);
        assert_eq!(hunk.lines[1].kind, DiffLineKind::Addition);
        assert_eq!(hunk.lines[2].kind, DiffLineKind::Deletion);
        assert_eq!(hunk.lines[3].kind, DiffLineKind::Context);
    }

    #[test]
    fn test_syntax_highlighting() {
        let tokens = highlight_syntax("pub fn init_workspace() -> Result<()> {", "rs");
        let has_keyword = tokens
            .iter()
            .any(|t| t.token_type == SyntaxTokenType::Keyword);
        let has_type = tokens.iter().any(|t| t.token_type == SyntaxTokenType::Type);
        assert!(has_keyword, "debe detectar keywords como pub y fn");
        assert!(has_type, "debe detectar tipos como Result");

        let comment = highlight_syntax("// Esto es un comentario", "rs");
        assert_eq!(comment[0].token_type, SyntaxTokenType::Comment);
    }

    #[test]
    fn test_render_terminal() {
        let files = DiffEngine::parse_unified_diff(SAMPLE_DIFF);
        let rendered = DiffEngine::render_terminal(&files);
        assert!(rendered.contains("src/main.rs"));
        assert!(rendered.contains("(+1)"));
        assert!(rendered.contains("(-1)"));
    }

    #[test]
    fn test_parse_large_diff_performance_over_2000_lines() {
        use std::time::Instant;

        let mut diff = String::with_capacity(120_000);
        diff.push_str("diff --git a/big_file.rs b/big_file.rs\nindex 0000000..1111111 100644\n--- a/big_file.rs\n+++ b/big_file.rs\n@@ -1,2500 +1,2500 @@\n");
        for i in 0..2500 {
            if i % 3 == 0 {
                diff.push_str(&format!("+    let var_{i}: u64 = compute_hash({i}); // insert\n"));
            } else if i % 3 == 1 {
                diff.push_str(&format!("-    let old_var_{i} = calculate({i}); // remove\n"));
            } else {
                diff.push_str(&format!("     fn step_{i}() -> Result<()> {{ Ok(()) }}\n"));
            }
        }

        let start = Instant::now();
        let parsed = DiffEngine::parse_unified_diff(&diff);
        let elapsed = start.elapsed();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].hunks[0].lines.len(), 2500);
        assert!(
            elapsed.as_millis() < 150,
            "El parseo de 2500 líneas tardó {}ms (debe ser < 150ms)",
            elapsed.as_millis()
        );
    }
}
