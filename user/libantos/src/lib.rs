//! libantos — Sovereign Standard User Runtime Library for antOS (T23.5, extended T26.5).
//!
//! Provides typed syscall interfaces, IPC channels, a global heap allocator
//! backed by `SYS_MMAP` allowing usage of `alloc` primitives (`Vec`, `String`,
//! `Box`, `BTreeMap`) in userspace, and `print!`/`println!`/`read_line` (see
//! [`io`]) — everything `system/shell`'s interactive shell (the `antos-init`
//! binary, PID 1) needs without linking against `glibc`/`musl`.

#![no_std]

extern crate alloc;

pub mod allocator;
pub mod channel;
pub mod io;
pub mod syscall;

pub use allocator::UserHeapAllocator;
pub use channel::Channel;
pub use syscall::*;
