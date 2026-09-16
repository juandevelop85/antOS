//! Transporte del socket IPC: aceptar, enmarcar (una línea = un
//! mensaje), leer y escribir — separado del despacho de peticiones,
//! que sigue en `handle_connection` (T31.15: extraído de `ipc.rs`,
//! coordinado con los límites y tiempos de espera de T31.8).
#![allow(unused_imports, dead_code)]

use crate::capability::Catalog;
use crate::ctx::Ctx;
use crate::protocol::{ExecutionResult, Proposal, SessionHandler};
use crate::terminal;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// One line of JSON is one message (T31.8). A client that never sends `\n`
/// must not be able to grow the receive buffer until the daemon runs out of
/// memory — generous enough for any real `Request`/`Event`, including a
/// large diff proposal.
pub(crate) const MAX_MESSAGE_BYTES: u64 = 4 * 1024 * 1024;

/// How long a freshly accepted connection has to send its first message
/// before it's considered abandoned (T31.8). Applies only to that initial
/// wait — see the module doc comment on why the read timeout is lifted
/// once a real request arrives.
pub(crate) const IPC_READ_TIMEOUT: Duration = Duration::from_secs(10);
/// Applies for the whole connection: a write that can't complete within
/// this means the peer is gone, at any point in the session.
pub(crate) const IPC_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

pub fn socket_path(ctx: &Ctx) -> PathBuf {
    ctx.state.join("antos.sock")
}

// ------------------------------------------------------------- el protocolo

pub use antos_protocol::{Event, PackageAppType, Request};

/// Una línea de JSON por mensaje. Sin marco binario ni longitudes: se puede
/// leer con `nc` y depurar mirándolo, que a esta escala vale más que los
/// bytes que ahorraría.
pub(crate) fn send<T: Serialize>(dest: &mut impl Write, msg: &T) -> Result<()> {
    writeln!(dest, "{}", serde_json::to_string(msg)?)?;
    dest.flush()?;
    Ok(())
}

/// What a single call to [`receive`] found on the wire.
pub(crate) enum Received<T> {
    /// A well-formed message, within the size limit.
    Message(T),
    /// The peer closed the connection before sending anything — not an
    /// error, just nothing left to do.
    Eof,
    /// The peer sent a line longer than `MAX_MESSAGE_BYTES` without a
    /// newline (T31.8). The connection is still open; the caller decides
    /// how to respond — typically an `Event::Error`, then closing.
    TooLarge,
}

pub(crate) fn receive<T: for<'a> Deserialize<'a>>(
    source: &mut impl BufRead,
) -> Result<Received<T>> {
    // Reading one byte past the limit is what lets this tell "the line is
    // exactly at the boundary" apart from "the line is longer than the
    // boundary and got truncated here" (T31.8).
    let mut line = String::new();
    let n = source
        .by_ref()
        .take(MAX_MESSAGE_BYTES + 1)
        .read_line(&mut line)?;
    if n == 0 {
        return Ok(Received::Eof);
    }
    if line.len() as u64 > MAX_MESSAGE_BYTES {
        return Ok(Received::TooLarge);
    }
    Ok(Received::Message(serde_json::from_str(line.trim())?))
}

// ------------------------------------------------------------- lado servidor

/// The daemon's session handler: writes to the socket instead of painting;
/// waits for a response instead of asking.
pub(crate) struct SocketHandler<'a> {
    pub(crate) writer: &'a mut UnixStream,
    pub(crate) reader: &'a mut BufReader<UnixStream>,
}

impl SessionHandler for SocketHandler<'_> {
    fn on_start(&mut self, intent: &str, planner: &str) -> Result<()> {
        send(
            self.writer,
            &Event::Start {
                intent: intent.to_string(),
                planner: planner.to_string(),
            },
        )
    }

    fn on_note(&mut self, text: &str) -> Result<()> {
        send(self.writer, &Event::Note(text.to_string()))
    }

    fn on_proposal(&mut self, proposal: &Proposal) -> Result<bool> {
        send(self.writer, &Event::Proposal(Box::new(proposal.clone())))?;

        match receive::<Request>(self.reader)? {
            Received::Message(Request::Approval(decision)) => Ok(decision),
            // A client that leaves without answering, disconnects, or sends
            // something oversized does not approve anything (T31.8).
            // Silence is never a yes.
            _ => Ok(false),
        }
    }

    fn on_output(&mut self, text: &str) -> Result<()> {
        send(self.writer, &Event::Output(text.to_string()))
    }

    fn on_result(&mut self, result: &ExecutionResult) -> Result<()> {
        send(self.writer, &Event::Result(result.clone()))
    }
}

/// Binds the IPC socket file under a temporarily restrictive process
/// `umask`, then restores the previous one (T31.8). This is the piece that
/// actually closes the permissions window: a `umask` of `0o077` denies
/// group/other access to *any* file the process creates while it's in
/// effect, so the socket can never exist — not even for the instant between
/// `bind` returning and an explicit `chmod` — with broader-than-owner
/// access. The follow-up `set_permissions` in [`bind_socket`] is exactness,
/// not the actual guarantee.
pub(crate) fn bind_socket_with_restrictive_umask(path: &Path) -> Result<UnixListener> {
    #[cfg(unix)]
    let previous_umask = unsafe { libc::umask(0o077) };

    let result =
        UnixListener::bind(path).with_context(|| format!("no pude escuchar en {}", path.display()));

    #[cfg(unix)]
    unsafe {
        libc::umask(previous_umask);
    }

    result
}

/// Binds the IPC socket with owner-only (`0600`) permissions from the
/// instant it exists (T31.8).
pub(crate) fn bind_socket(path: &Path) -> Result<UnixListener> {
    let listener = bind_socket_with_restrictive_umask(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Recognizes the specific I/O error a timed-out read or write produces, so
/// `serve` can log an abandoned connection distinctly (T31.8) instead of a
/// generic "session ended with error".
pub(crate) fn is_idle_timeout(err: &anyhow::Error) -> bool {
    err.downcast_ref::<std::io::Error>()
        .map(|e| {
            matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            )
        })
        .unwrap_or(false)
}

pub fn serve(ctx: &Ctx, catalog: &Catalog) -> Result<()> {
    let path = socket_path(ctx);
    // Un socket huérfano de una ejecución anterior impediría escuchar.
    let _ = std::fs::remove_file(&path);

    let listener = bind_socket(&path)?;

    println!(
        "{} {}",
        terminal::paint("antOS · demonio escuchando en", terminal::BOLD),
        terminal::paint(&path.display().to_string(), terminal::DIM)
    );

    let ctx = std::sync::Arc::new(ctx.clone());
    let catalog = std::sync::Arc::new(catalog.clone());

    for connection in listener.incoming() {
        let stream = match connection {
            Ok(s) => s,
            Err(e) => {
                eprintln!("conexión rechazada: {e}");
                continue;
            }
        };

        // T31.8: bounded from the start — lifted once inside
        // `handle_connection` for a connection that turns out to be live.
        let _ = stream.set_read_timeout(Some(IPC_READ_TIMEOUT));
        let _ = stream.set_write_timeout(Some(IPC_WRITE_TIMEOUT));

        let ctx_clone = std::sync::Arc::clone(&ctx);
        let catalog_clone = std::sync::Arc::clone(&catalog);

        // T32.3: Despacho concurrente de conexiones en hilos de trabajo dedicados.
        // Las peticiones de solo lectura responden inmediatamente sin verse bloqueadas
        // por intenciones interactivas o de larga duración.
        std::thread::spawn(move || {
            if let Err(e) = super::handle_connection(&ctx_clone, &catalog_clone, stream) {
                if is_idle_timeout(&e) {
                    eprintln!(
                        "conexión IPC abandonada: sin actividad durante {IPC_READ_TIMEOUT:?}, cerrada"
                    );
                } else {
                    eprintln!("sesión terminada con error: {e:#}");
                }
            }
        });
    }
    Ok(())
}

/// El mismo socket, para un run de agente (T33.2): los pasos y el informe
/// salen como eventos propios; la confirmación reutiliza la puerta
/// `Proposal`/`Approval` de una intención — silencio nunca es un sí.
impl crate::agent::AgentHandler for SocketHandler<'_> {
    fn on_step(&mut self, event: &antos_protocol::AgentStepEvent) -> Result<()> {
        send(self.writer, &Event::AgentStep(event.clone()))
    }
    fn on_confirm(&mut self, proposal: &Proposal) -> Result<bool> {
        SessionHandler::on_proposal(self, proposal)
    }
    fn on_note(&mut self, text: &str) -> Result<()> {
        send(self.writer, &Event::Note(text.to_string()))
    }
    fn on_done(&mut self, report: &antos_protocol::AgentReport) -> Result<()> {
        send(self.writer, &Event::AgentDone(Box::new(report.clone())))
    }
}
