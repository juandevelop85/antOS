//! AArch64 (ARM 64-bit) architecture support for antOS Kernel.

pub mod dtb;
pub mod entry;
pub mod exceptions;
pub mod gic;
pub mod mmu;
pub mod pl011;
pub mod syscall;
pub mod timer;
pub mod virtio_gpu;

pub use pl011 as serial;

use crate::arch::traits::{ArchInterrupts, ArchSyscall};
use crate::sync::SpinLock;

pub use mmu::ArmMmu;
pub use pl011::Pl011Uart;

/// Global PL011 serial port for AArch64 console logging.
pub static SERIAL: SpinLock<Pl011Uart> = SpinLock::new(Pl011Uart::new(pl011::DEFAULT_PL011_BASE));

pub struct ArmInterrupts;

impl ArchInterrupts for ArmInterrupts {
    #[inline]
    fn enable() {
        exceptions::enable_irq();
    }

    #[inline]
    fn disable() {
        exceptions::disable_irq();
    }

    #[inline]
    fn halt() {
        exceptions::wait_for_interrupt();
    }

    #[inline]
    fn enable_and_halt() {
        exceptions::enable_irq();
        exceptions::wait_for_interrupt();
    }

    #[inline]
    fn are_enabled() -> bool {
        let daif: u64;
        unsafe {
            core::arch::asm!("mrs {}, daif", out(reg) daif, options(nomem, nostack));
        }
        // Bit 7 is I (IRQ mask bit): 0 means enabled
        (daif & (1 << 7)) == 0
    }
}

pub type Interrupts = ArmInterrupts;

pub struct ArmSyscall;

impl ArchSyscall for ArmSyscall {
    #[inline]
    fn init() {
        syscall::ArmSyscall::init();
    }
}
