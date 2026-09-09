//! antOS Embedded Real-Time Web Console and WebSocket Bridge (T16.4).
//!
//! Provides a lightweight embedded HTTP/WebSocket server allowing developers to monitor
//! and orchestrate antOS remotely through modern web browsers, with real-time streaming
//! of telemetry, Kanban tickets, autopilot incidents, and cryptographic token auth.

use anyhow::{bail, Context, Result};
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

/// Reads `n` bytes of OS-provided cryptographic randomness from `/dev/urandom`
/// (present on both macOS and Linux, the two supported hosts). Session tokens
/// must never be derived from the clock, a label, or any other predictable
/// input — see T31.1.
fn secure_random_bytes(n: usize) -> Result<Vec<u8>> {
    let mut f = fs::File::open("/dev/urandom").context("opening /dev/urandom for secure token generation")?;
    let mut buf = vec![0u8; n];
    f.read_exact(&mut buf).context("reading secure randomness from /dev/urandom")?;
    Ok(buf)
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Constant-time comparison of two equal-length, hex-encoded digests, so that
/// token validation does not leak a stored hash through timing side-channels.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (ab, bb) = (a.as_bytes(), b.as_bytes());
    if ab.len() != bb.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in ab.iter().zip(bb.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Seconds since the Unix epoch, without panicking if the system clock is
/// ever set before 1970 (the wider sweep of this pattern is T31.7).
fn unix_now() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is set before the Unix epoch")?
        .as_secs())
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

/// On-disk representation of a session. Deliberately **not** `WebAuthSession`:
/// the plaintext token is shown to the caller exactly once, at generation
/// time, and never written to disk again. Only a salted digest is persisted
/// (T31.1), so a leak or backup of `sessions.json` cannot be replayed.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSessionRecord {
    /// Random per-session salt, hex-encoded.
    salt: String,
    /// `sha256(salt ":" token)`, hex-encoded.
    token_hash: String,
    created_at: u64,
    expires_at: u64,
    client_label: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct StoredSessions {
    #[serde(default)]
    sessions: Vec<StoredSessionRecord>,
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
    ///
    /// The token carries 256 bits of OS-provided entropy (T31.1): it is never
    /// derived from the clock, the client label, or any other guessable
    /// input. Only the returned [`WebAuthSession`] carries the plaintext
    /// token — disk storage keeps a salted digest only.
    pub fn generate_token(
        state_dir: &Path,
        client_label: Option<String>,
        ttl_secs: Option<u64>,
    ) -> Result<WebAuthSession> {
        let dir = Self::web_dir(state_dir);
        fs::create_dir_all(&dir)?;

        let now = unix_now()?;
        let ttl = ttl_secs.unwrap_or(86400); // 24 hours default
        let expires_at = now.saturating_add(ttl);

        let token = format!("ant_{}", to_hex(&secure_random_bytes(32)?));
        let salt = to_hex(&secure_random_bytes(16)?);
        let token_hash = crate::pkg::crypto::sha256(format!("{salt}:{token}").as_bytes());

        let session = WebAuthSession {
            token,
            created_at: now,
            expires_at,
            client_label: client_label.clone(),
        };

        let mut stored = Self::load_sessions(state_dir);
        // Prune expired sessions
        stored.sessions.retain(|s| s.expires_at > now);
        stored.sessions.push(StoredSessionRecord {
            salt,
            token_hash,
            created_at: now,
            expires_at,
            client_label,
        });

        Self::save_sessions(state_dir, &stored)?;
        Ok(session)
    }

    /// Validates an incoming token against stored active sessions.
    ///
    /// Compares the salted digest of `token` against each stored record in
    /// constant time, and rejects anything expired. A clock that cannot be
    /// read denies rather than panics.
    pub fn validate_token(state_dir: &Path, token: &str) -> bool {
        let Ok(now) = unix_now() else {
            return false;
        };
        let stored = Self::load_sessions(state_dir);
        stored.sessions.iter().any(|s| {
            if s.expires_at <= now {
                return false;
            }
            let candidate_hash = crate::pkg::crypto::sha256(format!("{}:{}", s.salt, token).as_bytes());
            constant_time_eq(&candidate_hash, &s.token_hash)
        })
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
        StoredSessions::default()
    }

    /// Persists sessions with owner-only permissions, mirroring `Vault::save`.
    fn save_sessions(state_dir: &Path, stored: &StoredSessions) -> Result<()> {
        let path = Self::sessions_path(state_dir);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, serde_json::to_string_pretty(stored)?)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&path, perms)?;
        }

        Ok(())
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
        let now = unix_now()?;
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

    /// Refuses to bind to a non-loopback address unless the operator has
    /// explicitly opted in via `ANTOS_WEB_ALLOW_REMOTE_BIND`, so a console
    /// meant for local development doesn't silently become reachable from
    /// the network (T31.1).
    fn ensure_safe_bind_addr(bind_addr: &str) -> Result<()> {
        let is_loopback = matches!(bind_addr, "127.0.0.1" | "localhost" | "::1")
            || bind_addr.parse::<std::net::IpAddr>().map(|ip| ip.is_loopback()).unwrap_or(false);

        if is_loopback || std::env::var_os("ANTOS_WEB_ALLOW_REMOTE_BIND").is_some() {
            return Ok(());
        }

        bail!(
            "antOS Web Console: se rechaza el enlace a «{bind_addr}» por no ser loopback; \
             expondría la consola a la red. Si es intencional, repite el comando con la \
             variable de entorno ANTOS_WEB_ALLOW_REMOTE_BIND=1."
        );
    }

    /// Starts the embedded HTTP and WebSocket server.
    pub fn start(
        state_dir: &Path,
        workspace_dir: &Path,
        config: WebConsoleConfig,
    ) -> Result<WebConsoleStatus> {
        Self::ensure_safe_bind_addr(&config.bind_addr)?;
        let auth_required = config.auth_required;
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
                            let _ = Self::handle_client(&mut stream, &s_dir, &w_dir, auth_required);
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
        Self::ensure_safe_bind_addr(&config.bind_addr)?;
        let auth_required = config.auth_required;
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
                        let _ = Self::handle_client(&mut stream, &s_dir, &w_dir, auth_required);
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

    /// Writes a `401 Unauthorized` response and nothing else — no status,
    /// ticket, or incident data ever reaches an unauthenticated caller
    /// (T31.1).
    fn respond_unauthorized(stream: &mut TcpStream) -> Result<()> {
        let body = b"Unauthorized: a valid antOS web console token is required\n";
        let response = format!(
            "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Bearer realm=\"antOS Web Console\"\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(response.as_bytes())?;
        stream.write_all(body)?;
        Ok(())
    }

    /// Handles a single incoming HTTP request or WebSocket upgrade.
    ///
    /// When `auth_required` is set (the default, from
    /// [`WebConsoleConfig::auth_required`]), every route that can return
    /// data — the JSON API and the WebSocket upgrade — requires a valid,
    /// unexpired token before doing any work (T31.1). The token is accepted
    /// via `Authorization: Bearer` everywhere; the `?token=` query parameter
    /// is honored **only** for the WebSocket upgrade, because the browser
    /// `WebSocket` constructor cannot set custom request headers on its
    /// handshake and there is no other way for this page's own event stream
    /// to authenticate itself. Every other route ignores it, so a token
    /// never has to appear in a URL, browser history, or access log.
    fn handle_client(stream: &mut TcpStream, state_dir: &Path, workspace_dir: &Path, auth_required: bool) -> Result<()> {
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

        // `?token=`: accepted only for the WebSocket upgrade below (see the
        // doc comment on this function for why).
        let mut query_token = None;
        if !query.is_empty() {
            for param in query.split('&') {
                if let Some((k, v)) = param.split_once('=') {
                    if k == "token" {
                        query_token = Some(v.to_string());
                        break;
                    }
                }
            }
        }

        let mut is_ws_upgrade = false;
        let mut ws_key = None;
        let mut header_token = None;

        for line in lines {
            let lower = line.to_lowercase();
            if lower.starts_with("authorization: bearer ") {
                header_token = Some(line[22..].trim().to_string());
            } else if lower.starts_with("upgrade:") && lower.contains("websocket") {
                is_ws_upgrade = true;
            } else if lower.starts_with("sec-websocket-key:") {
                ws_key = line.split_once(':').map(|(_, k)| k.trim().to_string());
            }
        }

        let header_authed = header_token.as_deref().map(|t| Self::validate_token(state_dir, t)).unwrap_or(false);

        // Check if WebSocket upgrade requested
        if is_ws_upgrade {
            if let Some(key) = ws_key.as_deref() {
                if auth_required {
                    let ws_authed = header_authed
                        || query_token.as_deref().map(|t| Self::validate_token(state_dir, t)).unwrap_or(false);
                    if !ws_authed {
                        return Self::respond_unauthorized(stream);
                    }
                }

                let accept = compute_ws_accept(key);
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
        }

        // API endpoints — token required via `Authorization: Bearer` only.
        if path == "/api/status" {
            if auth_required && !header_authed {
                return Self::respond_unauthorized(stream);
            }
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
            if auth_required && !header_authed {
                return Self::respond_unauthorized(stream);
            }
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
            if auth_required && !header_authed {
                return Self::respond_unauthorized(stream);
            }
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

        // Serve embedded HTML Single Page App. This is the static shell only
        // — it carries no system data, so it is intentionally reachable
        // without a token; every route above that does return data requires
        // one.
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

    // ---------------------------------------------------------------- T31.1

    fn t31_1_temp_state_dir(label: &str) -> PathBuf {
        let temp_dir = std::env::temp_dir().join(format!("antos_test_web_t31_1_{label}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let state_dir = temp_dir.join(".antos");
        fs::create_dir_all(&state_dir).unwrap();
        state_dir
    }

    #[test]
    fn test_two_tokens_generated_in_the_same_second_are_distinct() {
        let state_dir = t31_1_temp_state_dir("distinct_tokens");

        let a = WebEngine::generate_token(&state_dir, Some("laptop".into()), Some(3600)).unwrap();
        let b = WebEngine::generate_token(&state_dir, Some("laptop".into()), Some(3600)).unwrap();

        assert_ne!(a.token, b.token, "tokens must carry real entropy, not a clock-derived value");
        assert!(WebEngine::validate_token(&state_dir, &a.token));
        assert!(WebEngine::validate_token(&state_dir, &b.token));

        let _ = fs::remove_dir_all(state_dir.parent().unwrap());
    }

    #[test]
    fn test_sessions_file_never_contains_the_plaintext_token() {
        let state_dir = t31_1_temp_state_dir("no_cleartext");

        let session = WebEngine::generate_token(&state_dir, Some("laptop".into()), Some(3600)).unwrap();
        let raw = fs::read_to_string(WebEngine::sessions_path(&state_dir)).unwrap();

        assert!(!raw.contains(&session.token), "sessions.json must never store the plaintext token");
        assert!(raw.contains("token_hash"), "sessions.json should store a salted digest instead");

        let _ = fs::remove_dir_all(state_dir.parent().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn test_sessions_file_has_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let state_dir = t31_1_temp_state_dir("perms");
        let _ = WebEngine::generate_token(&state_dir, None, Some(3600)).unwrap();

        let mode = fs::metadata(WebEngine::sessions_path(&state_dir)).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "sessions.json must be readable only by its owner");

        let _ = fs::remove_dir_all(state_dir.parent().unwrap());
    }

    #[test]
    fn test_legacy_clock_derived_token_no_longer_validates() {
        let state_dir = t31_1_temp_state_dir("legacy_scheme");

        let label = Some("legacy-client".to_string());
        let ttl = 3600u64;
        let session = WebEngine::generate_token(&state_dir, label.clone(), Some(ttl)).unwrap();

        // Reconstruct exactly what the pre-T31.1 scheme would have produced
        // for the very same inputs and creation second.
        let seed = format!("{}-{ttl}-{:?}", session.created_at, label);
        let legacy_hash = sha1(seed.as_bytes());
        let legacy_token = format!("ant_{}", to_hex(&legacy_hash));

        assert_ne!(legacy_token, session.token, "sanity: the new scheme must not coincide with the old one");
        assert!(!WebEngine::validate_token(&state_dir, &legacy_token));

        let _ = fs::remove_dir_all(state_dir.parent().unwrap());
    }

    #[test]
    fn test_web_console_refuses_non_loopback_bind_without_explicit_opt_in() {
        std::env::remove_var("ANTOS_WEB_ALLOW_REMOTE_BIND");
        assert!(WebEngine::ensure_safe_bind_addr("127.0.0.1").is_ok());
        assert!(WebEngine::ensure_safe_bind_addr("localhost").is_ok());

        let err = WebEngine::ensure_safe_bind_addr("0.0.0.0").expect_err("must refuse a non-loopback bind");
        assert!(err.to_string().contains("ANTOS_WEB_ALLOW_REMOTE_BIND"));

        std::env::set_var("ANTOS_WEB_ALLOW_REMOTE_BIND", "1");
        assert!(WebEngine::ensure_safe_bind_addr("0.0.0.0").is_ok());
        std::env::remove_var("ANTOS_WEB_ALLOW_REMOTE_BIND");
    }

    /// Sends `raw_request` to a fresh, single-shot `handle_client` and
    /// returns the raw HTTP response text.
    fn t31_1_send_request(state_dir: &Path, workspace_dir: &Path, raw_request: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let req_owned = raw_request.to_string();
        let client_handle = thread::spawn(move || {
            let mut client = TcpStream::connect(addr).unwrap();
            client.write_all(req_owned.as_bytes()).unwrap();
            let mut resp = Vec::new();
            client.read_to_end(&mut resp).ok();
            String::from_utf8_lossy(&resp).to_string()
        });
        let (mut server_stream, _) = listener.accept().unwrap();
        let _ = WebEngine::handle_client(&mut server_stream, state_dir, workspace_dir, true);
        drop(server_stream);
        client_handle.join().unwrap()
    }

    #[test]
    fn test_protected_routes_reject_missing_or_invalid_tokens_and_leak_nothing() {
        let state_dir = t31_1_temp_state_dir("protected_routes");
        let workspace_dir = state_dir.parent().unwrap().join("workspace");
        fs::create_dir_all(&workspace_dir).unwrap();

        let ws_upgrade_line = "GET /ws/events HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n";

        let no_token_requests = [
            ("/api/status", "GET /api/status HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n".to_string()),
            ("/api/tickets", "GET /api/tickets HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n".to_string()),
            ("/api/autopilot", "GET /api/autopilot HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n".to_string()),
            ("/ws/events upgrade", format!("{ws_upgrade_line}\r\n")),
        ];

        for (label, raw_request) in &no_token_requests {
            let response = t31_1_send_request(&state_dir, &workspace_dir, raw_request);
            assert!(response.starts_with("HTTP/1.1 401"), "route «{label}» must reject a request with no token, got: {response}");
            assert!(!response.contains("Content-Type: application/json"), "route «{label}» must not leak JSON data on 401");
        }

        // An unknown-but-well-formed token must be rejected on every route too.
        let invalid_token_requests = [
            (
                "/api/status",
                "GET /api/status HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer ant_deadbeef\r\nConnection: close\r\n\r\n".to_string(),
            ),
            (
                "/api/tickets",
                "GET /api/tickets HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer ant_deadbeef\r\nConnection: close\r\n\r\n".to_string(),
            ),
            (
                "/api/autopilot",
                "GET /api/autopilot HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer ant_deadbeef\r\nConnection: close\r\n\r\n".to_string(),
            ),
            ("/ws/events upgrade", format!("{ws_upgrade_line}Authorization: Bearer ant_deadbeef\r\n\r\n")),
        ];

        for (label, raw_request) in &invalid_token_requests {
            let response = t31_1_send_request(&state_dir, &workspace_dir, raw_request);
            assert!(response.starts_with("HTTP/1.1 401"), "route «{label}» must reject an invalid token, got: {response}");
            assert!(!response.contains("Content-Type: application/json"), "route «{label}» must not leak JSON data on 401");
        }

        // A freshly generated, genuine token must be accepted.
        let session = WebEngine::generate_token(&state_dir, Some("test-client".into()), Some(3600)).unwrap();
        let good_req = format!(
            "GET /api/status HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
            session.token
        );
        let response = t31_1_send_request(&state_dir, &workspace_dir, &good_req);
        assert!(response.starts_with("HTTP/1.1 200"), "a valid token must be accepted, got: {response}");
        assert!(response.contains("Content-Type: application/json"));

        let _ = fs::remove_dir_all(state_dir.parent().unwrap());
    }
}
