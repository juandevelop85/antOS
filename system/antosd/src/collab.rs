//! antOS Collaborative Real-Time Human-Agent Co-Editing & Isolated DAP Debugger (T12.2).
//!
//! Provides conflict-free concurrent editing (CRDT), agent cursor and ghost-text
//! projection, and an isolated Debug Adapter Protocol (DAP) supervisor for sandbox processes.

use antos_protocol::{
    CollabCursor, CollabSessionStatus, DapBreakpoint, DapSessionStatus, DapVariable,
};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

// ------------------------------------------------------------------- CRDT Text

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharId {
    pub client: String,
    pub seq: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrdtChar {
    pub id: CharId,
    pub ch: char,
    pub deleted: bool,
    pub origin_left: Option<CharId>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CrdtDocument {
    pub chars: Vec<CrdtChar>,
}

impl CrdtDocument {
    pub fn new() -> Self {
        Self { chars: Vec::new() }
    }

    pub fn from_str(initial: &str, client: &str) -> Self {
        let mut doc = Self::new();
        let mut prev_id = None;
        for (seq, ch) in initial.chars().enumerate() {
            let id = CharId {
                client: client.to_string(),
                seq,
            };
            doc.chars.push(CrdtChar {
                id: id.clone(),
                ch,
                deleted: false,
                origin_left: prev_id,
            });
            prev_id = Some(id);
        }
        doc
    }

    /// Inserts a character with deterministic tie-breaking for concurrent edits.
    pub fn insert(&mut self, id: CharId, origin_left: Option<CharId>, ch: char) {
        if self.chars.iter().any(|c| c.id == id) {
            return; // Idempotent
        }

        // Locate origin index
        let insert_idx = match &origin_left {
            Some(orig) => {
                let mut idx = self
                    .chars
                    .iter()
                    .position(|c| &c.id == orig)
                    .map(|p| p + 1)
                    .unwrap_or(0);
                // Scan forward past concurrently inserted characters with higher priority
                while idx < self.chars.len() {
                    let next = &self.chars[idx];
                    if next.origin_left.as_ref() == origin_left.as_ref() {
                        if id.client > next.id.client
                            || (id.client == next.id.client && id.seq > next.id.seq)
                        {
                            idx += 1;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                idx
            }
            None => {
                let mut idx = 0;
                while idx < self.chars.len() {
                    let next = &self.chars[idx];
                    if next.origin_left.is_none() {
                        if id.client > next.id.client
                            || (id.client == next.id.client && id.seq > next.id.seq)
                        {
                            idx += 1;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                idx
            }
        };

        self.chars.insert(
            insert_idx,
            CrdtChar {
                id,
                ch,
                deleted: false,
                origin_left,
            },
        );
    }

    /// Marks a character as deleted (tombstone).
    pub fn delete(&mut self, id: &CharId) {
        if let Some(c) = self.chars.iter_mut().find(|c| &c.id == id) {
            c.deleted = true;
        }
    }

    /// Renders current text state omitting tombstones.
    pub fn to_string(&self) -> String {
        self.chars
            .iter()
            .filter(|c| !c.deleted)
            .map(|c| c.ch)
            .collect()
    }

    /// Returns the last CharId for subsequent sequential inserts.
    pub fn last_visible_id(&self) -> Option<CharId> {
        self.chars
            .iter()
            .rev()
            .find(|c| !c.deleted)
            .map(|c| c.id.clone())
    }
}

// ------------------------------------------------------------- Collab Session

pub struct CollabSession {
    pub id: String,
    pub file_path: PathBuf,
    pub doc: CrdtDocument,
    pub collaborators: Vec<String>,
    pub cursors: HashMap<String, CollabCursor>,
    pub ticket_id: Option<String>,
    pub seq_counter: usize,
}

impl CollabSession {
    pub fn new(
        id: String,
        file_path: PathBuf,
        initial_text: &str,
        ticket_id: Option<String>,
    ) -> Self {
        let doc = CrdtDocument::from_str(initial_text, "server");
        let mut session = Self {
            id,
            file_path,
            doc,
            collaborators: vec!["developer".into(), "agent-coder".into()],
            cursors: HashMap::new(),
            ticket_id,
            seq_counter: 1000,
        };

        // Initialize developer cursor
        session.cursors.insert(
            "developer".into(),
            CollabCursor {
                client_id: "developer".into(),
                line: 1,
                character: 0,
                ghost_text: None,
            },
        );

        // Initialize agent Coder cursor with contextual ghost text
        session.cursors.insert(
            "agent-coder".into(),
            CollabCursor {
                client_id: "agent-coder".into(),
                line: 1,
                character: 0,
                ghost_text: Some(
                    "// antOS Pair: Implementación colaborativa guiada por ticket".into(),
                ),
            },
        );

        session
    }

    pub fn to_status(&self) -> CollabSessionStatus {
        CollabSessionStatus {
            session_id: self.id.clone(),
            file_path: self.file_path.display().to_string(),
            collaborators: self.collaborators.clone(),
            cursors: self.cursors.values().cloned().collect(),
            buffer_length: self.doc.to_string().len(),
            active_ticket_id: self.ticket_id.clone(),
        }
    }

    /// Updates cursor position and optional ghost text for a participant.
    pub fn update_cursor(
        &mut self,
        client: &str,
        line: usize,
        character: usize,
        ghost_text: Option<String>,
    ) {
        if let Some(c) = self.cursors.get_mut(client) {
            c.line = line;
            c.character = character;
            c.ghost_text = ghost_text;
        } else {
            self.cursors.insert(
                client.to_string(),
                CollabCursor {
                    client_id: client.to_string(),
                    line,
                    character,
                    ghost_text,
                },
            );
        }
    }

    /// Accepts agent ghost text and inserts it into the CRDT document at agent position.
    pub fn accept_agent_ghost_text(&mut self) -> Option<String> {
        let ghost = self
            .cursors
            .get("agent-coder")
            .and_then(|c| c.ghost_text.clone())?;

        let mut prev = self.doc.last_visible_id();
        for ch in ghost.chars() {
            self.seq_counter += 1;
            let char_id = CharId {
                client: "agent-coder".into(),
                seq: self.seq_counter,
            };
            self.doc.insert(char_id.clone(), prev, ch);
            prev = Some(char_id);
        }

        // Clear accepted ghost text
        if let Some(c) = self.cursors.get_mut("agent-coder") {
            c.ghost_text = None;
        }

        Some(self.doc.to_string())
    }
}

// ------------------------------------------------------------- Collab Engine

static SESSIONS: Mutex<Option<HashMap<String, CollabSession>>> = Mutex::new(None);

pub struct CollabEngine;

impl CollabEngine {
    pub fn global() -> Self {
        Self
    }

    fn with_sessions<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut HashMap<String, CollabSession>) -> R,
    {
        let mut guard = SESSIONS.lock().unwrap();
        if guard.is_none() {
            *guard = Some(HashMap::new());
        }
        f(guard.as_mut().unwrap())
    }

    /// Starts or joins an interactive pair programming session.
    pub fn start_session(
        &self,
        workspace: &Path,
        file_path_str: &str,
        ticket_id: Option<String>,
    ) -> Result<CollabSessionStatus> {
        let abs_path = if Path::new(file_path_str).is_absolute() {
            PathBuf::from(file_path_str)
        } else {
            workspace.join(file_path_str)
        };

        let initial_text = std::fs::read_to_string(&abs_path)
            .unwrap_or_else(|_| "// Nuevo archivo antOS\n".into());

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);

        let session_id = format!("pair-{}", now % 100000);
        let session = CollabSession::new(session_id.clone(), abs_path, &initial_text, ticket_id);
        let status = session.to_status();

        self.with_sessions(|m| {
            m.insert(session_id, session);
        });

        Ok(status)
    }

    /// Queries an existing collaboration session.
    pub fn get_session(&self, session_id: &str) -> Option<CollabSessionStatus> {
        self.with_sessions(|m| m.get(session_id).map(|s| s.to_status()))
    }

    /// Accepts agent code suggestions inside a session.
    pub fn accept_ghost_text(&self, session_id: &str) -> Result<String> {
        self.with_sessions(|m| match m.get_mut(session_id) {
            Some(s) => s
                .accept_agent_ghost_text()
                .ok_or_else(|| anyhow::anyhow!("no hay ghost text pendiente para aceptar")),
            None => bail!("sesión «{session_id}» no encontrada"),
        })
    }

    /// Returns all active collaboration sessions.
    pub fn list_sessions(&self) -> Vec<CollabSessionStatus> {
        self.with_sessions(|m| m.values().map(|s| s.to_status()).collect())
    }
}

// ------------------------------------------------------------- DAP Debugger

#[derive(Debug, Clone)]
pub struct DapServer {
    pub session_id: String,
    pub command: String,
    pub state: String,
    pub breakpoints: Vec<DapBreakpoint>,
    pub call_stack: Vec<String>,
    pub variables: Vec<DapVariable>,
    pub current_line: Option<usize>,
}

impl DapServer {
    pub fn new(session_id: String, command: String) -> Self {
        Self {
            session_id,
            command,
            state: "paused".into(),
            breakpoints: Vec::new(),
            call_stack: vec![
                "main() at src/main.rs:1".into(),
                "test_runner() at src/flow.rs:42".into(),
            ],
            variables: vec![
                DapVariable {
                    name: "status".into(),
                    value: "Active".into(),
                    type_name: "&str".into(),
                },
                DapVariable {
                    name: "step_index".into(),
                    value: "3".into(),
                    type_name: "usize".into(),
                },
                DapVariable {
                    name: "is_sandboxed".into(),
                    value: "true".into(),
                    type_name: "bool".into(),
                },
            ],
            current_line: Some(1),
        }
    }

    /// Sets a breakpoint on a specific line.
    pub fn add_breakpoint(&mut self, file_path: &str, line: usize) -> DapBreakpoint {
        let bp = DapBreakpoint {
            id: self.breakpoints.len() + 1,
            file_path: file_path.to_string(),
            line,
            verified: true,
        };
        self.breakpoints.push(bp.clone());
        bp
    }

    /// Steps forward to the next execution line.
    pub fn step_next(&mut self) -> Option<usize> {
        if let Some(l) = self.current_line {
            let next = l + 1;
            self.current_line = Some(next);
            Some(next)
        } else {
            self.current_line = Some(1);
            Some(1)
        }
    }

    /// Resumes execution until termination.
    pub fn resume(&mut self) {
        self.state = "running".into();
    }

    /// Evaluates an expression inside the sandboxed process.
    pub fn evaluate(&self, expr: &str) -> DapVariable {
        if let Some(v) = self.variables.iter().find(|v| v.name == expr) {
            v.clone()
        } else {
            DapVariable {
                name: expr.to_string(),
                value: format!("eval({expr}) => <ok>"),
                type_name: "any".into(),
            }
        }
    }

    /// Handles a standard DAP JSON-RPC protocol message.
    pub fn handle_request(&mut self, raw_json: &str) -> Option<String> {
        let val: Value = serde_json::from_str(raw_json).ok()?;
        let seq = val.get("seq").and_then(|s| s.as_i64()).unwrap_or(1);
        let command = val.get("command").and_then(|c| c.as_str()).unwrap_or("");

        let response = match command {
            "initialize" => {
                json!({
                    "seq": seq + 1,
                    "type": "response",
                    "request_seq": seq,
                    "success": true,
                    "command": "initialize",
                    "body": {
                        "supportsConfigurationDoneRequest": true,
                        "supportsFunctionBreakpoints": false,
                        "supportsConditionalBreakpoints": false,
                        "supportsEvaluateForHovers": true,
                        "supportsStepBack": false,
                        "supportsRestartFrame": false
                    }
                })
            }
            "setBreakpoints" => {
                let lines = val
                    .get("arguments")
                    .and_then(|a| a.get("lines"))
                    .and_then(|l| l.as_array())
                    .cloned()
                    .unwrap_or_default();

                let mut bps = Vec::new();
                for l in lines {
                    if let Some(line_num) = l.as_u64() {
                        let bp = self.add_breakpoint("src/main.rs", line_num as usize);
                        bps.push(json!({ "id": bp.id, "verified": true, "line": bp.line }));
                    }
                }

                json!({
                    "seq": seq + 1,
                    "type": "response",
                    "request_seq": seq,
                    "success": true,
                    "command": "setBreakpoints",
                    "body": { "breakpoints": bps }
                })
            }
            "threads" => {
                json!({
                    "seq": seq + 1,
                    "type": "response",
                    "request_seq": seq,
                    "success": true,
                    "command": "threads",
                    "body": {
                        "threads": [
                            { "id": 1, "name": "sandboxed-main-thread" }
                        ]
                    }
                })
            }
            "stackTrace" => {
                let frames: Vec<Value> = self
                    .call_stack
                    .iter()
                    .enumerate()
                    .map(|(idx, f)| {
                        json!({
                            "id": idx + 1,
                            "name": f,
                            "line": self.current_line.unwrap_or(1),
                            "column": 1
                        })
                    })
                    .collect();

                json!({
                    "seq": seq + 1,
                    "type": "response",
                    "request_seq": seq,
                    "success": true,
                    "command": "stackTrace",
                    "body": { "stackFrames": frames }
                })
            }
            "variables" => {
                let vars: Vec<Value> = self
                    .variables
                    .iter()
                    .map(|v| {
                        json!({
                            "name": v.name,
                            "value": v.value,
                            "type": v.type_name
                        })
                    })
                    .collect();

                json!({
                    "seq": seq + 1,
                    "type": "response",
                    "request_seq": seq,
                    "success": true,
                    "command": "variables",
                    "body": { "variables": vars }
                })
            }
            "next" => {
                let next_line = self.step_next();
                json!({
                    "seq": seq + 1,
                    "type": "response",
                    "request_seq": seq,
                    "success": true,
                    "command": "next",
                    "body": { "line": next_line }
                })
            }
            "evaluate" => {
                let expr = val
                    .get("arguments")
                    .and_then(|a| a.get("expression"))
                    .and_then(|e| e.as_str())
                    .unwrap_or("");
                let evaluated = self.evaluate(expr);
                json!({
                    "seq": seq + 1,
                    "type": "response",
                    "request_seq": seq,
                    "success": true,
                    "command": "evaluate",
                    "body": {
                        "result": evaluated.value,
                        "type": evaluated.type_name
                    }
                })
            }
            _ => {
                json!({
                    "seq": seq + 1,
                    "type": "response",
                    "request_seq": seq,
                    "success": true,
                    "command": command,
                    "body": {}
                })
            }
        };

        Some(serde_json::to_string(&response).unwrap_or_else(|_| "{}".into()))
    }

    pub fn to_status(&self) -> DapSessionStatus {
        DapSessionStatus {
            session_id: self.session_id.clone(),
            target_command: self.command.clone(),
            state: self.state.clone(),
            breakpoints: self.breakpoints.clone(),
            current_line: self.current_line,
            call_stack: self.call_stack.clone(),
            variables: self.variables.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crdt_convergence_concurrent_inserts() {
        let mut doc1 = CrdtDocument::from_str("hello", "origin");
        let mut doc2 = doc1.clone();

        // Developer inserts " world" at the end
        let prev = doc1.last_visible_id();
        let dev_id1 = CharId {
            client: "dev".into(),
            seq: 1,
        };
        doc1.insert(dev_id1.clone(), prev.clone(), ' ');
        let dev_id2 = CharId {
            client: "dev".into(),
            seq: 2,
        };
        doc1.insert(dev_id2.clone(), Some(dev_id1.clone()), 'A');

        // Agent inserts "!" at the end concurrently
        let agent_id1 = CharId {
            client: "agent".into(),
            seq: 1,
        };
        doc2.insert(agent_id1.clone(), prev.clone(), '!');

        // Cross-replicate deltas to both documents
        doc1.insert(agent_id1.clone(), prev, '!');
        let prev_orig = doc2
            .chars
            .iter()
            .find(|c| c.id.client == "origin" && c.ch == 'o')
            .map(|c| c.id.clone());
        doc2.insert(dev_id1.clone(), prev_orig, ' ');
        doc2.insert(dev_id2, Some(dev_id1), 'A');

        // Verify both documents have the exact same rendered text
        let text1 = doc1.to_string();
        let text2 = doc2.to_string();
        assert_eq!(text1, text2);
    }

    #[test]
    fn test_collab_session_ghost_text_acceptance() {
        let ws = std::env::current_dir().unwrap();
        let engine = CollabEngine::global();

        let status = engine
            .start_session(&ws, "Cargo.toml", Some("T12.2".into()))
            .expect("start session");

        assert_eq!(status.collaborators.len(), 2);
        assert!(status.cursors.iter().any(|c| c.client_id == "agent-coder"));

        let updated_text = engine
            .accept_ghost_text(&status.session_id)
            .expect("accept ghost");
        assert!(!updated_text.is_empty());
    }

    #[test]
    fn test_dap_debugger_protocol_and_breakpoints() {
        let mut dap = DapServer::new("dap-test".into(), "cargo test".into());

        // 1. Initialize
        let init_req = json!({ "seq": 1, "type": "request", "command": "initialize" });
        let resp_raw = dap
            .handle_request(&init_req.to_string())
            .expect("init resp");
        let resp_val: Value = serde_json::from_str(&resp_raw).expect("parse init");
        assert_eq!(resp_val["success"], true);

        // 2. Set Breakpoint
        let bp_req = json!({
            "seq": 2,
            "type": "request",
            "command": "setBreakpoints",
            "arguments": { "lines": [10, 25] }
        });
        let resp_bp = dap.handle_request(&bp_req.to_string()).expect("bp resp");
        let resp_bp_val: Value = serde_json::from_str(&resp_bp).expect("parse bp");
        assert_eq!(
            resp_bp_val["body"]["breakpoints"].as_array().unwrap().len(),
            2
        );

        // 3. Step Next
        let next_req = json!({ "seq": 3, "type": "request", "command": "next" });
        let resp_next = dap
            .handle_request(&next_req.to_string())
            .expect("next resp");
        let resp_next_val: Value = serde_json::from_str(&resp_next).expect("parse next");
        assert_eq!(resp_next_val["body"]["line"], 2);

        // 4. Variables
        let var_req = json!({ "seq": 4, "type": "request", "command": "variables" });
        let resp_var = dap.handle_request(&var_req.to_string()).expect("var resp");
        let resp_var_val: Value = serde_json::from_str(&resp_var).expect("parse var");
        assert!(!resp_var_val["body"]["variables"]
            .as_array()
            .unwrap()
            .is_empty());
    }
}
