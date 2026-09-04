//! Process Control Block (PCB), Thread Control Block (TCB) and CPU Context (T23.2).
//!
//! Provides the core data structures for preemptive process and thread management
//! in the antOS kernel, supporting both x86_64 and AArch64 architectures.

use alloc::vec::Vec;

/// Execution lifecycle states of a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Process is ready to run and has one or more ready threads.
    Ready,
    /// Process is currently executing on a CPU core.
    Running,
    /// Process is waiting for an event, IPC, or I/O.
    Blocked,
    /// Process has terminated and is awaiting resource cleanup / parent reap.
    Terminated,
}

/// Execution lifecycle states of a thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    /// Thread is waiting in the ready queue for a CPU time slice.
    Ready,
    /// Thread is actively executing instructions.
    Running,
    /// Thread is blocked on a timer, waker, or synchronization lock.
    Blocked,
    /// Thread has exited and will not be scheduled again.
    Terminated,
}

/// Hardware register state for context saving, preemption, and restoring.
#[cfg(target_arch = "x86_64")]
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuContext {
    // Callee-saved and caller-saved registers pushed by the timer ISR wrapper
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rax: u64,
    // Interrupt stack frame pushed by hardware on interrupt entry
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

#[cfg(target_arch = "x86_64")]
const _: () = {
    assert!(core::mem::size_of::<CpuContext>() == 160);
};

#[cfg(target_arch = "x86_64")]
impl CpuContext {
    /// Creates a fresh context initialized for user mode (Ring 3).
    ///
    /// - CS: User code segment selector (`0x33` in antOS GDT: index 6, RPL 3).
    /// - SS: User data segment selector (`0x2B` in antOS GDT: index 5, RPL 3).
    /// - RFLAGS: `0x202` (bit 1 reserved + bit 9 IF interrupt flag enabled).
    pub fn new_user(entry: u64, user_sp: u64, arg: u64) -> Self {
        const USER_CS: u64 = crate::gdt::USER_CODE_SELECTOR as u64;
        const USER_SS: u64 = crate::gdt::USER_DATA_SELECTOR as u64;
        const RFLAGS_IF: u64 = 0x202;

        Self {
            rdi: arg,
            rip: entry,
            cs: USER_CS,
            rflags: RFLAGS_IF,
            rsp: user_sp,
            ss: USER_SS,
            ..Default::default()
        }
    }

    /// Creates a fresh context initialized for kernel mode (Ring 0).
    pub fn new_kernel(entry: u64, kernel_sp: u64, arg: u64) -> Self {
        const KERNEL_CS: u64 = crate::gdt::CODE_SELECTOR as u64;
        const RFLAGS_IF: u64 = 0x202;

        Self {
            rdi: arg,
            rip: entry,
            cs: KERNEL_CS,
            rflags: RFLAGS_IF,
            rsp: kernel_sp,
            ss: 0,
            ..Default::default()
        }
    }

    #[inline]
    pub fn stack_pointer(&self) -> u64 {
        self.rsp
    }
}

/// Hardware register state for context saving on AArch64.
#[cfg(target_arch = "aarch64")]
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuContext {
    pub x: [u64; 30],
    pub x30: u64,
    pub elr_el1: u64,
    pub spsr_el1: u64,
}

#[cfg(target_arch = "aarch64")]
impl CpuContext {
    #[inline]
    pub fn stack_pointer(&self) -> u64 {
        self.elr_el1
    }

    /// Creates a fresh user mode context for EL0.
    pub fn new_user(entry: u64, user_sp: u64, arg: u64) -> Self {
        let mut ctx = Self::default();
        ctx.x[0] = arg;
        unsafe {
            core::arch::asm!("msr sp_el0, {}", in(reg) user_sp, options(nomem, nostack));
        }
        ctx.elr_el1 = entry;
        ctx.spsr_el1 = 0; // EL0t with unmasked interrupts
        ctx
    }

    /// Creates a fresh kernel mode context for EL1.
    pub fn new_kernel(entry: u64, kernel_sp: u64, arg: u64) -> Self {
        let mut ctx = Self::default();
        ctx.x[0] = arg;
        ctx.x30 = kernel_sp;
        ctx.elr_el1 = entry;
        ctx.spsr_el1 = 0b0101; // EL1h
        ctx
    }
}

/// Thread Control Block (TCB).
///
/// Manages the runtime state, stack pointers, priority, and saved registers
/// of a single schedulable execution entity.
#[derive(Debug, Clone)]
pub struct ThreadControlBlock {
    pub tid: u64,
    pub pid: u64,
    pub user_sp: u64,
    pub kernel_sp: u64,
    pub state: ThreadState,
    pub priority: u8,
    pub context: CpuContext,
    pub time_slice: u32,
    pub total_ticks: u64,
}

impl ThreadControlBlock {
    pub fn new(
        tid: u64,
        pid: u64,
        user_sp: u64,
        kernel_sp: u64,
        priority: u8,
        context: CpuContext,
        default_quantum: u32,
    ) -> Self {
        Self {
            tid,
            pid,
            user_sp,
            kernel_sp,
            state: ThreadState::Ready,
            priority,
            context,
            time_slice: default_quantum,
            total_ticks: 0,
        }
    }

    #[inline]
    pub fn is_ready(&self) -> bool {
        self.state == ThreadState::Ready
    }

    #[inline]
    pub fn mark_running(&mut self) {
        self.state = ThreadState::Running;
    }

    #[inline]
    pub fn mark_ready(&mut self) {
        if self.state != ThreadState::Terminated {
            self.state = ThreadState::Ready;
        }
    }

    #[inline]
    pub fn mark_blocked(&mut self) {
        if self.state != ThreadState::Terminated {
            self.state = ThreadState::Blocked;
        }
    }

    #[inline]
    pub fn mark_terminated(&mut self) {
        self.state = ThreadState::Terminated;
    }
}

/// Process Control Block (PCB).
///
/// Encapsulates process isolation: memory address space (page table root),
/// list of belonging threads, lifecycle status, and exit code.
#[derive(Debug, Clone)]
pub struct ProcessControlBlock {
    pub pid: u64,
    pub name: &'static str,
    /// Physical address of the page table root: CR3 in x86_64, TTBR0_EL1 in AArch64.
    pub page_table_root: u64,
    pub state: ProcessState,
    pub threads: Vec<u64>,
    pub exit_code: Option<u64>,
    pub is_user: bool,
}

impl ProcessControlBlock {
    pub fn new(
        pid: u64,
        name: &'static str,
        page_table_root: u64,
        is_user: bool,
    ) -> Self {
        Self {
            pid,
            name,
            page_table_root,
            state: ProcessState::Ready,
            threads: Vec::new(),
            exit_code: None,
            is_user,
        }
    }

    pub fn add_thread(&mut self, tid: u64) {
        if !self.threads.contains(&tid) {
            self.threads.push(tid);
        }
    }

    pub fn remove_thread(&mut self, tid: u64) {
        self.threads.retain(|&id| id != tid);
    }

    pub fn terminate(&mut self, code: u64) {
        self.state = ProcessState::Terminated;
        self.exit_code = Some(code);
    }
}
