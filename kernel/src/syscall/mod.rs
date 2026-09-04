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
/// Returns: starting virtual address of the mapped region, or `ENOMEM`/`EINVAL` on error.
pub const SYS_MMAP: u64 = 6;

/// Unmap previously mapped virtual memory pages.
///
/// Arguments: `arg1` = starting virtual address, `arg2` = size in bytes.
/// Returns: 0 on success, or error code.
pub const SYS_MUNMAP: u64 = 7;

/// Spawn a new user process from an executable ELF path in the VFS.
///
/// Arguments: `arg1` = pointer to path string, `arg2` = length of path, `arg3` = start mode.
/// Returns: PID of the spawned process, or error code.
pub const SYS_SPAWN: u64 = 8;

/// Wait for a child or specified process to terminate.
///
/// Arguments: `arg1` = target PID (or 0 for any).
/// Returns: exit code of the terminated process, or error code.
pub const SYS_WAITPID: u64 = 9;

/// Create a bidirectional or unidirectional IPC message channel.
///
/// Arguments: none.
/// Returns: channel identifier (`u64`), or error code.
pub const SYS_CHANNEL_CREATE: u64 = 10;

/// Send a message buffer through an IPC channel.
///
/// Arguments: `arg1` = channel ID, `arg2` = pointer to message buffer, `arg3` = length in bytes.
/// Returns: number of bytes sent, or error code.
pub const SYS_CHANNEL_SEND: u64 = 11;

/// Receive a message from an IPC channel.
///
/// Arguments: `arg1` = channel ID, `arg2` = pointer to destination buffer, `arg3` = max length.
/// Returns: number of bytes received, or error code (`EAGAIN` if empty).
pub const SYS_CHANNEL_RECV: u64 = 12;

/// Total number of defined syscalls (for bounds checking).
pub const SYSCALL_COUNT: u64 = 13;

// ────────────────────────────────────────────── Standard POSIX Error Codes

pub const EPERM: u64 = (-1i64) as u64;
pub const ENOENT: u64 = (-2i64) as u64;
pub const ESRCH: u64 = (-3i64) as u64;
pub const EIO: u64 = (-5i64) as u64;
pub const EAGAIN: u64 = (-11i64) as u64;
pub const ENOMEM: u64 = (-12i64) as u64;
pub const EFAULT: u64 = (-14i64) as u64;
pub const EINVAL: u64 = (-22i64) as u64;
pub const ENOSYS: u64 = (-38i64) as u64;

/// The maximum size of a single `SYS_WRITE` or `SYS_READ` buffer (4 KiB).
pub const MAX_BUFFER_SIZE: u64 = 4096;

/// User-space address limit: addresses at or above this belong to the kernel.
///
/// In antOS on x86_64 the kernel image and physical map start at 1 TiB (0x100_0000_0000).
/// On AArch64 the split is at the TTBR1 boundary.
#[cfg(target_arch = "x86_64")]
pub const USER_ADDRESS_LIMIT: u64 = 0x0000_0100_0000_0000;

#[cfg(target_arch = "aarch64")]
pub const USER_ADDRESS_LIMIT: u64 = 0x0000_FFFF_FFFF_FFFF;

/// Validates that a user-space buffer is entirely within the user address range.
#[inline]
pub fn validate_user_buffer(ptr: u64, len: u64) -> bool {
    validate_user_ptr(ptr, len).is_ok()
}

/// Strictly validates user-space pointer safety (SMAP / EFAULT protection).
/// Returns `Ok(())` if buffer is strictly in userspace, or `Err(EFAULT)`.
#[inline]
pub fn validate_user_ptr(ptr: u64, len: u64) -> Result<(), u64> {
    if ptr == 0 {
        return Err(EFAULT);
    }
    let Some(end) = ptr.checked_add(len) else {
        return Err(EFAULT);
    };
    if end > USER_ADDRESS_LIMIT {
        return Err(EFAULT);
    }
    Ok(())
}
