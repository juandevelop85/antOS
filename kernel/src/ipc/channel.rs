//! Microkernel IPC Channels for antOS (T23.5).
//!
//! Provides bounded, thread-safe message queues associated with channel IDs,
//! allowing ring-3 processes to exchange typed messages without disk I/O.

extern crate alloc;

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use crate::sync::SpinLock;
use crate::syscall::{EAGAIN, EINVAL, ENOENT, ENOMEM};

pub const MAX_MESSAGE_SIZE: usize = 1024;
pub const MAX_CHANNEL_MESSAGES: usize = 32;

/// An individual message stored inside a kernel IPC channel.
#[derive(Debug, Clone)]
pub struct IpcMessage {
    pub sender_pid: u64,
    pub data: Vec<u8>,
}

/// A bounded, bidirectional message-passing channel.
pub struct IpcChannel {
    pub id: u64,
    pub messages: VecDeque<IpcMessage>,
    pub max_messages: usize,
    pub waiting_receiver: Option<u64>,
}

impl IpcChannel {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            messages: VecDeque::new(),
            max_messages: MAX_CHANNEL_MESSAGES,
            waiting_receiver: None,
        }
    }

    pub fn send(&mut self, sender_pid: u64, data: &[u8]) -> Result<usize, u64> {
        if data.len() > MAX_MESSAGE_SIZE {
            return Err(EINVAL);
        }
        if self.messages.len() >= self.max_messages {
            return Err(EAGAIN);
        }

        self.messages.push_back(IpcMessage {
            sender_pid,
            data: data.to_vec(),
        });

        Ok(data.len())
    }

    pub fn recv(&mut self, _receiver_pid: u64, out: &mut [u8]) -> Result<usize, u64> {
        if let Some(msg) = self.messages.pop_front() {
            let copy_len = msg.data.len().min(out.len());
            out[..copy_len].copy_from_slice(&msg.data[..copy_len]);
            Ok(copy_len)
        } else {
            Err(EAGAIN)
        }
    }
}

pub struct ChannelRegistry {
    channels: BTreeMap<u64, IpcChannel>,
}

impl ChannelRegistry {
    pub const fn new() -> Self {
        Self {
            channels: BTreeMap::new(),
        }
    }
}

static NEXT_CHANNEL_ID: AtomicU64 = AtomicU64::new(1);
static REGISTRY: SpinLock<ChannelRegistry> = SpinLock::new(ChannelRegistry::new());

/// Creates a new IPC channel and returns its unique channel ID.
pub fn create_channel() -> Result<u64, u64> {
    let id = NEXT_CHANNEL_ID.fetch_add(1, Ordering::SeqCst);
    let mut reg = REGISTRY.lock();
    if reg.channels.contains_key(&id) {
        return Err(ENOMEM);
    }
    reg.channels.insert(id, IpcChannel::new(id));
    Ok(id)
}

/// Sends data to the specified channel from `sender_pid`.
pub fn send_message(channel_id: u64, sender_pid: u64, data: &[u8]) -> Result<usize, u64> {
    let mut reg = REGISTRY.lock();
    let chan = reg.channels.get_mut(&channel_id).ok_or(ENOENT)?;
    chan.send(sender_pid, data)
}

/// Receives data from the specified channel into `out`.
pub fn recv_message(channel_id: u64, receiver_pid: u64, out: &mut [u8]) -> Result<usize, u64> {
    let mut reg = REGISTRY.lock();
    let chan = reg.channels.get_mut(&channel_id).ok_or(ENOENT)?;
    chan.recv(receiver_pid, out)
}

/// Returns the total number of active channels.
pub fn active_channel_count() -> usize {
    REGISTRY.lock().channels.len()
}
