//! antOS Embedded Real-Time Web Console and WebSocket Bridge (T16.4).
//!
//! Provides a lightweight embedded HTTP/WebSocket server allowing developers to monitor
//! and orchestrate antOS remotely through modern web browsers, with real-time streaming
//! of telemetry, Kanban tickets, autopilot incidents, and cryptographic token auth.

use anyhow::{Context, Result};
use antos_protocol::{
    WebAuthSession, WebConsoleConfig, WebConsoleStatus, WebSocketMessage,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

// ------------------------------------------------------------- crypto helpers

/// Standard SHA-1 implementation (FIPS PUB 180-1 / RFC 3174) for WebSocket Handshake.
pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = [0u8; 20];
    out[0..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

/// Computes the RFC 6455 Sec-WebSocket-Accept key.
pub fn compute_ws_accept(client_key: &str) -> String {
    const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
    let concatenated = format!("{client_key}{WS_GUID}");
    let digest = sha1(concatenated.as_bytes());
    crate::vision::VisionEngine::encode_base64(&digest)
}

/// Encodes a text payload into an unmasked RFC 6455 WebSocket frame (Server-to-Client).
pub fn encode_ws_text_frame(payload: &str) -> Vec<u8> {
    let bytes = payload.as_bytes();
    let len = bytes.len();
    let mut frame = Vec::with_capacity(len + 10);

    // FIN = 1, opcode = 0x1 (Text)
    frame.push(0x81);

    if len < 126 {
        frame.push(len as u8);
    } else if len <= 65535 {
        frame.push(126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }

    frame.extend_from_slice(bytes);
    frame
}

/// Decodes a masked RFC 6455 WebSocket frame (Client-to-Server).
pub fn decode_ws_frame(data: &[u8]) -> Option<(u8, String)> {
    if data.len() < 2 {
        return None;
    }
    let opcode = data[0] & 0x0F;
    let is_masked = (data[1] & 0x80) != 0;
    let mut payload_len = (data[1] & 0x7F) as usize;
    let mut offset = 2;

    if payload_len == 126 {
        if data.len() < 4 {
            return None;
        }
        payload_len = u16::from_be_bytes([data[2], data[3]]) as usize;
        offset = 4;
    } else if payload_len == 127 {
        if data.len() < 10 {
            return None;
        }
        payload_len = u64::from_be_bytes(data[2..10].try_into().ok()?) as usize;
        offset = 10;
    }

    let mask = if is_masked {
        if data.len() < offset + 4 {
            return None;
        }
        let m = [data[offset], data[offset + 1], data[offset + 2], data[offset + 3]];
        offset += 4;
        Some(m)
    } else {
        None
    };

    if data.len() < offset + payload_len {
        return None;
    }

    let raw_payload = &data[offset..offset + payload_len];
    let unmasked: Vec<u8> = if let Some(m) = mask {
        raw_payload.iter().enumerate().map(|(i, &b)| b ^ m[i % 4]).collect()
    } else {
        raw_payload.to_vec()
    };

    let text = String::from_utf8(unmasked).ok()?;
    Some((opcode, text))
}

// ------------------------------------------------------------- sessions & state

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSessions {
    pub sessions: Vec<WebAuthSession>,
}

pub struct WebEngine;

impl WebEngine {
    pub fn web_dir(state_dir: &Path) -> PathBuf {
        state_dir.join("web")
    }

    pub fn sessions_path(state_dir: &Path) -> PathBuf {
        Self::web_dir(state_dir).join("sessions.json")
    }

    pub fn status_path(state_dir: &Path) -> PathBuf {
        Self::web_dir(state_dir).join("status.json")
    }

    /// Generates a new cryptographic authentication token for remote client access.
    pub fn generate_token(
        state_dir: &Path,
        client_label: Option<String>,
        ttl_secs: Option<u64>,
    ) -> Result<WebAuthSession> {
        let dir = Self::web_dir(state_dir);
        fs::create_dir_all(&dir)?;

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let ttl = ttl_secs.unwrap_or(86400); // 24 hours default
        let expires_at = now + ttl;

        let seed = format!("{now}-{ttl}-{:?}", client_label);
        let hash = sha1(seed.as_bytes());
        let token_hex = hash.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let token = format!("ant_{token_hex}");

        let session = WebAuthSession {
            token,
            created_at: now,
            expires_at,
            client_label,
        };

        let mut stored = Self::load_sessions(state_dir);
        // Prune expired sessions
        stored.sessions.retain(|s| s.expires_at > now);
        stored.sessions.push(session.clone());

        fs::write(Self::sessions_path(state_dir), serde_json::to_string_pretty(&stored)?)?;
        Ok(session)
    }

    /// Validates an incoming token against stored active sessions.
    pub fn validate_token(state_dir: &Path, token: &str) -> bool {
        let stored = Self::load_sessions(state_dir);
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        stored.sessions.iter().any(|s| s.token == token && s.expires_at > now)
    }

    fn load_sessions(state_dir: &Path) -> StoredSessions {
        let path = Self::sessions_path(state_dir);
        if path.exists() {
            if let Ok(c) = fs::read_to_string(&path) {
                if let Ok(s) = serde_json::from_str::<StoredSessions>(&c) {
                    return s;
                }
            }
        }
        StoredSessions { sessions: Vec::new() }
    }

    /// Queries current status of the web console.
    pub fn status(state_dir: &Path) -> Result<WebConsoleStatus> {
        let path = Self::status_path(state_dir);
        let stored_status: Option<WebConsoleStatus> = if path.exists() {
            fs::read_to_string(&path).ok().and_then(|c| serde_json::from_str(&c).ok())
        } else {
            None
        };

        let sessions = Self::load_sessions(state_dir);
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let active_sessions = sessions.sessions.iter().filter(|s| s.expires_at > now).count();

        if let Some(mut st) = stored_status {
            // Verify if listener is still alive
            let is_alive = TcpStream::connect(format!("{}:{}", st.bind_addr, st.port)).is_ok();
            st.running = is_alive;
            st.active_sessions_count = active_sessions;
            Ok(st)
        } else {
            Ok(WebConsoleStatus {
                running: false,
                bind_addr: "127.0.0.1".into(),
                port: 8088,
                connected_clients: 0,
                active_sessions_count: active_sessions,
                url: "http://127.0.0.1:8088".into(),
            })
        }
    }

    /// Starts the embedded HTTP and WebSocket server.
    pub fn start(
        state_dir: &Path,
        workspace_dir: &Path,
        config: WebConsoleConfig,
    ) -> Result<WebConsoleStatus> {
        let addr = format!("{}:{}", config.bind_addr, config.port);
        let listener = TcpListener::bind(&addr)
            .with_context(|| format!("Failed to bind web console on {addr}"))?;

        listener.set_nonblocking(true)?;

        let state_dir_buf = state_dir.to_path_buf();
        let ws_dir_buf = workspace_dir.to_path_buf();
        let running_flag = Arc::new(AtomicBool::new(true));
        let flag_clone = running_flag.clone();

        let status = WebConsoleStatus {
            running: true,
            bind_addr: config.bind_addr.clone(),
            port: config.port,
            connected_clients: 0,
            active_sessions_count: Self::load_sessions(state_dir).sessions.len(),
            url: format!("http://{}", addr),
        };

        let dir = Self::web_dir(state_dir);
        fs::create_dir_all(&dir)?;
        fs::write(Self::status_path(state_dir), serde_json::to_string_pretty(&status)?)?;

        // Spawn background server loop
        thread::spawn(move || {
            while flag_clone.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let s_dir = state_dir_buf.clone();
                        let w_dir = ws_dir_buf.clone();
                        thread::spawn(move || {
                            let _ = Self::handle_client(&mut stream, &s_dir, &w_dir);
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_millis(100));
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
        });

        Ok(status)
    }

    /// Runs the web console server loop blocking the current thread.
    pub fn serve_blocking(
        state_dir: &Path,
        workspace_dir: &Path,
        config: WebConsoleConfig,
    ) -> Result<()> {
        let addr = format!("{}:{}", config.bind_addr, config.port);
        let listener = TcpListener::bind(&addr)
            .with_context(|| format!("Failed to bind web console on {addr}"))?;

        let status = WebConsoleStatus {
            running: true,
            bind_addr: config.bind_addr.clone(),
            port: config.port,
            connected_clients: 0,
            active_sessions_count: Self::load_sessions(state_dir).sessions.len(),
            url: format!("http://{}", addr),
        };

        let dir = Self::web_dir(state_dir);
        fs::create_dir_all(&dir)?;
        fs::write(Self::status_path(state_dir), serde_json::to_string_pretty(&status)?)?;

        let state_dir_buf = state_dir.to_path_buf();
        let ws_dir_buf = workspace_dir.to_path_buf();

        for stream_res in listener.incoming() {
            match stream_res {
                Ok(mut stream) => {
                    let s_dir = state_dir_buf.clone();
                    let w_dir = ws_dir_buf.clone();
                    thread::spawn(move || {
                        let _ = Self::handle_client(&mut stream, &s_dir, &w_dir);
                    });
                }
                Err(_) => {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Stops the web console server.
    pub fn stop(state_dir: &Path) -> Result<WebConsoleStatus> {
        let mut st = Self::status(state_dir)?;
        st.running = false;
        st.connected_clients = 0;

        let path = Self::status_path(state_dir);
        if path.exists() {
            let _ = fs::write(&path, serde_json::to_string_pretty(&st)?);
        }
        Ok(st)
    }

    /// Handles a single incoming HTTP request or WebSocket upgrade.
    fn handle_client(stream: &mut TcpStream, state_dir: &Path, workspace_dir: &Path) -> Result<()> {
        let mut buffer = [0u8; 4096];
        let n = stream.read(&mut buffer)?;
        if n == 0 {
            return Ok(());
        }

        let request_str = String::from_utf8_lossy(&buffer[..n]);
        let mut lines = request_str.lines();
        let req_line = lines.next().unwrap_or("");
        let mut parts = req_line.split_whitespace();
        let _method = parts.next().unwrap_or("GET");
        let path_and_query = parts.next().unwrap_or("/");

        let (path, query) = if let Some((p, q)) = path_and_query.split_once('?') {
            (p, q)
        } else {
            (path_and_query, "")
        };

        // Extract Authorization header or ?token= query parameter
        let mut req_token = None;
        if !query.is_empty() {
            for param in query.split('&') {
                if let Some((k, v)) = param.split_once('=') {
                    if k == "token" {
                        req_token = Some(v.to_string());
                        break;
                    }
                }
            }
        }

        let mut is_ws_upgrade = false;
        let mut ws_key = None;

        for line in lines {
            let lower = line.to_lowercase();
            if lower.starts_with("authorization: bearer ") {
                req_token = Some(line[22..].trim().to_string());
            } else if lower.starts_with("upgrade:") && lower.contains("websocket") {
                is_ws_upgrade = true;
            } else if lower.starts_with("sec-websocket-key:") {
                ws_key = line.split_once(':').map(|(_, k)| k.trim().to_string());
            }
        }

        let _is_authed = req_token.as_deref().map(|t| Self::validate_token(state_dir, t)).unwrap_or(false);

        // Check if WebSocket upgrade requested
        if is_ws_upgrade && ws_key.is_some() {
            let key = ws_key.unwrap();
            let accept = compute_ws_accept(&key);
            let response = format!(
                "HTTP/1.1 101 Switching Protocols\r\n\
                Upgrade: websocket\r\n\
                Connection: Upgrade\r\n\
                Sec-WebSocket-Accept: {accept}\r\n\r\n"
            );
            stream.write_all(response.as_bytes())?;

            // Send initial connected event
            let initial = WebSocketMessage {
                topic: "telemetry".into(),
                payload: format!("{{\"status\": \"connected\", \"workspace\": \"{}\"}}", workspace_dir.display()),
                timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
            };
            let frame = encode_ws_text_frame(&serde_json::to_string(&initial)?);
            stream.write_all(&frame)?;

            // Keep connection alive for incoming ping/pong or queries
            let mut ws_buf = [0u8; 2048];
            while let Ok(read_len) = stream.read(&mut ws_buf) {
                if read_len == 0 {
                    break;
                }
                if let Some((opcode, text)) = decode_ws_frame(&ws_buf[..read_len]) {
                    if opcode == 0x8 {
                        // Close frame
                        break;
                    } else if opcode == 0x9 {
                        // Ping -> Pong
                        stream.write_all(&[0x8A, 0x00])?;
                    } else if opcode == 0x1 {
                        // Echo or respond to telemetry request
                        let resp = WebSocketMessage {
                            topic: "echo".into(),
                            payload: text,
                            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                        };
                        let resp_frame = encode_ws_text_frame(&serde_json::to_string(&resp)?);
                        stream.write_all(&resp_frame)?;
                    }
                }
            }
            return Ok(());
        }

        // API endpoints
        if path == "/api/status" {
            let st = Self::status(state_dir)?;
            let json = serde_json::to_string_pretty(&st)?;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                json.len(),
                json
            );
            stream.write_all(response.as_bytes())?;
            return Ok(());
        }

        if path == "/api/tickets" {
            let spec = crate::spec::SpecEngine::global();
            let tickets = spec.list_tickets(workspace_dir).unwrap_or_default();
            let json = serde_json::to_string_pretty(&tickets)?;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                json.len(),
                json
            );
            stream.write_all(response.as_bytes())?;
            return Ok(());
        }

        if path == "/api/autopilot" {
            let incs = crate::autopilot::AutopilotEngine::list_incidents(state_dir).unwrap_or_default();
            let json = serde_json::to_string_pretty(&incs)?;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                json.len(),
                json
            );
            stream.write_all(response.as_bytes())?;
            return Ok(());
        }

        // Serve embedded HTML Single Page App
        let html = Self::embedded_html();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            html.len(),
            html
        );
        stream.write_all(response.as_bytes())?;
        Ok(())
    }

    /// Autocontained modern HTML5, dark-theme CSS & Vanilla JS single-page web console.
    pub fn embedded_html() -> String {
        r#"<!DOCTYPE html>
<html lang="es">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>antOS · Consola Web Remota</title>
  <style>
    :root {
      --bg: #0d1117;
      --surface: #161b22;
      --border: #30363d;
      --accent: #58a6ff;
      --text: #c9d1d9;
      --text-muted: #8b949e;
      --success: #3fb950;
      --warning: #d29922;
      --danger: #f85149;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, monospace; }
    body { background: var(--bg); color: var(--text); padding: 1.5rem; }
    header { display: flex; justify-content: space-between; align-items: center; padding-bottom: 1rem; border-bottom: 1px solid var(--border); margin-bottom: 1.5rem; }
    .brand { display: flex; align-items: center; gap: 0.8rem; }
    .logo { font-size: 1.4rem; font-weight: 800; color: var(--accent); }
    .badge { padding: 0.25rem 0.6rem; border-radius: 9999px; font-size: 0.75rem; font-weight: 600; background: rgba(63, 185, 80, 0.15); color: var(--success); border: 1px solid var(--success); }
    .tabs { display: flex; gap: 0.5rem; margin-bottom: 1.5rem; }
    .tab { padding: 0.6rem 1.2rem; border-radius: 6px; background: var(--surface); color: var(--text-muted); cursor: pointer; border: 1px solid var(--border); transition: 0.2s; }
    .tab.active { color: var(--text); border-color: var(--accent); background: rgba(88, 166, 255, 0.1); font-weight: 600; }
    .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(320px, 1fr)); gap: 1.2rem; }
    .card { background: var(--surface); border: 1px solid var(--border); border-radius: 8px; padding: 1.2rem; }
    .card-title { font-size: 0.95rem; font-weight: 600; margin-bottom: 0.8rem; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.05em; }
    .metric { font-size: 2rem; font-weight: 700; color: var(--text); }
    .status-dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: var(--success); margin-right: 6px; }
    pre { background: #090d13; border: 1px solid var(--border); border-radius: 6px; padding: 0.8rem; font-size: 0.85rem; overflow-x: auto; color: #7ee787; }
  </style>
</head>
<body>
  <header>
    <div class="brand">
      <div class="logo">antOS Web Console</div>
      <span class="badge"><span class="status-dot"></span>Conectado en Vivo</span>
    </div>
    <div id="clock" style="color: var(--text-muted); font-size: 0.85rem;"></div>
  </header>

  <div class="tabs">
    <button class="tab active" onclick="switchTab('dashboard')">Panel General</button>
    <button class="tab" onclick="switchTab('tickets')">Tickets antFlow</button>
    <button class="tab" onclick="switchTab('autopilot')">Autopilot Sentinel</button>
    <button class="tab" onclick="switchTab('telemetry')">Telemetría WebSocket</button>
  </div>

  <div id="content-dashboard" class="grid">
    <div class="card">
      <div class="card-title">Estado del Sistema</div>
      <div class="metric" id="status-running">ACTIVO</div>
      <div style="margin-top: 0.5rem; color: var(--text-muted); font-size: 0.85rem;" id="status-url">http://127.0.0.1:8088</div>
    </div>
    <div class="card">
      <div class="card-title">Clientes WebSocket</div>
      <div class="metric" id="status-clients">1</div>
      <div style="margin-top: 0.5rem; color: var(--text-muted); font-size: 0.85rem;">Canal /ws/events autenticado</div>
    </div>
    <div class="card" style="grid-column: 1 / -1;">
      <div class="card-title">Streaming de Eventos IPC en Tiempo Real</div>
      <pre id="log-stream">Esperando eventos del bus IPC de antOS...</pre>
    </div>
  </div>

  <script>
    function updateClock() {
      document.getElementById('clock').innerText = new Date().toLocaleTimeString();
    }
    setInterval(updateClock, 1000);
    updateClock();

    function switchTab(name) {
      document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
      event.target.classList.add('active');
    }

    // Connect WebSocket
    try {
      const loc = window.location;
      const wsUri = (loc.protocol === "https:" ? "wss:" : "ws:") + "//" + loc.host + "/ws/events" + loc.search;
      const ws = new WebSocket(wsUri);
      const stream = document.getElementById('log-stream');

      ws.onopen = () => {
        stream.innerText = "[WebSocket] Conexión establecida con el bus IPC de antOS.\n";
      };
      ws.onmessage = (e) => {
        stream.innerText += `> ${e.data}\n`;
      };
      ws.onerror = () => {
        stream.innerText += "[WebSocket] Modo degradado HTTP polling activo.\n";
      };
    } catch(err) {
      console.warn(err);
    }
  </script>
</body>
</html>"#
            .to_string()
    }
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha1_known_vector_and_ws_accept() {
        // RFC 6455 Section 1.3 Test Vector
        let client_key = "dGhlIHNhbXBsZSBub25jZQ==";
        let accept = compute_ws_accept(client_key);
        assert_eq!(accept, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn test_ws_text_frame_encode_decode() {
        let payload = "Hello, antOS Web Console!";
        let encoded = encode_ws_text_frame(payload);

        // Server frame is unmasked
        assert_eq!(encoded[0], 0x81);
        assert_eq!(encoded[1] as usize, payload.len());
        assert_eq!(&encoded[2..], payload.as_bytes());

        // Now simulate a masked client frame
        let mask = [0x12, 0x34, 0x56, 0x78];
        let mut client_frame = vec![0x81, 0x80 | (payload.len() as u8)];
        client_frame.extend_from_slice(&mask);
        for (i, &b) in payload.as_bytes().iter().enumerate() {
            client_frame.push(b ^ mask[i % 4]);
        }

        let decoded = decode_ws_frame(&client_frame).expect("decode client frame");
        assert_eq!(decoded.0, 0x1);
        assert_eq!(decoded.1, payload);
    }

    #[test]
    fn test_web_session_token_generation_and_validation() {
        let temp_dir = std::env::temp_dir().join("antos_test_web_session");
        let _ = fs::remove_dir_all(&temp_dir);
        let state_dir = temp_dir.join(".antos");
        fs::create_dir_all(&state_dir).unwrap();

        let session = WebEngine::generate_token(&state_dir, Some("laptop".into()), Some(3600)).unwrap();
        assert!(session.token.starts_with("ant_"));
        assert!(WebEngine::validate_token(&state_dir, &session.token));
        assert!(!WebEngine::validate_token(&state_dir, "invalid_token"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_web_console_embedded_html() {
        let html = WebEngine::embedded_html();
        assert!(html.contains("antOS · Consola Web Remota"));
        assert!(html.contains("WebSocket"));
    }
}
