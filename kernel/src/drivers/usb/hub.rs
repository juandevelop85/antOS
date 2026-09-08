//! USB Hub (class 0x09) descriptor parsing and topology helpers.
//!
//! The xHCI slot context addresses a device deep in the bus topology with a
//! 20-bit *route string* (xHCI 1.2 §8.9): four bits per hub tier, tier 1 in
//! bits [3:0] up to tier 5 in bits [19:16], each nibble holding the 1-based
//! downstream port number that leads toward the device. This module owns the
//! route-string arithmetic and the parser for the Hub Descriptor returned by a
//! class GET_DESCRIPTOR request, so both stay unit-testable off-target.

/// `bDeviceClass` / `bInterfaceClass` for a USB hub.
pub const CLASS_HUB: u8 = 0x09;

/// `bDescriptorType` for the (USB 2.0) Hub Descriptor.
pub const DESC_TYPE_HUB: u8 = 0x29;

/// Hub-class request: `bmRequestType` for a device-to-host class request on the
/// hub itself (GET_DESCRIPTOR / GET_STATUS of the hub).
pub const REQ_TYPE_HUB_IN: u8 = 0xA0;
/// Hub-class request: host-to-device, directed at a downstream port.
pub const REQ_TYPE_PORT_OUT: u8 = 0x23;
/// Hub-class request: device-to-host, directed at a downstream port (GET_STATUS).
pub const REQ_TYPE_PORT_IN: u8 = 0xA3;

/// Standard `bRequest` codes reused by the hub class.
pub const REQ_GET_STATUS: u8 = 0x00;
pub const REQ_CLEAR_FEATURE: u8 = 0x01;
pub const REQ_SET_FEATURE: u8 = 0x03;
pub const REQ_GET_DESCRIPTOR: u8 = 0x06;

/// Downstream-port feature selectors (USB 2.0 §11.24.2).
pub const PORT_CONNECTION: u16 = 0;
pub const PORT_ENABLE: u16 = 1;
pub const PORT_RESET: u16 = 4;
pub const PORT_POWER: u16 = 8;
pub const C_PORT_CONNECTION: u16 = 16;
pub const C_PORT_ENABLE: u16 = 17;
pub const C_PORT_SUSPEND: u16 = 18;
pub const C_PORT_OVER_CURRENT: u16 = 19;
pub const C_PORT_RESET: u16 = 20;

/// Bits of the `wPortStatus` word returned by GET_STATUS on a downstream port.
pub mod port_status {
    pub const CONNECTION: u16 = 1 << 0;
    pub const ENABLE: u16 = 1 << 1;
    pub const SUSPEND: u16 = 1 << 2;
    pub const OVER_CURRENT: u16 = 1 << 3;
    pub const RESET: u16 = 1 << 4;
    pub const POWER: u16 = 1 << 8;
    pub const LOW_SPEED: u16 = 1 << 9;
    pub const HIGH_SPEED: u16 = 1 << 10;
}

/// Maximum hub tiers the route string can encode (xHCI 1.2 §8.9).
pub const MAX_HUB_TIERS: usize = 5;

/// Parsed USB 2.0 Hub Descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HubDescriptor {
    /// `bNbrPorts`: number of downstream ports on this hub.
    pub num_ports: u8,
    /// `wHubCharacteristics`.
    pub characteristics: u16,
    /// `bPwrOn2PwrGood`, expressed in 2 ms units.
    pub power_on_2_power_good: u8,
    /// `bHubContrCurrent`, hub controller current draw in mA.
    pub hub_control_current: u8,
}

impl HubDescriptor {
    /// Parses the fixed head of a Hub Descriptor. The trailing `DeviceRemovable`
    /// / `PortPwrCtrlMask` bitmaps are variable-length and not needed here.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 7 || bytes[0] < 7 || bytes[1] != DESC_TYPE_HUB {
            return None;
        }
        Some(Self {
            num_ports: bytes[2],
            characteristics: u16::from_le_bytes([bytes[3], bytes[4]]),
            power_on_2_power_good: bytes[5],
            hub_control_current: bytes[6],
        })
    }

    /// Port power-on settling time in milliseconds (`bPwrOn2PwrGood * 2`), with
    /// a 10 ms floor for hubs that under-report.
    pub fn power_on_delay_ms(&self) -> u32 {
        ((self.power_on_2_power_good as u32) * 2).max(10)
    }

    /// `true` when the hub is part of a compound device (bit 2 of the
    /// characteristics word).
    pub fn is_compound(&self) -> bool {
        (self.characteristics & (1 << 2)) != 0
    }
}

/// Builds a route string from the ordered list of downstream port numbers that
/// lead from the root hub toward the device (tier 1 first). Ports beyond
/// [`MAX_HUB_TIERS`] and nibble overflow (port > 15) are clamped, matching the
/// hardware field width.
pub fn route_string(path: &[u8]) -> u32 {
    let mut rs = 0u32;
    for (tier, &port) in path.iter().take(MAX_HUB_TIERS).enumerate() {
        let nibble = (port as u32).min(15) & 0xF;
        rs |= nibble << (tier * 4);
    }
    rs
}

/// Returns `base` with the nibble for `tier` (0-based) replaced by `port`.
/// Used to extend a parent hub's route string by one downstream hop.
pub fn route_string_append(base: u32, tier: usize, port: u8) -> u32 {
    if tier >= MAX_HUB_TIERS {
        return base;
    }
    let shift = tier * 4;
    let nibble = (port as u32).min(15) & 0xF;
    (base & !(0xF << shift)) | (nibble << shift)
}

/// Number of hub tiers already encoded in `route_string` — i.e. the tier index a
/// hub at this route occupies when appending its own downstream ports. Since
/// active tiers always carry a non-zero (1-based) port nibble, this is the
/// position past the highest non-zero nibble.
pub fn route_depth(route_string: u32) -> usize {
    for tier in (0..MAX_HUB_TIERS).rev() {
        if (route_string >> (tier * 4)) & 0xF != 0 {
            return tier + 1;
        }
    }
    0
}

/// Extracts the negotiated device speed code (matching `portsc::SPEED_*`) from a
/// downstream port's `wPortStatus` word.
pub fn speed_from_port_status(status: u16) -> u8 {
    use crate::drivers::usb::xhci::registers::portsc;
    if (status & port_status::LOW_SPEED) != 0 {
        portsc::SPEED_LOW as u8
    } else if (status & port_status::HIGH_SPEED) != 0 {
        portsc::SPEED_HIGH as u8
    } else {
        portsc::SPEED_FULL as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_hub_descriptor() -> [u8; 9] {
        // bLength=9, bDescriptorType=0x29, bNbrPorts=4,
        // wHubCharacteristics=0x00AC (bit 2 = compound device), bPwrOn2PwrGood=50 (100 ms),
        // bHubContrCurrent=100, DeviceRemovable byte, PortPwrCtrlMask byte.
        [9, 0x29, 4, 0xAC, 0x00, 50, 100, 0x00, 0xFF]
    }

    #[test]
    fn parses_hub_descriptor_head() {
        let d = HubDescriptor::from_bytes(&sample_hub_descriptor()).expect("valid descriptor");
        assert_eq!(d.num_ports, 4);
        assert_eq!(d.characteristics, 0x00AC);
        assert_eq!(d.power_on_2_power_good, 50);
        assert_eq!(d.hub_control_current, 100);
        assert_eq!(d.power_on_delay_ms(), 100);
        assert!(d.is_compound());
    }

    #[test]
    fn rejects_wrong_descriptor_type_or_short_buffer() {
        assert!(HubDescriptor::from_bytes(&[]).is_none());
        assert!(HubDescriptor::from_bytes(&[9, 0x02, 4, 0, 0, 0, 0]).is_none());
        assert!(HubDescriptor::from_bytes(&[3, 0x29, 4]).is_none());
    }

    #[test]
    fn power_on_delay_has_a_floor() {
        let d = HubDescriptor {
            num_ports: 2,
            characteristics: 0,
            power_on_2_power_good: 1, // 2 ms reported
            hub_control_current: 0,
        };
        assert_eq!(d.power_on_delay_ms(), 10);
    }

    #[test]
    fn route_string_packs_four_bits_per_tier() {
        assert_eq!(route_string(&[]), 0);
        assert_eq!(route_string(&[3]), 0x3);
        assert_eq!(route_string(&[3, 5]), 0x53);
        assert_eq!(route_string(&[1, 2, 3, 4, 5]), 0x5_4321);
    }

    #[test]
    fn route_string_clamps_depth_and_width() {
        // Sixth tier is dropped.
        assert_eq!(route_string(&[1, 1, 1, 1, 1, 1]), 0x1_1111);
        // Port numbers above 15 saturate to the nibble maximum.
        assert_eq!(route_string(&[20]), 0xF);
    }

    #[test]
    fn route_string_append_extends_parent_path() {
        let parent = route_string(&[3, 5]); // 0x53, hub sits at tier 2
        let child = route_string_append(parent, 2, 7);
        assert_eq!(child, 0x753);
        // Re-appending at the same tier overwrites rather than ORs.
        assert_eq!(route_string_append(child, 2, 2), 0x253);
        // Out-of-range tier is a no-op.
        assert_eq!(route_string_append(parent, MAX_HUB_TIERS, 4), parent);
    }

    #[test]
    fn route_depth_counts_encoded_tiers() {
        assert_eq!(route_depth(0), 0);
        assert_eq!(route_depth(route_string(&[3])), 1);
        assert_eq!(route_depth(route_string(&[3, 5])), 2);
        assert_eq!(route_depth(route_string(&[1, 2, 3, 4, 5])), 5);
        // A child appended at the reported depth lands one tier deeper.
        let hub = route_string(&[3, 5]);
        let child = route_string_append(hub, route_depth(hub), 7);
        assert_eq!(child, route_string(&[3, 5, 7]));
    }

    #[test]
    fn speed_decodes_from_port_status_bits() {
        use crate::drivers::usb::xhci::registers::portsc;
        assert_eq!(
            speed_from_port_status(port_status::CONNECTION | port_status::LOW_SPEED),
            portsc::SPEED_LOW as u8
        );
        assert_eq!(
            speed_from_port_status(port_status::CONNECTION | port_status::HIGH_SPEED),
            portsc::SPEED_HIGH as u8
        );
        assert_eq!(
            speed_from_port_status(port_status::CONNECTION),
            portsc::SPEED_FULL as u8
        );
    }
}
