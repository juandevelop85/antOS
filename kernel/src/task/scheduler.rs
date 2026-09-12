//! Round-Robin Preemptive Scheduler for antOS (T23.2).
//!
//! Manages process and thread scheduling, fair time-slice allocation,
//! interrupt-driven preemption hooks, and address space switching.

use crate::sync::SpinLock;
use crate::task::pcb::{CpuContext, ProcessControlBlock, ThreadControlBlock, ThreadState};
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

/// Default quantum in timer ticks (e.g. 2 ticks = 20 ms at 100 Hz).
pub const DEFAULT_QUANTUM_TICKS: u32 = 2;

/// Default stack size in bytes for kernel threads (16 KiB).
pub const KERNEL_STACK_SIZE: usize = 16384;

static NEXT_PID: AtomicU64 = AtomicU64::new(1);
static NEXT_TID: AtomicU64 = AtomicU64::new(1);

/// Statistics snapshot of the scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerMetrics {
    pub total_processes: usize,
    pub total_threads: usize,
    pub ready_threads: usize,
    pub context_switches: u64,
    pub recycled_stacks: usize,
}

pub struct Scheduler {
    processes: BTreeMap<u64, Arc<SpinLock<ProcessControlBlock>>>,
    threads: BTreeMap<u64, Arc<SpinLock<ThreadControlBlock>>>,
    ready_queue: VecDeque<u64>,
    current_tid: Option<u64>,
    active: bool,
    default_quantum: u32,
    context_switches: u64,
    /// Pool of reusable 16 KiB kernel stack pointers (stack tops).
    stack_pool: Vec<u64>,
    /// Set of stack tops allocated by this scheduler to safely track ownership.
    owned_stacks: BTreeSet<u64>,
    /// Stack pointer awaiting recycling once the next thread context is activated.
    pending_free_stack: Option<u64>,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            processes: BTreeMap::new(),
            threads: BTreeMap::new(),
            ready_queue: VecDeque::new(),
            current_tid: None,
            active: false,
            default_quantum: DEFAULT_QUANTUM_TICKS,
            context_switches: 0,
            stack_pool: Vec::new(),
            owned_stacks: BTreeSet::new(),
            pending_free_stack: None,
        }
    }

    /// Allocates or reuses a 16 KiB kernel stack. Returns the top address.
    pub fn alloc_kernel_stack(&mut self) -> u64 {
        if let Some(stack_top) = self.stack_pool.pop() {
            // Reutilizar una pila ya existente del pool
            stack_top
        } else {
            // Asignar un nuevo buffer de pila en el heap del kernel
            let stack_vec = alloc::vec![0u8; KERNEL_STACK_SIZE].leak();
            let stack_top = (stack_vec.as_ptr() as u64) + KERNEL_STACK_SIZE as u64;
            self.owned_stacks.insert(stack_top);
            stack_top
        }
    }

    /// Marks a kernel stack to be reclaimed if it belongs to our managed pool.
    pub fn defer_recycle_stack(&mut self, stack_top: u64) {
        if self.owned_stacks.contains(&stack_top) {
            self.pending_free_stack = Some(stack_top);
        }
    }

    /// Consumes any pending stack marked for recycling now that CPU context has switched.
    fn drain_pending_stack(&mut self) {
        if let Some(stack_top) = self.pending_free_stack.take() {
            self.stack_pool.push(stack_top);
        }
    }

    /// Spawns a new process and its primary thread.
    ///
    /// T31.10: clippy counts 8 (7 params + `&mut self`). The free function
    /// `scheduler::spawn_process` right below is the real public entry point
    /// and already sits at exactly 7 by itself; this method is only ever
    /// called from that one wrapper, so grouping these into a struct would
    /// add indirection at both call sites without a second, independent
    /// caller to justify it.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_process(
        &mut self,
        name: &'static str,
        page_table_root: u64,
        entry: u64,
        user_sp: u64,
        kernel_sp: u64,
        is_user: bool,
        arg: u64,
    ) -> (u64, u64) {
        let pid = NEXT_PID.fetch_add(1, Ordering::Relaxed);
        let tid = NEXT_TID.fetch_add(1, Ordering::Relaxed);

        let mut pcb = ProcessControlBlock::new(pid, name, page_table_root, is_user);
        pcb.add_thread(tid);

        let actual_kernel_sp = if kernel_sp != 0 {
            kernel_sp
        } else {
            self.alloc_kernel_stack()
        };

        let context = if is_user {
            CpuContext::new_user(entry, user_sp, arg)
        } else {
            CpuContext::new_kernel(entry, actual_kernel_sp, arg)
        };

        let tcb = ThreadControlBlock::new(
            tid,
            pid,
            user_sp,
            actual_kernel_sp,
            10, // default priority
            context,
            self.default_quantum,
        );

        self.processes.insert(pid, Arc::new(SpinLock::new(pcb)));
        self.threads.insert(tid, Arc::new(SpinLock::new(tcb)));
        self.ready_queue.push_back(tid);

        (pid, tid)
    }

    /// Spawns a new thread belonging to an existing process.
    pub fn spawn_thread(
        &mut self,
        pid: u64,
        entry: u64,
        user_sp: u64,
        kernel_sp: u64,
        is_user: bool,
        arg: u64,
    ) -> Option<u64> {
        let process_arc = self.processes.get(&pid)?.clone();
        let tid = NEXT_TID.fetch_add(1, Ordering::Relaxed);

        let actual_kernel_sp = if kernel_sp != 0 {
            kernel_sp
        } else {
            self.alloc_kernel_stack()
        };

        let context = if is_user {
            CpuContext::new_user(entry, user_sp, arg)
        } else {
            CpuContext::new_kernel(entry, actual_kernel_sp, arg)
        };

        let tcb = ThreadControlBlock::new(
            tid,
            pid,
            user_sp,
            actual_kernel_sp,
            10,
            context,
            self.default_quantum,
        );

        process_arc.lock().add_thread(tid);
        self.threads.insert(tid, Arc::new(SpinLock::new(tcb)));
        self.ready_queue.push_back(tid);

        Some(tid)
    }

    /// Hook invoked on every timer tick.
    ///
    /// Evaluates current thread quantum and performs transparent context switch
    /// by replacing the registers in `ctx` if a switch is necessary.
    pub fn on_tick(&mut self, ctx: &mut CpuContext) -> bool {
        if !self.active {
            return false;
        }

        if self.current_tid.is_none() {
            // First time scheduler takes control: capture current kernel context as TID 0
            let mut kernel_pcb = ProcessControlBlock::new(0, "kernel-main", 0, false);
            kernel_pcb.add_thread(0);
            let mut kernel_tcb = ThreadControlBlock::new(
                0,
                0,
                0,
                ctx.stack_pointer(),
                10,
                *ctx,
                self.default_quantum,
            );
            kernel_tcb.mark_ready();
            self.processes
                .insert(0, Arc::new(SpinLock::new(kernel_pcb)));
            self.threads.insert(0, Arc::new(SpinLock::new(kernel_tcb)));
            self.ready_queue.push_back(0);
        } else if let Some(curr_tid) = self.current_tid {
            if let Some(curr_thread_arc) = self.threads.get(&curr_tid).cloned() {
                let mut curr = curr_thread_arc.lock();
                curr.total_ticks = curr.total_ticks.saturating_add(1);

                if curr.state == ThreadState::Running {
                    if curr.time_slice > 1 && !self.ready_queue.is_empty() {
                        // Quantum still remaining
                        curr.time_slice -= 1;
                        return false;
                    }

                    // Quantum expired or yield requested:
                    if self.ready_queue.is_empty() {
                        // No other thread ready: reset quantum and keep running
                        curr.time_slice = self.default_quantum;
                        return false;
                    }

                    // Preempt current thread
                    curr.mark_ready();
                    curr.time_slice = self.default_quantum;
                    curr.context = *ctx;
                    self.ready_queue.push_back(curr_tid);
                } else if curr.state == ThreadState::Terminated {
                    // Current thread terminated, do not push back to ready_queue
                }
            }
        }

        // Pick next ready thread from the ready queue
        self.pick_and_switch_next(ctx)
    }

    /// Selects the next thread from the ready queue and performs register and page table switch.
    fn pick_and_switch_next(&mut self, ctx: &mut CpuContext) -> bool {
        while let Some(next_tid) = self.ready_queue.pop_front() {
            let Some(next_thread_arc) = self.threads.get(&next_tid).cloned() else {
                continue;
            };

            let mut next = next_thread_arc.lock();
            if next.state != ThreadState::Ready {
                continue;
            }

            let next_pid = next.pid;
            let current_pid = self
                .current_tid
                .and_then(|tid| self.threads.get(&tid).map(|t| t.lock().pid));

            if Some(next_pid) != current_pid {
                if let Some(pcb_arc) = self.processes.get(&next_pid).cloned() {
                    let pcb = pcb_arc.lock();
                    if pcb.page_table_root != 0 {
                        unsafe {
                            switch_page_table(pcb.page_table_root);
                        }
                    }
                }
            }

            // Update TSS privilege stack pointer (RSP0) so next ring 3 interrupt lands cleanly
            #[cfg(target_arch = "x86_64")]
            {
                crate::arch::x86_64::gdt::set_tss_rsp0(next.kernel_sp);
            }

            // Mark thread running and update context
            next.mark_running();
            next.time_slice = self.default_quantum;
            *ctx = next.context;

            self.current_tid = Some(next_tid);
            self.context_switches = self.context_switches.saturating_add(1);

            // CPU context switched: safe to return pending dead stack to the recycling pool
            self.drain_pending_stack();
            return true;
        }

        false
    }

    /// Sets the current thread's remaining quantum to 1 so the next timer tick triggers a switch.
    pub fn expire_current_quantum(&mut self) {
        if let Some(curr_tid) = self.current_tid {
            if let Some(th) = self.threads.get(&curr_tid) {
                th.lock().time_slice = 1;
            }
        }
    }

    /// Voluntarily yields the remainder of the current thread's time slice.
    pub fn yield_current(&mut self, ctx: &mut CpuContext) -> bool {
        if let Some(curr_tid) = self.current_tid {
            if let Some(curr_thread_arc) = self.threads.get(&curr_tid).cloned() {
                let mut curr = curr_thread_arc.lock();
                if curr.state == ThreadState::Running {
                    curr.time_slice = 0;
                }
            }
        }
        self.on_tick(ctx)
    }

    /// Terminates the current thread with an exit code.
    pub fn exit_current(&mut self, exit_code: u64, ctx: &mut CpuContext) -> bool {
        if let Some(curr_tid) = self.current_tid {
            let mut pid_to_check = None;
            let mut kernel_sp_to_free = None;
            if let Some(curr_thread_arc) = self.threads.get(&curr_tid).cloned() {
                let mut curr = curr_thread_arc.lock();
                curr.mark_terminated();
                pid_to_check = Some(curr.pid);
                kernel_sp_to_free = Some(curr.kernel_sp);
            }

            if let Some(ksp) = kernel_sp_to_free {
                self.defer_recycle_stack(ksp);
            }

            if let Some(pid) = pid_to_check {
                if let Some(pcb_arc) = self.processes.get(&pid).cloned() {
                    let mut pcb = pcb_arc.lock();
                    // Check if all threads in this process are terminated
                    let all_terminated = pcb.threads.iter().all(|&tid| {
                        self.threads
                            .get(&tid)
                            .map(|t| t.lock().state == ThreadState::Terminated)
                            .unwrap_or(true)
                    });
                    if all_terminated {
                        pcb.terminate(exit_code);
                    }
                }
            }
        }

        self.pick_and_switch_next(ctx)
    }

    pub fn get_metrics(&self) -> SchedulerMetrics {
        SchedulerMetrics {
            total_processes: self.processes.len(),
            total_threads: self.threads.len(),
            ready_threads: self.ready_queue.len(),
            context_switches: self.context_switches,
            recycled_stacks: self.stack_pool.len(),
        }
    }
}

/// Global system scheduler singleton.
pub static SCHEDULER: SpinLock<Scheduler> = SpinLock::new(Scheduler::new());

/// Switches the active CPU address space / page table root.
#[cfg(target_arch = "x86_64")]
#[inline]
pub unsafe fn switch_page_table(cr3_physical: u64) {
    if cr3_physical != 0 {
        let current_cr3: u64;
        core::arch::asm!("mov {}, cr3", out(reg) current_cr3, options(nomem, nostack));
        if current_cr3 != cr3_physical {
            core::arch::asm!("mov cr3, {}", in(reg) cr3_physical, options(nostack));
        }
    }
}

/// Switches the active CPU address space on AArch64.
#[cfg(target_arch = "aarch64")]
#[inline]
pub unsafe fn switch_page_table(ttbr0_physical: u64) {
    if ttbr0_physical != 0 {
        core::arch::asm!(
            "msr ttbr0_el1, {}",
            "isb",
            "tlbi vmalle1is",
            "dsb ish",
            "isb",
            in(reg) ttbr0_physical,
            options(nostack)
        );
    }
}

// ────────────────────────────────────────────── Public Kernel API

/// Activates the preemptive scheduler.
pub fn init() {
    let mut sched = SCHEDULER.lock();
    sched.active = true;
}

/// Returns true if the preemptive scheduler is active.
pub fn is_active() -> bool {
    SCHEDULER.lock().active
}

/// Spawns a new process and its initial thread into the scheduler.
pub fn spawn_process(
    name: &'static str,
    page_table_root: u64,
    entry: u64,
    user_sp: u64,
    kernel_sp: u64,
    is_user: bool,
    arg: u64,
) -> (u64, u64) {
    SCHEDULER.lock().spawn_process(
        name,
        page_table_root,
        entry,
        user_sp,
        kernel_sp,
        is_user,
        arg,
    )
}

/// Spawns a secondary thread inside an existing process.
pub fn spawn_thread(
    pid: u64,
    entry: u64,
    user_sp: u64,
    kernel_sp: u64,
    is_user: bool,
    arg: u64,
) -> Option<u64> {
    SCHEDULER
        .lock()
        .spawn_thread(pid, entry, user_sp, kernel_sp, is_user, arg)
}

/// Invoked from the timer interrupt service routine.
pub fn on_timer_tick(ctx: &mut CpuContext) -> bool {
    SCHEDULER.lock().on_tick(ctx)
}

/// Voluntarily yields the CPU time slice.
pub fn yield_current(ctx: &mut CpuContext) -> bool {
    SCHEDULER.lock().yield_current(ctx)
}

/// Marks the current thread's quantum as nearly expired.
pub fn expire_current_quantum() {
    SCHEDULER.lock().expire_current_quantum();
}

/// Terminates the current thread.
pub fn exit_current(exit_code: u64, ctx: &mut CpuContext) -> bool {
    SCHEDULER.lock().exit_current(exit_code, ctx)
}

/// Retrieves real-time metrics from the scheduler.
pub fn metrics() -> SchedulerMetrics {
    SCHEDULER.lock().get_metrics()
}

/// Restores a thread's context and jumps to it via iretq (x86_64) or eret (AArch64).
#[cfg(target_arch = "x86_64")]
pub unsafe fn resume_context(ctx: &CpuContext) -> ! {
    core::arch::asm!(
        "mov rsp, {ctx}",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rdi",
        "pop rsi",
        "pop rbp",
        "pop rbx",
        "pop rdx",
        "pop rcx",
        "pop rax",
        "iretq",
        ctx = in(reg) ctx,
        options(noreturn)
    );
}

#[cfg(target_arch = "aarch64")]
pub unsafe fn resume_context(ctx: &CpuContext) -> ! {
    core::arch::asm!(
        "mov sp, {ctx}",
        "ldr x3, [sp, #16 * 16]",
        "ldp x30, x2, [sp, #16 * 15]",
        "msr spsr_el1, x3",
        "msr elr_el1, x2",
        "ldp x28, x29, [sp, #16 * 14]",
        "ldp x26, x27, [sp, #16 * 13]",
        "ldp x24, x25, [sp, #16 * 12]",
        "ldp x22, x23, [sp, #16 * 11]",
        "ldp x20, x21, [sp, #16 * 10]",
        "ldp x18, x19, [sp, #16 * 9]",
        "ldp x16, x17, [sp, #16 * 8]",
        "ldp x14, x15, [sp, #16 * 7]",
        "ldp x12, x13, [sp, #16 * 6]",
        "ldp x10, x11, [sp, #16 * 5]",
        "ldp x8, x9, [sp, #16 * 4]",
        "ldp x6, x7, [sp, #16 * 3]",
        "ldp x4, x5, [sp, #16 * 2]",
        "ldp x2, x3, [sp, #16 * 1]",
        "ldp x0, x1, [sp, #0]",
        "add sp, sp, #272",
        "eret",
        ctx = in(reg) ctx,
        options(noreturn)
    );
}

/// Invoked when a userspace thread executes SYS_EXIT.
///
/// Marks the thread terminated, checks if the process has any surviving threads,
/// and immediately resumes the next ready thread via iretq/eret.
pub fn exit_current_syscall(exit_code: u64) -> bool {
    let mut sched = SCHEDULER.lock();
    if let Some(curr_tid) = sched.current_tid {
        let mut pid_to_check = None;
        let mut kernel_sp_to_free = None;
        if let Some(curr_thread_arc) = sched.threads.get(&curr_tid).cloned() {
            let mut curr = curr_thread_arc.lock();
            curr.mark_terminated();
            pid_to_check = Some(curr.pid);
            kernel_sp_to_free = Some(curr.kernel_sp);
        }

        if let Some(ksp) = kernel_sp_to_free {
            sched.defer_recycle_stack(ksp);
        }

        if let Some(pid) = pid_to_check {
            if let Some(pcb_arc) = sched.processes.get(&pid).cloned() {
                let mut pcb = pcb_arc.lock();
                let all_terminated = pcb.threads.iter().all(|&tid| {
                    sched
                        .threads
                        .get(&tid)
                        .map(|t| t.lock().state == ThreadState::Terminated)
                        .unwrap_or(true)
                });
                if all_terminated {
                    pcb.terminate(exit_code);
                }
            }
        }
    }

    let mut next_context = None;
    while let Some(next_tid) = sched.ready_queue.pop_front() {
        let Some(next_thread_arc) = sched.threads.get(&next_tid).cloned() else {
            continue;
        };

        let mut next = next_thread_arc.lock();
        if next.state != ThreadState::Ready {
            continue;
        }

        let next_pid = next.pid;
        if let Some(pcb_arc) = sched.processes.get(&next_pid).cloned() {
            let pcb = pcb_arc.lock();
            if pcb.page_table_root != 0 {
                unsafe {
                    switch_page_table(pcb.page_table_root);
                }
            }
        }

        #[cfg(target_arch = "x86_64")]
        {
            crate::arch::x86_64::gdt::set_tss_rsp0(next.kernel_sp);
        }

        next.mark_running();
        next.time_slice = sched.default_quantum;
        sched.current_tid = Some(next_tid);
        sched.context_switches = sched.context_switches.saturating_add(1);
        sched.drain_pending_stack();
        next_context = Some(next.context);
        break;
    }
    drop(sched);

    if let Some(ctx) = next_context {
        unsafe {
            resume_context(&ctx);
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_scheduler_stack_pool_allocation_and_recycling() {
        let mut scheduler = Scheduler::new();
        assert_eq!(scheduler.stack_pool.len(), 0);

        // Pedir dos pilas del pool
        let s1 = scheduler.alloc_kernel_stack();
        let s2 = scheduler.alloc_kernel_stack();
        assert_ne!(s1, s2);
        assert!(scheduler.owned_stacks.contains(&s1));
        assert!(scheduler.owned_stacks.contains(&s2));
        assert_eq!(scheduler.stack_pool.len(), 0);

        // Marcar s1 para reciclaje diferido
        scheduler.defer_recycle_stack(s1);
        assert_eq!(scheduler.stack_pool.len(), 0);

        // Simular switch consumiendo la pila pendiente
        scheduler.drain_pending_stack();
        assert_eq!(scheduler.stack_pool.len(), 1);

        // La siguiente petición debe reutilizar s1 sin asignar nueva memoria
        let s3 = scheduler.alloc_kernel_stack();
        assert_eq!(s3, s1);
        assert_eq!(scheduler.stack_pool.len(), 0);
    }

    #[test_case]
    fn test_scheduler_thread_lifecycle_recycles_stack() {
        let mut scheduler = Scheduler::new();
        scheduler.active = true;

        // Spawn proceso y primary thread
        let (pid, _t1) = scheduler.spawn_process("test-proc", 0, 0x1000, 0, 0, false, 0);

        // Spawn thread secundario
        let t2 = scheduler.spawn_thread(pid, 0x2000, 0, 0, false, 0).unwrap();

        let mut ctx = CpuContext::default();
        // Conmutar al hilo 1
        assert!(scheduler.pick_and_switch_next(&mut ctx));
        assert_eq!(scheduler.current_tid, Some(_t1));

        // Terminar hilo 1: debe conmutar al hilo 2 y reciclar la pila del hilo 1
        assert!(scheduler.exit_current(0, &mut ctx));
        assert_eq!(scheduler.current_tid, Some(t2));
        assert_eq!(scheduler.get_metrics().recycled_stacks, 1);

        // Crear un tercer hilo: debe reutilizar la pila reciclada
        let t3 = scheduler.spawn_thread(pid, 0x3000, 0, 0, false, 0).unwrap();
        assert_eq!(scheduler.get_metrics().recycled_stacks, 0);

        // Conmutar a t3 al terminar t2
        assert!(scheduler.exit_current(0, &mut ctx));
        assert_eq!(scheduler.current_tid, Some(t3));
        assert_eq!(scheduler.get_metrics().recycled_stacks, 1);
    }
}

