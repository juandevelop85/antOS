//! Architectural Generic Timer for AArch64, with a resilient fallback.
//!
//! The virtual timer (`CNTV_*`, PPI 27) is tried first. Some hypervisors
//! (notably VirtualBox on Apple silicon) never deliver its interrupt, leaving
//! the kernel tick frozen at 0 and every poll loop that depends on it inert.
//! When the first ticks fail to arrive the driver falls back to the EL1
//! physical timer (`CNTP_*`, PPI 30), and `ticks()` stays monotonic either way.

use crate::arch::aarch64::gic;
use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};

/// PPI INTID for the EL1 virtual timer.
pub const TIMER_IRQ_VIRTUAL: u32 = 27;
/// PPI INTID for the EL1 physical timer.
pub const TIMER_IRQ_PHYSICAL: u32 = 30;

/// Ticks per second the kernel schedules.
pub const TICK_HZ: u64 = 100;

/// Active timer source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerSource {
    Virtual,
    Physical,
}

const SOURCE_VIRTUAL: u8 = 0;
const SOURCE_PHYSICAL: u8 = 1;

static TICKS: AtomicU64 = AtomicU64::new(0);
static TIMER_INTERVAL: AtomicU64 = AtomicU64::new(0);
static TIMER_SOURCE: AtomicU8 = AtomicU8::new(SOURCE_VIRTUAL);

/// Reads counter frequency in Hz from `CNTFRQ_EL0`.
#[inline]
pub fn counter_frequency() -> u64 {
    let freq: u64;
    unsafe {
        core::arch::asm!("mrs {}, cntfrq_el0", out(reg) freq, options(nomem, nostack));
    }
    freq
}

/// Reads the current virtual counter value from `CNTVCT_EL0`.
#[inline]
pub fn current_counter() -> u64 {
    let count: u64;
    unsafe {
        core::arch::asm!("mrs {}, cntvct_el0", out(reg) count, options(nomem, nostack));
    }
    count
}

/// Computes the down-counter reload value for a periodic tick.
///
/// Returns `1` rather than `0` for a degenerate frequency so the timer still
/// makes progress instead of firing continuously.
pub const fn reload_interval(freq_hz: u64, tick_hz: u64) -> u64 {
    if freq_hz == 0 || tick_hz == 0 {
        return 1;
    }
    let interval = freq_hz / tick_hz;
    if interval == 0 {
        1
    } else {
        interval
    }
}

/// Chooses the timer source after a trial period: keep the virtual timer only
/// if it actually delivered a tick, otherwise fall back to physical.
pub const fn select_source(virtual_ticked: bool) -> TimerSource {
    if virtual_ticked {
        TimerSource::Virtual
    } else {
        TimerSource::Physical
    }
}

/// Current timer source.
pub fn source() -> TimerSource {
    match TIMER_SOURCE.load(Ordering::Relaxed) {
        SOURCE_PHYSICAL => TimerSource::Physical,
        _ => TimerSource::Virtual,
    }
}

/// INTID the active timer source raises.
pub fn active_irq() -> u32 {
    match source() {
        TimerSource::Virtual => TIMER_IRQ_VIRTUAL,
        TimerSource::Physical => TIMER_IRQ_PHYSICAL,
    }
}

/// `true` when `intid` is one of the timer PPIs (either source).
#[inline]
pub fn is_timer_irq(intid: u32) -> bool {
    intid == TIMER_IRQ_VIRTUAL || intid == TIMER_IRQ_PHYSICAL
}

pub fn source_name() -> &'static str {
    match source() {
        TimerSource::Virtual => "virtual (CNTV, PPI 27)",
        TimerSource::Physical => "físico EL1 (CNTP, PPI 30)",
    }
}

/// Precise busy-wait delay in milliseconds using the hardware counter.
pub fn delay_ms(ms: u64) {
    let freq = counter_frequency();
    if freq == 0 {
        for _ in 0..(ms * 5_000) {
            core::hint::spin_loop();
        }
        return;
    }
    let ticks = (ms * freq) / 1000;
    let start = current_counter();
    while current_counter().wrapping_sub(start) < ticks {
        core::hint::spin_loop();
    }
}

#[inline]
unsafe fn arm_virtual(interval: u64) {
    core::arch::asm!("msr cntv_tval_el0, {}", in(reg) interval, options(nomem, nostack));
    core::arch::asm!("msr cntv_ctl_el0, {}", in(reg) 1u64, options(nomem, nostack));
    // enable, unmasked
}

#[inline]
unsafe fn disarm_virtual() {
    core::arch::asm!("msr cntv_ctl_el0, {}", in(reg) 0u64, options(nomem, nostack));
}

#[inline]
unsafe fn arm_physical(interval: u64) {
    core::arch::asm!("msr cntp_tval_el0, {}", in(reg) interval, options(nomem, nostack));
    core::arch::asm!("msr cntp_ctl_el0, {}", in(reg) 1u64, options(nomem, nostack));
}

/// Programs the virtual timer for `TICK_HZ` and enables its PPI. The source may
/// still be switched to physical by [`verify_and_fallback`].
pub fn init() {
    let interval = reload_interval(counter_frequency(), TICK_HZ);
    TIMER_INTERVAL.store(interval, Ordering::Relaxed);
    TIMER_SOURCE.store(SOURCE_VIRTUAL, Ordering::Relaxed);

    unsafe {
        arm_virtual(interval);
    }
    gic::enable_interrupt(TIMER_IRQ_VIRTUAL);
}

/// Switches to the EL1 physical timer: mask the virtual timer, arm `CNTP_*`, and
/// enable PPI 30. Returns `false` (staying on the virtual timer) if `CNTP_*`
/// access traps — a guest booted at EL1 whose EL2 never set
/// `CNTHCTL_EL2.EL1PCEN`.
fn switch_to_physical() -> bool {
    let interval = TIMER_INTERVAL.load(Ordering::Relaxed);
    let armed = crate::arch::aarch64::exceptions::probe_guard(|| unsafe {
        arm_physical(interval);
    });
    if !armed {
        return false;
    }
    unsafe {
        disarm_virtual();
    }
    gic::enable_interrupt(TIMER_IRQ_PHYSICAL);
    TIMER_SOURCE.store(SOURCE_PHYSICAL, Ordering::Relaxed);
    true
}

/// Waits a bounded time for the first ticks. If none arrive on the virtual
/// timer, falls back to the physical timer and waits again. IRQs must already
/// be unmasked (`daifclr`). Returns the source that ended up delivering ticks
/// (or `Virtual` if nothing did — the counter itself may be frozen).
pub fn verify_and_fallback() -> TimerSource {
    // If the virtual counter is not even advancing the virtual timer is
    // hopeless; go straight to physical.
    if !virtual_counter_running() {
        crate::println!("  timer        contador virtual congelado · probando timer físico EL1");
        let _ = switch_to_physical();
    }

    if wait_for_tick() {
        return source();
    }

    if source() == TimerSource::Virtual {
        crate::println!(
            "  timer        sin pulsos del timer virtual (PPI 27) · fallback a físico EL1 (PPI 30)"
        );
        if switch_to_physical() && wait_for_tick() {
            return TimerSource::Physical;
        }
    }

    source()
}

/// Reads `CNTVCT_EL0` twice around a bounded spin; `true` if it advanced.
fn virtual_counter_running() -> bool {
    let a = current_counter();
    for _ in 0..200_000 {
        core::hint::spin_loop();
    }
    current_counter() != a
}

/// Spins until `ticks()` advances or a bound elapses.
fn wait_for_tick() -> bool {
    let start = ticks();
    let mut spin = 5_000_000u32;
    while spin > 0 {
        if ticks() > start {
            return true;
        }
        spin -= 1;
        core::hint::spin_loop();
    }
    false
}

/// Handles a timer interrupt: bumps the tick counter and re-arms the active
/// down-counter. Both timer sources funnel here.
pub fn handle_timer_interrupt() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    let interval = TIMER_INTERVAL.load(Ordering::Relaxed);
    unsafe {
        match source() {
            TimerSource::Virtual => {
                core::arch::asm!("msr cntv_tval_el0, {}", in(reg) interval, options(nomem, nostack));
            }
            TimerSource::Physical => {
                core::arch::asm!("msr cntp_tval_el0, {}", in(reg) interval, options(nomem, nostack));
            }
        }
    }
}

/// Total ticks elapsed since [`init`].
pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// Uptime in whole seconds, useful for `info`. Falls back to deriving from the
/// hardware counter when the tick interrupt never started.
pub fn uptime_seconds() -> u64 {
    let t = ticks();
    if t > 0 {
        return t / TICK_HZ;
    }
    // No tick interrupt yet — derive from the always-readable virtual counter.
    current_counter()
        .checked_div(counter_frequency())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn reload_interval_divides_frequency_by_tick_rate() {
        assert_eq!(reload_interval(62_500_000, 100), 625_000);
        assert_eq!(reload_interval(24_000_000, 100), 240_000);
        assert_eq!(reload_interval(1_000, 100), 10);
    }

    #[test_case]
    fn reload_interval_never_returns_zero() {
        assert_eq!(reload_interval(0, 100), 1);
        assert_eq!(reload_interval(50, 100), 1); // freq < tick rate
        assert_eq!(reload_interval(100, 0), 1);
    }

    #[test_case]
    fn source_selection_prefers_virtual_only_when_it_ticks() {
        assert_eq!(select_source(true), TimerSource::Virtual);
        assert_eq!(select_source(false), TimerSource::Physical);
    }

    #[test_case]
    fn timer_irq_predicate_matches_both_ppis() {
        assert!(is_timer_irq(TIMER_IRQ_VIRTUAL));
        assert!(is_timer_irq(TIMER_IRQ_PHYSICAL));
        assert!(!is_timer_irq(48));
    }
}
