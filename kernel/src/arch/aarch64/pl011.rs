//! PrimeCell UART (PL011) driver for AArch64 (QEMU virt).
//!
//! Maps UART registers via Memory-Mapped I/O (MMIO) at the standard
//! base address `0x0900_0000`.

use core::fmt;
use crate::arch::traits::ArchConsole;

/// Default PL011 base address on QEMU `virt` board.
pub const DEFAULT_PL011_BASE: usize = 0x0900_0000;

/// VirtualBox ARM64 MMIO base address for arm-pl011.
pub const VBOX_PL011_BASE: usize = 0xffdd_e000;

/// Probes whether PL011 is located at QEMU base (0x0900_0000)
/// or VirtualBox base (0xffdd_e000) by inspecting PrimeCell identification registers.
pub fn probe_pl011_base() -> usize {
    unsafe {
        let qemu_id0 = core::ptr::read_volatile((DEFAULT_PL011_BASE + 0xFF0) as *const u32);
        let qemu_id1 = core::ptr::read_volatile((DEFAULT_PL011_BASE + 0xFF4) as *const u32);
        if (qemu_id0 & 0xFF) == 0x0D && (qemu_id1 & 0xFF) == 0xF0 {
            return DEFAULT_PL011_BASE;
        }
    }
    VBOX_PL011_BASE
}

// Register offsets from PL011 Technical Reference Manual:
const UARTDR: usize = 0x000;  // Data Register
const UARTFR: usize = 0x018;  // Flag Register
const UARTIBRD: usize = 0x024; // Integer Baud Rate Divisor
const UARTFBRD: usize = 0x028; // Fractional Baud Rate Divisor
const UARTLCR_H: usize = 0x02C;// Line Control Register
const UARTCR: usize = 0x030;  // Control Register

// Flag register bits:
const FR_RXFE: u32 = 1 << 4; // Receive FIFO empty
const FR_TXFF: u32 = 1 << 5; // Transmit FIFO full

pub struct Pl011Uart {
    base_address: usize,
}

impl Pl011Uart {
    pub const fn new(base_address: usize) -> Self {
        Self { base_address }
    }

    #[inline]
    unsafe fn read_reg(&self, offset: usize) -> u32 {
        core::ptr::read_volatile((self.base_address + offset) as *const u32)
    }

    #[inline]
    unsafe fn write_reg(&self, offset: usize, value: u32) {
        core::ptr::write_volatile((self.base_address + offset) as *mut u32, value);
    }

    /// Initializes the PL011 UART: disables UART, sets 8N1 FIFO mode, and re-enables.
    pub fn init(&mut self) {
        self.base_address = probe_pl011_base();
        unsafe {
            // Disable UART before configuration
            self.write_reg(UARTCR, 0x0);

            // 115200 baud for 24MHz reference clock:
            // 24000000 / (16 * 115200) = 13.0208 -> IBRD=13, FBRD=round(0.0208 * 64) = 1
            self.write_reg(UARTIBRD, 13);
            self.write_reg(UARTFBRD, 1);

            // 8 bits word length, enable FIFOs (bits 5, 6, 4: WLEN=3 (0b11), FEN=1)
            self.write_reg(UARTLCR_H, (1 << 4) | (3 << 5));

            // Enable UART, Transmit and Receive (UARTEN=bit 0, TXE=bit 8, RXE=bit 9)
            self.write_reg(UARTCR, (1 << 0) | (1 << 8) | (1 << 9));
        }
    }

    /// Sends a single byte over the UART. Blocks if transmit FIFO is full (with safety timeout).
    pub fn write_byte(&mut self, byte: u8) {
        unsafe {
            // Wait until transmit FIFO is not full, bounded by timeout to prevent hangs
            let mut timeout = 100_000u32;
            while (self.read_reg(UARTFR) & FR_TXFF) != 0 && timeout > 0 {
                timeout -= 1;
                core::hint::spin_loop();
            }
            self.write_reg(UARTDR, byte as u32);
        }
    }

    /// Reads a single byte from the UART receive FIFO if available.
    /// Non-blocking: returns None if the receive FIFO is empty.
    pub fn read_byte(&mut self) -> Option<u8> {
        unsafe {
            if (self.read_reg(UARTFR) & FR_RXFE) == 0 {
                Some((self.read_reg(UARTDR) & 0xFF) as u8)
            } else {
                None
            }
        }
    }
}

impl ArchConsole for Pl011Uart {
    fn init(&mut self) {
        self.init();
    }

    fn write_byte(&mut self, byte: u8) {
        self.write_byte(byte);
    }
}

impl fmt::Write for Pl011Uart {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(byte);
        }
        Ok(())
    }
}

/// Direct unbuffered console for emergency panic reporting.
pub fn emergency() -> Pl011Uart {
    let mut uart = Pl011Uart::new(DEFAULT_PL011_BASE);
    uart.init();
    uart
}

/// Implementation detail for print! and println! macros on AArch64.
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    let _ = crate::arch::aarch64::SERIAL.lock().write_fmt(args);
}
