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
/// L1 table for the 512 GiB..1024 GiB half of the address space, reached via
/// `L0_TABLE[1]`. Only needed for the QEMU `-M virt` high PCIe MMIO window
/// (`0x80_0000_0000`), where 64-bit device BARs land (T28.2).
static mut L1_TABLE_HIGH: PageTable = PageTable::empty();

/// Fills the `0x1000_0000..0x4000_0000` range of `L2_TABLE_PERIPHERALS` with
/// 2 MiB Device blocks. On the QEMU/UTM `virt` machine this window holds the
/// PCIe 32-bit MMIO BAR area (`0x1000_0000..0x3eff_0000`), the PCIe ECAM
/// configuration space (`0x3f00_0000`, 16 MiB, buses 0..=15) and the fw_cfg
/// block — none of which the kernel mapped before, so `scan_pci_bus` faulted
/// on `virt` and no xHCI was ever found (T28.2). Indices 2/64/72/80 (user
/// window, GIC, PL011, VirtIO-MMIO) sit below `0x1000_0000` and are untouched.
///
/// # Safety
/// Must run with `L2_TABLE_PERIPHERALS` still owned exclusively by the boot
/// path (before the MMU is handed off to the rest of the kernel).
unsafe fn map_pcie_window() {
    let mut idx = (0x1000_0000u64 / 0x20_0000) as usize; // 128
    while idx < 512 {
        L2_TABLE_PERIPHERALS.entries[idx] = ((idx as u64) * 0x20_0000) | DEVICE_BLOCK_FLAGS;
        idx += 1;
    }

    // High PCIe ECAM window: modern QEMU `-M virt` defaults `highmem-ecam` on
    // for 64-bit guests and puts the ECAM at 0x40_1000_0000 (buses 0..=255),
    // not the legacy 0x3f00_0000 window. Map the 1 GiB L1 block that contains
    // it (0x40_0000_0000..0x40_4000_0000, also covers the high GIC redist).
    // L0[0] -> L1_TABLE spans 0..512 GiB, so index 256 == 256 GiB is in range.
    L1_TABLE.entries[256] = 0x40_0000_0000 | DEVICE_BLOCK_FLAGS;
}

/// Installs `L0_TABLE[1]` -> `L1_TABLE_HIGH` and maps the bottom 2 GiB of the
/// QEMU `-M virt` high PCIe MMIO window (`0x80_0000_0000..0x80_8000_0000`) as
/// Device memory. 64-bit device BARs (the `qemu-xhci` controller among them)
/// are allocated from the bottom of this window, and touching an unmapped
/// register there is exactly the Data Abort at `0x80_0000_8000` that T28.2
/// chases. `l0_table_phys` is the physical address of `L0_TABLE` (equal to its
/// virtual address on the direct-boot path, slid on the Limine path).
///
/// # Safety
/// Same contract as [`map_pcie_window`]: boot-path exclusive access to the
/// static tables.
unsafe fn map_high_pcie_mmio(l1_high_phys: u64) {
    L0_TABLE.entries[1] = l1_high_phys | DESC_TABLE;
    // 0x80_0000_0000 is exactly 512 GiB, i.e. index 0 of the 512..1024 GiB
    // half; map the first two 1 GiB blocks.
    L1_TABLE_HIGH.entries[0] = 0x80_0000_0000 | DEVICE_BLOCK_FLAGS;
    L1_TABLE_HIGH.entries[1] = 0x80_4000_0000 | DEVICE_BLOCK_FLAGS;
}

// Descriptor bitfields
const DESC_TABLE: u64 = 0b11;
const DESC_BLOCK: u64 = 0b01;

// Memory Attributes indexes in MAIR_EL1:
// Attr 0 = Normal Write-Back Cacheable (0xFF) - Required by Limine for TTBR1 kernel mappings
// Attr 1 = Normal Write-Back Cacheable (0xFF) - Used by antOS NORMAL_BLOCK_FLAGS
// Attr 2 = Normal Non-Cacheable (0x44)        - Used by antOS FRAMEBUFFER_BLOCK_FLAGS
// Attr 3 = Device-nGnRnE (0x00)               - Used by antOS DEVICE_BLOCK_FLAGS
const _ATTR_NORMAL_0: u64 = 0 << 2;
const ATTR_NORMAL: u64 = 1 << 2;
const ATTR_NON_CACHEABLE: u64 = 2 << 2;
const ATTR_DEVICE: u64 = 3 << 2;

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
const FRAMEBUFFER_BLOCK_FLAGS: u64 = DESC_BLOCK | ATTR_NON_CACHEABLE | AP_RW_EL1 | SH_OUTER | ACCESS_FLAG | PXN_FLAG | UXN_FLAG;

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

        // L1[2] -> 1 GiB Block mapping PCI MMIO32 (0x8000_0000..0xc000_0000) for PCI BARs
        L1_TABLE.entries[2] = 0x8000_0000 | DEVICE_BLOCK_FLAGS;

        // L1[3] -> 1 GiB Block mapping high peripherals (0xc000_0000..0x1_0000_0000) for VirtualBox MMIO (e.g. PL011 at 0xffdde000)
        L1_TABLE.entries[3] = 0xc000_0000 | DEVICE_BLOCK_FLAGS;

        // User Space mapping: 0x0040_0000..0x0060_0000 (Index 2 = 0x0040_0000 / 2MiB)
        // Mapped to physical RAM with EL0 Read/Write/Execute permissions
        L2_TABLE_PERIPHERALS.entries[2] = USER_SPACE_PHYS | USER_BLOCK_FLAGS;

        // L2 mappings for peripherals in 0..1 GiB (each entry covers 2 MiB):
        // GIC at 0x0800_0000..0x0820_0000 (Index 64 = 0x0800_0000 / 2MiB)
        L2_TABLE_PERIPHERALS.entries[64] = 0x0800_0000 | DEVICE_BLOCK_FLAGS;

        // PL011 UART at 0x0900_0000..0x0920_0000 (Index 72 = 0x0900_0000 / 2MiB)
        L2_TABLE_PERIPHERALS.entries[72] = 0x0900_0000 | DEVICE_BLOCK_FLAGS;

        // VirtIO MMIO at 0x0a00_0000..0x0a20_0000 (Index 80 = 0x0a00_0000 / 2MiB)
        L2_TABLE_PERIPHERALS.entries[80] = 0x0a00_0000 | DEVICE_BLOCK_FLAGS;

        // PCIe MMIO32 + ECAM window for QEMU/UTM `virt` (T28.2).
        map_pcie_window();
        map_high_pcie_mmio(core::ptr::addr_of!(L1_TABLE_HIGH) as u64);

        // 2. Configure MAIR_EL1:
        // Attr 0: 0xFF = Normal Memory Write-Back (preserves Limine TTBR1 mappings)
        // Attr 1: 0xFF = Normal Memory Write-Back
        // Attr 2: 0x44 = Normal Memory Non-Cacheable (for linear framebuffer display)
        // Attr 3: 0x00 = Device-nGnRnE
        let mair: u64 = (0xFF << 0) | (0xFF << 8) | (0x44 << 16) | (0x00 << 24);
        core::arch::asm!("msr mair_el1, {}", in(reg) mair, options(nomem, nostack));

        // 3. Configure TCR_EL1:
        // Query hardware physical address range to set IPS properly
        let mut mmfr0: u64;
        core::arch::asm!("mrs {}, id_aa64mmfr0_el1", out(reg) mmfr0, options(nomem, nostack));
        let pa_range = (mmfr0 & 0x7).min(5);

        // T0SZ = 16 (48-bit address space for TTBR0)
        // TG0 = 4KB (0b00)
        // IRGN0 = Normal WB (0b01), ORGN0 = Normal WB (0b01), SH0 = Inner Shareable (0b11)
        // EPD1 = 1 (disable translation walks for TTBR1)
        // IPS = pa_range
        let tcr: u64 = 16 | (1 << 8) | (1 << 10) | (3 << 12) | (1 << 23) | (pa_range << 32);
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
        // Clear WXN (bit 19) and UWXN (bit 20) so EL0 can execute code on writable user pages
        sctlr &= !((1 << 19) | (1 << 20));
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

/// TTBR0-only counterpart to [`init`], for booting under Limine (T27.1).
///
/// Limine hands off with the MMU **already enabled** (`SCTLR_EL1.M = 1`) and
/// `TTBR1_EL1` already pointing at *its own* page tables, which map this
/// kernel at its higher-half link address — the very code executing this
/// function lives there. [`init`] was written for the opposite situation
/// (direct-QEMU-boot, MMU off) and reflects that: it overwrites the whole of
/// `TCR_EL1`, including `EPD1` (bit 23), which disables translation walks
/// for `TTBR1_EL1` — the moment that write retires, the next instruction
/// fetch from this same higher-half code has nothing left to translate it
/// and the machine dies. Limine's protocol explicitly leaves `TTBR0_EL1`
/// (and, before base revision 6, always `TCR_EL1`'s `TTBR0`-related fields)
/// "unspecified, free for the kernel to use" — so this function only ever
/// touches that half: `TCR_EL1` bits `[15:0]` (`T0SZ`/`EPD0`/`IRGN0`/
/// `ORGN0`/`SH0`/`TG0`) and `TTBR0_EL1` itself. `TTBR1_EL1`, `TCR_EL1` bits
/// `[23:16]` (`T1SZ`/`A1`/`EPD1`) and above, and `SCTLR_EL1` are left
/// exactly as Limine configured them.
///
/// The peripheral, user-space and RAM mappings this installs into
/// `TTBR0_EL1` are identical to the ones [`init`] installs — GIC, PL011,
/// VirtIO MMIO and the user-space ELF window all live at the same low
/// addresses either way, only how the kernel itself got mapped differs
/// between the two boot paths.
#[cfg(not(feature = "limine"))]
pub fn init_ttbr0_under_limine() {
    unreachable!("kmain_arm64 only takes this branch when booted_via_limine is true, which requires the `limine` feature that gates the real implementation below");
}

/// Translates a kernel virtual address to its underlying physical address.
/// Under direct-boot (identity-mapped), this is an identity function.
/// Under Limine (higher-half), this subtracts virtual_base and adds physical_base.
pub fn kernel_virt_to_phys(vaddr: u64) -> u64 {
    #[cfg(feature = "limine")]
    {
        let response_ptr = crate::limine::EXECUTABLE_ADDRESS_REQUEST.response;
        if !response_ptr.is_null() {
            let resp = unsafe { &*response_ptr };
            return vaddr.wrapping_sub(resp.virtual_base).wrapping_add(resp.physical_base);
        }
    }
    vaddr
}

#[cfg(feature = "limine")]
pub fn init_ttbr0_under_limine() {
    // `L0_TABLE`, `L1_TABLE` and `L2_TABLE_PERIPHERALS` are statics inside
    // this very kernel image, so `addr_of!` only ever gives their *virtual*
    // address — under direct-QEMU-boot's `init` that happened to equal their
    // physical address too, because the MMU was off, but under Limine the
    // kernel runs at a virtual higher-half address a slide away from where
    // it was physically loaded. TTBR0_EL1 and every table entry below need
    // the physical address, so every one of these gets run through this
    // conversion — skipping even one is exactly the bug this comment is
    // here to keep from recurring (found by reading a QEMU exception trace:
    // a Data Abort writing PL011's UARTCR shortly after enabling TTBR0).
    let response_ptr = crate::limine::EXECUTABLE_ADDRESS_REQUEST.response;
    if response_ptr.is_null() {
        panic!("limine: no executable-address response (unsupported base revision?)");
    }
    let response = unsafe { &*response_ptr };
    let slide_virtual_to_physical =
        |vaddr: u64| vaddr.wrapping_sub(response.virtual_base).wrapping_add(response.physical_base);

    unsafe {
        let l1_addr = slide_virtual_to_physical(core::ptr::addr_of!(L1_TABLE) as u64);
        L0_TABLE.entries[0] = l1_addr | DESC_TABLE;

        let l2_addr = slide_virtual_to_physical(core::ptr::addr_of!(L2_TABLE_PERIPHERALS) as u64);
        L1_TABLE.entries[0] = l2_addr | DESC_TABLE;

        L1_TABLE.entries[1] = 0x4000_0000 | NORMAL_BLOCK_FLAGS;
        L1_TABLE.entries[2] = 0x8000_0000 | DEVICE_BLOCK_FLAGS;
        L1_TABLE.entries[3] = 0xc000_0000 | DEVICE_BLOCK_FLAGS;

        L2_TABLE_PERIPHERALS.entries[2] = USER_SPACE_PHYS | USER_BLOCK_FLAGS;
        L2_TABLE_PERIPHERALS.entries[64] = 0x0800_0000 | DEVICE_BLOCK_FLAGS;
        L2_TABLE_PERIPHERALS.entries[72] = 0x0900_0000 | DEVICE_BLOCK_FLAGS;
        L2_TABLE_PERIPHERALS.entries[80] = 0x0a00_0000 | DEVICE_BLOCK_FLAGS;

        // PCIe MMIO32 + ECAM window for QEMU/UTM `virt` (T28.2).
        map_pcie_window();
        map_high_pcie_mmio(slide_virtual_to_physical(core::ptr::addr_of!(L1_TABLE_HIGH) as u64));

        let mair: u64 = (0xFF << 0) | (0xFF << 8) | (0x44 << 16) | (0x00 << 24);
        core::arch::asm!("msr mair_el1, {}", in(reg) mair, options(nomem, nostack));

        let mut mmfr0: u64;
        core::arch::asm!("mrs {}, id_aa64mmfr0_el1", out(reg) mmfr0, options(nomem, nostack));
        let pa_range = (mmfr0 & 0x7).min(5);
        // Same T0SZ/IRGN0/ORGN0/SH0/TG0 recipe as `init`, but folded into
        // whatever TCR_EL1 Limine already left behind instead of replacing
        // it outright — and with IPS left untouched, since it is shared
        // between both translation regimes and Limine already set it.
        let t0_bits: u64 = 16 | (1 << 8) | (1 << 10) | (3 << 12);
        let mut tcr: u64;
        core::arch::asm!("mrs {}, tcr_el1", out(reg) tcr, options(nomem, nostack));
        tcr = (tcr & !0xFFFF) | t0_bits;
        let _ = pa_range; // IPS intentionally left as Limine set it.
        core::arch::asm!("msr tcr_el1, {}", in(reg) tcr, options(nomem, nostack));

        let l0_addr = slide_virtual_to_physical(core::ptr::addr_of!(L0_TABLE) as u64);
        core::arch::asm!("msr ttbr0_el1, {}", in(reg) l0_addr, options(nomem, nostack));

        core::arch::asm!("isb", options(nomem, nostack));
        // Global, not just TTBR0-scoped: safe even so, because TTBR1's
        // walks stay enabled (EPD1 untouched) and its tables are untouched
        // in memory, so any evicted TTBR1 translation is simply re-walked
        // on next use.
        core::arch::asm!("tlbi vmalle1is", "dsb ish", "isb", options(nomem, nostack));
    }
}

/// Dynamically maps a physical framebuffer memory range into the AArch64 page tables
/// using Normal Non-Cacheable memory attributes (Attr 2).
///
/// Returns the virtual address where the framebuffer is mapped (identity mapped).
pub fn map_framebuffer_range(phys_addr: u64, size: usize) -> Result<u64, ()> {
    unsafe {
        if phys_addr < 0x4000_0000 {
            // First 1 GiB peripheral space: map 2 MiB blocks in L2_TABLE_PERIPHERALS
            let start_2m = phys_addr & !(0x20_0000 - 1);
            let end_2m = (phys_addr + size as u64 + 0x1f_ffff) & !(0x20_0000 - 1);
            let mut cur = start_2m;
            while cur < end_2m && cur < 0x4000_0000 {
                let idx = (cur / 0x20_0000) as usize;
                if idx < 512 {
                    L2_TABLE_PERIPHERALS.entries[idx] = cur | FRAMEBUFFER_BLOCK_FLAGS;
                    ArmMmu::flush_tlb(cur);
                }
                cur += 0x20_0000;
            }
        } else if phys_addr >= 0x4000_0000 && phys_addr < 0x8000_0000 {
            // Already identity mapped in L1_TABLE[1] (0x4000_0000..0x8000_0000)
        } else if phys_addr >= 0x8000_0000 && phys_addr < 0xc000_0000 {
            // 2 GiB..3 GiB: install 1 GiB block in L1_TABLE[2]
            L1_TABLE.entries[2] = 0x8000_0000 | FRAMEBUFFER_BLOCK_FLAGS;
            ArmMmu::flush_tlb(0x8000_0000);
        } else if phys_addr >= 0xc000_0000 && phys_addr < 0x1_0000_0000 {
            // 3 GiB..4 GiB: install 1 GiB block in L1_TABLE[3]
            L1_TABLE.entries[3] = 0xc000_0000 | FRAMEBUFFER_BLOCK_FLAGS;
            ArmMmu::flush_tlb(0xc000_0000);
        }

        core::arch::asm!("isb", options(nomem, nostack));
    }
    Ok(phys_addr)
}

