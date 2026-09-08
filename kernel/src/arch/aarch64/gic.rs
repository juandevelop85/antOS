//! Generic Interrupt Controller driver for AArch64 — GICv2 *and* GICv3.
//!
//! The distributor (GICD) is shared; the CPU interface differs: GICv2 uses the
//! memory-mapped GICC block, GICv3 uses the `ICC_*_EL1` system registers plus a
//! per-CPU redistributor (GICR). The version is picked from the device tree's
//! `compatible` string, defaulting to GICv2 (QEMU `-M virt` and the historical
//! path) when discovery yields nothing.

use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

// ── Memory-mapped bases (QEMU `-M virt` defaults; GICR overridden from DTB) ──

pub const GICD_BASE: usize = 0x0800_0000;
pub const GICC_BASE: usize = 0x0801_0000;
/// GICv3 redistributor frame 0 base on QEMU `-M virt,gic-version=3`.
pub const GICR_BASE_DEFAULT: usize = 0x080A_0000;

// GIC Distributor register offsets (common to v2/v3).
const GICD_CTLR: usize = 0x000;
const GICD_ISENABLER: usize = 0x100;
const GICD_IPRIORITYR: usize = 0x400;
const GICD_ITARGETSR: usize = 0x800;
const GICD_IGROUPR: usize = 0x080;
const GICD_IROUTER: usize = 0x6000;

// GICv2 CPU Interface register offsets.
const GICC_CTLR: usize = 0x000;
const GICC_PMR: usize = 0x004;
const GICC_IAR: usize = 0x00C;
const GICC_EOIR: usize = 0x010;

// GICv3 redistributor offsets.
const GICR_WAKER: usize = 0x0014;
const GICR_SGI_BASE: usize = 0x1_0000;
const GICR_IGROUPR0: usize = GICR_SGI_BASE + 0x080;
const GICR_ISENABLER0: usize = GICR_SGI_BASE + 0x100;
const GICR_IPRIORITYR: usize = GICR_SGI_BASE + 0x400;

const GICR_WAKER_PROCESSOR_SLEEP: u32 = 1 << 1;
const GICR_WAKER_CHILDREN_ASLEEP: u32 = 1 << 2;

/// Which GIC architecture the running platform exposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GicVersion {
    V2,
    V3,
}

const VERSION_V2: u8 = 2;
const VERSION_V3: u8 = 3;

static GIC_VERSION: AtomicU8 = AtomicU8::new(VERSION_V2);
static GICR_BASE: AtomicUsize = AtomicUsize::new(GICR_BASE_DEFAULT);

/// Returns the GIC version chosen at [`init`] time.
pub fn version() -> GicVersion {
    match GIC_VERSION.load(Ordering::Relaxed) {
        VERSION_V3 => GicVersion::V3,
        _ => GicVersion::V2,
    }
}

/// Human-readable version tag for the boot log.
pub fn version_name() -> &'static str {
    match version() {
        GicVersion::V2 => "GICv2",
        GicVersion::V3 => "GICv3",
    }
}

/// Classifies a GIC node `compatible` property (a NUL-separated string list)
/// into a [`GicVersion`]. Unknown / empty input is treated as GICv2, the safe
/// historical default.
pub fn gic_version_from_compatible(compatible: &[u8]) -> GicVersion {
    let has = |needle: &str| {
        let n = needle.as_bytes();
        n.len() <= compatible.len() && compatible.windows(n.len()).any(|w| w == n)
    };
    if has("arm,gic-v3") || has("arm,gic-v4") {
        GicVersion::V3
    } else {
        // arm,cortex-a15-gic / arm,gic-400 / arm,gic-v2 / arm,arm11mp-gic …
        GicVersion::V2
    }
}

#[inline]
unsafe fn read_reg(base: usize, offset: usize) -> u32 {
    core::ptr::read_volatile((base + offset) as *const u32)
}

#[inline]
unsafe fn write_reg(base: usize, offset: usize, val: u32) {
    core::ptr::write_volatile((base + offset) as *mut u32, val);
}

#[inline]
unsafe fn read_gicd(offset: usize) -> u32 {
    read_reg(GICD_BASE, offset)
}

#[inline]
unsafe fn write_gicd(offset: usize, val: u32) {
    write_reg(GICD_BASE, offset, val);
}

// ── GICv3 system-register CPU interface ────────────────────────────────────

#[inline]
unsafe fn write_icc_sre_el1(v: u64) {
    core::arch::asm!("msr ICC_SRE_EL1, {}", "isb", in(reg) v, options(nomem, nostack));
}

#[inline]
unsafe fn read_icc_sre_el1() -> u64 {
    let v: u64;
    core::arch::asm!("mrs {}, ICC_SRE_EL1", out(reg) v, options(nomem, nostack));
    v
}

#[inline]
unsafe fn write_icc_pmr_el1(v: u64) {
    core::arch::asm!("msr ICC_PMR_EL1, {}", in(reg) v, options(nomem, nostack));
}

#[inline]
unsafe fn write_icc_igrpen1_el1(v: u64) {
    core::arch::asm!("msr ICC_IGRPEN1_EL1, {}", "isb", in(reg) v, options(nomem, nostack));
}

#[inline]
unsafe fn write_icc_bpr1_el1(v: u64) {
    core::arch::asm!("msr ICC_BPR1_EL1, {}", in(reg) v, options(nomem, nostack));
}

#[inline]
unsafe fn read_icc_iar1_el1() -> u64 {
    let v: u64;
    core::arch::asm!("mrs {}, ICC_IAR1_EL1", out(reg) v, options(nomem, nostack));
    v
}

#[inline]
unsafe fn write_icc_eoir1_el1(v: u64) {
    core::arch::asm!("msr ICC_EOIR1_EL1, {}", in(reg) v, options(nomem, nostack));
}

/// Selects the GIC version (call before [`init`]). Normally driven by
/// `dtb::find_gic()`; exposed so `main` can log/force it.
pub fn set_version(v: GicVersion, gicr_base: Option<usize>) {
    GIC_VERSION.store(
        match v {
            GicVersion::V2 => VERSION_V2,
            GicVersion::V3 => VERSION_V3,
        },
        Ordering::Relaxed,
    );
    if let Some(b) = gicr_base {
        GICR_BASE.store(b, Ordering::Relaxed);
    }
}

/// Initializes the GIC distributor and this CPU's interface for the selected
/// version.
pub fn init() {
    match version() {
        GicVersion::V2 => init_v2(),
        GicVersion::V3 => init_v3(),
    }
}

fn init_v2() {
    unsafe {
        write_gicd(GICD_CTLR, 0);
        write_reg(GICC_BASE, GICC_PMR, 0xFF);
        write_reg(GICC_BASE, GICC_CTLR, 0b11);
        write_gicd(GICD_CTLR, 1);
    }
}

fn init_v3() {
    unsafe {
        // Distributor: enable affinity routing and Group 1.
        let mut ctlr = read_gicd(GICD_CTLR);
        ctlr |= (1 << 4) | (1 << 5); // ARE_S | ARE_NS (RES0 bits are ignored)
        write_gicd(GICD_CTLR, ctlr);
        ctlr |= (1 << 0) | (1 << 1); // EnableGrp1 (both views)
        write_gicd(GICD_CTLR, ctlr);
        wait_rwp_v3();

        // Redistributor: wake this CPU's frame.
        let gicr = GICR_BASE.load(Ordering::Relaxed);
        let mut waker = read_reg(gicr, GICR_WAKER);
        waker &= !GICR_WAKER_PROCESSOR_SLEEP;
        write_reg(gicr, GICR_WAKER, waker);
        let mut spin = 1_000_000u32;
        while read_reg(gicr, GICR_WAKER) & GICR_WAKER_CHILDREN_ASLEEP != 0 && spin > 0 {
            spin -= 1;
            core::hint::spin_loop();
        }

        // SGIs + PPIs to Group 1, all masked until explicitly enabled.
        write_reg(gicr, GICR_IGROUPR0, 0xFFFF_FFFF);
        write_reg(gicr, GICR_ISENABLER0, 0);

        // CPU interface via system registers.
        let sre = read_icc_sre_el1() | 1; // SRE = 1
        write_icc_sre_el1(sre);
        write_icc_pmr_el1(0xF0); // accept all but the very highest priorities
        write_icc_bpr1_el1(0);
        write_icc_igrpen1_el1(1); // enable Group 1 interrupts
    }
}

#[inline]
unsafe fn wait_rwp_v3() {
    let mut spin = 1_000_000u32;
    while read_gicd(GICD_CTLR) & (1 << 31) != 0 && spin > 0 {
        spin -= 1;
        core::hint::spin_loop();
    }
}

/// Enables interrupt `id` at a mid priority, routed to CPU 0.
pub fn enable_interrupt(id: u32) {
    match version() {
        GicVersion::V2 => enable_interrupt_v2(id),
        GicVersion::V3 => enable_interrupt_v3(id),
    }
}

fn enable_interrupt_v2(id: u32) {
    let reg_index = (id / 32) as usize;
    let bit_mask = 1u32 << (id % 32);

    unsafe {
        let prio_reg = (id / 4) as usize;
        let prio_shift = (id % 4) * 8;
        let mut cur_prio = read_gicd(GICD_IPRIORITYR + prio_reg * 4);
        cur_prio &= !(0xFF << prio_shift);
        cur_prio |= 0x80 << prio_shift;
        write_gicd(GICD_IPRIORITYR + prio_reg * 4, cur_prio);

        if id >= 32 {
            let target_reg = (id / 4) as usize;
            let target_shift = (id % 4) * 8;
            let mut cur_target = read_gicd(GICD_ITARGETSR + target_reg * 4);
            cur_target &= !(0xFF << target_shift);
            cur_target |= 0x01 << target_shift;
            write_gicd(GICD_ITARGETSR + target_reg * 4, cur_target);
        }

        write_gicd(GICD_ISENABLER + reg_index * 4, bit_mask);
    }
}

fn enable_interrupt_v3(id: u32) {
    unsafe {
        if id < 32 {
            // PPI / SGI live in this CPU's redistributor SGI frame.
            let gicr = GICR_BASE.load(Ordering::Relaxed);
            let prio_off = GICR_IPRIORITYR + id as usize;
            let mut p = read_reg(gicr, prio_off & !0x3);
            let shift = (id % 4) * 8;
            p &= !(0xFF << shift);
            p |= 0x80 << shift;
            write_reg(gicr, prio_off & !0x3, p);
            write_reg(gicr, GICR_ISENABLER0, 1u32 << id);
        } else {
            // SPI: priority, Group 1, route to affinity 0, then enable.
            let prio_reg = (id / 4) as usize;
            let prio_shift = (id % 4) * 8;
            let mut p = read_gicd(GICD_IPRIORITYR + prio_reg * 4);
            p &= !(0xFF << prio_shift);
            p |= 0x80 << prio_shift;
            write_gicd(GICD_IPRIORITYR + prio_reg * 4, p);

            let grp_reg = (id / 32) as usize;
            let grp = read_gicd(GICD_IGROUPR + grp_reg * 4) | (1u32 << (id % 32));
            write_gicd(GICD_IGROUPR + grp_reg * 4, grp);

            // GICD_IROUTER is 64-bit per SPI; affinity 0.0.0.0, IRM = 0.
            let router = GICD_IROUTER + (id as usize) * 8;
            write_gicd(router, 0);
            write_gicd(router + 4, 0);

            write_gicd(GICD_ISENABLER + grp_reg * 4, 1u32 << (id % 32));
        }
    }
}

/// Acknowledges the pending interrupt and returns its INTID. `1020..=1023` are
/// special (spurious / group mismatch) and must not be EOI'd.
#[inline]
pub fn acknowledge() -> u32 {
    match version() {
        GicVersion::V2 => unsafe { read_reg(GICC_BASE, GICC_IAR) & 0x3FF },
        GicVersion::V3 => unsafe { (read_icc_iar1_el1() & 0xFF_FFFF) as u32 },
    }
}

/// Signals End Of Interrupt for `id`.
#[inline]
pub fn end_of_interrupt(id: u32) {
    match version() {
        GicVersion::V2 => unsafe { write_reg(GICC_BASE, GICC_EOIR, id) },
        GicVersion::V3 => unsafe { write_icc_eoir1_el1(id as u64) },
    }
}

/// `true` when `intid` is one of the GIC's special "no real interrupt" values.
#[inline]
pub fn is_spurious(intid: u32) -> bool {
    (1020..=1023).contains(&intid)
}

/// Enables the shared-peripheral interrupts the kernel knows how to service:
/// the VirtIO-MMIO transports and the PCIe legacy INTx lines on QEMU `-M virt`.
/// Servicing drains the device in the dispatcher, so a level-triggered line
/// that fires is de-asserted before EOI — no interrupt storm from enabling one
/// that happens to be unused.
pub fn enable_peripheral_irqs() {
    // QEMU `-M virt`: PCIe INTA..INTD = SPI 3..6 -> INTID 35..38.
    for intid in 35..=38 {
        enable_interrupt(intid);
    }
    // QEMU `-M virt`: 32 virtio-mmio slots = SPI 16..47 -> INTID 48..79.
    for intid in 48..=79 {
        enable_interrupt(intid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_v3_from_compatible_list() {
        assert_eq!(
            gic_version_from_compatible(b"arm,gic-v3\0"),
            GicVersion::V3
        );
        assert_eq!(
            gic_version_from_compatible(b"arm,gic-v3\0arm,cortex-a15-gic\0"),
            GicVersion::V3
        );
    }

    #[test]
    fn detects_v2_variants_and_defaults() {
        assert_eq!(
            gic_version_from_compatible(b"arm,cortex-a15-gic\0"),
            GicVersion::V2
        );
        assert_eq!(gic_version_from_compatible(b"arm,gic-400\0"), GicVersion::V2);
        assert_eq!(gic_version_from_compatible(b""), GicVersion::V2);
        assert_eq!(gic_version_from_compatible(b"something-else"), GicVersion::V2);
    }

    #[test]
    fn spurious_intids_are_recognised() {
        assert!(is_spurious(1023));
        assert!(is_spurious(1020));
        assert!(!is_spurious(27));
        assert!(!is_spurious(1019));
    }
}
