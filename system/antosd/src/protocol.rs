//! The boundary between the intent session and whoever observes it.
//!
//! The terminal implements this by printing and reading stdin; the daemon by
//! writing and reading the socket. The session does not know which is which,
//! and that ignorance is exactly what allows more than one interface without
//! duplicating logic or guarantees.

#[allow(unused_imports, deprecated)]
pub use antos_protocol::{BlastRadius, Enclosure, ExecutionResult, Proposal};
#[allow(unused_imports, deprecated)]
pub use antos_protocol::{Propuesta, Radio, Recinto, Resultado};

/// Handler for the interactive session lifecycle.
///
/// The terminal implements it by printing and reading stdin; the daemon by
/// writing and reading the socket. The session does not know which is which.
pub trait SessionHandler {
    /// Called when an intent planning process starts.
    fn on_start(&mut self, intent: &str, planner: &str) -> anyhow::Result<()>;

    /// Informational note during the session.
    fn on_note(&mut self, text: &str) -> anyhow::Result<()>;

    /// Presents the proposal and returns whether it is approved.
    ///
    /// Presenting and deciding are the same act, hence the same method:
    /// separating them would allow an interface that decides without showing.
    fn on_proposal(&mut self, proposal: &Proposal) -> anyhow::Result<bool>;

    /// Output produced by a read capability.
    fn on_output(&mut self, text: &str) -> anyhow::Result<()>;

    /// Final execution result.
    fn on_result(&mut self, result: &ExecutionResult) -> anyhow::Result<()>;
}

/// Backwards compatibility alias.
#[deprecated(note = "use SessionHandler")]
pub trait Interlocutor: SessionHandler {}
#[allow(deprecated)]
impl<T: SessionHandler> Interlocutor for T {}
