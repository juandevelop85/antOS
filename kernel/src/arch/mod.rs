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
