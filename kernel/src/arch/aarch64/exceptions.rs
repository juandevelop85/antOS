//! AArch64 Exception Vector Table and Interrupt handling (`VBAR_EL1`).

/// Initializes the Vector Base Address Register (`VBAR_EL1`).
pub fn init() {
    // Will set VBAR_EL1 in T18.2 once the 16 vector stubs are assembled.
}

/// Enables IRQ interrupts on the current CPU core (clears `I` bit in `DAIF`).
#[inline]
pub fn enable_irq() {
    unsafe {
        core::arch::asm!("msr daifclr, #2", options(nomem, nostack));
    }
}

/// Disables IRQ interrupts on the current CPU core (sets `I` bit in `DAIF`).
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
