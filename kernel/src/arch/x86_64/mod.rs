//! x86_64 architecture support for antOS Kernel.

pub mod apic;
pub mod gdt;
pub mod interrupts;
#[cfg(feature = "limine")]
pub mod limine_boot;
pub mod mmu;
pub mod port;
pub mod serial;
pub mod userspace;

use crate::arch::traits::{ArchConsole, ArchInterrupts, ArchSyscall};

pub use mmu::X86Mmu;

impl ArchConsole for serial::SerialPort {
    fn init(&mut self) {
        self.init();
    }

    fn write_byte(&mut self, byte: u8) {
        self.write_byte(byte);
    }
}

pub struct X86Interrupts;

impl ArchInterrupts for X86Interrupts {
    #[inline]
    fn enable() {
        interrupts::enable();
    }

    #[inline]
    fn disable() {
        interrupts::disable();
    }

    #[inline]
    fn halt() {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }

    #[inline]
    fn enable_and_halt() {
        unsafe {
            core::arch::asm!("sti; hlt", options(nomem, nostack));
        }
    }

    #[inline]
    fn are_enabled() -> bool {
        let flags: u64;
        unsafe {
            core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(preserves_flags));
        }
        flags & (1 << 9) != 0
    }
}

pub type Interrupts = X86Interrupts;

pub struct X86Syscall;

impl ArchSyscall for X86Syscall {
    #[inline]
    fn init() {
        userspace::init();
    }
}
