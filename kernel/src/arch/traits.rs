//! Hardware Abstraction Layer (HAL) Traits for antOS Kernel.
//!
//! Defines architecture-agnostic contracts for low-level CPU operations:
//! console/serial output, interrupt management, memory management unit (MMU),
//! and userspace system calls.

use core::fmt;

/// Console device abstraction for low-level kernel serial logging.
pub trait ArchConsole: fmt::Write + Send {
    /// Initializes the console device hardware (UART, baud rate, FIFOs).
    fn init(&mut self);
    /// Writes a single byte directly to the serial transmitter.
    fn write_byte(&mut self, byte: u8);
}

/// Hardware interrupt controller and CPU interrupt state management.
pub trait ArchInterrupts {
    /// Enables CPU interrupts (e.g., `sti` on x86, unmask DAIF on ARM).
    fn enable();
    /// Disables CPU interrupts (e.g., `cli` on x86, mask DAIF on ARM).
    fn disable();
    /// Halts CPU execution until the next hardware interrupt arrives (`hlt` or `wfi`).
    fn halt();
    /// Atomically enables interrupts and halts the CPU until the next interrupt (`sti; hlt` or `daifclr; wfi`).
    fn enable_and_halt();
    /// Checks whether hardware interrupts are currently enabled.
    fn are_enabled() -> bool;
}

/// Low-level Memory Management Unit (MMU) operations.
pub trait ArchMmu {
    /// Reads the physical address of the root page table (CR3 on x86, TTBR0_EL1 on ARM).
    fn read_root_table() -> u64;
    /// Flushes / invalidates the TLB entry for a given virtual address.
    fn flush_tlb(virtual_address: u64);
}

/// Userspace system call dispatcher and context switcher.
pub trait ArchSyscall {
    /// Initializes system call mechanism on the CPU (e.g., MSRs on x86, VBAR/SVC on ARM).
    fn init();
}
