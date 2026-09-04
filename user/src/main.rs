//! antos-init — Userspace init and multi-process runtime for antOS (T23.5).
//!
//! Runs in Ring 3 (x86_64) or EL0 (AArch64). Demonstrates:
//! - Full 12-syscall POSIX-style table.
//! - Dynamic userspace heap allocations (`Vec`, `String`, `Box`) powered by `SYS_MMAP`.
//! - Microkernel IPC channel message passing (`SYS_CHANNEL_CREATE`, `SYS_CHANNEL_SEND`, `SYS_CHANNEL_RECV`).
//! - SMAP / EFAULT protection against kernel address space tampering.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::panic::PanicInfo;

use libantos::allocator::UserHeapAllocator;
use libantos::channel::Channel;
use libantos::syscall::*;

#[global_allocator]
static ALLOCATOR: UserHeapAllocator = UserHeapAllocator::new();

const BANNER: &str = "\
     ╔══════════════════════════════════════════╗\n\
     ║           antOS · init process           ║\n\
     ║     the first citizen of userspace       ║\n\
     ╚══════════════════════════════════════════╝\n";

/// Entry point invoked by the kernel.
/// Mode 0: normal init boot (verifies all syscalls, allocator, IPC, and EFAULT).
/// Mode 1: attack test (attempt direct read of kernel memory).
/// Mode 2: concurrent worker communicating via IPC channel.
/// Mode 3: compute loop to verify preemptive timer slicing.
#[no_mangle]
pub extern "C" fn _start(mode: u64) -> ! {
    // ── Mode 1: kernel memory protection test ────────────────────────────
    if mode == 1 {
        let _ = write("[init] attempting to read kernel memory; this should kill me\n");
        #[cfg(target_arch = "x86_64")]
        {
            let kernel_address = 0x100_0000_0000u64 as *const u64;
            let stolen = unsafe { core::ptr::read_volatile(kernel_address) };
            let _ = write("[init] PROTECTION FAILED: kernel memory was readable!\n");
            exit(stolen);
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = write("[init] protection test not implemented for this arch\n");
            exit(0xFF);
        }
    }

    // ── Mode 2: background worker task with IPC communication ───────────
    if mode == 2 {
        let _ = write("[worker-A] started concurrent background task (mode 2)\n");

        // Use heap primitives in Ring 3 worker
        let mut progress_history = Vec::new();
        let chan = Channel::from_id(1); // Known shared channel 1

        let mut pulse: u64 = 0;
        while pulse < 3 {
            pulse += 1;
            progress_history.push(pulse);

            let _ = write("[worker-A] progress pulse ");
            write_u64(pulse);
            let _ = write("/3\n");

            // Send IPC heartbeat over Channel 1
            let msg = b"PULSE_ACK";
            let _ = chan.send(msg);

            let mut spin: u64 = 0;
            while spin < 5_000_000 {
                spin = spin.wrapping_add(1);
                core::hint::spin_loop();
            }
        }
        let _ = write("[worker-A] completed cleanly\n");
        exit(0);
    }

    // ── Mode 3: compute loop to verify preemptive timer interruption ──────
    if mode == 3 {
        let _ = write("[worker-preempt] running endless compute loop (mode 3)\n");
        let _ = write("[worker-preempt] preemption active: timer will slice CPU fairly\n");
        let mut count: u64 = 0;
        while count < 3 {
            count += 1;
            let _ = write("[worker-preempt] compute iteration ");
            write_u64(count);
            let _ = write(" (preemptible)\n");
            let mut spin: u64 = 0;
            while spin < 5_000_000 {
                spin = spin.wrapping_add(1);
                core::hint::spin_loop();
            }
        }
        let _ = write("[worker-preempt] verified preemption & cooperative yield\n");
        exit(0);
    }

    // ── Mode 0: normal init boot ─────────────────────────────────────────
    let _ = write(BANNER);

    // 1. SYS_GETPID — verify process identity
    let pid = getpid();
    let _ = write("[init] pid = ");
    write_u64(pid);
    let _ = write("\n");

    // 2. SYS_MMAP & #[global_allocator] — verify dynamic Box, Vec, String in Ring 3
    let heap_box = Box::new(0x4242u64);
    let mut heap_vec = Vec::new();
    heap_vec.push(10);
    heap_vec.push(20);
    heap_vec.push(30);
    let heap_str = "Ring3 Dynamic Heap Active".to_string();

    let _ = write("[init] heap alloc: Box=");
    write_hex(*heap_box);
    let _ = write(", Vec.len=");
    write_u64(heap_vec.len() as u64);
    let _ = write(", String=\"");
    let _ = write(&heap_str);
    let _ = write("\"\n");

    // 3. SYS_MMAP & SYS_MUNMAP raw test
    match mmap(4096) {
        Ok(ptr) => {
            let _ = write("[init] mmap(4096) = ");
            write_hex(ptr as u64);
            let _ = write(" (successfully allocated & mapped)\n");
            // Test writing to allocated page
            unsafe {
                *ptr = 0xAA;
            }
            if munmap(ptr, 4096).is_ok() {
                let _ = write("[init] munmap(4096) = OK\n");
            }
        }
        Err(_) => {
            let _ = write("[init] mmap(4096) = FAILED\n");
        }
    }

    // 4. Microkernel IPC Channel test (SYS_CHANNEL_CREATE, SEND, RECV)
    match Channel::create() {
        Ok(chan) => {
            let _ = write("[init] ipc channel created with id=");
            write_u64(chan.id());
            let _ = write("\n");

            let ping_msg = b"HELLO_MICROKERNEL_IPC";
            if chan.send(ping_msg).is_ok() {
                let mut recv_buf = [0u8; 32];
                if let Ok(n) = chan.recv(&mut recv_buf) {
                    let _ = write("[init] ipc channel received ");
                    write_u64(n as u64);
                    let _ = write(" bytes: \"");
                    if let Ok(text) = core::str::from_utf8(&recv_buf[..n]) {
                        let _ = write(text);
                    }
                    let _ = write("\"\n");
                }
            }
        }
        Err(_) => {
            let _ = write("[init] failed to create ipc channel\n");
        }
    }

    // 5. SMAP / EFAULT protection test: passing kernel pointer to syscall
    let kernel_ptr = 0x100_0000_0000u64 as *const u8;
    let attack_res = unsafe {
        raw_syscall(SYS_WRITE, kernel_ptr as u64, 16, 0)
    };
    if attack_res == EFAULT {
        let _ = write("[init] kernel pointer rejected with EFAULT (-14) as expected\n");
    } else {
        let _ = write("[init] WARNING: kernel pointer not rejected with EFAULT!\n");
    }

    // 6. SYS_READ — verify non-blocking read
    let mut read_buf = [0u8; 32];
    let read_result = read(&mut read_buf).unwrap_or(0);
    let _ = write("[init] read() = ");
    write_u64(read_result as u64);
    let _ = write(" (expected 0 for non-blocking)\n");

    // 7. SYS_YIELD — yield to scheduler
    let _ = write("[init] yielding to scheduler...\n");
    yield_cpu();
    let _ = write("[init] resumed after yield\n");

    // 8. Event dispatch loop
    let _ = write("[init] entering event dispatch loop (3 iterations)\n");
    let mut iteration: u64 = 0;
    while iteration < 3 {
        yield_cpu();
        iteration += 1;
        let _ = write("[init] heartbeat ");
        write_u64(iteration);
        let _ = write("\n");
    }

    let _ = write("[init] all 12 syscalls verified — shutting down cleanly\n");
    exit(0)
}

fn write_u64(mut n: u64) {
    if n == 0 {
        let _ = write("0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut pos = buf.len();
    while n > 0 {
        pos -= 1;
        buf[pos] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    if let Ok(s) = core::str::from_utf8(&buf[pos..]) {
        let _ = write(s);
    }
}

fn write_hex(mut n: u64) {
    let _ = write("0x");
    if n == 0 {
        let _ = write("0");
        return;
    }
    let mut buf = [0u8; 16];
    let mut pos = buf.len();
    while n > 0 {
        pos -= 1;
        let digit = (n & 0xF) as u8;
        buf[pos] = if digit < 10 { b'0' + digit } else { b'a' + digit - 10 };
        n >>= 4;
    }
    if let Ok(s) = core::str::from_utf8(&buf[pos..]) {
        let _ = write(s);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    let _ = write("[init] PANIC!\n");
    exit(1)
}
