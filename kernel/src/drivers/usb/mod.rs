//! Universal Serial Bus (USB) Subsystem for antOS.
//!
//! Provides host controller drivers (xHCI), device enumeration,
//! and class drivers (HID keyboard & mouse).

pub mod xhci;

pub use xhci::{XhciController, PortInfo};

use crate::sync::SpinLock;

/// Global xHCI controller instance (if present on the PCIe bus).
pub static XHCI: SpinLock<Option<XhciController>> = SpinLock::new(None);

/// Probes the PCIe bus for an xHCI USB 3.0 controller and initializes it.
pub fn init() {
    if let Some(controller) = XhciController::probe_and_init() {
        *XHCI.lock() = Some(controller);
    }
}
