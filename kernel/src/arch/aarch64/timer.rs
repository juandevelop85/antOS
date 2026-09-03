//! Architectural Generic Timer for AArch64.
//!
//! Uses the ARM Virtual Timer (`CNTV_CTL_EL0`, `CNTV_TVAL_EL0`) and GIC PPI 27
//! to provide periodic system ticks for scheduling and timekeeping.

use core::sync::atomic::{AtomicU64, Ordering};
use crate::arch::aarch64::gic;

pub const TIMER_IRQ: u32 = 27; // ARM Virtual Timer PPI

static TICKS: AtomicU64 = AtomicU64::new(0);
static TIMER_INTERVAL: AtomicU64 = AtomicU64::new(0);

/// Reads counter frequency in Hz from `CNTFRQ_EL0`.
#[inline]
pub fn counter_frequency() -> u64 {
    let freq: u64;
    unsafe {
        core::arch::asm!("mrs {}, cntfrq_el0", out(reg) freq, options(nomem, nostack));
    }
    freq
}

/// Initializes the ARM Generic Virtual Timer for periodic interrupts (100 Hz).
pub fn init() {
    let freq = counter_frequency();
    // 100 ticks per second (every 10 ms)
    let interval = freq / 100;
    TIMER_INTERVAL.store(interval, Ordering::Relaxed);

    // 1. Program initial timer countdown
    unsafe {
        core::arch::asm!("msr cntv_tval_el0, {}", in(reg) interval, options(nomem, nostack));
        // Bit 0 = 1 (enable), Bit 1 = 0 (unmask interrupt)
        core::arch::asm!("msr cntv_ctl_el0, {}", in(reg) 1u64, options(nomem, nostack));
    }

    // 2. Enable Timer IRQ 27 in GIC
    gic::enable_interrupt(TIMER_IRQ);
}

/// Handles a timer interrupt tick: increments counter and re-arms timer.
pub fn handle_timer_interrupt() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    let interval = TIMER_INTERVAL.load(Ordering::Relaxed);
    unsafe {
        core::arch::asm!("msr cntv_tval_el0, {}", in(reg) interval, options(nomem, nostack));
    }
}

/// Returns the total ticks elapsed since timer initialization.
pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}
