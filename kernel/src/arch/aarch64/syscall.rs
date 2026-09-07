//! System call handling and userspace transition (EL0) for AArch64 (T22.5).
//!
//! Implements syscall dispatch for `svc #0` (EC 0x15), userspace transition
//! via `eret` to EL0, and return to kernel on `SYS_EXIT`.

use core::sync::atomic::{AtomicU64, Ordering};
use crate::arch::aarch64::exceptions::ExceptionContext;
use crate::arch::traits::ArchSyscall;
use crate::syscall;
use crate::println;

static KERNEL_SP: AtomicU64 = AtomicU64::new(0);
static KERNEL_LR: AtomicU64 = AtomicU64::new(0);

/// Current process ID (always 1 for init).
static CURRENT_PID: AtomicU64 = AtomicU64::new(1);

/// `SYS_MMAP` on AArch64 (T26.5) hands out pages from a bump region 1 MiB
/// into the same pre-mapped 2 MiB user window that the ELF loader and stack
/// already live in (`USER_SPACE_VIRT`, see `arch::aarch64::mmu`) — there is
/// no per-process page table yet on this architecture, so "mapping" a page
/// means claiming a slice of the one window the CPU already trusts EL0 with.
const AARCH64_MMAP_BASE_OFFSET: u64 = 0x0010_0000;
/// Stop 8 KiB before the window ends: the last 4 KiB are the process stack
/// (`elf::load_aarch64`) and the 4 KiB before that are a guard gap.
const AARCH64_MMAP_LIMIT_OFFSET: u64 = 0x0020_0000 - 0x2000;
static AARCH64_MMAP_NEXT: AtomicU64 =
    AtomicU64::new(crate::arch::aarch64::mmu::USER_SPACE_VIRT + AARCH64_MMAP_BASE_OFFSET);

pub struct ArmSyscall;

impl ArchSyscall for ArmSyscall {
    #[inline]
    fn init() {
        // Enable FP/SIMD access for EL0 and EL1 (prevent traps)
        unsafe {
            let mut cpacr: u64;
            core::arch::asm!("mrs {}, cpacr_el1", out(reg) cpacr, options(nomem, nostack));
            cpacr |= 0b11 << 20; // FPEN = 0b11
            core::arch::asm!("msr cpacr_el1, {}", in(reg) cpacr, options(nomem, nostack));
            core::arch::asm!("isb", options(nomem, nostack));
        }
    }
}

/// Translates a validated user-space address inside the small pre-mapped
/// 2 MiB window into the kernel's own view of the same physical memory,
/// sidestepping PAN (Privileged Access Never) faults on real ARMv8.1+ CPUs.
/// Addresses outside that window are returned unchanged (they will already
/// have failed `validate_user_ptr`/`validate_user_buffer` by the time this
/// runs, or belong to future, more general mappings).
#[inline]
fn translate(uaddr: u64) -> u64 {
    let virt = crate::arch::aarch64::mmu::USER_SPACE_VIRT;
    if uaddr >= virt && uaddr < virt + 0x0020_0000 {
        (uaddr - virt) + crate::arch::aarch64::mmu::USER_SPACE_PHYS
    } else {
        uaddr
    }
}

/// Dispatches an AArch64 syscall triggered by `svc #0` (EC == 0x15).
///
/// ABI:
/// - Syscall ID in `x8`
/// - Arguments in `x0`..`x5`
/// - Return value stored in `x0`
pub fn dispatch(ctx: &mut ExceptionContext) {
    let syscall_no = ctx.x[8];

    match syscall_no {
        syscall::SYS_EXIT => {
            let exit_code = ctx.x[0];
            unsafe {
                return_to_kernel(exit_code);
            }
        }

        syscall::SYS_WRITE => {
            let uaddr = ctx.x[0];
            let len = ctx.x[1] as usize;

            if uaddr != 0 && len > 0 && syscall::validate_user_buffer(uaddr, len as u64) {
                // Safely translate user virtual address to kernel-mapped physical RAM
                // to avoid PAN (Privileged Access Never) hardware faults on Apple Silicon / ARMv8.1+
                let kaddr = if uaddr >= crate::arch::aarch64::mmu::USER_SPACE_VIRT
                    && uaddr < crate::arch::aarch64::mmu::USER_SPACE_VIRT + 0x0020_0000
                {
                    (uaddr - crate::arch::aarch64::mmu::USER_SPACE_VIRT) + crate::arch::aarch64::mmu::USER_SPACE_PHYS
                } else {
                    uaddr
                };
                let ptr = kaddr as *const u8;
                let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
                if let Ok(s) = core::str::from_utf8(slice) {
                    // Write the already-validated UTF-8 bytes straight to the
                    // wire. The previous per-byte `b as char` reinterpreted
                    // each byte of a multi-byte sequence as its own code
                    // point and re-encoded it, mangling anything outside
                    // ASCII (T26.5's box-drawing shell banner exposed this).
                    use core::fmt::Write as _;
                    let _ = crate::arch::aarch64::SERIAL.lock().write_str(s);

                    if crate::ui::compositor::is_desktop_active() {
                        let mut comp_guard = crate::ui::COMPOSITOR.lock();
                        if let Some(comp) = comp_guard.as_mut() {
                            comp.terminal_mut().write_str(s);
                            if let Some(c) = crate::console::CONSOLE.lock().as_mut() {
                                comp.render_to_framebuffer(
                                    c.framebuffer_mut(),
                                    crate::allocator::used(),
                                    crate::AARCH64_HEAP_SIZE,
                                    crate::arch::aarch64::timer::ticks(),
                                );
                            }
                        }
                    } else {
                        crate::console::_print(format_args!("{}", s));
                    }
                }
            }
            ctx.x[0] = len as u64;
        }

        syscall::SYS_READ => {
            let uaddr = ctx.x[0];
            let len = (ctx.x[1] as usize).min(syscall::MAX_BUFFER_SIZE as usize);
            if let Err(e) = syscall::validate_user_ptr(uaddr, len as u64) {
                ctx.x[0] = e;
            } else {
                let kaddr = translate(uaddr);
                let buf = unsafe { core::slice::from_raw_parts_mut(kaddr as *mut u8, len) };
                let mut n = crate::input::drain_ascii(buf);
                if n == 0 && len > 0 {
                    let mut serial = crate::arch::aarch64::SERIAL.lock();
                    while n < len {
                        if let Some(b) = serial.read_byte() {
                            buf[n] = b;
                            n += 1;
                        } else {
                            break;
                        }
                    }
                }
                ctx.x[0] = n as u64;
            }
        }

        syscall::SYS_YIELD => {
            if crate::ui::compositor::is_desktop_active() {
                if crate::ui::dispatch_pending_inputs(1024, 768) {
                    if let Some(c) = crate::console::CONSOLE.lock().as_mut() {
                        crate::ui::render_desktop(
                            c.framebuffer_mut(),
                            crate::allocator::used(),
                            crate::AARCH64_HEAP_SIZE,
                            crate::arch::aarch64::timer::ticks(),
                        );
                    }
                }
            }
            crate::arch::aarch64::exceptions::wait_for_interrupt();
            ctx.x[0] = 0;
        }


        syscall::SYS_GETPID => {
            ctx.x[0] = CURRENT_PID.load(Ordering::Relaxed);
        }

        syscall::SYS_MMAP => {
            let requested = ctx.x[0];
            if requested == 0 || requested > AARCH64_MMAP_LIMIT_OFFSET - AARCH64_MMAP_BASE_OFFSET {
                ctx.x[0] = syscall::EINVAL;
            } else {
                let size = (requested + 0xFFF) & !0xFFF;
                let limit = crate::arch::aarch64::mmu::USER_SPACE_VIRT + AARCH64_MMAP_LIMIT_OFFSET;
                let addr = AARCH64_MMAP_NEXT.fetch_add(size, Ordering::Relaxed);
                if addr + size > limit {
                    ctx.x[0] = syscall::ENOMEM;
                } else {
                    let phys = translate(addr);
                    unsafe { core::ptr::write_bytes(phys as *mut u8, 0, size as usize) };
                    ctx.x[0] = addr;
                }
            }
        }

        syscall::SYS_MUNMAP => {
            ctx.x[0] = 0;
        }

        syscall::SYS_SPAWN => {
            let path_ptr = ctx.x[0];
            let path_len = ctx.x[1];
            let _mode = ctx.x[2];

            if let Err(e) = syscall::validate_user_ptr(path_ptr, path_len) {
                ctx.x[0] = e;
            } else if path_len > 256 {
                ctx.x[0] = syscall::EINVAL;
            } else {
                let kaddr = if path_ptr >= crate::arch::aarch64::mmu::USER_SPACE_VIRT
                    && path_ptr < crate::arch::aarch64::mmu::USER_SPACE_VIRT + 0x0020_0000
                {
                    (path_ptr - crate::arch::aarch64::mmu::USER_SPACE_VIRT) + crate::arch::aarch64::mmu::USER_SPACE_PHYS
                } else {
                    path_ptr
                };
                let path_bytes = unsafe { core::slice::from_raw_parts(kaddr as *const u8, path_len as usize) };
                if let Ok(path_str) = core::str::from_utf8(path_bytes) {
                    if let Ok(binary) = crate::fs::vfs::read_all(path_str) {
                        match unsafe { crate::elf::load_aarch64(&binary) } {
                            Ok((_entry, _stack_top)) => {
                                let new_pid = CURRENT_PID.fetch_add(1, Ordering::Relaxed);
                                ctx.x[0] = new_pid;
                            }
                            Err(_) => {
                                ctx.x[0] = syscall::EINVAL;
                            }
                        }
                    } else {
                        ctx.x[0] = syscall::ENOENT;
                    }
                } else {
                    ctx.x[0] = syscall::EINVAL;
                }
            }
        }

        syscall::SYS_WAITPID => {
            ctx.x[0] = 0;
        }

        syscall::SYS_CHANNEL_CREATE => {
            match crate::ipc::create_channel() {
                Ok(id) => ctx.x[0] = id,
                Err(e) => ctx.x[0] = e,
            }
        }

        syscall::SYS_CHANNEL_SEND => {
            let chan = ctx.x[0];
            let ptr = ctx.x[1];
            let len = ctx.x[2];
            if let Err(e) = syscall::validate_user_ptr(ptr, len) {
                ctx.x[0] = e;
            } else {
                let slice = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };
                match crate::ipc::send_message(chan, CURRENT_PID.load(Ordering::Relaxed), slice) {
                    Ok(n) => ctx.x[0] = n as u64,
                    Err(e) => ctx.x[0] = e,
                }
            }
        }

        syscall::SYS_CHANNEL_RECV => {
            let chan = ctx.x[0];
            let ptr = ctx.x[1];
            let max_len = ctx.x[2];
            if let Err(e) = syscall::validate_user_ptr(ptr, max_len) {
                ctx.x[0] = e;
            } else {
                let slice = unsafe { core::slice::from_raw_parts_mut(ptr as *mut u8, max_len as usize) };
                match crate::ipc::recv_message(chan, CURRENT_PID.load(Ordering::Relaxed), slice) {
                    Ok(n) => ctx.x[0] = n as u64,
                    Err(e) => ctx.x[0] = e,
                }
            }
        }

        syscall::SYS_FS_LIST => {
            let (path_ptr, path_len, out_ptr, out_len) = (ctx.x[0], ctx.x[1], ctx.x[2], ctx.x[3]);
            ctx.x[0] = match read_user_path(path_ptr, path_len) {
                Ok(path) => match crate::fs::vfs::list_dir(path) {
                    Ok(entries) => {
                        let mut listing = alloc::string::String::new();
                        for entry in &entries {
                            let kind = if entry.is_dir { "DIR " } else { "FILE" };
                            let _ = core::fmt::write(
                                &mut listing,
                                format_args!("{kind}  {:>8}  {}\n", entry.size, entry.name),
                            );
                        }
                        copy_out(out_ptr, out_len, listing.as_bytes())
                    }
                    Err(_) => syscall::ENOENT,
                },
                Err(e) => e,
            };
        }

        syscall::SYS_FS_READFILE => {
            let (path_ptr, path_len, out_ptr, out_len) = (ctx.x[0], ctx.x[1], ctx.x[2], ctx.x[3]);
            ctx.x[0] = match read_user_path(path_ptr, path_len) {
                Ok(path) => match crate::fs::vfs::read_all(path) {
                    Ok(bytes) => copy_out(out_ptr, out_len, &bytes),
                    Err(_) => syscall::ENOENT,
                },
                Err(e) => e,
            };
        }

        syscall::SYS_SYSINFO => {
            let text = alloc::format!(
                "arch: aarch64\nheap_used: {} B\nuptime_ticks: {}\n",
                crate::allocator::used(),
                crate::arch::aarch64::timer::ticks(),
            );
            ctx.x[0] = copy_out(ctx.x[0], ctx.x[1], text.as_bytes());
        }

        syscall::SYS_LAUNCH_DESKTOP => {
            if crate::ui::COMPOSITOR.lock().is_some() {
                crate::ui::compositor::set_desktop_active(true);
                if let Some(c) = crate::console::CONSOLE.lock().as_mut() {
                    crate::ui::render_desktop(
                        c.framebuffer_mut(),
                        crate::allocator::used(),
                        crate::AARCH64_HEAP_SIZE,
                        crate::arch::aarch64::timer::ticks(),
                    );
                }
                ctx.x[0] = 0;
            } else {
                ctx.x[0] = syscall::ENOSYS;
            }
        }


        _ => {
            println!("  unknown syscall: {}", syscall_no);
            ctx.x[0] = syscall::EINVAL;
        }
    }
}

/// Reads a validated user-space path string out of the small AArch64 user
/// window, bounded to a sane length. Shared by `SYS_FS_LIST`/`SYS_FS_READFILE`.
fn read_user_path(ptr: u64, len: u64) -> Result<&'static str, u64> {
    syscall::validate_user_ptr(ptr, len)?;
    if len == 0 || len > 256 {
        return Err(syscall::EINVAL);
    }
    let kaddr = translate(ptr);
    let bytes = unsafe { core::slice::from_raw_parts(kaddr as *const u8, len as usize) };
    core::str::from_utf8(bytes).map_err(|_| syscall::EINVAL)
}

/// Copies `bytes` into a validated user-space output buffer inside the
/// AArch64 user window, truncating to its capacity.
fn copy_out(out_ptr: u64, out_len: u64, bytes: &[u8]) -> u64 {
    if let Err(e) = syscall::validate_user_ptr(out_ptr, out_len) {
        return e;
    }
    let kaddr = translate(out_ptr);
    let out = unsafe { core::slice::from_raw_parts_mut(kaddr as *mut u8, out_len as usize) };
    let n = bytes.len().min(out.len());
    out[..n].copy_from_slice(&bytes[..n]);
    n as u64
}

/// Transitions the CPU to EL0 (userspace) and executes `entry_point`.
///
/// Saves kernel state so `SYS_EXIT` can return here.
pub unsafe fn enter_user_mode(entry_point: u64, stack_top: u64, arg0: u64, arg1: u64) -> u64 {
    ArmSyscall::init();

    let exit_code: u64;

    core::arch::asm!(
        // 1. Save callee-saved registers on kernel stack
        "stp x19, x20, [sp, #-16]!",
        "stp x21, x22, [sp, #-16]!",
        "stp x23, x24, [sp, #-16]!",
        "stp x25, x26, [sp, #-16]!",
        "stp x27, x28, [sp, #-16]!",
        "stp x29, x30, [sp, #-16]!",

        // 2. Save kernel SP and return address
        "mov x2, sp",
        "adr x3, 2f",
        "str x2, [{k_sp}]",
        "str x3, [{k_lr}]",

        // 3. Configure EL0 state
        "msr sp_el0, {user_stack}",
        "msr elr_el1, {user_entry}",
        // Pass arguments to user function
        "mov x0, {arg0}",
        "mov x1, {arg1}",
        // SPSR_EL1: Mode EL0t (0b0000), mask DAIF (0x3c0) during standalone test to prevent spurious interrupts
        "mov x2, #0x3c0",
        "msr spsr_el1, x2",
        "isb",

        // 4. Drop to EL0!
        "eret",

        // 5. Landing point when SYS_EXIT returns to kernel
        "2:",
        "ldp x29, x30, [sp], #16",
        "ldp x27, x28, [sp], #16",
        "ldp x25, x26, [sp], #16",
        "ldp x23, x24, [sp], #16",
        "ldp x21, x22, [sp], #16",
        "ldp x19, x20, [sp], #16",

        k_sp = in(reg) &KERNEL_SP as *const _ as u64,
        k_lr = in(reg) &KERNEL_LR as *const _ as u64,
        user_stack = in(reg) stack_top,
        user_entry = in(reg) entry_point,
        arg0 = in(reg) arg0,
        arg1 = in(reg) arg1,
        lateout("x0") exit_code,
        out("x1") _,
        out("x2") _,
        out("x3") _,
        options(nostack)
    );

    exit_code
}

/// Restores the kernel execution context from `SYS_EXIT`.
pub unsafe fn return_to_kernel(exit_code: u64) -> ! {
    let k_sp = KERNEL_SP.load(Ordering::Relaxed);
    let k_lr = KERNEL_LR.load(Ordering::Relaxed);

    core::arch::asm!(
        "mov sp, {sp}",
        "mov x0, {code}",
        "br {lr}",
        sp = in(reg) k_sp,
        code = in(reg) exit_code,
        lr = in(reg) k_lr,
        options(noreturn)
    );
}

/// Prepares user space at `USER_SPACE_VIRT` with test user code and message.
pub unsafe fn setup_test_userspace() -> (u64, u64, u64, u64) {
    let virt_entry = crate::arch::aarch64::mmu::USER_SPACE_VIRT;
    let virt_msg = virt_entry + 0x1000;
    let stack_top = virt_entry + 0x0010_0000;

    let phys_entry = crate::arch::aarch64::mmu::USER_SPACE_PHYS;
    let phys_msg = phys_entry + 0x1000;

    let msg = b"  userspace    antOS init running in EL0 via svc #0!\n";
    // Write through EL1 physical alias to avoid PAN (Privileged Access Never) faults
    core::ptr::copy_nonoverlapping(msg.as_ptr(), phys_msg as *mut u8, msg.len());

    let src = user_test_entry as *const u8;
    core::ptr::copy_nonoverlapping(src, phys_entry as *mut u8, 64);

    // Clean data cache to Point of Unification and invalidate instruction caches
    core::arch::asm!(
        "dc cvau, {dst}",
        "dsb ish",
        "ic iallu",
        "dsb ish",
        "isb",
        dst = in(reg) phys_entry,
        options(nostack)
    );

    (virt_entry, stack_top, virt_msg, msg.len() as u64)
}

/// Standalone assembly routine executed by EL0.
///
/// Uses the formal ABI: SYS_WRITE=2, SYS_EXIT=1.
#[no_mangle]
#[link_section = ".text.user"]
pub unsafe extern "C" fn user_test_entry() {
    core::arch::asm!(
        "mov x8, #2", // SYS_WRITE: x0 = ptr, x1 = len
        "svc #0",

        "mov x8, #1", // SYS_EXIT
        "mov x0, #42", // exit code
        "svc #0",
        options(noreturn)
    );
}
