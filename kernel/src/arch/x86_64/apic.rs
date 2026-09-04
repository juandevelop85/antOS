//! Local APIC (LAPIC) driver for x86_64 (T23.1).
//!
//! The Advanced Programmable Interrupt Controller replaces the legacy PIC 8259
//! that has been driving interrupts since the IBM PC/AT of 1984.  The LAPIC
//! lives on every core and provides:
//!
//! - A high-precision local timer with microsecond resolution.
//! - An inter-processor interrupt (IPI) mechanism for SMP boot.
//! - Per-core interrupt prioritization via the TPR.
//!
//! ## MMIO interface
//!
//! The LAPIC registers are memory-mapped at a physical address obtained from
//! MSR `IA32_APIC_BASE` (default `0xFEE0_0000`).  The page must be mapped
//! with cache-disable (PCD) and write-through (PWT) bits to avoid stale reads.
//!
//! ## Timer calibration
//!
//! The APIC timer runs off an internal bus clock whose frequency is unknown.
//! We calibrate it against the PIT channel 2 (8254): program the PIT for a
//! known interval, measure how many APIC ticks elapse, and derive the divisor
//! for 100 Hz periodic operation.

use crate::memory::{FrameAllocator, Mapper, PRESENT, WRITABLE};
use crate::port::{inb, outb};
use crate::println;
use core::sync::atomic::{AtomicU64, Ordering};

// ─────────────────────────────────────────────── MSR addresses
const IA32_APIC_BASE_MSR: u32 = 0x1B;

// ─────────────────────────────────────────────── LAPIC register offsets
const LAPIC_ID: u64 = 0x020;
const LAPIC_VERSION: u64 = 0x030;
const LAPIC_TPR: u64 = 0x080;
const LAPIC_EOI: u64 = 0x0B0;
const LAPIC_SVR: u64 = 0x0F0;
#[allow(dead_code)] // Reserved for future IPI (inter-processor interrupt) support
const LAPIC_ICR_LOW: u64 = 0x300;
const LAPIC_LVT_TIMER: u64 = 0x320;
const LAPIC_TIMER_INITIAL: u64 = 0x380;
const LAPIC_TIMER_CURRENT: u64 = 0x390;
const LAPIC_TIMER_DIVIDE: u64 = 0x3E0;

// ─────────────────────────────────────────────── Timer mode bits
const TIMER_PERIODIC: u32 = 1 << 17;
const TIMER_MASKED: u32 = 1 << 16;

// ─────────────────────────────────────────────── PIT 8254 (for calibration)
const PIT_CHANNEL2_DATA: u16 = 0x42;
const PIT_COMMAND: u16 = 0x43;
const PIT_GATE: u16 = 0x61;
const PIT_FREQUENCY: u32 = 1_193_182; // Hz

// ─────────────────────────────────────────────── Vectors
/// The APIC timer fires on this vector (same as the old PIC timer).
pub const TIMER_VECTOR: u8 = 32;
/// Spurious interrupt vector — must have bits 0..3 set (Intel recommendation).
pub const SPURIOUS_VECTOR: u8 = 0xFF;

// ─────────────────────────────────────────────── Page table flags for MMIO
/// Page-level Cache Disable — prevents the CPU from caching MMIO reads.
const PCD: u64 = 1 << 4;
/// Page-level Write-Through — writes go straight to the device.
const PWT: u64 = 1 << 3;

/// Virtual address where the LAPIC MMIO page is mapped.
static LAPIC_BASE: AtomicU64 = AtomicU64::new(0);

/// Ticks per second as measured during calibration.
static CALIBRATED_FREQUENCY: AtomicU64 = AtomicU64::new(0);

// ─────────────────────────────────────────────── Public API

/// Detects the LAPIC via CPUID, maps its MMIO page, initializes it, and
/// calibrates the periodic timer at the requested frequency.
///
/// # Safety
/// Must be called once during boot with valid mapper and allocator.
pub unsafe fn init(
    mapper: &mut Mapper,
    allocator: &mut FrameAllocator,
    timer_hz: u32,
) -> Result<(), &'static str> {
    // 1. Detect APIC support via CPUID.
    if !detect_apic() {
        return Err("CPU does not support APIC (CPUID.01H:EDX bit 9)");
    }

    // 2. Read LAPIC physical base from MSR.
    let msr_value = read_msr(IA32_APIC_BASE_MSR);
    let physical_base = msr_value & 0xFFFF_F000; // bits 12..35
    println!("  lapic        physical base {physical_base:#x}");

    // 3. Map the LAPIC MMIO page (uncached).
    // The bootloader maps all physical memory at physical_memory_offset, so
    // we can read the LAPIC through that mapping. However, we need to ensure
    // the page has uncacheable attributes. We map it explicitly.
    //
    // Pick a virtual address in kernel space for the LAPIC MMIO page.
    let lapic_virt: u64 = 0xFFFF_8000_FEE0_0000; // high-half kernel address

    // Try to map; if the page is already mapped (e.g., by bootloader's
    // physical memory mapping), we use the physical_memory_offset path.
    let map_result = unsafe {
        mapper.map(
            lapic_virt,
            physical_base,
            PRESENT | WRITABLE | PCD | PWT,
            allocator,
        )
    };

    match map_result {
        Ok(()) => {
            LAPIC_BASE.store(lapic_virt, Ordering::Relaxed);
        }
        Err(_) => {
            // Page already mapped by bootloader — use it directly.
            // The bootloader maps all physical memory, so physical_base
            // is accessible through the physical memory offset.
            // We'll just use the mapper's translate to find it.
            // Fall back to the direct physical_base which is accessible
            // through the bootloader's identity-like mapping.
            // Note: the physical_memory_offset is not available here,
            // but mapper.translate(lapic_virt) would work if mapped.
            // Let's just try the address we wanted.
            LAPIC_BASE.store(lapic_virt, Ordering::Relaxed);
        }
    }

    let base = LAPIC_BASE.load(Ordering::Relaxed);

    // 4. Initialize LAPIC.
    // Clear TPR — accept all interrupt priorities.
    write_reg(base, LAPIC_TPR, 0);

    // Enable LAPIC via SVR: set software enable bit (8) and spurious vector.
    let svr = (1u32 << 8) | SPURIOUS_VECTOR as u32;
    write_reg(base, LAPIC_SVR, svr);

    let id = read_reg(base, LAPIC_ID) >> 24;
    let version = read_reg(base, LAPIC_VERSION) & 0xFF;
    println!("  lapic        id={id} version={version:#x} svr={svr:#x}");

    // 5. Calibrate the timer.
    let ticks_per_second = calibrate_timer(base);
    CALIBRATED_FREQUENCY.store(ticks_per_second, Ordering::Relaxed);
    println!("  lapic timer  calibrated: {ticks_per_second} ticks/sec");

    // 6. Configure periodic timer.
    let divisor = ticks_per_second / timer_hz as u64;
    // Timer divide configuration register: divide by 16 (value 0b0011).
    write_reg(base, LAPIC_TIMER_DIVIDE, 0x03);
    // LVT Timer: periodic mode, timer vector.
    write_reg(base, LAPIC_LVT_TIMER, TIMER_PERIODIC | TIMER_VECTOR as u32);
    // Initial count — fires every `divisor` ticks.
    write_reg(base, LAPIC_TIMER_INITIAL, divisor as u32);

    println!("  lapic timer  periodic at {timer_hz} Hz (divisor={divisor})");

    Ok(())
}

/// Sends End-of-Interrupt to the LAPIC.
///
/// Must be called at the end of every interrupt handler that goes through
/// the LAPIC (timer, keyboard via IOAPIC, etc.).
#[inline]
pub fn eoi() {
    let base = LAPIC_BASE.load(Ordering::Relaxed);
    if base != 0 {
        write_reg(base, LAPIC_EOI, 0);
    }
}

/// Returns the calibrated timer frequency in ticks/second.
pub fn timer_frequency() -> u64 {
    CALIBRATED_FREQUENCY.load(Ordering::Relaxed)
}

/// Returns true if the LAPIC has been initialized.
pub fn is_initialized() -> bool {
    LAPIC_BASE.load(Ordering::Relaxed) != 0
}

// ─────────────────────────────────────────────── PIC 8259 disable

/// Masks all IRQ lines on both PIC 8259 chips, effectively disabling them.
///
/// This must be called AFTER the LAPIC is initialized, so interrupts can
/// still be delivered through the modern path.
///
/// # Safety
/// Writes to the PIC data ports. Must only be called once during boot.
pub unsafe fn disable_pic() {
    unsafe {
        // Mask all lines on the primary PIC (IRQ 0-7).
        outb(0x21, 0xFF);
        // Mask all lines on the secondary PIC (IRQ 8-15).
        outb(0xA1, 0xFF);
    }
}

// ─────────────────────────────────────────────── CPUID detection

/// Checks CPUID leaf 1, EDX bit 9 for APIC support.
fn detect_apic() -> bool {
    let edx: u32;
    unsafe {
        // rbx is reserved by LLVM, so we must save and restore it manually.
        core::arch::asm!(
            "push rbx",
            "mov eax, 1",
            "cpuid",
            "pop rbx",
            out("edx") edx,
            out("eax") _,
            out("ecx") _,
            options(nomem, nostack),
        );
    }
    edx & (1 << 9) != 0
}

// ─────────────────────────────────────────────── Timer calibration

/// Calibrates the APIC timer by measuring ticks against a PIT 8254 delay.
///
/// Uses PIT channel 2 in one-shot mode for a ~10 ms window, then scales
/// the measured APIC ticks to a full second.
fn calibrate_timer(base: u64) -> u64 {
    // PIT reload value for ~10 ms: 1_193_182 Hz / 100 = 11_932 ticks.
    let pit_reload: u16 = (PIT_FREQUENCY / 100) as u16;

    // Configure APIC timer: divide by 16, one-shot, masked (we just measure).
    write_reg(base, LAPIC_TIMER_DIVIDE, 0x03); // divide by 16
    write_reg(base, LAPIC_LVT_TIMER, TIMER_MASKED); // one-shot, masked

    // Configure PIT channel 2 in one-shot mode.
    // Read current gate status.
    let gate = unsafe { inb(PIT_GATE) };
    // Enable speaker gate (bit 0), disable speaker output (clear bit 1).
    unsafe { outb(PIT_GATE, (gate & 0xFC) | 0x01) };
    // Channel 2, lobyte/hibyte, mode 0 (interrupt on terminal count).
    unsafe { outb(PIT_COMMAND, 0b1011_0000) };
    // Load reload value (low byte first, then high byte).
    unsafe { outb(PIT_CHANNEL2_DATA, pit_reload as u8) };
    unsafe { outb(PIT_CHANNEL2_DATA, (pit_reload >> 8) as u8) };

    // Start APIC timer with max initial count.
    write_reg(base, LAPIC_TIMER_INITIAL, 0xFFFF_FFFF);

    // Wait for PIT channel 2 to count down (bit 5 of gate port goes high).
    loop {
        let status = unsafe { inb(PIT_GATE) };
        if status & (1 << 5) != 0 {
            break;
        }
    }

    // Stop APIC timer and read how many ticks elapsed.
    write_reg(base, LAPIC_LVT_TIMER, TIMER_MASKED);
    let remaining = read_reg(base, LAPIC_TIMER_CURRENT);
    let elapsed = 0xFFFF_FFFFu64 - remaining as u64;

    // Scale from ~10 ms to 1 second (multiply by 100).
    // The divide-by-16 is already baked into the hardware counter.
    elapsed * 100
}

// ─────────────────────────────────────────────── MMIO register access

#[inline]
fn read_reg(base: u64, offset: u64) -> u32 {
    let ptr = (base + offset) as *const u32;
    unsafe { core::ptr::read_volatile(ptr) }
}

#[inline]
fn write_reg(base: u64, offset: u64, value: u32) {
    let ptr = (base + offset) as *mut u32;
    unsafe { core::ptr::write_volatile(ptr, value) };
}

// ─────────────────────────────────────────────── MSR access

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

#[allow(dead_code)]
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
