//! Typed System Call Wrappers for antOS Userspace (T23.5).

pub const SYS_EXIT: u64 = 1;
pub const SYS_WRITE: u64 = 2;
pub const SYS_READ: u64 = 3;
pub const SYS_YIELD: u64 = 4;
pub const SYS_GETPID: u64 = 5;
pub const SYS_MMAP: u64 = 6;
pub const SYS_MUNMAP: u64 = 7;
pub const SYS_SPAWN: u64 = 8;
pub const SYS_WAITPID: u64 = 9;
pub const SYS_CHANNEL_CREATE: u64 = 10;
pub const SYS_CHANNEL_SEND: u64 = 11;
pub const SYS_CHANNEL_RECV: u64 = 12;
pub const SYS_FS_LIST: u64 = 13;
pub const SYS_FS_READFILE: u64 = 14;
pub const SYS_SYSINFO: u64 = 15;
pub const SYS_LAUNCH_DESKTOP: u64 = 16;

pub const EPERM: u64 = (-1i64) as u64;
pub const ENOENT: u64 = (-2i64) as u64;
pub const ESRCH: u64 = (-3i64) as u64;
pub const EIO: u64 = (-5i64) as u64;
pub const EAGAIN: u64 = (-11i64) as u64;
pub const ENOMEM: u64 = (-12i64) as u64;
pub const EFAULT: u64 = (-14i64) as u64;
pub const EINVAL: u64 = (-22i64) as u64;
pub const ENOSYS: u64 = (-38i64) as u64;

/// Raw 3-argument syscall. Kept for call sites that never need a 4th
/// argument; forwards to [`raw_syscall4`] with `arg4 = 0`.
///
/// # Safety
///
/// See [`raw_syscall4`]: the arguments must be whatever `number` (one of the
/// `SYS_*` constants) expects, since the kernel interprets them without any
/// further validation on this side of the boundary.
#[inline(always)]
pub unsafe fn raw_syscall(number: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    unsafe { raw_syscall4(number, arg1, arg2, arg3, 0) }
}

/// Raw 4-argument syscall. `SYS_FS_LIST`, `SYS_FS_READFILE` and `SYS_SYSINFO`
/// are the only ones that use the 4th slot today (T26.5).
///
/// # Safety
///
/// `number` must be one of the `SYS_*` constants above, and `arg1..arg4`
/// must be valid for whatever that syscall expects (e.g. a pointer/length
/// pair naming memory this process actually owns) — the kernel trusts these
/// values as-is once the trap instruction fires.
#[cfg(target_arch = "x86_64")]
#[inline]
pub unsafe fn raw_syscall4(number: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64 {
    let result: u64;
    core::arch::asm!(
        "syscall",
        inlateout("rax") number => result,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        in("r10") arg4,
        lateout("rcx") _,
        lateout("r11") _,
        clobber_abi("sysv64"),
    );
    result
}

/// # Safety
///
/// Same contract as the x86_64 variant above.
#[cfg(target_arch = "aarch64")]
#[inline]
pub unsafe fn raw_syscall4(number: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64 {
    let result: u64;
    core::arch::asm!(
        "svc #0",
        in("x8") number,
        inlateout("x0") arg1 => result,
        in("x1") arg2,
        in("x2") arg3,
        in("x3") arg4,
        options(nomem, nostack),
    );
    result
}

#[inline(always)]
pub fn exit(code: u64) -> ! {
    unsafe {
        raw_syscall(SYS_EXIT, code, 0, 0);
    }
    // SYS_EXIT never returns in practice; this is the safety net for the
    // (should-be-impossible) case where the kernel hands control back
    // anyway. `spin_loop` (not an empty `loop {}`) hints the CPU it's a
    // deliberate spin-wait, not a bug.
    loop {
        core::hint::spin_loop();
    }
}

#[inline(always)]
pub fn write(text: &str) -> Result<usize, u64> {
    let res = unsafe { raw_syscall(SYS_WRITE, text.as_ptr() as u64, text.len() as u64, 0) };
    if res == EFAULT || res == EINVAL || res == u64::MAX {
        Err(res)
    } else {
        Ok(res as usize)
    }
}

#[inline(always)]
pub fn read(buf: &mut [u8]) -> Result<usize, u64> {
    let res = unsafe { raw_syscall(SYS_READ, buf.as_mut_ptr() as u64, buf.len() as u64, 0) };
    if res == EFAULT || res == EINVAL || res == u64::MAX {
        Err(res)
    } else {
        Ok(res as usize)
    }
}

#[inline(always)]
pub fn yield_cpu() {
    unsafe {
        raw_syscall(SYS_YIELD, 0, 0, 0);
    }
}

/// Voluntarily yields the remaining time quantum. Same syscall as
/// [`yield_cpu`]; this is the name used by the rest of `libantos` (T26.5).
#[inline(always)]
pub fn yield_now() {
    yield_cpu()
}

#[inline(always)]
pub fn getpid() -> u64 {
    unsafe { raw_syscall(SYS_GETPID, 0, 0, 0) }
}

#[inline(always)]
pub fn mmap(size: usize) -> Result<*mut u8, u64> {
    let res = unsafe { raw_syscall(SYS_MMAP, size as u64, 0, 0) };
    if res == ENOMEM || res == EINVAL || res == u64::MAX {
        Err(res)
    } else {
        Ok(res as *mut u8)
    }
}

#[inline(always)]
pub fn munmap(ptr: *mut u8, size: usize) -> Result<(), u64> {
    let res = unsafe { raw_syscall(SYS_MUNMAP, ptr as u64, size as u64, 0) };
    if res == 0 {
        Ok(())
    } else {
        Err(res)
    }
}

#[inline(always)]
pub fn spawn(path: &str, mode: u64) -> Result<u64, u64> {
    let res = unsafe { raw_syscall(SYS_SPAWN, path.as_ptr() as u64, path.len() as u64, mode) };
    if res == ENOENT || res == EINVAL || res == ENOMEM || res == EFAULT {
        Err(res)
    } else {
        Ok(res)
    }
}

#[inline(always)]
pub fn waitpid(pid: u64) -> Result<u64, u64> {
    let res = unsafe { raw_syscall(SYS_WAITPID, pid, 0, 0) };
    Ok(res)
}

#[inline(always)]
pub fn channel_create() -> Result<u64, u64> {
    let res = unsafe { raw_syscall(SYS_CHANNEL_CREATE, 0, 0, 0) };
    if res == ENOMEM || res == EINVAL {
        Err(res)
    } else {
        Ok(res)
    }
}

#[inline(always)]
pub fn channel_send(channel_id: u64, data: &[u8]) -> Result<usize, u64> {
    let res = unsafe {
        raw_syscall(
            SYS_CHANNEL_SEND,
            channel_id,
            data.as_ptr() as u64,
            data.len() as u64,
        )
    };
    if res == ENOENT || res == EINVAL || res == EAGAIN || res == EFAULT {
        Err(res)
    } else {
        Ok(res as usize)
    }
}

#[inline(always)]
pub fn channel_recv(channel_id: u64, buf: &mut [u8]) -> Result<usize, u64> {
    let res = unsafe {
        raw_syscall(
            SYS_CHANNEL_RECV,
            channel_id,
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
        )
    };
    if res == ENOENT || res == EINVAL || res == EAGAIN || res == EFAULT {
        Err(res)
    } else {
        Ok(res as usize)
    }
}

/// Lists VFS directory entries under `path`, formatted as `"KIND  SIZE  name\n"`
/// lines, into `out`. Returns the number of bytes written (T26.5).
#[inline(always)]
pub fn fs_list(path: &str, out: &mut [u8]) -> Result<usize, u64> {
    let res = unsafe {
        raw_syscall4(
            SYS_FS_LIST,
            path.as_ptr() as u64,
            path.len() as u64,
            out.as_mut_ptr() as u64,
            out.len() as u64,
        )
    };
    if res == ENOENT || res == EINVAL || res == EFAULT {
        Err(res)
    } else {
        Ok(res as usize)
    }
}

/// Reads the whole file at `path` from the VFS into `out`, truncating if it
/// does not fit. Returns the number of bytes copied (T26.5).
#[inline(always)]
pub fn fs_read_file(path: &str, out: &mut [u8]) -> Result<usize, u64> {
    let res = unsafe {
        raw_syscall4(
            SYS_FS_READFILE,
            path.as_ptr() as u64,
            path.len() as u64,
            out.as_mut_ptr() as u64,
            out.len() as u64,
        )
    };
    if res == ENOENT || res == EINVAL || res == EFAULT {
        Err(res)
    } else {
        Ok(res as usize)
    }
}

/// Fills `out` with basic system information text (architecture, heap usage,
/// uptime). Returns the number of bytes written (T26.5).
#[inline(always)]
pub fn sysinfo(out: &mut [u8]) -> Result<usize, u64> {
    let res = unsafe { raw_syscall(SYS_SYSINFO, out.as_mut_ptr() as u64, out.len() as u64, 0) };
    if res == EFAULT || res == EINVAL {
        Err(res)
    } else {
        Ok(res as usize)
    }
}

/// Asks the kernel to render (or hand off to) the native Desktop Shell
/// compositor. `Ok(())` means a graphical session is active; `Err` means
/// no framebuffer session is available this boot (T26.5).
#[inline(always)]
pub fn launch_desktop() -> Result<(), u64> {
    let res = unsafe { raw_syscall(SYS_LAUNCH_DESKTOP, 0, 0, 0) };
    if res == 0 {
        Ok(())
    } else {
        Err(res)
    }
}
