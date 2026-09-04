//! Higher-level Channel Abstraction for Userspace IPC (T23.5).

use crate::syscall;

/// A typed wrapper around an antOS kernel IPC channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Channel {
    id: u64,
}

impl Channel {
    /// Creates a new IPC channel.
    pub fn create() -> Result<Self, u64> {
        let id = syscall::channel_create()?;
        Ok(Self { id })
    }

    /// Wraps an existing known channel ID.
    pub const fn from_id(id: u64) -> Self {
        Self { id }
    }

    /// Returns the raw channel ID.
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Sends a buffer across the channel.
    pub fn send(&self, data: &[u8]) -> Result<usize, u64> {
        syscall::channel_send(self.id, data)
    }

    /// Receives a buffer from the channel.
    pub fn recv(&self, buf: &mut [u8]) -> Result<usize, u64> {
        syscall::channel_recv(self.id, buf)
    }
}
