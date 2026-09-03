//! Generic Interrupt Controller (GICv2) driver for AArch64 (QEMU virt).
//!
//! Controls interrupt routing and prioritization via Distributor (GICD)
//! and CPU Interface (GICC).

pub const GICD_BASE: usize = 0x0800_0000;
pub const GICC_BASE: usize = 0x0801_0000;

// GIC Distributor register offsets
const GICD_CTLR: usize = 0x000;
const GICD_ISENABLER: usize = 0x100;
const GICD_IPRIORITYR: usize = 0x400;
const GICD_ITARGETSR: usize = 0x800;

// GIC CPU Interface register offsets
const GICC_CTLR: usize = 0x000;
const GICC_PMR: usize = 0x004;
const GICC_IAR: usize = 0x00C;
const GICC_EOIR: usize = 0x010;

#[inline]
unsafe fn read_gicd(offset: usize) -> u32 {
    core::ptr::read_volatile((GICD_BASE + offset) as *const u32)
}

#[inline]
unsafe fn write_gicd(offset: usize, val: u32) {
    core::ptr::write_volatile((GICD_BASE + offset) as *mut u32, val);
}

#[inline]
unsafe fn read_gicc(offset: usize) -> u32 {
    core::ptr::read_volatile((GICC_BASE + offset) as *const u32)
}

#[inline]
unsafe fn write_gicc(offset: usize, val: u32) {
    core::ptr::write_volatile((GICC_BASE + offset) as *mut u32, val);
}

/// Initializes the GIC Distributor and CPU Interface.
pub fn init() {
    unsafe {
        // 1. Disable Distributor while configuring
        write_gicd(GICD_CTLR, 0);

        // 2. Configure CPU Interface:
        // Set Priority Mask to lowest priority (0xFF) so all interrupts are accepted
        write_gicc(GICC_PMR, 0xFF);
        // Enable CPU interface (bit 0 = EnableGrp0, bit 1 = EnableGrp1)
        write_gicc(GICC_CTLR, 0b11);

        // 3. Enable Distributor
        write_gicd(GICD_CTLR, 1);
    }
}

/// Enables a specific interrupt ID in the GIC Distributor.
pub fn enable_interrupt(id: u32) {
    let reg_index = (id / 32) as usize;
    let bit_mask = 1u32 << (id % 32);

    unsafe {
        // Set priority to 0x80 (mid-level)
        let prio_reg = (id / 4) as usize;
        let prio_shift = (id % 4) * 8;
        let mut cur_prio = read_gicd(GICD_IPRIORITYR + prio_reg * 4);
        cur_prio &= !(0xFF << prio_shift);
        cur_prio |= 0x80 << prio_shift;
        write_gicd(GICD_IPRIORITYR + prio_reg * 4, cur_prio);

        // Target CPU 0 for SPIs (IDs >= 32)
        if id >= 32 {
            let target_reg = (id / 4) as usize;
            let target_shift = (id % 4) * 8;
            let mut cur_target = read_gicd(GICD_ITARGETSR + target_reg * 4);
            cur_target &= !(0xFF << target_shift);
            cur_target |= 0x01 << target_shift; // CPU 0
            write_gicd(GICD_ITARGETSR + target_reg * 4, cur_target);
        }

        // Enable interrupt in GICD_ISENABLER
        write_gicd(GICD_ISENABLER + reg_index * 4, bit_mask);
    }
}

/// Acknowledges the pending interrupt and returns its interrupt ID.
#[inline]
pub fn acknowledge() -> u32 {
    unsafe { read_gicc(GICC_IAR) & 0x3FF }
}

/// Signals End of Interrupt (EOI) to the GIC CPU Interface.
#[inline]
pub fn end_of_interrupt(id: u32) {
    unsafe { write_gicc(GICC_EOIR, id) };
}
