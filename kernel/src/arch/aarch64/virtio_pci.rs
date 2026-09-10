//! Modern VirtIO-over-PCI transport discovery (virtio spec 1.x §4.1).
//!
//! A modern virtio PCI device advertises its register regions through
//! *vendor-specific* PCI capabilities (`cap_vndr == 0x09`). Each one names a
//! BAR plus an offset/length and a `cfg_type` selecting which structure lives
//! there: common config, notify, ISR, device-specific config, or the
//! alternative PCI-config access window. This module owns the capability
//! decoding so it stays unit-testable off-target; the GPU bring-up that uses it
//! is in [`super::virtio_gpu_pci`].

use crate::drivers::pci::{PciBar, PciDevice};

/// `cap_vndr` value marking a PCI vendor-specific capability.
pub const PCI_CAP_ID_VNDR: u8 = 0x09;

/// `cfg_type` values (virtio spec §4.1.4.3).
pub const VIRTIO_PCI_CAP_COMMON_CFG: u8 = 1;
pub const VIRTIO_PCI_CAP_NOTIFY_CFG: u8 = 2;
pub const VIRTIO_PCI_CAP_ISR_CFG: u8 = 3;
pub const VIRTIO_PCI_CAP_DEVICE_CFG: u8 = 4;
pub const VIRTIO_PCI_CAP_PCI_CFG: u8 = 5;

/// PCI vendor id shared by every Red Hat / VirtIO device.
pub const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
/// Modern (1.x) VirtIO-GPU PCI device id.
pub const VIRTIO_GPU_DEVICE_ID: u16 = 0x1050;
/// Transitional VirtIO-GPU PCI device id (`virtio-gpu-pci` on some QEMU builds).
pub const VIRTIO_GPU_DEVICE_ID_LEGACY: u16 = 0x1010;

/// One decoded VirtIO PCI capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtioPciCap {
    pub cfg_type: u8,
    /// BAR index (0..=5) the region lives in.
    pub bar: u8,
    /// Byte offset of the region within that BAR.
    pub offset: u32,
    /// Length of the region in bytes.
    pub length: u32,
    /// `notify_off_multiplier`, present only on a `NOTIFY_CFG` capability.
    pub notify_off_multiplier: Option<u32>,
}

/// Decodes a raw VirtIO PCI capability from up to five little-endian config
/// dwords starting at the capability header. Layout (virtio spec §4.1.4):
///
/// | dword | bytes | fields                                   |
/// |------:|-------|------------------------------------------|
/// | 0     | 0..4  | cap_vndr, cap_next, cap_len, cfg_type     |
/// | 1     | 4..8  | bar, id, padding[2]                       |
/// | 2     | 8..12 | offset (LE u32)                           |
/// | 3     | 12..16| length (LE u32)                          |
/// | 4     | 16..20| notify_off_multiplier (NOTIFY only)      |
pub fn parse_virtio_pci_cap(raw: &[u32]) -> Option<VirtioPciCap> {
    if raw.len() < 4 {
        return None;
    }
    if (raw[0] & 0xFF) as u8 != PCI_CAP_ID_VNDR {
        return None;
    }
    let cfg_type = ((raw[0] >> 24) & 0xFF) as u8;
    let bar = (raw[1] & 0xFF) as u8;
    let offset = raw[2];
    let length = raw[3];
    let notify_off_multiplier = if cfg_type == VIRTIO_PCI_CAP_NOTIFY_CFG && raw.len() >= 5 {
        Some(raw[4])
    } else {
        None
    };
    Some(VirtioPciCap {
        cfg_type,
        bar,
        offset,
        length,
        notify_off_multiplier,
    })
}

/// The register regions of a modern VirtIO PCI device, resolved to absolute
/// MMIO addresses.
#[derive(Debug, Clone, Copy, Default)]
pub struct VirtioPciRegions {
    pub common_cfg: u64,
    pub notify_base: u64,
    pub notify_off_multiplier: u32,
    pub isr_cfg: u64,
    pub device_cfg: u64,
}

impl VirtioPciRegions {
    pub fn is_complete(&self) -> bool {
        self.common_cfg != 0 && self.notify_base != 0 && self.device_cfg != 0
    }
}

/// Walks a PCI device's capability list and resolves every VirtIO capability
/// region to an absolute address using the device's decoded BARs.
///
/// # Safety
/// Reads the device's PCI configuration space.
pub unsafe fn discover_regions(dev: &PciDevice) -> Option<VirtioPciRegions> {
    // Capabilities List bit in the Status register (offset 0x06, bit 4).
    let status = dev.read_config_u16(0x06);
    if status & (1 << 4) == 0 {
        return None;
    }

    let mut regions = VirtioPciRegions::default();
    let mut ptr = (dev.read_config_u16(0x34) & 0xFF) as u8; // Capabilities Pointer
    let mut guard = 48u8; // bounded walk

    while ptr != 0 && ptr != 0xFF && guard > 0 {
        guard -= 1;
        let aligned = ptr & !0x3;
        let d0 = dev.read_config_u32(aligned);
        let mut dwords = [0u32; 5];
        for (i, slot) in dwords.iter_mut().enumerate() {
            *slot = dev.read_config_u32(aligned.wrapping_add((i as u8) * 4));
        }

        if let Some(cap) = parse_virtio_pci_cap(&dwords) {
            if let Some(bar_addr) = bar_base(dev, cap.bar) {
                let region = bar_addr + cap.offset as u64;
                match cap.cfg_type {
                    VIRTIO_PCI_CAP_COMMON_CFG => regions.common_cfg = region,
                    VIRTIO_PCI_CAP_NOTIFY_CFG => {
                        regions.notify_base = region;
                        regions.notify_off_multiplier = cap.notify_off_multiplier.unwrap_or(0);
                    }
                    VIRTIO_PCI_CAP_ISR_CFG => regions.isr_cfg = region,
                    VIRTIO_PCI_CAP_DEVICE_CFG => regions.device_cfg = region,
                    _ => {}
                }
            }
        }

        // cap_next is byte 1 of the capability header.
        let next = ((d0 >> 8) & 0xFF) as u8;
        if next == ptr {
            break;
        }
        ptr = next;
    }

    if regions.is_complete() {
        Some(regions)
    } else {
        None
    }
}

fn bar_base(dev: &PciDevice, index: u8) -> Option<u64> {
    match dev.get_bar(index as usize)? {
        PciBar::Memory32 { addr, .. } => Some(addr as u64),
        PciBar::Memory64 { addr, .. } => Some(addr),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds the five config dwords for a capability from its byte fields.
    fn cap_dwords(
        cap_next: u8,
        cap_len: u8,
        cfg_type: u8,
        bar: u8,
        offset: u32,
        length: u32,
        notify_mul: u32,
    ) -> [u32; 5] {
        [
            (PCI_CAP_ID_VNDR as u32)
                | ((cap_next as u32) << 8)
                | ((cap_len as u32) << 16)
                | ((cfg_type as u32) << 24),
            bar as u32,
            offset,
            length,
            notify_mul,
        ]
    }

    #[test_case]
    fn parses_common_cfg_capability() {
        let raw = cap_dwords(0x50, 16, VIRTIO_PCI_CAP_COMMON_CFG, 4, 0x0000, 0x1000, 0);
        let cap = parse_virtio_pci_cap(&raw).expect("valid cap");
        assert_eq!(cap.cfg_type, VIRTIO_PCI_CAP_COMMON_CFG);
        assert_eq!(cap.bar, 4);
        assert_eq!(cap.offset, 0);
        assert_eq!(cap.length, 0x1000);
        assert_eq!(cap.notify_off_multiplier, None);
    }

    #[test_case]
    fn parses_notify_capability_with_multiplier() {
        let raw = cap_dwords(0x60, 20, VIRTIO_PCI_CAP_NOTIFY_CFG, 4, 0x3000, 0x1000, 4);
        let cap = parse_virtio_pci_cap(&raw).expect("valid cap");
        assert_eq!(cap.cfg_type, VIRTIO_PCI_CAP_NOTIFY_CFG);
        assert_eq!(cap.offset, 0x3000);
        assert_eq!(cap.notify_off_multiplier, Some(4));
    }

    #[test_case]
    fn rejects_non_vendor_capability_and_short_input() {
        // cap_vndr = 0x05 (MSI) instead of 0x09.
        let mut raw = cap_dwords(0, 8, 0, 0, 0, 0, 0);
        raw[0] = (raw[0] & !0xFF) | 0x05;
        assert!(parse_virtio_pci_cap(&raw).is_none());
        assert!(parse_virtio_pci_cap(&[0x09]).is_none());
    }

    #[test_case]
    fn regions_completeness_requires_the_three_essential_windows() {
        let mut r = VirtioPciRegions::default();
        assert!(!r.is_complete());
        r.common_cfg = 0x1000_0000;
        r.notify_base = 0x1000_3000;
        assert!(!r.is_complete());
        r.device_cfg = 0x1000_4000;
        assert!(r.is_complete());
    }
}
