//! xHCI Register Definitions and Accessors (eXtensible Host Controller Interface).
//!
//! Covers Capability Registers, Operational Registers, Runtime Registers,
//! and Port Status & Control (PORTSC) registers.

/// Bit masks for USBCMD (USB Command Register).
pub mod usbcmd {
    pub const RS: u32 = 1 << 0; // Run/Stop: 1 = Run, 0 = Stop
    pub const HCRST: u32 = 1 << 1; // Host Controller Reset
    pub const INTE: u32 = 1 << 2; // Interrupter Enable
    pub const HSEE: u32 = 1 << 3; // Host System Error Enable
}

/// Bit masks for USBSTS (USB Status Register).
pub mod usbsts {
    pub const HCH: u32 = 1 << 0; // HC Halted: 1 = Halted, 0 = Running
    pub const HSE: u32 = 1 << 2; // Host System Error
    pub const EINT: u32 = 1 << 3; // Event Interrupt
    pub const PCD: u32 = 1 << 4; // Port Change Detect
    pub const CNR: u32 = 1 << 11; // Controller Not Ready: 1 = Not ready
}

/// Bit masks and shifts for PORTSC (Port Status and Control Register).
pub mod portsc {
    pub const CCS: u32 = 1 << 0; // Current Connect Status: 1 = Device connected
    pub const PED: u32 = 1 << 1; // Port Enabled/Disabled
    pub const OCA: u32 = 1 << 3; // Over-current Active
    pub const PR: u32 = 1 << 4; // Port Reset
    pub const PP: u32 = 1 << 9; // Port Power
    pub const CSC: u32 = 1 << 17; // Connect Status Change (W1C)
    pub const PRC: u32 = 1 << 21; // Port Reset Change (W1C)

    pub const PLS_SHIFT: u32 = 5;
    pub const PLS_MASK: u32 = 0xF;

    pub const SPEED_SHIFT: u32 = 10;
    pub const SPEED_MASK: u32 = 0xF;

    pub const SPEED_FULL: u32 = 1; // Full-speed (12 Mb/s)
    pub const SPEED_LOW: u32 = 2; // Low-speed (1.5 Mb/s)
    pub const SPEED_HIGH: u32 = 3; // High-speed (480 Mb/s)
    pub const SPEED_SUPER: u32 = 4; // SuperSpeed (5 Gb/s)
}

/// Bit masks for Interrupter registers (IMAN).
pub mod iman {
    pub const IP: u32 = 1 << 0; // Interrupt Pending
    pub const IE: u32 = 1 << 1; // Interrupt Enable
}

/// Decoded capability parameters from HCSPARAMS1 and HCCPARAMS1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HcParams {
    pub max_slots: u8,
    pub max_intrs: u16,
    pub max_ports: u8,
    pub csz_64: bool,
}

/// Represents the MMIO register interface of an xHCI controller.
pub struct XhciRegisters {
    base: usize,
    cap_len: usize,
    dboff: usize,
    rtsoff: usize,
    params: HcParams,
}

impl XhciRegisters {
    /// Probes and initializes register offsets from the physical MMIO base.
    ///
    /// # Safety
    /// Base must point to valid, MMU-mapped xHCI MMIO memory.
    pub unsafe fn new(base: usize) -> Result<Self, &'static str> {
        let caplength_and_version = core::ptr::read_volatile(base as *const u32);
        let cap_len = (caplength_and_version & 0xFF) as usize;
        let hciversion = (caplength_and_version >> 16) as u16;

        if cap_len == 0 || cap_len > 0x100 {
            return Err("Invalid xHCI CAPLENGTH");
        }

        let hcsparams1 = core::ptr::read_volatile((base + 0x04) as *const u32);
        let max_slots = (hcsparams1 & 0xFF) as u8;
        let max_intrs = ((hcsparams1 >> 8) & 0x7FF) as u16;
        let max_ports = ((hcsparams1 >> 24) & 0xFF) as u8;

        let hccparams1 = core::ptr::read_volatile((base + 0x10) as *const u32);
        let csz_64 = (hccparams1 & (1 << 2)) != 0;

        let dboff = (core::ptr::read_volatile((base + 0x14) as *const u32) & !0x3) as usize;
        let rtsoff = (core::ptr::read_volatile((base + 0x18) as *const u32) & !0x1F) as usize;

        let params = HcParams {
            max_slots,
            max_intrs,
            max_ports,
            csz_64,
        };

        let _ = hciversion;

        Ok(Self {
            base,
            cap_len,
            dboff,
            rtsoff,
            params,
        })
    }

    #[inline]
    pub fn params(&self) -> HcParams {
        self.params
    }

    #[inline]
    pub fn base(&self) -> usize {
        self.base
    }

    // ── Operational Register Accessors (offset from base + cap_len) ──────

    #[inline]
    unsafe fn read_op_u32(&self, offset: usize) -> u32 {
        core::ptr::read_volatile((self.base + self.cap_len + offset) as *const u32)
    }

    #[inline]
    unsafe fn write_op_u32(&self, offset: usize, val: u32) {
        core::ptr::write_volatile((self.base + self.cap_len + offset) as *mut u32, val);
    }

    #[allow(dead_code)]
    #[inline]
    unsafe fn read_op_u64(&self, offset: usize) -> u64 {
        core::ptr::read_volatile((self.base + self.cap_len + offset) as *const u64)
    }

    #[inline]
    unsafe fn write_op_u64(&self, offset: usize, val: u64) {
        core::ptr::write_volatile((self.base + self.cap_len + offset) as *mut u64, val);
    }

    pub fn read_usbcmd(&self) -> u32 {
        unsafe { self.read_op_u32(0x00) }
    }

    pub fn write_usbcmd(&self, val: u32) {
        unsafe { self.write_op_u32(0x00, val) }
    }

    pub fn read_usbsts(&self) -> u32 {
        unsafe { self.read_op_u32(0x04) }
    }

    pub fn write_usbsts(&self, val: u32) {
        unsafe { self.write_op_u32(0x04, val) }
    }

    pub fn set_config_max_slots(&self, max_slots_en: u8) {
        unsafe { self.write_op_u32(0x38, max_slots_en as u32) }
    }

    pub fn set_crcr(&self, val: u64) {
        unsafe { self.write_op_u64(0x18, val) }
    }

    pub fn set_dcbaap(&self, val: u64) {
        unsafe { self.write_op_u64(0x30, val) }
    }

    // ── Port Registers (PORTSC) ──────────────────────────────────────────

    #[inline]
    fn portsc_offset(&self, port: u8) -> usize {
        // Ports are 1-indexed. Port Register Set starts at operational offset 0x400.
        0x400 + ((port as usize - 1) * 0x10)
    }

    pub fn read_portsc(&self, port: u8) -> u32 {
        if port == 0 || port > self.params.max_ports {
            return 0;
        }
        unsafe { self.read_op_u32(self.portsc_offset(port)) }
    }

    pub fn write_portsc(&self, port: u8, val: u32) {
        if port == 0 || port > self.params.max_ports {
            return;
        }
        unsafe { self.write_op_u32(self.portsc_offset(port), val) }
    }

    // ── Runtime Registers (Interrupter 0) ────────────────────────────────

    #[inline]
    fn interrupter0_offset(&self) -> usize {
        self.rtsoff + 0x20
    }

    pub fn set_interrupter0_erstsz(&self, size: u32) {
        unsafe {
            core::ptr::write_volatile(
                (self.base + self.interrupter0_offset() + 0x08) as *mut u32,
                size,
            );
        }
    }

    pub fn set_interrupter0_erstba(&self, addr: u64) {
        unsafe {
            core::ptr::write_volatile(
                (self.base + self.interrupter0_offset() + 0x10) as *mut u64,
                addr,
            );
        }
    }

    pub fn set_interrupter0_erdp(&self, addr: u64) {
        unsafe {
            core::ptr::write_volatile(
                (self.base + self.interrupter0_offset() + 0x18) as *mut u64,
                addr,
            );
        }
    }

    pub fn set_interrupter0_iman(&self, val: u32) {
        unsafe {
            core::ptr::write_volatile((self.base + self.interrupter0_offset()) as *mut u32, val);
        }
    }

    pub fn read_interrupter0_iman(&self) -> u32 {
        unsafe { core::ptr::read_volatile((self.base + self.interrupter0_offset()) as *const u32) }
    }

    // ── Doorbell Registers ───────────────────────────────────────────────

    pub fn ring_doorbell(&self, target_slot: u8, target_endpoint: u8) {
        unsafe {
            let db_ptr = (self.base + self.dboff + (target_slot as usize * 4)) as *mut u32;
            core::ptr::write_volatile(db_ptr, target_endpoint as u32);
        }
    }
}
