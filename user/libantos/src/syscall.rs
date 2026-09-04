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

pub const EPERM: u64 = (-1i64) as u64;
pub const ENOENT: u64 = (-2i64) as u64;
pub const ESRCH: u64 = (-3i64) as u64;
pub const EIO: u64 = (-5i64) as u64;
pub const EAGAIN: u64 = (-11i64) as u64;
pub const ENOMEM: u64 = (-12i64) as u64;
pub const EFAULT: u64 = (-14i64) as u64;
pub const EINVAL: u64 = (-22i64) as u64;
pub const ENOSYS: u64 = (-38i64) as u64;

#[cfg(target_arch = "x86_64")]
#[inline]
pub unsafe fn raw_syscall(number: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let result: u64;
    core::arch::asm!(
        "syscall",
        inlateout("rax") number => result,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        lateout("rcx") _,
        lateout("r11") _,
        clobber_abi("sysv64"),
    );
    result
}

#[cfg(target_arch = "aarch64")]
#[inline]
pub unsafe fn raw_syscall(number: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let result: u64;
    core::arch::asm!(
        "svc #0",
        in("x8") number,
        inlateout("x0") arg1 => result,
        in("x1") arg2,
        in("x2") arg3,
        options(nomem, nostack),
    );
    result
}

#[inline(always)]
pub fn exit(code: u64) -> ! {
    unsafe {
        raw_syscall(SYS_EXIT, code, 0, 0);
    }
    loop {}
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
    let res = unsafe { raw_syscall(SYS_CHANNEL_SEND, channel_id, data.as_ptr() as u64, data.len() as u64) };
    if res == ENOENT || res == EINVAL || res == EAGAIN || res == EFAULT {
        Err(res)
    } else {
        Ok(res as usize)
    }
}

#[inline(always)]
pub fn channel_recv(channel_id: u64, buf: &mut [u8]) -> Result<usize, u64> {
    let res = unsafe { raw_syscall(SYS_CHANNEL_RECV, channel_id, buf.as_mut_ptr() as u64, buf.len() as u64) };
    if res == ENOENT || res == EINVAL || res == EAGAIN || res == EFAULT {
        Err(res)
    } else {
        Ok(res as usize)
    }
}
