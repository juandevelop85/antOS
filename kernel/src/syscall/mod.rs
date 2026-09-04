//! antOS Syscall ABI — stable system call interface (T22.5).
//!
//! This module defines the **shared contract** between the kernel and userspace.
//! Every syscall number and its calling convention are defined here so that both
//! sides agree on the same constants. The user-space runtime (`libantos`) uses
//! the same numbers through its own copy of this table.
//!
//! ## Calling Convention
//!
//! ### x86_64
//! - Instruction: `syscall`
//! - Syscall number: `RAX`
//! - Arguments: `RDI`, `RSI`, `RDX` (3 max for now)
//! - Return value: `RAX`
//! - Clobbered by CPU: `RCX` (return RIP), `R11` (saved RFLAGS)
//!
//! ### AArch64
//! - Instruction: `svc #0`
//! - Syscall number: `x8`
//! - Arguments: `x0`..`x5`
//! - Return value: `x0`

/// Terminate the current process with an exit code.
///
/// Arguments: `arg1` = exit code.
/// Returns: does not return.
pub const SYS_EXIT: u64 = 1;

/// Write bytes to a file descriptor (currently only fd 1 = serial console).
///
/// Arguments: `arg1` = pointer to buffer, `arg2` = length in bytes.
/// Returns: number of bytes written, or `u64::MAX` on error.
pub const SYS_WRITE: u64 = 2;

/// Non-blocking read from a file descriptor (currently only fd 0 = keyboard/UART).
///
/// Arguments: `arg1` = pointer to destination buffer, `arg2` = max length.
/// Returns: number of bytes actually read (0 if nothing available), or `u64::MAX` on error.
pub const SYS_READ: u64 = 3;

/// Voluntarily yield the remaining time quantum to the scheduler.
///
/// Arguments: none.
/// Returns: 0 on success.
pub const SYS_YIELD: u64 = 4;

/// Get the process/task identifier of the calling process.
///
/// Arguments: none.
/// Returns: the PID (currently always 1 for init).
pub const SYS_GETPID: u64 = 5;

/// Map anonymous virtual memory pages.
///
/// Arguments: `arg1` = requested size in bytes (rounded up to page boundary).
/// Returns: starting virtual address of the mapped region, or `u64::MAX` on error.
pub const SYS_MMAP: u64 = 6;

/// Total number of defined syscalls (for bounds checking).
pub const SYSCALL_COUNT: u64 = 7;

/// The maximum size of a single `SYS_WRITE` or `SYS_READ` buffer (4 KiB).
pub const MAX_BUFFER_SIZE: u64 = 4096;

/// User-space address limit: addresses above this belong to the kernel.
///
/// On x86_64 the split is at the canonical address boundary.
/// On AArch64 the split is at the TTBR1 boundary.
#[cfg(target_arch = "x86_64")]
pub const USER_ADDRESS_LIMIT: u64 = 0x0000_8000_0000_0000;

#[cfg(target_arch = "aarch64")]
pub const USER_ADDRESS_LIMIT: u64 = 0x0000_FFFF_FFFF_FFFF;

/// Validates that a user-space buffer is entirely within the user address range.
#[inline]
pub fn validate_user_buffer(ptr: u64, len: u64) -> bool {
    if ptr == 0 || len > MAX_BUFFER_SIZE {
        return false;
    }
    let Some(end) = ptr.checked_add(len) else {
        return false;
    };
    end <= USER_ADDRESS_LIMIT
}
