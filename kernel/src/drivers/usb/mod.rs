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

/// Renders a compact, human-readable dump of the enumerated USB HID devices and
/// their live interrupt-report counters. Wired into `SYS_SYSINFO` so the
/// sovereign shell's `info` command can show it without access to the boot log.
pub fn debug_report() -> alloc::string::String {
    use core::fmt::Write as _;

    let mut out = alloc::string::String::new();
    let guard = XHCI.lock();
    let Some(ctrl) = guard.as_ref() else {
        let _ = writeln!(out, "usb: sin controlador xHCI");
        return out;
    };

    let _ = writeln!(
        out,
        "usb: xHCI {}:{}.{} · {} interfaz(es) HID",
        ctrl.pci_device.bus,
        ctrl.pci_device.slot,
        ctrl.pci_device.func,
        ctrl.devices.len()
    );

    for d in &ctrl.devices {
        let kind = if d.is_keyboard {
            "teclado"
        } else if d.is_mouse {
            "raton  "
        } else {
            "hid    "
        };
        let n = (d.last_report_len as usize).min(d.last_report.len());
        let _ = writeln!(
            out,
            "  slot {} pto {} vel {} {} dci {} mps {} cls {:#04x} proto {:#04x} ev {} last {:02x?}",
            d.slot_id,
            d.port,
            d.speed,
            kind,
            d.ep_int_dci,
            d.ep_int_max_packet,
            d.iface_class,
            d.iface_protocol,
            d.report_events,
            &d.last_report[..n]
        );
    }

    out
}
