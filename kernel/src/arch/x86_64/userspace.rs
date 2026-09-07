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

/// Allocates and maps a user program stack at a custom top virtual address.
pub unsafe fn map_user_stack_at(
    stack_top: u64,
    mapper: &mut Mapper,
    allocator: &mut FrameAllocator,
) -> Result<u64, &'static str> {
    for page in 0..USER_STACK_PAGES {
        let address = stack_top - (page + 1) * PAGE_SIZE;
        let frame = allocator.allocate().ok_or("no frames for user stack")?;
        unsafe { mapper.map(address, frame, PRESENT | WRITABLE | USER, allocator)? };
    }
    Ok(stack_top)
}

/// Allocates and maps the default user program stack.
///
/// # Safety
/// Call only once; the range must not overlap kernel memory.
pub unsafe fn map_user_stack(
    mapper: &mut Mapper,
    allocator: &mut FrameAllocator,
) -> Result<u64, &'static str> {
    unsafe { map_user_stack_at(USER_STACK_TOP, mapper, allocator) }
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

        // Translate our ABI (RAX = number, RDI/RSI/RDX/R10 = args) to the C
        // ABI (RDI/RSI/RDX/RCX/R8). R10 stands in for the 4th argument
        // because RCX is already spoken for by `syscall` itself — the same
        // trick Linux uses.
        "mov r8, r10",
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

extern "C" fn handle_syscall(number: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64 {
    match number {
        syscall::SYS_EXIT => {
            if crate::task::scheduler::is_active() {
                crate::task::scheduler::exit_current_syscall(arg1);
                loop {
                    unsafe { core::arch::asm!("sti; hlt", options(nomem, nostack)) };
                }
            }
            return_to_kernel(arg1)
        }
        syscall::SYS_WRITE => sys_write(arg1, arg2),
        syscall::SYS_READ => sys_read(arg1, arg2),
        syscall::SYS_YIELD => sys_yield(),
        syscall::SYS_GETPID => sys_getpid(),
        syscall::SYS_MMAP => sys_mmap(arg1),
        syscall::SYS_MUNMAP => sys_munmap(arg1, arg2),
        syscall::SYS_SPAWN => sys_spawn(arg1, arg2, arg3),
        syscall::SYS_WAITPID => sys_waitpid(arg1),
        syscall::SYS_CHANNEL_CREATE => sys_channel_create(),
        syscall::SYS_CHANNEL_SEND => sys_channel_send(arg1, arg2, arg3),
        syscall::SYS_CHANNEL_RECV => sys_channel_recv(arg1, arg2, arg3),
        syscall::SYS_FS_LIST => sys_fs_list(arg1, arg2, arg3, arg4),
        syscall::SYS_FS_READFILE => sys_fs_readfile(arg1, arg2, arg3, arg4),
        syscall::SYS_SYSINFO => sys_sysinfo(arg1, arg2),
        syscall::SYS_LAUNCH_DESKTOP => sys_launch_desktop(),
        _ => {
            println!("  unknown syscall: {number}");
            syscall::EINVAL
        }
    }
}

fn sys_write(pointer: u64, length: u64) -> u64 {
    if let Err(e) = syscall::validate_user_ptr(pointer, length) {
        return e;
    }
    if length > syscall::MAX_BUFFER_SIZE {
        return syscall::EINVAL;
    }

    let bytes = unsafe { core::slice::from_raw_parts(pointer as *const u8, length as usize) };
    match core::str::from_utf8(bytes) {
        Ok(text) => {
            print!("     [user] {text}");
            length
        }
        Err(_) => syscall::EINVAL,
    }
}

fn sys_read(pointer: u64, max_length: u64) -> u64 {
    if let Err(e) = syscall::validate_user_ptr(pointer, max_length) {
        return e;
    }
    let len = (max_length as usize).min(syscall::MAX_BUFFER_SIZE as usize);
    let buf = unsafe { core::slice::from_raw_parts_mut(pointer as *mut u8, len) };
    crate::input::drain_ascii(buf) as u64
}

fn sys_yield() -> u64 {
    if crate::task::scheduler::is_active() {
        crate::task::scheduler::expire_current_quantum();
    }
    unsafe {
        core::arch::asm!("sti; hlt", options(nomem, nostack));
    }
    0
}

fn sys_getpid() -> u64 {
    CURRENT_PID.load(Ordering::Relaxed)
}

fn sys_mmap(requested_size: u64) -> u64 {
    if requested_size == 0 || requested_size > 256 * PAGE_SIZE {
        return syscall::EINVAL;
    }

    let pages = (requested_size + PAGE_SIZE - 1) / PAGE_SIZE;
    let size = pages * PAGE_SIZE;
    let addr = MMAP_NEXT.fetch_add(size, Ordering::Relaxed);

    match crate::memory::mmap_user_pages(addr, pages as usize) {
        Ok(mapped_addr) => mapped_addr,
        Err(e) => e,
    }
}

fn sys_munmap(addr: u64, size: u64) -> u64 {
    if addr % PAGE_SIZE != 0 || size == 0 {
        return syscall::EINVAL;
    }
    if let Err(e) = syscall::validate_user_ptr(addr, size) {
        return e;
    }
    let pages = (size + PAGE_SIZE - 1) / PAGE_SIZE;
    match crate::memory::munmap_user_pages(addr, pages as usize) {
        Ok(()) => 0,
        Err(e) => e,
    }
}

fn sys_spawn(path_ptr: u64, path_len: u64, mode: u64) -> u64 {
    if let Err(e) = syscall::validate_user_ptr(path_ptr, path_len) {
        return e;
    }
    if path_len > 256 {
        return syscall::EINVAL;
    }

    let path_bytes = unsafe { core::slice::from_raw_parts(path_ptr as *const u8, path_len as usize) };
    let Ok(path_str) = core::str::from_utf8(path_bytes) else {
        return syscall::EINVAL;
    };

    let Ok(binary) = crate::fs::vfs::read_all(path_str) else {
        return syscall::ENOENT;
    };

    let Some(entry) = crate::elf::parse_entry(&binary) else {
        return syscall::EINVAL;
    };

    let cr3_root: u64;
    unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3_root, options(nomem, nostack)) };

    // Allocate stack space for new process (64 KiB = 16 pages)
    let stack_base = MMAP_NEXT.fetch_add(16 * PAGE_SIZE, Ordering::Relaxed);
    if crate::memory::mmap_user_pages(stack_base, 16).is_err() {
        return syscall::ENOMEM;
    }
    let stack_top = stack_base + 16 * PAGE_SIZE;

    let (pid, _tid) = crate::task::scheduler::spawn_process(
        path_str,
        cr3_root,
        entry,
        stack_top,
        0,
        true,
        mode,
    );
    pid
}

fn sys_waitpid(_target_pid: u64) -> u64 {
    if crate::task::scheduler::is_active() {
        sys_yield();
    }
    0
}

fn sys_channel_create() -> u64 {
    match crate::ipc::create_channel() {
        Ok(id) => id,
        Err(e) => e,
    }
}

fn sys_channel_send(channel_id: u64, buf_ptr: u64, len: u64) -> u64 {
    if let Err(e) = syscall::validate_user_ptr(buf_ptr, len) {
        return e;
    }
    let current_pid = sys_getpid();
    let slice = unsafe { core::slice::from_raw_parts(buf_ptr as *const u8, len as usize) };
    match crate::ipc::send_message(channel_id, current_pid, slice) {
        Ok(sent_bytes) => sent_bytes as u64,
        Err(e) => e,
    }
}

fn sys_channel_recv(channel_id: u64, buf_ptr: u64, max_len: u64) -> u64 {
    if let Err(e) = syscall::validate_user_ptr(buf_ptr, max_len) {
        return e;
    }
    let current_pid = sys_getpid();
    let slice = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, max_len as usize) };
    match crate::ipc::recv_message(channel_id, current_pid, slice) {
        Ok(recv_bytes) => recv_bytes as u64,
        Err(e) => e,
    }
}

/// Reads a validated user-space path string, bounded to a sane length.
fn read_user_path(ptr: u64, len: u64) -> Result<&'static str, u64> {
    syscall::validate_user_ptr(ptr, len)?;
    if len == 0 || len > 256 {
        return Err(syscall::EINVAL);
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };
    core::str::from_utf8(bytes).map_err(|_| syscall::EINVAL)
}

/// Copies `text` into a validated user-space output buffer, truncating to
/// its capacity. Shared by `SYS_FS_LIST`, `SYS_FS_READFILE` and `SYS_SYSINFO`.
fn copy_out(out_ptr: u64, out_len: u64, bytes: &[u8]) -> u64 {
    if let Err(e) = syscall::validate_user_ptr(out_ptr, out_len) {
        return e;
    }
    let out = unsafe { core::slice::from_raw_parts_mut(out_ptr as *mut u8, out_len as usize) };
    let n = bytes.len().min(out.len());
    out[..n].copy_from_slice(&bytes[..n]);
    n as u64
}

/// `SYS_FS_LIST` — lists VFS directory entries as `"KIND  SIZE  name\n"` lines (T26.5).
fn sys_fs_list(path_ptr: u64, path_len: u64, out_ptr: u64, out_len: u64) -> u64 {
    let path = match read_user_path(path_ptr, path_len) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let entries = match crate::fs::vfs::list_dir(path) {
        Ok(e) => e,
        Err(_) => return syscall::ENOENT,
    };
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

/// `SYS_FS_READFILE` — reads a whole file from the VFS into a user buffer (T26.5).
fn sys_fs_readfile(path_ptr: u64, path_len: u64, out_ptr: u64, out_len: u64) -> u64 {
    let path = match read_user_path(path_ptr, path_len) {
        Ok(p) => p,
        Err(e) => return e,
    };
    match crate::fs::vfs::read_all(path) {
        Ok(bytes) => copy_out(out_ptr, out_len, &bytes),
        Err(_) => syscall::ENOENT,
    }
}

/// `SYS_SYSINFO` — reports architecture, heap usage and uptime as text (T26.5).
fn sys_sysinfo(out_ptr: u64, out_len: u64) -> u64 {
    let text = alloc::format!(
        "arch: x86_64\nheap_used: {} B\nuptime_ticks: {}\n",
        crate::allocator::used(),
        crate::task::timer::ticks(),
    );
    copy_out(out_ptr, out_len, text.as_bytes())
}

/// `SYS_LAUNCH_DESKTOP` — renders one frame of the native Desktop Shell
/// compositor (T26.2) onto the boot framebuffer, initializing it on first use
/// (T26.5 bridge between the userspace shell and the kernel-space compositor).
fn sys_launch_desktop() -> u64 {
    use core::sync::atomic::{AtomicBool, Ordering};
    static COMPOSITOR_READY: AtomicBool = AtomicBool::new(false);

    if !COMPOSITOR_READY.swap(true, Ordering::Relaxed) {
        crate::ui::init("x86_64");
    }
    if let Some(console) = crate::console::CONSOLE.lock().as_mut() {
        crate::ui::render_desktop(
            console.framebuffer_mut(),
            crate::allocator::used(),
            crate::memory::HEAP_SIZE,
            crate::task::timer::ticks(),
        );
        0
    } else {
        syscall::ENOSYS
    }
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
