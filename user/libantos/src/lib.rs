//! libantos — Standard User Runtime Library for antOS (T23.5).
//!
//! Provides typed syscall interfaces, IPC channels, and a global heap allocator
//! backed by `SYS_MMAP` allowing usage of `alloc` primitives (`Vec`, `String`, `Box`)
//! in userspace.

#![no_std]

pub mod allocator;
pub mod channel;
pub mod syscall;

pub use allocator::UserHeapAllocator;
pub use channel::Channel;
pub use syscall::*;
