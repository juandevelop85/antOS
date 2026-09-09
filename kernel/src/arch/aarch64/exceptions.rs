//! AArch64 Exception Vector Table and Interrupt handling (`VBAR_EL1`).
//!
//! Implements the 16 standard exception vectors aligned to 2048 bytes,
//! register preservation, and detailed decoding of `ESR_EL1` / `FAR_EL1`.

use core::arch::global_asm;
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Set while a "safe probe" access is in flight; the dispatcher checks this
/// before deciding to panic on a Data/Instruction Abort.
static PROBE_ARMED: AtomicBool = AtomicBool::new(false);
/// Set by the dispatcher when a guarded probe access actually aborted.
static PROBE_FAULTED: AtomicBool = AtomicBool::new(false);

/// Storm breaker: the last non-timer INTID seen and how many times in a row it
/// has fired without any other interrupt in between. A line that keeps
/// re-asserting with no driver to quiesce it (a half-initialised PCIe device on
/// a shared INTx, say) would otherwise live-lock the CPU in the IRQ handler.
static LAST_IRQ_ID: AtomicU32 = AtomicU32::new(u32::MAX);
static SAME_IRQ_STREAK: AtomicU32 = AtomicU32::new(0);
/// Consecutive repeats of the same unhandled INTID before it is masked.
const IRQ_STORM_LIMIT: u32 = 4_000;

/// Reads a 32-bit value from a possibly-nonexistent MMIO/physical address
/// without crashing the kernel if the access raises a Data or Instruction
/// Abort (translation fault *or* synchronous external abort).
///
/// Needed for hardware discovery (T27.2 PCIe ECAM probing) that must try
/// addresses which differ between emulators/hypervisors (QEMU vs
/// VirtualBox) and may simply not exist under the one currently running.
/// Returns `None` if the read aborted, `Some(value)` otherwise.
pub fn safe_probe_read_u32(addr: usize) -> Option<u32> {
    PROBE_FAULTED.store(false, Ordering::SeqCst);
    PROBE_ARMED.store(true, Ordering::SeqCst);
    let value = unsafe { core::ptr::read_volatile(addr as *const u32) };
    PROBE_ARMED.store(false, Ordering::SeqCst);
    if PROBE_FAULTED.swap(false, Ordering::SeqCst) {
        None
    } else {
        Some(value)
    }
}

/// Runs `f` with the fault guard armed, so a synchronous abort *or* a trapped
/// system-register access (`EC 0x18`) inside it skips the offending instruction
/// instead of panicking. Returns `true` if `f` completed without faulting.
///
/// Used by the timer fallback: on a guest booted at EL1 from an EL2 that never
/// enabled `CNTHCTL_EL2.EL1PCEN`, writing `CNTP_TVAL_EL0` traps — this lets the
/// kernel notice and stay on the virtual timer rather than crash.
pub fn probe_guard<F: FnOnce()>(f: F) -> bool {
    PROBE_FAULTED.store(false, Ordering::SeqCst);
    PROBE_ARMED.store(true, Ordering::SeqCst);
    f();
    PROBE_ARMED.store(false, Ordering::SeqCst);
    !PROBE_FAULTED.swap(false, Ordering::SeqCst)
}

#[repr(C)]
pub struct ExceptionContext {
    pub x: [u64; 30],
    pub x30: u64,
    pub elr_el1: u64,
    pub spsr_el1: u64,
}

global_asm!(
    r#"
    .section .text.vectors, "ax"
    .global exception_vector_table
    .align 11
exception_vector_table:
    // Current EL with SP0
    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    mov x0, #0
    b vector_common
    .align 7

    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    mov x0, #1
    b vector_common
    .align 7

    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    mov x0, #2
    b vector_common
    .align 7

    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    mov x0, #3
    b vector_common
    .align 7

    // Current EL with SPx
    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    mov x0, #4
    b vector_common
    .align 7

    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    mov x0, #5
    b vector_common
    .align 7

    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    mov x0, #6
    b vector_common
    .align 7

    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    mov x0, #7
    b vector_common
    .align 7

    // Lower EL using AArch64
    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    mov x0, #8
    b vector_common
    .align 7

    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    mov x0, #9
    b vector_common
    .align 7

    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    mov x0, #10
    b vector_common
    .align 7

    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    mov x0, #11
    b vector_common
    .align 7

    // Lower EL using AArch32
    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    mov x0, #12
    b vector_common
    .align 7

    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    mov x0, #13
    b vector_common
    .align 7

    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    mov x0, #14
    b vector_common
    .align 7

    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    mov x0, #15
    b vector_common
    .align 7

vector_common:
    stp x2, x3, [sp, #16 * 1]
    stp x4, x5, [sp, #16 * 2]
    stp x6, x7, [sp, #16 * 3]
    stp x8, x9, [sp, #16 * 4]
    stp x10, x11, [sp, #16 * 5]
    stp x12, x13, [sp, #16 * 6]
    stp x14, x15, [sp, #16 * 7]
    stp x16, x17, [sp, #16 * 8]
    stp x18, x19, [sp, #16 * 9]
    stp x20, x21, [sp, #16 * 10]
    stp x22, x23, [sp, #16 * 11]
    stp x24, x25, [sp, #16 * 12]
    stp x26, x27, [sp, #16 * 13]
    stp x28, x29, [sp, #16 * 14]
    mrs x2, elr_el1
    mrs x3, spsr_el1
    stp x30, x2, [sp, #16 * 15]
    str x3, [sp, #16 * 16]

    mov x1, x0
    mov x0, sp
    bl aarch64_exception_dispatch

    ldr x3, [sp, #16 * 16]
    ldp x30, x2, [sp, #16 * 15]
    msr spsr_el1, x3
    msr elr_el1, x2
    ldp x28, x29, [sp, #16 * 14]
    ldp x26, x27, [sp, #16 * 13]
    ldp x24, x25, [sp, #16 * 12]
    ldp x22, x23, [sp, #16 * 11]
    ldp x20, x21, [sp, #16 * 10]
    ldp x18, x19, [sp, #16 * 9]
    ldp x16, x17, [sp, #16 * 8]
    ldp x14, x15, [sp, #16 * 7]
    ldp x12, x13, [sp, #16 * 6]
    ldp x10, x11, [sp, #16 * 5]
    ldp x8, x9, [sp, #16 * 4]
    ldp x6, x7, [sp, #16 * 3]
    ldp x4, x5, [sp, #16 * 2]
    ldp x2, x3, [sp, #16 * 1]
    ldp x0, x1, [sp, #0]
    add sp, sp, #272
    eret
"#
);

extern "C" {
    static exception_vector_table: u8;
}

/// Initializes the Vector Base Address Register (`VBAR_EL1`).
pub fn init() {
    unsafe {
        let vbar = &exception_vector_table as *const u8 as u64;
        core::arch::asm!(
            "msr vbar_el1, {}",
            "isb",
            in(reg) vbar,
            options(nomem, nostack)
        );
    }
}

/// Enables IRQ interrupts on current CPU core (clears `I` bit in `DAIF`).
#[inline]
pub fn enable_irq() {
    unsafe {
        core::arch::asm!("msr daifclr, #2", options(nomem, nostack));
    }
}

/// Disables IRQ interrupts on current CPU core (sets `I` bit in `DAIF`).
#[inline]
pub fn disable_irq() {
    unsafe {
        core::arch::asm!("msr daifset, #2", options(nomem, nostack));
    }
}

/// Halts CPU execution until the next interrupt arrives (`wfi` - Wait For Interrupt).
#[inline]
pub fn wait_for_interrupt() {
    unsafe {
        core::arch::asm!("wfi", options(nomem, nostack));
    }
}

/// Triggers a test software breakpoint on AArch64 (`brk #0`).
#[inline]
pub fn trigger_breakpoint() {
    unsafe {
        core::arch::asm!("brk #0", options(nomem, nostack));
    }
}

/// Dispatches all exceptions routed through `VBAR_EL1`.
#[no_mangle]
pub extern "C" fn aarch64_exception_dispatch(ctx: &mut ExceptionContext, vector_id: u64) {
    // Handle IRQs first (Vectors 1, 5, 9, 13).
    // ESR_EL1 is only updated on synchronous exceptions, not on IRQs.
    if vector_id == 5 || vector_id == 1 || vector_id == 9 || vector_id == 13 {
        use crate::arch::aarch64::{gic, timer};
        let irq_id = gic::acknowledge();

        // Spurious / group-mismatch INTIDs (1020..=1023) carry no interrupt and
        // must not be EOI'd.
        if gic::is_spurious(irq_id) {
            return;
        }

        if timer::is_timer_irq(irq_id) {
            LAST_IRQ_ID.store(u32::MAX, Ordering::Relaxed);
            SAME_IRQ_STREAK.store(0, Ordering::Relaxed);
            timer::handle_timer_interrupt();
            crate::drivers::usb::poll();
            // Preemption hook: check quantum and switch context if needed
            let cpu_ctx = unsafe {
                &mut *(ctx as *mut ExceptionContext as *mut crate::task::pcb::CpuContext)
            };
            crate::task::scheduler::on_timer_tick(cpu_ctx);
        } else {
            // A shared-peripheral IRQ (xHCI, VirtIO-Input/GPU). Servicing the
            // devices drains their state so a level-triggered line de-asserts
            // before EOI. Cheap enough to run for any non-timer INTID.
            let (w, h) = crate::console::resolution();
            crate::drivers::usb::poll();
            crate::drivers::virtio_input::poll_virtio_inputs(w, h);

            // Storm breaker: if the same INTID keeps coming back with nothing
            // draining it, mask it at the GIC and move on. Prevents a
            // half-initialised PCIe device on a shared INTx line from
            // live-locking the boot.
            let streak = if LAST_IRQ_ID.swap(irq_id, Ordering::Relaxed) == irq_id {
                SAME_IRQ_STREAK.fetch_add(1, Ordering::Relaxed) + 1
            } else {
                SAME_IRQ_STREAK.store(1, Ordering::Relaxed);
                1
            };
            if streak >= IRQ_STORM_LIMIT {
                gic::disable_interrupt(irq_id);
                SAME_IRQ_STREAK.store(0, Ordering::Relaxed);
                crate::println!(
                    "  irq-storm    INTID {} sin manejador · enmascarada tras {} repeticiones",
                    irq_id,
                    streak
                );
            }
        }

        gic::end_of_interrupt(irq_id);
        return;
    }

    let esr: u64;
    let far: u64;
    unsafe {
        core::arch::asm!("mrs {}, esr_el1", out(reg) esr, options(nomem, nostack));
        core::arch::asm!("mrs {}, far_el1", out(reg) far, options(nomem, nostack));
    }

    let ec = (esr >> 26) & 0x3f;
    let iss = esr & 0x01ff_ffff;

    // Recover from a guarded "safe probe" access instead of panicking.
    // Hardware discovery may target an address that doesn't exist on the
    // emulator/hypervisor currently running it (see safe_probe_read_u32).
    // 0x18 = trapped MSR/MRS/system instruction (e.g. CNTP_* not enabled for
    // EL1 by EL2); 0x20/0x21/0x24/0x25 = instruction/data aborts.
    if PROBE_ARMED.load(Ordering::SeqCst) && matches!(ec, 0x18 | 0x20 | 0x21 | 0x24 | 0x25) {
        PROBE_FAULTED.store(true, Ordering::SeqCst);
        PROBE_ARMED.store(false, Ordering::SeqCst);
        ctx.elr_el1 += 4;
        return;
    }

    // Handle software breakpoint (BRK #0, EC == 0x3c)
    if ec == 0x3c {
        let mut serial = crate::arch::aarch64::pl011::emergency();
        let _ = writeln!(
            serial,
            "  breakpoint   manejado (brk #0) en AArch64 · reanudando..."
        );
        // Skip over the 4-byte brk instruction
        ctx.elr_el1 += 4;
        return;
    }

    // Handle Syscall (SVC #0 from EL0, EC == 0x15)
    if ec == 0x15 {
        crate::arch::aarch64::syscall::dispatch(ctx);
        return;
    }

    let mut serial = crate::arch::aarch64::pl011::emergency();
    let vector_name = match vector_id {
        0 => "Current EL with SP0 (Synchronous)",
        1 => "Current EL with SP0 (IRQ)",
        2 => "Current EL with SP0 (FIQ)",
        3 => "Current EL with SP0 (SError)",
        4 => "Current EL with SPx (Synchronous Abort)",
        5 => "Current EL with SPx (IRQ)",
        6 => "Current EL with SPx (FIQ)",
        7 => "Current EL with SPx (SError)",
        8 => "Lower EL using AArch64 (Synchronous)",
        9 => "Lower EL using AArch64 (IRQ)",
        10 => "Lower EL using AArch64 (FIQ)",
        11 => "Lower EL using AArch64 (SError)",
        _ => "Other Exception",
    };

    let ec_name = match ec {
        0x00 => "Unknown reason",
        0x01 => "Trapped WFI/WFE instruction",
        0x15 => "SVC instruction execution (AArch64 syscall)",
        0x20 => "Instruction Abort (lower EL)",
        0x21 => "Instruction Abort (same EL)",
        0x22 => "PC alignment fault",
        0x24 => "Data Abort (lower EL)",
        0x25 => "Data Abort (same EL)",
        0x26 => "SP alignment fault",
        0x3c => "BRK instruction execution",
        _ => "Unhandled Exception Class",
    };

    let _ = writeln!(
        serial,
        "\n╔════════════════════════════════════════════════════════"
    );
    let _ = writeln!(serial, "║ antOS AArch64 EXCEPTION / PANIC");
    let _ = writeln!(
        serial,
        "╠════════════════════════════════════════════════════════"
    );
    let _ = writeln!(serial, "║ Vector:      [{vector_id}] {vector_name}");
    let _ = writeln!(
        serial,
        "║ ESR_EL1:     {esr:#018x} (EC: {ec:#04x} [{ec_name}], ISS: {iss:#08x})"
    );
    let _ = writeln!(serial, "║ FAR_EL1:     {far:#018x} (Fault Address)");
    let _ = writeln!(
        serial,
        "║ ELR_EL1:     {:#018x} (Return Address / PC)",
        ctx.elr_el1
    );
    let _ = writeln!(serial, "║ SPSR_EL1:    {:#018x}", ctx.spsr_el1);
    let _ = writeln!(
        serial,
        "╟────────────────────────────────────────────────────────"
    );
    let _ = writeln!(serial, "║ Volcado de Registros:");
    for i in (0..30).step_by(2) {
        let _ = writeln!(
            serial,
            "║   x{:02}: {:016x}   x{:02}: {:016x}",
            i,
            ctx.x[i],
            i + 1,
            ctx.x[i + 1]
        );
    }
    let _ = writeln!(serial, "║   x30 (LR): {:016x}", ctx.x30);
    let _ = writeln!(
        serial,
        "╚════════════════════════════════════════════════════════"
    );
    let _ = writeln!(serial, "CPU detenida por pánico en AArch64.");

    crate::console::_print(format_args!(
        "\n\x1b[1;31m╔════════════════════════════════════════════════════════\n\
         ║ antOS AArch64 EXCEPTION / PANIC\n\
         ╠════════════════════════════════════════════════════════\n\
         ║ Vector:   [{}] {}\n\
         ║ ESR_EL1:  {:#018x} (EC: {:#04x} [{}], ISS: {:#08x})\n\
         ║ FAR_EL1:  {:#018x}\n\
         ║ ELR_EL1:  {:#018x}\n\
         ╚════════════════════════════════════════════════════════\x1b[0m\n",
        vector_id, vector_name, esr, ec, ec_name, iss, far, ctx.elr_el1
    ));

    loop {
        wait_for_interrupt();
    }
}
