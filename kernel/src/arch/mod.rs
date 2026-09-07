//! Hardware Abstraction Layer (HAL) for antOS Kernel.
//!
//! Exposes unified traits and conditionally selects the active architecture
//! implementation (`x86_64` or `aarch64`).

pub mod traits;

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

#[cfg(target_arch = "x86_64")]
pub use self::x86_64 as current;

#[cfg(target_arch = "aarch64")]
pub mod aarch64;

#[cfg(target_arch = "aarch64")]
pub use self::aarch64 as current;

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::arch::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => ($crate::print!($($arg)*));
}

#[macro_export]
macro_rules! kprintln {
    ($($arg:tt)*) => ($crate::println!($($arg)*));
}

/// Dispatches formatted output to serial and graphical console (if active).
#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    current::serial::_print(args);
    crate::console::_print(args);
}
