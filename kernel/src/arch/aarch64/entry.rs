//! Bare-metal entry point for AArch64 (ARM 64-bit).
//!
//! Prepares initial stack pointer in EL1 and jumps to `kernel_entry`.

use core::arch::global_asm;

#[no_mangle]
#[link_section = ".bss.stack"]
static mut BOOT_STACK: [u8; 65536] = [0; 65536];

global_asm!(
    r#"
    .section .text._start, "ax"
    .global _start
_start:
    // Preserve DTB pointer in x19
    mov  x19, x0

    // Check CurrentEL (bits [3:2])
    mrs  x0, CurrentEL
    and  x0, x0, #0x0c
    cmp  x0, #0x08
    b.eq drop_from_el2
    cmp  x0, #0x0c
    b.eq drop_from_el3
    b    el1_entry

drop_from_el3:
    // SCR_EL3: RW=1 (EL2/EL1 is AArch64), NS=1 (Non-secure)
    mov  x0, #(1 << 10) | (1 << 0)
    msr  scr_el3, x0
    // SPSR_EL3: EL1h with DAIF masked
    mov  x0, #0x3c5
    msr  spsr_el3, x0
    adr  x0, el1_entry
    msr  elr_el3, x0
    eret

drop_from_el2:
    // HCR_EL2: RW=1 (bit 31 -> EL1 is AArch64)
    mov  x0, #0x80000000
    msr  hcr_el2, x0

    // CNTHCTL_EL2: allow EL1 access to virtual timer without trapping
    mov  x0, #3
    msr  cnthctl_el2, x0
    msr  cntvoff_el2, xzr

    // CPTR_EL2: don't trap FP/SIMD to EL2
    msr  cptr_el2, xzr

    // SPSR_EL2: return to EL1h (0x3c5)
    mov  x0, #0x3c5
    msr  spsr_el2, x0
    adr  x0, el1_entry
    msr  elr_el2, x0
    eret

el1_entry:
    // Enable FP/SIMD (NEON) registers at EL1 and EL0 (CPACR_EL1.FPEN = 0b11)
    mov  x0, #(3 << 20)
    msr  cpacr_el1, x0
    isb

    // Ensure CPU uses SP_EL1 when executing in EL1
    msr  spsel, #1
    // Configure stack pointer to end of BOOT_STACK
    adrp x1, BOOT_STACK
    add  x1, x1, :lo12:BOOT_STACK
    add  x1, x1, 65536
    mov  sp, x1
    // Also initialize SP_EL0 to a valid stack
    msr  sp_el0, x1

    // Restore DTB pointer in x0
    mov  x0, x19
    b    kernel_entry
"#
);

#[no_mangle]
pub extern "C" fn kernel_entry(dtb_ptr: u64) -> ! {
    crate::kmain_arm64(dtb_ptr);
}
