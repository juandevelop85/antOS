//! Low-level MMU and TLB operations for AArch64.

use crate::arch::traits::ArchMmu;

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
