//! Ring 3 and system calls (T22.5 — formal ABI).
//!
//! Until now all code ran in ring 0, with permission to do anything.  A
//! userspace program runs in **ring 3**: it cannot read kernel memory, talk to
//! I/O ports, or execute privileged instructions.  To request anything it must
//! cross the boundary through the only gate we leave open: the `syscall`
//! instruction.
//!
//! ## Why `syscall` instead of an interrupt
//!
//! You could enter the kernel with `int 0x80`, and that is how it used to be
//! done.  But an interrupt consults the IDT, switches stacks via the TSS and
//! pushes five values: hundreds of cycles.  `syscall` does none of that — it
//! saves the return RIP in RCX and RFLAGS in R11, loads CS and SS from a
//! configuration register, and jumps.  **It does not switch stacks**: the
//! kernel has to do that by hand, and it is the first thing our entry point
//! does.

use crate::gdt;
use crate::memory::{FrameAllocator, Mapper, PAGE_SIZE, PRESENT, USER, WRITABLE};
use crate::syscall;
use crate::{print, println};
use core::sync::atomic::{AtomicU64, Ordering};

// Model-specific registers for syscall/sysret configuration.
const IA32_EFER: u32 = 0xC000_0080;
const IA32_STAR: u32 = 0xC000_0081;
const IA32_LSTAR: u32 = 0xC000_0082;
const IA32_FMASK: u32 = 0xC000_0084;

/// Where the user stack starts, and how many pages it occupies.
const USER_STACK_TOP: u64 = 0x7000_0000;
const USER_STACK_PAGES: u64 = 4;

/// Base virtual address for anonymous mmap allocations.
const MMAP_BASE: u64 = 0x1000_0000;

/// Kernel stack for the syscall entry point.
static SYSCALL_STACK_TOP: AtomicU64 = AtomicU64::new(0);
/// Saved user RSP while the syscall handler runs.
static USER_RSP: AtomicU64 = AtomicU64::new(0);

// Kernel state for returning when the program exits.
static KERNEL_RSP: AtomicU64 = AtomicU64::new(0);
static KERNEL_RIP: AtomicU64 = AtomicU64::new(0);
static EXIT_CODE: AtomicU64 = AtomicU64::new(0);

/// Next virtual address available for anonymous mmap.
static MMAP_NEXT: AtomicU64 = AtomicU64::new(MMAP_BASE);

/// Current process ID (always 1 for init).
static CURRENT_PID: AtomicU64 = AtomicU64::new(1);

pub fn init() {
    SYSCALL_STACK_TOP.store(gdt::syscall_stack_top(), Ordering::Relaxed);

    // SAFETY: MSRs are the documented Intel registers and values derive from
    // our own GDT.
    unsafe {
        // Bit 0 of EFER (SCE): without it, `syscall` is an invalid opcode.
        write_msr(IA32_EFER, read_msr(IA32_EFER) | 1);

        // STAR stores selectors: bits 47:32 are used by `syscall` to enter
        // the kernel, bits 63:48 by `sysret` to return.
        let star = ((gdt::SYSRET_BASE as u64) << 48) | ((gdt::CODE_SELECTOR as u64) << 32);
        write_msr(IA32_STAR, star);

        // Where `syscall` jumps to.
        write_msr(IA32_LSTAR, syscall_entry as *const () as u64);

        // Which flags are cleared on entry. IF is critical: without clearing
        // it an interrupt could fire while RSP still points to the user stack.
        write_msr(IA32_FMASK, (1 << 9) | (1 << 10));
    }
}

/// Allocates and maps the user program stack.
///
/// # Safety
/// Call only once; the range must not overlap kernel memory.
pub unsafe fn map_user_stack(
    mapper: &mut Mapper,
    allocator: &mut FrameAllocator,
) -> Result<u64, &'static str> {
    for page in 0..USER_STACK_PAGES {
        let address = USER_STACK_TOP - (page + 1) * PAGE_SIZE;
        let frame = allocator.allocate().ok_or("no frames for user stack")?;
        unsafe { mapper.map(address, frame, PRESENT | WRITABLE | USER, allocator)? };
    }
    Ok(USER_STACK_TOP)
}

/// Drops to ring 3 and does not return until the program exits.
///
/// # Safety
/// `entry` and `stack_top` must point to pages mapped with the user bit.
pub unsafe fn enter(entry: u64, stack_top: u64, mode: u64) -> u64 {
    unsafe {
        core::arch::asm!(
            // Save callee-saved registers: the user program may trash them.
            "push rbp",
            "push rbx",
            "push r12",
            "push r13",
            "push r14",
            "push r15",

            // Record where to return. The program does not return with `ret`:
            // it dies, and the kernel restores itself from here.
            "lea rax, [rip + 2f]",
            "mov [rip + {kernel_rip}], rax",
            "mov [rip + {kernel_rsp}], rsp",

            "mov rsp, {user_stack}",
            // `sysretq` jumps to RCX with RFLAGS from R11, in ring 3.
            "sysretq",

            // Landing pad for `return_to_kernel`.
            "2:",
            "pop r15",
            "pop r14",
            "pop r13",
            "pop r12",
            "pop rbx",
            "pop rbp",

            kernel_rip = sym KERNEL_RIP,
            kernel_rsp = sym KERNEL_RSP,
            user_stack = in(reg) stack_top,

            // RCX and R11 are not arguments: `sysret` interprets them as the
            // destination address and flags. 0x202 leaves IF active.
            inlateout("rcx") entry => _,
            inlateout("r11") 0x202u64 => _,
            // First argument to the user `_start`.
            inlateout("rdi") mode => _,

            lateout("rax") _,
            lateout("rdx") _,
            lateout("rsi") _,
            lateout("r8") _,
            lateout("r9") _,
            lateout("r10") _,
        );
    }
    EXIT_CODE.load(Ordering::Relaxed)
}

/// Kills the user process and returns control to the kernel.
pub fn return_to_kernel(code: u64) -> ! {
    EXIT_CODE.store(code, Ordering::Relaxed);
    // SAFETY: KERNEL_RSP and KERNEL_RIP were set by `enter` before jumping.
    unsafe {
        core::arch::asm!(
            "mov rsp, [rip + {kernel_rsp}]",
            "jmp qword ptr [rip + {kernel_rip}]",
            kernel_rsp = sym KERNEL_RSP,
            kernel_rip = sym KERNEL_RIP,
            options(noreturn)
        )
    }
}

/// The `syscall` entry point, in pure assembly.
///
/// Must be `naked` because the very first thing — switching stacks — is
/// incompatible with any compiler-generated prologue.
#[unsafe(naked)]
unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        // Save user stack and switch to the kernel's.
        "mov [rip + {user_rsp}], rsp",
        "mov rsp, [rip + {syscall_stack}]",

        // RCX and R11 carry the return address; `sysretq` needs them intact.
        "push rcx",
        "push r11",

        // Translate our ABI (RAX = number, RDI/RSI/RDX = args) to the C ABI.
        "mov rcx, rdx",
        "mov rdx, rsi",
        "mov rsi, rdi",
        "mov rdi, rax",
        "call {handler}",
        // Result comes back in RAX, which is where the user expects it.

        "pop r11",
        "pop rcx",
        "mov rsp, [rip + {user_rsp}]",
        "sysretq",

        user_rsp = sym USER_RSP,
        syscall_stack = sym SYSCALL_STACK_TOP,
        handler = sym handle_syscall,
    )
}

extern "C" fn handle_syscall(number: u64, arg1: u64, arg2: u64, _arg3: u64) -> u64 {
    match number {
        syscall::SYS_EXIT => return_to_kernel(arg1),
        syscall::SYS_WRITE => sys_write(arg1, arg2),
        syscall::SYS_READ => sys_read(arg1, arg2),
        syscall::SYS_YIELD => sys_yield(),
        syscall::SYS_GETPID => sys_getpid(),
        syscall::SYS_MMAP => sys_mmap(arg1),
        _ => {
            println!("  unknown syscall: {number}");
            u64::MAX
        }
    }
}

fn sys_write(pointer: u64, length: u64) -> u64 {
    // Every pointer from userspace is hostile until validated.
    if !syscall::validate_user_buffer(pointer, length) {
        return u64::MAX;
    }

    // LIMITATION: we check the RANGE, not that the pages are mapped.
    let bytes = unsafe { core::slice::from_raw_parts(pointer as *const u8, length as usize) };

    match core::str::from_utf8(bytes) {
        Ok(text) => {
            print!("     [user] {text}");
            length
        }
        Err(_) => u64::MAX,
    }
}

fn sys_read(pointer: u64, max_length: u64) -> u64 {
    if !syscall::validate_user_buffer(pointer, max_length) {
        return u64::MAX;
    }

    // Non-blocking: return 0 if no input is available.
    // TODO: wire to the keyboard ring buffer when the keyboard task is active.
    0
}

fn sys_yield() -> u64 {
    // Voluntarily yield to the scheduler by halting until the next interrupt.
    unsafe {
        core::arch::asm!("sti; hlt", options(nomem, nostack));
    }
    0
}

fn sys_getpid() -> u64 {
    CURRENT_PID.load(Ordering::Relaxed)
}

fn sys_mmap(requested_size: u64) -> u64 {
    if requested_size == 0 || requested_size > 16 * PAGE_SIZE {
        return u64::MAX;
    }

    // Round up to page boundary.
    let pages = (requested_size + PAGE_SIZE - 1) / PAGE_SIZE;
    let size = pages * PAGE_SIZE;

    let addr = MMAP_NEXT.fetch_add(size, Ordering::Relaxed);

    // We cannot actually map pages here without the mapper/allocator, which
    // are not accessible from the syscall handler. For now, report the
    // address that WOULD be mapped. The kernel main will pre-map a pool.
    // TODO: pass mapper/allocator through a global or per-CPU structure.
    addr
}

/// # Safety
/// Reading a non-existent MSR causes a general protection fault.
unsafe fn read_msr(msr: u32) -> u64 {
    let (high, low): (u32, u32);
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
    ((high as u64) << 32) | low as u64
}

/// # Safety
/// Writing an MSR changes CPU behaviour. An invalid value can crash the machine.
unsafe fn write_msr(msr: u32, value: u64) {
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") value as u32,
            in("edx") (value >> 32) as u32,
            options(nomem, nostack, preserves_flags)
        );
    }
}
