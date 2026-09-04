//! antos-init — the first userspace process of antOS (T22.5).
//!
//! Runs in ring 3 (x86_64) or EL0 (AArch64). Cannot read kernel memory, talk
//! to hardware, or execute privileged instructions. Every request goes through
//! a system call.
//!
//! This binary is linked separately from the kernel and communicates with it
//! exclusively through the formal ABI defined in `kernel/src/syscall/mod.rs`.

#![no_std]
#![no_main]

use core::panic::PanicInfo;

// ────────────────────────────────────────────── Syscall ABI constants (T22.5)
// Mirrored from kernel/src/syscall/mod.rs — the shared contract.

const SYS_EXIT: u64 = 1;
const SYS_WRITE: u64 = 2;
const SYS_READ: u64 = 3;
const SYS_YIELD: u64 = 4;
const SYS_GETPID: u64 = 5;
const SYS_MMAP: u64 = 6;

// ────────────────────────────────────────────── Syscall wrapper (x86_64)

/// Raw syscall wrapper using the antOS ABI:
///   RAX = syscall number · RDI, RSI, RDX = arguments · RAX = result
///
/// RCX and R11 are clobbered by the `syscall` instruction itself.
#[cfg(target_arch = "x86_64")]
unsafe fn syscall(number: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let result: u64;
    unsafe {
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
    }
    result
}

// ────────────────────────────────────────────── Syscall wrapper (AArch64)

/// Raw syscall wrapper using the antOS ABI:
///   x8 = syscall number · x0..x5 = arguments · x0 = result
#[cfg(target_arch = "aarch64")]
unsafe fn syscall(number: u64, arg1: u64, arg2: u64, _arg3: u64) -> u64 {
    let result: u64;
    unsafe {
        core::arch::asm!(
            "svc #0",
            in("x8") number,
            inlateout("x0") arg1 => result,
            in("x1") arg2,
            in("x2") _arg3,
            options(nomem, nostack),
        );
    }
    result
}

// ────────────────────────────────────────────── High-level syscall API

fn write(message: &str) {
    unsafe { syscall(SYS_WRITE, message.as_ptr() as u64, message.len() as u64, 0) };
}

fn exit(code: u64) -> ! {
    unsafe { syscall(SYS_EXIT, code, 0, 0) };
    // The kernel does not return control, but the compiler does not know that.
    loop {}
}

fn yield_cpu() {
    unsafe { syscall(SYS_YIELD, 0, 0, 0) };
}

fn getpid() -> u64 {
    unsafe { syscall(SYS_GETPID, 0, 0, 0) }
}

fn mmap(size: u64) -> u64 {
    unsafe { syscall(SYS_MMAP, size, 0, 0) }
}

fn read(buf: &mut [u8]) -> u64 {
    unsafe { syscall(SYS_READ, buf.as_mut_ptr() as u64, buf.len() as u64, 0) }
}

// ────────────────────────────────────────────── antOS-init entry point

const BANNER: &str = "\
     ╔══════════════════════════════════════════╗\n\
     ║           antOS · init process           ║\n\
     ║     the first citizen of userspace       ║\n\
     ╚══════════════════════════════════════════╝\n";

/// The kernel passes the mode in RDI (x86_64) or x0 (AArch64).
///
/// Mode 0: normal boot — print banner, verify syscalls, enter event loop.
/// Mode 1: attack test — attempt to read kernel memory (should be killed).
#[no_mangle]
pub extern "C" fn _start(mode: u64) -> ! {
    // ── Mode 1: kernel memory protection test ────────────────────────────
    if mode == 1 {
        write("[init] attempting to read kernel memory; this should kill me\n");
        #[cfg(target_arch = "x86_64")]
        {
            let kernel_address = 0x100_0000_0000u64 as *const u64;
            let stolen = unsafe { core::ptr::read_volatile(kernel_address) };
            write("[init] PROTECTION FAILED: kernel memory was readable!\n");
            exit(stolen);
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            write("[init] protection test not implemented for this arch\n");
            exit(0xFF);
        }
    }

    // ── Mode 0: normal init boot ─────────────────────────────────────────
    write(BANNER);

    // SYS_GETPID — verify process identity.
    let pid = getpid();
    write("[init] pid = ");
    write_u64(pid);
    write("\n");

    // SYS_MMAP — verify anonymous memory mapping.
    let page = mmap(4096);
    if page != u64::MAX {
        write("[init] mmap(4096) = ");
        write_hex(page);
        write(" (anonymous page reserved)\n");
    } else {
        write("[init] mmap(4096) = FAILED\n");
    }

    // SYS_READ — verify non-blocking read (should return 0).
    let mut buf = [0u8; 64];
    let read_result = read(&mut buf);
    write("[init] read() = ");
    write_u64(read_result);
    write(" (expected 0 for non-blocking)\n");

    // SYS_YIELD — yield to scheduler.
    write("[init] yielding to scheduler...\n");
    yield_cpu();
    write("[init] resumed after yield\n");

    // ── Event dispatch loop ──────────────────────────────────────────────
    write("[init] entering event dispatch loop (3 iterations)\n");
    let mut iteration: u64 = 0;
    while iteration < 3 {
        yield_cpu();
        iteration += 1;
        write("[init] heartbeat ");
        write_u64(iteration);
        write("\n");
    }

    write("[init] all syscalls verified — shutting down cleanly\n");
    exit(0)
}

// ────────────────────────────────────────────── Formatting helpers
// We cannot use core::fmt::Write without an allocator, so we format numbers
// by hand using the stack.

fn write_u64(mut n: u64) {
    if n == 0 {
        write("0");
        return;
    }
    let mut buf = [0u8; 20]; // max digits in u64
    let mut pos = buf.len();
    while n > 0 {
        pos -= 1;
        buf[pos] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    let s = unsafe { core::str::from_utf8_unchecked(&buf[pos..]) };
    write(s);
}

fn write_hex(mut n: u64) {
    write("0x");
    if n == 0 {
        write("0");
        return;
    }
    let mut buf = [0u8; 16]; // max hex digits in u64
    let mut pos = buf.len();
    while n > 0 {
        pos -= 1;
        let digit = (n & 0xF) as u8;
        buf[pos] = if digit < 10 { b'0' + digit } else { b'a' + digit - 10 };
        n >>= 4;
    }
    let s = unsafe { core::str::from_utf8_unchecked(&buf[pos..]) };
    write(s);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    write("[init] PANIC!\n");
    exit(1)
}
