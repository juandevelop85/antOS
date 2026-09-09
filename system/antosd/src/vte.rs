//! Embedded Terminal Session and VTE Console Manager (Ticket T8.1).
//!
//! Provides interactive terminal sessions, ANSI color sequence parsing,
//! and command execution within the desktop environment shell.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};

/// A styled text span within terminal output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnsiSpan {
    pub text: String,
    pub bold: bool,
    pub dim: bool,
    pub color: Option<String>,
}

/// A line in the terminal console buffer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalLine {
    pub spans: Vec<AnsiSpan>,
    pub raw: String,
}

/// Active terminal session state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalSession {
    pub session_id: String,
    pub active_shell: String,
    pub buffer: Vec<TerminalLine>,
    pub exit_code: Option<i32>,
}

impl TerminalSession {
    pub fn new(session_id: &str) -> Self {
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|s| std::path::Path::new(s).exists())
            .unwrap_or_else(|| {
                if std::path::Path::new("/bin/bash").exists() {
                    "/bin/bash".into()
                } else if std::path::Path::new("/bin/sh").exists() {
                    "/bin/sh".into()
                } else {
                    "/bin/zsh".into()
                }
            });
        Self {
            session_id: session_id.to_string(),
            active_shell: shell,
            buffer: Vec::new(),
            exit_code: None,
        }
    }

    /// Executes a command and captures its output into the terminal buffer.
    pub fn execute_command(&mut self, cmd_line: &str) -> Result<()> {
        self.buffer.push(TerminalLine {
            spans: vec![
                AnsiSpan { text: "antos".into(), bold: true, dim: false, color: Some("green".into()) },
                AnsiSpan { text: " $ ".into(), bold: false, dim: true, color: None },
                AnsiSpan { text: cmd_line.into(), bold: true, dim: false, color: None },
            ],
            raw: format!("antos $ {cmd_line}"),
        });

        let output = Command::new(&self.active_shell)
            .arg("-c")
            .arg(cmd_line)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| format!("failed to execute command: {cmd_line}"))?;

        self.exit_code = output.status.code();

        let stdout_str = String::from_utf8_lossy(&output.stdout);
        for line in stdout_str.lines() {
            self.buffer.push(parse_ansi_line(line));
        }

        let stderr_str = String::from_utf8_lossy(&output.stderr);
        for line in stderr_str.lines() {
            let mut parsed = parse_ansi_line(line);
            for span in &mut parsed.spans {
                if span.color.is_none() {
                    span.color = Some("red".into());
                }
            }
            self.buffer.push(parsed);
        }

        Ok(())
    }
}

/// Parses ANSI color and formatting codes from a single terminal line.
pub fn parse_ansi_line(raw: &str) -> TerminalLine {
    let mut spans = Vec::new();
    let mut current_text = String::new();
    let mut current_color: Option<String> = None;
    let mut bold = false;
    let mut dim = false;

    let mut in_escape = false;
    let mut escape_buf = String::new();

    for c in raw.chars() {
        if c == '\x1b' {
            if !current_text.is_empty() {
                spans.push(AnsiSpan {
                    text: std::mem::take(&mut current_text),
                    bold,
                    dim,
                    color: current_color.clone(),
                });
            }
            in_escape = true;
            escape_buf.clear();
            continue;
        }

        if in_escape {
            escape_buf.push(c);
            if c == 'm' {
                in_escape = false;
                // Parse escape sequences e.g. [1m, [32m, [0m
                let seq = escape_buf.trim_start_matches('[').trim_end_matches('m');
                match seq {
                    "0" | "" => {
                        bold = false;
                        dim = false;
                        current_color = None;
                    }
                    "1" => bold = true,
                    "2" => dim = true,
                    "31" => current_color = Some("red".into()),
                    "32" => current_color = Some("green".into()),
                    "33" => current_color = Some("yellow".into()),
                    "34" => current_color = Some("blue".into()),
                    "35" => current_color = Some("magenta".into()),
                    "36" => current_color = Some("cyan".into()),
                    _ => {}
                }
            }
            continue;
        }

        current_text.push(c);
    }

    if !current_text.is_empty() {
        spans.push(AnsiSpan {
            text: current_text,
            bold,
            dim,
            color: current_color,
        });
    }

    TerminalLine {
        spans,
        raw: raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_parse_ansi_line() {
        let line = "\x1b[1m\x1b[32mantOS\x1b[0m running";
        let parsed = parse_ansi_line(line);
        assert_eq!(parsed.spans.len(), 2);
        assert_eq!(parsed.spans[0].text, "antOS");
        assert!(parsed.spans[0].bold);
        assert_eq!(parsed.spans[0].color.as_deref(), Some("green"));
        assert_eq!(parsed.spans[1].text, " running");
        assert!(!parsed.spans[1].bold);
    }

    #[test]
    fn test_terminal_session_execute() {
        let mut session = TerminalSession::new("test-sess");
        let res = session.execute_command("echo 'hello from vte'");
        assert!(res.is_ok());
        assert!(session.buffer.len() >= 2);
        assert_eq!(session.exit_code, Some(0));
    }
}
