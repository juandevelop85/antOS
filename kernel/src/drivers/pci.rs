//! PCI (Peripheral Component Interconnect) Bus Driver.
//!
//! Provides PCI configuration space enumeration via x86 I/O ports 0xCF8 (CONFIG_ADDRESS)
//! and 0xCFC (CONFIG_DATA), BAR decoding, and bus mastering configuration.

use alloc::vec::Vec;
#[cfg(target_arch = "x86_64")]
use crate::arch::x86_64::port::{inl, inw, outl, outw};

#[cfg(target_arch = "x86_64")]
const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
#[cfg(target_arch = "x86_64")]
const PCI_CONFIG_DATA: u16 = 0xCFC;

/// VirtualBox 7 Apple Silicon PCIe ECAM MMIO window base
pub const VBOX_ECAM_BASE: usize = 0xfedd_c000;
/// QEMU `virt` PCIe ECAM MMIO window base
pub const QEMU_ECAM_BASE: usize = 0x3f00_0000;

/// Calculates the PCIe ECAM memory-mapped offset for a given bus, slot, function, and register offset.
#[inline]
pub fn ecam_offset(bus: u8, slot: u8, func: u8, offset: u8) -> usize {
    ((bus as usize) << 20)
        | ((slot as usize) << 15)
        | ((func as usize) << 12)
        | ((offset as usize) & 0xFFF)
}

#[cfg(target_arch = "aarch64")]
static ACTIVE_ECAM_BASE: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(VBOX_ECAM_BASE);

#[cfg(target_arch = "aarch64")]
pub fn probe_ecam_base() -> usize {
    unsafe {
        let vbox_vendor = core::ptr::read_volatile(VBOX_ECAM_BASE as *const u32) & 0xFFFF;
        if vbox_vendor != 0xFFFF && vbox_vendor != 0 {
            ACTIVE_ECAM_BASE.store(VBOX_ECAM_BASE, core::sync::atomic::Ordering::Relaxed);
            return VBOX_ECAM_BASE;
        }
        ACTIVE_ECAM_BASE.store(QEMU_ECAM_BASE, core::sync::atomic::Ordering::Relaxed);
        QEMU_ECAM_BASE
    }
}

#[cfg(target_arch = "aarch64")]
#[inline]
pub fn get_ecam_base() -> usize {
    ACTIVE_ECAM_BASE.load(core::sync::atomic::Ordering::Relaxed)
}

/// Type and decoded location of a PCI Base Address Register (BAR).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PciBar {
    None,
    Io { port: u16 },
    Memory32 { addr: u32, prefetchable: bool },
    Memory64 { addr: u64, prefetchable: bool },
}

impl PciBar {
    /// Returns the physical memory base address if this BAR is Memory32 or Memory64.
    pub fn memory_address(&self) -> Option<u64> {
        match self {
            PciBar::Memory32 { addr, .. } => Some(*addr as u64),
            PciBar::Memory64 { addr, .. } => Some(*addr),
            _ => None,
        }
    }
}

/// Represents an identified physical or virtual PCI function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PciDevice {
    pub bus: u8,
    pub slot: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub subsystem_vendor_id: u16,
    pub subsystem_device_id: u16,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub irq_line: u8,
    pub bars: [PciBar; 6],
}

impl PciDevice {
    /// Retrieves a decoded BAR by index (0..5).
    pub fn get_bar(&self, index: usize) -> Option<PciBar> {
        self.bars.get(index).copied()
    }

    /// Reads a 32-bit dword from this device's configuration space.
    pub unsafe fn read_config_u32(&self, offset: u8) -> u32 {
        read_config_u32(self.bus, self.slot, self.func, offset)
    }

    /// Writes a 32-bit dword to this device's configuration space.
    pub unsafe fn write_config_u32(&self, offset: u8, value: u32) {
        write_config_u32(self.bus, self.slot, self.func, offset, value)
    }

    /// Reads a 16-bit word from this device's configuration space.
    pub unsafe fn read_config_u16(&self, offset: u8) -> u16 {
        read_config_u16(self.bus, self.slot, self.func, offset)
    }

    /// Writes a 16-bit word to this device's configuration space.
    pub unsafe fn write_config_u16(&self, offset: u8, value: u16) {
        write_config_u16(self.bus, self.slot, self.func, offset, value)
    }

    /// Enables I/O Space, Memory Space, and Bus Mastering (DMA) in the PCI Command register.
    pub fn enable_bus_mastering(&self) {
        unsafe {
            let cmd = self.read_config_u16(0x04);
            // Bit 0: I/O Space, Bit 1: Memory Space, Bit 2: Bus Master
            self.write_config_u16(0x04, cmd | 0x0007);
        }
    }

    /// Returns true if this PCI device belongs to Mass Storage Class (0x01).
    pub fn is_mass_storage(&self) -> bool {
        self.class == 0x01
    }

    /// Returns true if this PCI device is a Serial ATA AHCI 1.0+ Controller (0x01, 0x06, 0x01).
    pub fn is_ahci_controller(&self) -> bool {
        self.class == 0x01 && self.subclass == 0x06 && self.prog_if == 0x01
    }

    /// Returns true if this PCI device is a Non-Volatile Memory (NVMe) Controller (0x01, 0x08, 0x02).
    pub fn is_nvme_controller(&self) -> bool {
        self.class == 0x01 && self.subclass == 0x08 && self.prog_if == 0x02
    }

    /// Returns true if this PCI device is an IDE Controller (0x01, 0x01).
    pub fn is_ide_controller(&self) -> bool {
        self.class == 0x01 && self.subclass == 0x01
    }

    /// Returns true if this PCI device is an xHCI USB 3.0 Controller (0x0C, 0x03, 0x30).
    pub fn is_xhci_controller(&self) -> bool {
        self.class == 0x0C && self.subclass == 0x03 && self.prog_if == 0x30
    }
}

/// Reads a 32-bit dword from PCI configuration space.
#[cfg(target_arch = "x86_64")]
pub unsafe fn read_config_u32(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    let address = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    outl(PCI_CONFIG_ADDRESS, address);
    inl(PCI_CONFIG_DATA)
}

#[cfg(not(target_arch = "x86_64"))]
pub unsafe fn read_config_u32(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    let addr = get_ecam_base() + ecam_offset(bus, slot, func, offset);
    core::ptr::read_volatile(addr as *const u32)
}

/// Writes a 32-bit dword to PCI configuration space.
#[cfg(target_arch = "x86_64")]
pub unsafe fn write_config_u32(bus: u8, slot: u8, func: u8, offset: u8, value: u32) {
    let address = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    outl(PCI_CONFIG_ADDRESS, address);
    outl(PCI_CONFIG_DATA, value);
}

#[cfg(not(target_arch = "x86_64"))]
pub unsafe fn write_config_u32(bus: u8, slot: u8, func: u8, offset: u8, value: u32) {
    let addr = get_ecam_base() + ecam_offset(bus, slot, func, offset);
    core::ptr::write_volatile(addr as *mut u32, value);
}

/// Reads a 16-bit word from PCI configuration space.
#[cfg(target_arch = "x86_64")]
pub unsafe fn read_config_u16(bus: u8, slot: u8, func: u8, offset: u8) -> u16 {
    let address = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    outl(PCI_CONFIG_ADDRESS, address);
    let port = PCI_CONFIG_DATA + ((offset & 2) as u16);
    inw(port)
}

#[cfg(not(target_arch = "x86_64"))]
pub unsafe fn read_config_u16(bus: u8, slot: u8, func: u8, offset: u8) -> u16 {
    let addr = get_ecam_base() + ecam_offset(bus, slot, func, offset);
    core::ptr::read_volatile(addr as *const u16)
}

/// Writes a 16-bit word to PCI configuration space.
#[cfg(target_arch = "x86_64")]
pub unsafe fn write_config_u16(bus: u8, slot: u8, func: u8, offset: u8, value: u16) {
    let address = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    outl(PCI_CONFIG_ADDRESS, address);
    let port = PCI_CONFIG_DATA + ((offset & 2) as u16);
    outw(port, value);
}

#[cfg(not(target_arch = "x86_64"))]
pub unsafe fn write_config_u16(bus: u8, slot: u8, func: u8, offset: u8, value: u16) {
    let addr = get_ecam_base() + ecam_offset(bus, slot, func, offset);
    core::ptr::write_volatile(addr as *mut u16, value);
}

/// Probes a specific PCI device function and extracts its configuration.
pub fn probe_device(bus: u8, slot: u8, func: u8) -> Option<PciDevice> {
    unsafe {
        let id_reg = read_config_u32(bus, slot, func, 0x00);
        let vendor_id = (id_reg & 0xFFFF) as u16;
        if vendor_id == 0xFFFF || vendor_id == 0x0000 {
            return None;
        }

        let device_id = ((id_reg >> 16) & 0xFFFF) as u16;
        let class_reg = read_config_u32(bus, slot, func, 0x08);
        let prog_if = ((class_reg >> 8) & 0xFF) as u8;
        let subclass = ((class_reg >> 16) & 0xFF) as u8;
        let class = ((class_reg >> 24) & 0xFF) as u8;

        let subsys_reg = read_config_u32(bus, slot, func, 0x2C);
        let subsystem_vendor_id = (subsys_reg & 0xFFFF) as u16;
        let subsystem_device_id = ((subsys_reg >> 16) & 0xFFFF) as u16;

        let intr_reg = read_config_u32(bus, slot, func, 0x3C);
        let irq_line = (intr_reg & 0xFF) as u8;

        let mut bars = [PciBar::None; 6];
        let mut bar_idx = 0;
        while bar_idx < 6 {
            let offset = 0x10 + (bar_idx as u8) * 4;
            let bar_val = read_config_u32(bus, slot, func, offset);

            if bar_val != 0 && bar_val != 0xFFFF_FFFF {
                if (bar_val & 1) == 1 {
                    // I/O space BAR
                    let port = (bar_val & 0xFFFC) as u16;
                    bars[bar_idx] = PciBar::Io { port };
                } else {
                    // Memory space BAR
                    let is_64bit = ((bar_val >> 1) & 0x03) == 0x02;
                    let prefetchable = (bar_val & 0x08) != 0;

                    if is_64bit && bar_idx + 1 < 6 {
                        let bar_high = read_config_u32(bus, slot, func, offset + 4);
                        let addr = ((bar_high as u64) << 32) | ((bar_val & 0xFFFF_FFF0) as u64);
                        bars[bar_idx] = PciBar::Memory64 { addr, prefetchable };
                        bar_idx += 1; // skip next 32-bit slot for 64-bit BAR
                    } else {
                        let addr = bar_val & 0xFFFF_FFF0;
                        bars[bar_idx] = PciBar::Memory32 { addr, prefetchable };
                    }
                }
            }
            bar_idx += 1;
        }

        Some(PciDevice {
            bus,
            slot,
            func,
            vendor_id,
            device_id,
            subsystem_vendor_id,
            subsystem_device_id,
            class,
            subclass,
            prog_if,
            irq_line,
            bars,
        })
    }
}

/// Scans the entire PCI bus hierarchy and returns all detected devices.
pub fn scan_pci_bus() -> Vec<PciDevice> {
    let mut devices = Vec::new();

    // Primary bus 0 is standard; scan buses 0..=7 (or 0..=32)
    for bus in 0..=8 {
        for slot in 0..32 {
            if let Some(dev0) = probe_device(bus, slot, 0) {
                let is_multifunction = unsafe {
                    let header_type = (read_config_u32(bus, slot, 0, 0x0C) >> 16) & 0xFF;
                    (header_type & 0x80) != 0
                };

                devices.push(dev0);

                if is_multifunction {
                    for func in 1..8 {
                        if let Some(dev_fn) = probe_device(bus, slot, func) {
                            devices.push(dev_fn);
                        }
                    }
                }
            }
        }
    }

    devices
}

/// Finds the first PCI device matching the given vendor and device IDs.
pub fn find_device(vendor_id: u16, device_id: u16) -> Option<PciDevice> {
    for bus in 0..=8 {
        for slot in 0..32 {
            if let Some(dev) = probe_device(bus, slot, 0) {
                if dev.vendor_id == vendor_id && dev.device_id == device_id {
                    return Some(dev);
                }
                for func in 1..8 {
                    if let Some(fdev) = probe_device(bus, slot, func) {
                        if fdev.vendor_id == vendor_id && fdev.device_id == device_id {
                            return Some(fdev);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Finds all PCI SATA AHCI controllers present on the bus.
pub fn find_ahci_controllers() -> Vec<PciDevice> {
    scan_pci_bus()
        .into_iter()
        .filter(|dev| dev.is_ahci_controller())
        .collect()
}

/// Finds all PCI NVMe controllers present on the bus.
pub fn find_nvme_controllers() -> Vec<PciDevice> {
    scan_pci_bus()
        .into_iter()
        .filter(|dev| dev.is_nvme_controller())
        .collect()
}

/// Finds all PCI Mass Storage controllers (AHCI, NVMe, IDE, SCSI) present on the bus.
pub fn find_storage_controllers() -> Vec<PciDevice> {
    scan_pci_bus()
        .into_iter()
        .filter(|dev| dev.is_mass_storage())
        .collect()
}

