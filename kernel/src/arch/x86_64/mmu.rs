//! Low-level MMU operations for x86_64 architecture.

use crate::arch::traits::ArchMmu;

pub const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;

pub struct X86Mmu;

impl ArchMmu for X86Mmu {
    #[inline]
    fn read_root_table() -> u64 {
        let cr3: u64;
        unsafe {
            core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack));
        }
        cr3 & ADDRESS_MASK
    }

    #[inline]
    fn flush_tlb(virtual_address: u64) {
        unsafe {
            core::arch::asm!("invlpg [{}]", in(reg) virtual_address, options(nostack));
        }
    }
}

pub type CurrentMmu = X86Mmu;
