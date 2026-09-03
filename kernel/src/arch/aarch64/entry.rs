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
    // Configure stack pointer to end of BOOT_STACK
    adrp x1, BOOT_STACK
    add  x1, x1, :lo12:BOOT_STACK
    add  x1, x1, 65536
    mov  sp, x1
    // Preserves x0 (DTB pointer provided by QEMU/firmware)
    b    kernel_entry
"#
);

#[no_mangle]
pub extern "C" fn kernel_entry(dtb_ptr: u64) -> ! {
    crate::kmain_arm64(dtb_ptr);
}
