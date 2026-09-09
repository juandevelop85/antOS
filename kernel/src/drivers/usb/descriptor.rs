//! USB Standard Descriptors and Hierarchy Parser.
//!
//! Covers Device, Configuration, Interface, HID, and Endpoint descriptors
//! conforming to the USB 2.0 and USB HID 1.11 specifications.

use alloc::vec::Vec;

pub const DESC_TYPE_DEVICE: u8 = 1;
pub const DESC_TYPE_CONFIGURATION: u8 = 2;
pub const DESC_TYPE_STRING: u8 = 3;
pub const DESC_TYPE_INTERFACE: u8 = 4;
pub const DESC_TYPE_ENDPOINT: u8 = 5;
pub const DESC_TYPE_HID: u8 = 0x21;
pub const DESC_TYPE_REPORT: u8 = 0x22;

pub const CLASS_HID: u8 = 0x03;
pub const SUBCLASS_BOOT_INTERFACE: u8 = 0x01;
pub const PROTOCOL_KEYBOARD: u8 = 0x01;
pub const PROTOCOL_MOUSE: u8 = 0x02;

/// Standard USB 18-byte Device Descriptor.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub bcd_usb: u16,
    pub device_class: u8,
    pub device_subclass: u8,
    pub device_protocol: u8,
    pub max_packet_size0: u8,
    pub id_vendor: u16,
    pub id_product: u16,
    pub bcd_device: u16,
    pub manufacturer_index: u8,
    pub product_index: u8,
    pub serial_index: u8,
    pub num_configurations: u8,
}

impl DeviceDescriptor {
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 18 || bytes[0] < 18 || bytes[1] != DESC_TYPE_DEVICE {
            return None;
        }
        let mut desc = Self::default();
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), &mut desc as *mut _ as *mut u8, 18);
        }
        Some(desc)
    }
}

/// Standard USB 9-byte Configuration Descriptor.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct ConfigDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub total_length: u16,
    pub num_interfaces: u8,
    pub configuration_value: u8,
    pub configuration_index: u8,
    pub attributes: u8,
    pub max_power: u8,
}

/// Standard USB 9-byte Interface Descriptor.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct InterfaceDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub interface_number: u8,
    pub alternate_setting: u8,
    pub num_endpoints: u8,
    pub interface_class: u8,
    pub interface_subclass: u8,
    pub interface_protocol: u8,
    pub interface_index: u8,
}

/// Standard USB 7-byte Endpoint Descriptor.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct EndpointDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub endpoint_address: u8,
    pub attributes: u8,
    pub max_packet_size: u16,
    pub interval: u8,
}

impl EndpointDescriptor {
    #[inline]
    pub fn is_in(&self) -> bool {
        (self.endpoint_address & 0x80) != 0
    }

    #[inline]
    pub fn endpoint_number(&self) -> u8 {
        self.endpoint_address & 0x0F
    }

    #[inline]
    pub fn is_interrupt(&self) -> bool {
        (self.attributes & 0x03) == 0x03
    }
}

/// Parsed USB HID Interface and its associated Interrupt IN endpoint.
#[derive(Debug, Clone)]
pub struct ParsedHidInterface {
    pub interface_number: u8,
    pub interface_class: u8,
    pub interface_subclass: u8,
    pub interface_protocol: u8,
    pub is_boot_keyboard: bool,
    pub is_boot_mouse: bool,
    pub ep_addr: u8,
    pub ep_max_packet: u16,
    pub ep_interval: u8,
}

/// Coarse classification of a HID Report Descriptor by its top-level usage,
/// used when `bInterfaceProtocol` is 0 (no boot protocol declared) — which is
/// exactly the case for the VirtualBox absolute pointing device, whose report
/// descriptor is a `Generic Desktop / Mouse` collection with 16-bit absolute
/// X/Y. Returns `(looks_like_keyboard, looks_like_mouse)`.
pub fn classify_report_descriptor(report_desc: &[u8]) -> (bool, bool) {
    // Scan for the top-level `Usage Page (Generic Desktop), Usage (x)` pair.
    //   05 01 09 06 -> Keyboard      05 01 09 02 -> Mouse
    //   05 01 09 01 -> Pointer       05 01 09 80 -> System Control
    let mut kbd = false;
    let mut mouse = false;
    let mut i = 0;
    while i + 3 < report_desc.len() {
        if report_desc[i] == 0x05 && report_desc[i + 1] == 0x01 && report_desc[i + 2] == 0x09 {
            match report_desc[i + 3] {
                0x06 => kbd = true,
                0x02 | 0x01 => mouse = true,
                _ => {}
            }
        }
        i += 1;
    }
    (kbd, mouse)
}

/// Parses a raw USB configuration descriptor bundle and extracts all HID interfaces.
pub fn parse_configuration_bundle(data: &[u8]) -> (Option<u8>, Vec<ParsedHidInterface>) {
    if data.len() < 9 || data[1] != DESC_TYPE_CONFIGURATION {
        return (None, Vec::new());
    }

    let config_val = data[5];
    let mut interfaces = Vec::new();

    let mut offset = 0;
    let mut current_iface: Option<InterfaceDescriptor> = None;

    while offset + 2 <= data.len() {
        let len = data[offset] as usize;
        if len == 0 || offset + len > data.len() {
            break;
        }
        let desc_type = data[offset + 1];

        match desc_type {
            DESC_TYPE_INTERFACE => {
                if len >= 9 {
                    let mut iface = InterfaceDescriptor::default();
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            data[offset..].as_ptr(),
                            &mut iface as *mut _ as *mut u8,
                            9,
                        );
                    }
                    current_iface = Some(iface);
                }
            }
            DESC_TYPE_ENDPOINT => {
                if len >= 7 {
                    let mut ep = EndpointDescriptor::default();
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            data[offset..].as_ptr(),
                            &mut ep as *mut _ as *mut u8,
                            7,
                        );
                    }

                    if let Some(iface) = current_iface {
                        if iface.interface_class == CLASS_HID && ep.is_in() && ep.is_interrupt() {
                            crate::println!(
                                "    usb-debug  hid iface {}: class={:#x} sub={:#x} proto={:#x}",
                                iface.interface_number,
                                iface.interface_class,
                                iface.interface_subclass,
                                iface.interface_protocol
                            );
                            let is_keyboard = iface.interface_protocol == PROTOCOL_KEYBOARD
                                || (iface.interface_subclass == SUBCLASS_BOOT_INTERFACE
                                    && iface.interface_protocol == 1);
                            let is_mouse = iface.interface_protocol == PROTOCOL_MOUSE
                                || (iface.interface_subclass == SUBCLASS_BOOT_INTERFACE
                                    && iface.interface_protocol == 2);

                            interfaces.push(ParsedHidInterface {
                                interface_number: iface.interface_number,
                                interface_class: iface.interface_class,
                                interface_subclass: iface.interface_subclass,
                                interface_protocol: iface.interface_protocol,
                                is_boot_keyboard: is_keyboard,
                                is_boot_mouse: is_mouse,
                                ep_addr: ep.endpoint_address,
                                ep_max_packet: ep.max_packet_size,
                                ep_interval: ep.interval,
                            });
                        }
                    }
                }
            }
            _ => {}
        }

        offset += len;
    }

    (Some(config_val), interfaces)
}
