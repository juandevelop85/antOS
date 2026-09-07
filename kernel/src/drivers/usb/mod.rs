//! Universal Serial Bus (USB) Subsystem for antOS.
//!
//! Provides host controller drivers (xHCI), device enumeration,
//! and class drivers (HID keyboard & mouse).

pub mod descriptor;
pub mod hid;
pub mod xhci;

pub use xhci::{XhciController, PortInfo};

use crate::sync::SpinLock;

/// Global xHCI controller instance (if present on the PCIe bus).
pub static XHCI: SpinLock<Option<XhciController>> = SpinLock::new(None);

/// Probes the PCIe bus for an xHCI USB 3.0 controller, initializes it,
/// and enumerates connected devices.
pub fn init() {
    if let Some(mut controller) = XhciController::probe_and_init() {
        controller.enumerate_connected_ports();
        *XHCI.lock() = Some(controller);
    }
}

/// Polls active USB host controllers for completed transfer events.
pub fn poll() {
    if let Some(ref mut controller) = *XHCI.lock() {
        controller.poll();
    }
}
