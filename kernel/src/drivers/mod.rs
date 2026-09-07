//! Device drivers subsystem for antOS.
//!
//! Includes PCI bus enumeration, VirtIO block device driver, and peripheral controllers.

#[cfg(target_arch = "x86_64")]
pub mod pci;
#[cfg(target_arch = "x86_64")]
pub mod storage;
#[cfg(target_arch = "x86_64")]
pub mod virtio_blk;
pub mod virtio_input;
