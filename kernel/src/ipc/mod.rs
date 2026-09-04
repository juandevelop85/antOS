//! Inter-Process Communication (IPC) subsystem for antOS (T23.5).
//!
//! Provides channel-based microkernel message passing between userspace processes.

pub mod channel;

pub use channel::{
    active_channel_count, create_channel, recv_message, send_message, IpcChannel, IpcMessage,
};
