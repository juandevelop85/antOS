//! Low-level MMU and Paging for AArch64.
//!
//! Configures MAIR_EL1, TCR_EL1, multi-level translation tables (L0, L1, L2),
//! and enables the MMU with data and instruction caching.

use crate::arch::traits::ArchMmu;

pub const PAGE_SIZE: u64 = 4096;

#[repr(align(4096))]
pub struct PageTable {
    entries: [u64; 512],
}

impl PageTable {
    pub const fn empty() -> Self {
        Self { entries: [0; 512] }
    }
}

// Statically allocated boot translation tables aligned to 4 KB
static mut L0_TABLE: PageTable = PageTable::empty();
static mut L1_TABLE: PageTable = PageTable::empty();
static mut L2_TABLE_PERIPHERALS: PageTable = PageTable::empty();

// Descriptor bitfields
const DESC_TABLE: u64 = 0b11;
const DESC_BLOCK: u64 = 0b01;

// Memory Attributes indexes in MAIR_EL1:
// Attr 0 = Device-nGnRnE (0x00)
// Attr 1 = Normal Write-Back Cacheable (0xFF)
const ATTR_DEVICE: u64 = 0 << 2;
const ATTR_NORMAL: u64 = 1 << 2;

// Access Permissions and flags:
// AP[2:1]: 0b00 = EL1 only, 0b01 = EL1 and EL0 Read/Write
const AP_RW_EL1: u64 = 0 << 6;
const AP_RW_USER: u64 = 1 << 6;
const SH_INNER: u64 = 0b11 << 8;
const SH_OUTER: u64 = 0b10 << 8;
const ACCESS_FLAG: u64 = 1 << 10;
const PXN_FLAG: u64 = 1 << 53;
const UXN_FLAG: u64 = 1 << 54;

pub const USER_SPACE_VIRT: u64 = 0x0040_0000;
pub const USER_SPACE_PHYS: u64 = 0x4100_0000;

const NORMAL_BLOCK_FLAGS: u64 = DESC_BLOCK | ATTR_NORMAL | AP_RW_EL1 | SH_INNER | ACCESS_FLAG | UXN_FLAG;
const USER_BLOCK_FLAGS: u64 = DESC_BLOCK | ATTR_NORMAL | AP_RW_USER | SH_INNER | ACCESS_FLAG | PXN_FLAG;
const DEVICE_BLOCK_FLAGS: u64 = DESC_BLOCK | ATTR_DEVICE | AP_RW_EL1 | SH_OUTER | ACCESS_FLAG | PXN_FLAG | UXN_FLAG;

pub struct ArmMmu;

impl ArchMmu for ArmMmu {
    #[inline]
    fn read_root_table() -> u64 {
        let ttbr0: u64;
        unsafe {
            core::arch::asm!("mrs {}, ttbr0_el1", out(reg) ttbr0, options(nomem, nostack));
        }
        ttbr0 & 0x0000_ffff_ffff_f000
    }

    #[inline]
    fn flush_tlb(virtual_address: u64) {
        let page = virtual_address >> 12;
        unsafe {
            core::arch::asm!(
                "tlbi vaae1is, {}",
                "dsb ish",
                "isb",
                in(reg) page,
                options(nostack)
            );
        }
    }
}

pub type CurrentMmu = ArmMmu;

/// Initializes and enables the AArch64 MMU and data/instruction caches.
pub fn init() {
    unsafe {
        // 1. Setup Table Hierarchy:
        // L0[0] -> L1_TABLE (covers 0..512 GiB)
        let l1_addr = core::ptr::addr_of!(L1_TABLE) as u64;
        L0_TABLE.entries[0] = l1_addr | DESC_TABLE;

        // L1[0] -> L2_TABLE_PERIPHERALS (covers 0..1 GiB)
        let l2_addr = core::ptr::addr_of!(L2_TABLE_PERIPHERALS) as u64;
        L1_TABLE.entries[0] = l2_addr | DESC_TABLE;

        // L1[1] -> 1 GiB Block mapping RAM (0x4000_0000..0x8000_0000) as Normal Cacheable memory
        L1_TABLE.entries[1] = 0x4000_0000 | NORMAL_BLOCK_FLAGS;

        // User Space mapping: 0x0040_0000..0x0060_0000 (Index 2 = 0x0040_0000 / 2MiB)
        // Mapped to physical RAM with EL0 Read/Write/Execute permissions
        L2_TABLE_PERIPHERALS.entries[2] = USER_SPACE_PHYS | USER_BLOCK_FLAGS;

        // L2 mappings for peripherals in 0..1 GiB (each entry covers 2 MiB):
        // GIC at 0x0800_0000..0x0820_0000 (Index 64 = 0x0800_0000 / 2MiB)
        L2_TABLE_PERIPHERALS.entries[64] = 0x0800_0000 | DEVICE_BLOCK_FLAGS;

        // PL011 UART at 0x0900_0000..0x0920_0000 (Index 72 = 0x0900_0000 / 2MiB)
        L2_TABLE_PERIPHERALS.entries[72] = 0x0900_0000 | DEVICE_BLOCK_FLAGS;

        // 2. Configure MAIR_EL1:
        // Attr 0: 0x00 = Device-nGnRnE
        // Attr 1: 0xFF = Normal Memory Write-Back
        let mair: u64 = (0x00 << 0) | (0xFF << 8);
        core::arch::asm!("msr mair_el1, {}", in(reg) mair, options(nomem, nostack));

        // 3. Configure TCR_EL1:
        // T0SZ = 16 (48-bit address space)
        // TG0 = 4KB (0b00)
        // IRGN0 = Normal WB (0b01), ORGN0 = Normal WB (0b01), SH0 = Inner (0b11)
        // IPS = 48-bit PA (0b101)
        let tcr: u64 = 16 | (1 << 8) | (1 << 10) | (3 << 12) | (5u64 << 32);
        core::arch::asm!("msr tcr_el1, {}", in(reg) tcr, options(nomem, nostack));

        // 4. Set TTBR0_EL1 to root L0 table
        let l0_addr = core::ptr::addr_of!(L0_TABLE) as u64;
        core::arch::asm!("msr ttbr0_el1, {}", in(reg) l0_addr, options(nomem, nostack));

        // Synchronize before enabling MMU
        core::arch::asm!("isb", options(nomem, nostack));

        // Invalidate all TLB entries
        core::arch::asm!(
            "tlbi vmalle1is",
            "dsb ish",
            "isb",
            options(nomem, nostack)
        );

        // 5. Enable MMU (M bit = 1), Data Cache (C bit = 1), Instruction Cache (I bit = 1) in SCTLR_EL1
        let mut sctlr: u64;
        core::arch::asm!("mrs {}, sctlr_el1", out(reg) sctlr, options(nomem, nostack));
        sctlr |= (1 << 0) | (1 << 2) | (1 << 12);
        core::arch::asm!(
            "dsb sy",
            "msr sctlr_el1, {}",
            "isb",
            in(reg) sctlr,
            options(nomem, nostack)
        );
    }
}
