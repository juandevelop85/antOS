//! The IPC contract between the antOS daemon and its clients.
//!
//! Originally lived inside `antosd` when the only client was its own terminal.
//! Extracted into a standalone crate as soon as a second client appeared—the
//! intention bar—because the alternative would be each having its own copy of
//! these types. A duplicated protocol is a diverging protocol.
//!
//! There is NO execution logic here: permission tiers are not decided, diffs
//! are not calculated, and nothing is validated. That belongs in the daemon by
//! design—a client capable of computing its own tier could simply choose it.

pub mod agent;
pub mod dev;
pub mod elf;
pub mod flow;
pub mod git;
pub mod input;
pub mod ipc;
pub mod launcher;
pub mod mesh;
pub mod plan;
pub mod runtime;
pub mod spec;
pub mod system;
pub mod ui;
pub mod vm;
pub mod wasm;

#[cfg(test)]
mod tests;

pub use agent::*;
pub use dev::*;
pub use elf::*;
pub use flow::*;
pub use git::*;
pub use input::*;
pub use ipc::*;
pub use launcher::*;
pub use mesh::*;
pub use plan::*;
pub use runtime::*;
pub use spec::*;
pub use system::*;
pub use ui::*;
pub use vm::*;
pub use wasm::*;
