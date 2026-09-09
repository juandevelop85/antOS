//! antOS VFS Semantic Write Interceptor and Syntax Guard (T10.2).
//!
//! Provides in-memory syntax and grammar validation before persisting file writes
//! to disk, preventing truncated or syntactically broken code from entering the repository.

use crate::util::lock_or_recover;
use antos_protocol::{SyntaxValidationError, ValidationResult, VfsGuardStatus};
use anyhow::Result;
use std::path::Path;
use std::sync::Mutex;

static GUARD_STATE: Mutex<GuardState> = Mutex::new(GuardState {
    enabled: true,
    total_intercepted: 0,
    total_rejected: 0,
    rejected_paths: Vec::new(),
});

struct GuardState {
    enabled: bool,
    total_intercepted: usize,
    total_rejected: usize,
    rejected_paths: Vec<String>,
}

pub struct VfsGuardEngine;

impl VfsGuardEngine {
    pub fn global() -> Self {
        Self
    }

    /// Detects programming or configuration language based on file extension.
    pub fn detect_language(file_path: &str) -> &'static str {
        let p = Path::new(file_path);
        match p.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
            "rs" => "rust",
            "ts" | "tsx" => "typescript",
            "js" | "jsx" | "mjs" | "cjs" => "javascript",
            "py" | "pyw" => "python",
            "json" => "json",
            "toml" => "toml",
            "yaml" | "yml" => "yaml",
            _ => "text",
        }
    }

    /// Validates code or configuration syntax in memory.
    pub fn validate_content(&self, file_path: &str, content: &str) -> ValidationResult {
        let lang = Self::detect_language(file_path);
        let line_count = content.lines().count().max(1);
        let mut errors = Vec::new();

        match lang {
            "json" => {
                if let Err(e) = serde_json::from_str::<serde_json::Value>(content) {
                    errors.push(SyntaxValidationError {
                        line: e.line(),
                        column: e.column(),
                        message: format!("JSON parse error: {e}"),
                        severity: "error".into(),
                    });
                }
            }
            "toml" => {
                if let Err(e) = toml::from_str::<toml::Value>(content) {
                    let (line, col) = if let Some(span) = e.span() {
                        let prefix = &content[..span.start.min(content.len())];
                        let l = prefix.lines().count().max(1);
                        let c = prefix
                            .lines()
                            .last()
                            .map(|line| line.len() + 1)
                            .unwrap_or(1);
                        (l, c)
                    } else {
                        (1, 1)
                    };
                    errors.push(SyntaxValidationError {
                        line,
                        column: col,
                        message: format!("TOML parse error: {e}"),
                        severity: "error".into(),
                    });
                }
            }
            "rust" | "typescript" | "javascript" | "python" => {
                // Perform delimiter balancing, quote verification, and syntax health check
                Self::validate_delimiters_and_strings(content, lang, &mut errors);

                if lang == "python" {
                    Self::validate_python_indentation(content, &mut errors);
                }
            }
            _ => {}
        }

        ValidationResult {
            is_valid: errors.is_empty(),
            language: lang.into(),
            errors,
            line_count,
            file_path: file_path.into(),
        }
    }

    /// Checks matching delimiters: ( ), [ ], { } and unclosed quotes/comments.
    fn validate_delimiters_and_strings(
        content: &str,
        lang: &str,
        errors: &mut Vec<SyntaxValidationError>,
    ) {
        let mut stack: Vec<(char, usize, usize)> = Vec::new(); // (char, line, col)
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        let mut in_backtick = false;
        let mut in_line_comment = false;
        let mut in_block_comment = false;
        let mut escaped = false;

        let mut line_num = 1usize;
        let mut col_num = 0usize;

        let chars: Vec<char> = content.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            let c = chars[i];
            col_num += 1;

            if c == '\n' {
                line_num += 1;
                col_num = 0;
                in_line_comment = false;
                if in_single_quote && lang != "python" {
                    errors.push(SyntaxValidationError {
                        line: line_num - 1,
                        column: col_num,
                        message: "Unclosed single quote on line end".into(),
                        severity: "error".into(),
                    });
                    in_single_quote = false;
                }
                if in_double_quote && lang != "python" {
                    errors.push(SyntaxValidationError {
                        line: line_num - 1,
                        column: col_num,
                        message: "Unclosed double quote on line end".into(),
                        severity: "error".into(),
                    });
                    in_double_quote = false;
                }
                escaped = false;
                i += 1;
                continue;
            }

            if in_line_comment {
                i += 1;
                continue;
            }

            if in_block_comment {
                if c == '*' && i + 1 < len && chars[i + 1] == '/' {
                    in_block_comment = false;
                    i += 2;
                    col_num += 1;
                    continue;
                }
                i += 1;
                continue;
            }

            if escaped {
                escaped = false;
                i += 1;
                continue;
            }

            if c == '\\' && (in_single_quote || in_double_quote || in_backtick) {
                escaped = true;
                i += 1;
                continue;
            }

            // Check comment starters outside of quotes
            if !in_single_quote && !in_double_quote && !in_backtick {
                if lang == "python" && c == '#' {
                    in_line_comment = true;
                    i += 1;
                    continue;
                }
                if (lang == "rust" || lang == "typescript" || lang == "javascript") && c == '/' {
                    if i + 1 < len && chars[i + 1] == '/' {
                        in_line_comment = true;
                        i += 2;
                        col_num += 1;
                        continue;
                    }
                    if i + 1 < len && chars[i + 1] == '*' {
                        in_block_comment = true;
                        i += 2;
                        col_num += 1;
                        continue;
                    }
                }
            }

            // Quotes handling
            if c == '\'' && !in_double_quote && !in_backtick {
                // In Rust, ignore character lifetimes like 'a, 'static
                let is_rust_lifetime = lang == "rust"
                    && (i + 1 < len && (chars[i + 1].is_alphabetic() || chars[i + 1] == '_'))
                    && (i + 2 >= len || chars[i + 2] != '\'');
                if !is_rust_lifetime {
                    in_single_quote = !in_single_quote;
                }
                i += 1;
                continue;
            }

            if c == '"' && !in_single_quote && !in_backtick {
                in_double_quote = !in_double_quote;
                i += 1;
                continue;
            }

            if c == '`'
                && (lang == "typescript" || lang == "javascript")
                && !in_single_quote
                && !in_double_quote
            {
                in_backtick = !in_backtick;
                i += 1;
                continue;
            }

            // If inside quotes, ignore delimiters
            if in_single_quote || in_double_quote || in_backtick {
                i += 1;
                continue;
            }

            // Delimiters
            match c {
                '(' | '[' | '{' => {
                    stack.push((c, line_num, col_num));
                }
                ')' => match stack.pop() {
                    Some(('(', _, _)) => {}
                    Some((expected, l, col)) => {
                        errors.push(SyntaxValidationError {
                            line: line_num,
                            column: col_num,
                            message: format!("Mismatched delimiter: found ')' but expected match for '{expected}' opened at {l}:{col}"),
                            severity: "error".into(),
                        });
                    }
                    None => {
                        errors.push(SyntaxValidationError {
                            line: line_num,
                            column: col_num,
                            message: "Unexpected closing delimiter ')' without matching opening"
                                .into(),
                            severity: "error".into(),
                        });
                    }
                },
                ']' => match stack.pop() {
                    Some(('[', _, _)) => {}
                    Some((expected, l, col)) => {
                        errors.push(SyntaxValidationError {
                            line: line_num,
                            column: col_num,
                            message: format!("Mismatched delimiter: found ']' but expected match for '{expected}' opened at {l}:{col}"),
                            severity: "error".into(),
                        });
                    }
                    None => {
                        errors.push(SyntaxValidationError {
                            line: line_num,
                            column: col_num,
                            message: "Unexpected closing delimiter ']' without matching opening"
                                .into(),
                            severity: "error".into(),
                        });
                    }
                },
                '}' => match stack.pop() {
                    Some(('{', _, _)) => {}
                    Some((expected, l, col)) => {
                        errors.push(SyntaxValidationError {
                            line: line_num,
                            column: col_num,
                            message: format!("Mismatched delimiter: found '}}' but expected match for '{expected}' opened at {l}:{col}"),
                            severity: "error".into(),
                        });
                    }
                    None => {
                        errors.push(SyntaxValidationError {
                            line: line_num,
                            column: col_num,
                            message: "Unexpected closing delimiter '}' without matching opening"
                                .into(),
                            severity: "error".into(),
                        });
                    }
                },
                _ => {}
            }

            i += 1;
        }

        if in_single_quote {
            errors.push(SyntaxValidationError {
                line: line_num,
                column: col_num,
                message: "Unclosed single quote reached EOF".into(),
                severity: "error".into(),
            });
        }
        if in_double_quote {
            errors.push(SyntaxValidationError {
                line: line_num,
                column: col_num,
                message: "Unclosed double quote reached EOF".into(),
                severity: "error".into(),
            });
        }
        if in_backtick {
            errors.push(SyntaxValidationError {
                line: line_num,
                column: col_num,
                message: "Unclosed template literal (backtick) reached EOF".into(),
                severity: "error".into(),
            });
        }
        if in_block_comment {
            errors.push(SyntaxValidationError {
                line: line_num,
                column: col_num,
                message: "Unclosed block comment reached EOF".into(),
                severity: "error".into(),
            });
        }

        // Remaining unclosed delimiters in stack
        while let Some((c, l, col)) = stack.pop() {
            let partner = match c {
                '(' => ')',
                '[' => ']',
                '{' => '}',
                other => other,
            };
            errors.push(SyntaxValidationError {
                line: l,
                column: col,
                message: format!(
                    "Unclosed delimiter '{c}', expected matching '{partner}' before EOF"
                ),
                severity: "error".into(),
            });
        }
    }

    /// Verifies Python indentation consistency (tab/space mixing).
    fn validate_python_indentation(content: &str, errors: &mut Vec<SyntaxValidationError>) {
        for (idx, line) in content.lines().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let indent_prefix = &line[..(line.len() - trimmed.len())];
            if indent_prefix.contains('\t') && indent_prefix.contains(' ') {
                errors.push(SyntaxValidationError {
                    line: line_num,
                    column: 1,
                    message: "Inconsistent indentation: mixed spaces and tabs in prefix".into(),
                    severity: "error".into(),
                });
            }
        }
    }

    /// Intercepts a write attempt. If invalid, rejects and logs the attempt.
    pub fn intercept_write(&self, file_path: &str, content: &str) -> Result<ValidationResult> {
        let mut state = lock_or_recover(&GUARD_STATE);
        state.total_intercepted += 1;

        if !state.enabled {
            return Ok(ValidationResult {
                is_valid: true,
                language: Self::detect_language(file_path).into(),
                errors: Vec::new(),
                line_count: content.lines().count().max(1),
                file_path: file_path.into(),
            });
        }

        let result = self.validate_content(file_path, content);
        if !result.is_valid {
            state.total_rejected += 1;
            if !state.rejected_paths.contains(&file_path.to_string()) {
                state.rejected_paths.push(file_path.to_string());
            }
        }

        Ok(result)
    }

    /// Returns current statistics of the VFS write guard.
    pub fn status(&self) -> Result<VfsGuardStatus> {
        let state = lock_or_recover(&GUARD_STATE);
        Ok(VfsGuardStatus {
            enabled: state.enabled,
            total_intercepted: state.total_intercepted,
            total_rejected: state.total_rejected,
            rejected_paths: state.rejected_paths.clone(),
        })
    }

    /// Resets interception counters.
    pub fn reset_stats(&self) {
        let mut state = lock_or_recover(&GUARD_STATE);
        state.total_intercepted = 0;
        state.total_rejected = 0;
        state.rejected_paths.clear();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_guard_valid_rust_code() {
        let engine = VfsGuardEngine::global();
        let code = r#"
            pub fn add(a: i32, b: i32) -> i32 {
                let s = "hello world (nested)";
                let c = '(';
                a + b
            }
        "#;
        let res = engine.validate_content("src/calc.rs", code);
        assert!(
            res.is_valid,
            "expected code to be valid, got: {:?}",
            res.errors
        );
        assert_eq!(res.language, "rust");
    }

    #[test]
    fn test_guard_reject_unclosed_brace_rust() {
        let engine = VfsGuardEngine::global();
        let broken = r#"
            pub fn broken() {
                let x = 10;
            // missing closing brace
        "#;
        let res = engine.validate_content("src/broken.rs", broken);
        assert!(!res.is_valid);
        assert_eq!(res.errors.len(), 1);
        assert!(res.errors[0].message.contains("Unclosed delimiter '{'"));
    }

    #[test]
    fn test_guard_reject_mismatched_brackets() {
        let engine = VfsGuardEngine::global();
        let broken = "const arr = [1, 2, 3);";
        let res = engine.validate_content("app.ts", broken);
        assert!(!res.is_valid);
        assert!(res
            .errors
            .iter()
            .any(|e| e.message.contains("Mismatched delimiter")));
    }

    #[test]
    fn test_guard_valid_and_invalid_json() {
        let engine = VfsGuardEngine::global();
        let valid = r#"{"name": "antOS", "version": 1}"#;
        let res_ok = engine.validate_content("config.json", valid);
        assert!(res_ok.is_valid);

        let invalid = r#"{"name": "antOS", "version": }"#;
        let res_err = engine.validate_content("config.json", invalid);
        assert!(!res_err.is_valid);
        assert!(!res_err.errors.is_empty());
    }

    #[test]
    fn test_intercept_write_lifecycle() {
        let engine = VfsGuardEngine::global();
        engine.reset_stats();

        let ok_res = engine
            .intercept_write("src/valid.rs", "fn test() {}")
            .unwrap();
        assert!(ok_res.is_valid);

        let err_res = engine.intercept_write("src/bad.rs", "fn bad() {").unwrap();
        assert!(!err_res.is_valid);

        let status = engine.status().unwrap();
        assert_eq!(status.total_intercepted, 2);
        assert_eq!(status.total_rejected, 1);
        assert_eq!(status.rejected_paths, vec!["src/bad.rs"]);
    }
}
