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
/// QEMU / UTM `virt` legacy PCIe ECAM window (used only with `highmem-ecam=off`).
pub const QEMU_ECAM_BASE: usize = 0x3f00_0000;
/// QEMU / UTM `virt` legacy ECAM window size: 16 MiB, buses 0..=15.
pub const QEMU_ECAM_SIZE: usize = 0x0100_0000;
/// QEMU / UTM `virt` high PCIe ECAM window — the default for 64-bit guests
/// (`highmem-ecam` on): 256 MiB at 0x40_1000_0000, buses 0..=255. Mapped by
/// `mmu::map_pcie_window` (T28.2).
pub const QEMU_HIGH_ECAM_BASE: usize = 0x40_1000_0000;

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

/// Reads the vendor id at `bus 0 / dev 0 / func 0` of the ECAM window at
/// `base` through the fault-tolerant probe, returning it only if the window
/// is both mapped and answered by a real host bridge. This is how the kernel
/// tells "mapped, nothing there" (probe returns `0xFFFF`) from "not mapped"
/// (probe returns `None` after swallowing the translation fault) apart.
#[cfg(target_arch = "aarch64")]
fn ecam_window_answers(base: usize) -> bool {
    match crate::arch::aarch64::exceptions::safe_probe_read_u32(base) {
        Some(word) => {
            let vendor = word & 0xFFFF;
            vendor != 0xFFFF && vendor != 0
        }
        None => false,
    }
}

/// Discovers the active PCIe ECAM configuration window and latches it for
/// `read_config_*` to use. Order of preference:
///   1. the `pci-host-ecam-generic` node in the device tree, if the firmware
///      handed one over (QEMU/UTM `virt`);
///   2. the fixed VirtualBox 7 Apple Silicon window (`0xfedd_c000`);
///   3. the fixed QEMU/UTM `virt` windows — the `highmem-ecam` window at
///      `0x40_1000_0000` first, then the legacy `0x3f00_0000` one — now that
///      `mmu::map_pcie_window` maps both (T28.2).
///
/// Each candidate is verified with [`ecam_window_answers`] before it is
/// accepted, so a wrong guess never wedges `scan_pci_bus`.
#[cfg(target_arch = "aarch64")]
pub fn probe_ecam_base() -> usize {
    use core::sync::atomic::Ordering;

    if let Some((dt_base, _bus_lo, _bus_hi)) = crate::arch::aarch64::dtb::find_pcie_ecam() {
        let dt_base = dt_base as usize;
        if ecam_window_answers(dt_base) {
            ACTIVE_ECAM_BASE.store(dt_base, Ordering::Relaxed);
            return dt_base;
        }
    }

    for candidate in [VBOX_ECAM_BASE, QEMU_HIGH_ECAM_BASE, QEMU_ECAM_BASE] {
        if ecam_window_answers(candidate) {
            ACTIVE_ECAM_BASE.store(candidate, Ordering::Relaxed);
            return candidate;
        }
    }

    // Nothing answered anywhere; leave a sane default latched.
    ACTIVE_ECAM_BASE.store(QEMU_HIGH_ECAM_BASE, Ordering::Relaxed);
    QEMU_HIGH_ECAM_BASE
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

/// Decodes one raw BAR slot (`bar_lo`, plus `bar_hi` for a 64-bit BAR — pass
/// `0` when the caller has not read it). Returns the decoded BAR and whether
/// it consumed *two* BAR slots (a 64-bit memory BAR).
///
/// A memory BAR whose address bits are all zero is reported as [`PciBar::None`]:
/// the BAR exists but no firmware has assigned it a window (the usual state on
/// `qemu-system-aarch64 -M virt -kernel <ELF>`, which runs no PCI resource
/// allocator). antOS has no BAR allocator of its own, so such a BAR is not
/// usable — decoding it as `Memory{32,64} { addr: 0 }` would hand a driver a
/// null MMIO base and fault on the first access. The 64-bit case still reports
/// `consumed_two` so the caller's slot walk stays aligned.
pub fn decode_bar(bar_lo: u32, bar_hi: u32) -> (PciBar, bool) {
    if bar_lo == 0 || bar_lo == 0xFFFF_FFFF {
        return (PciBar::None, false);
    }
    if bar_lo & 1 == 1 {
        return (PciBar::Io { port: (bar_lo & 0xFFFC) as u16 }, false);
    }
    let prefetchable = bar_lo & 0x08 != 0;
    let is_64bit = (bar_lo >> 1) & 0x03 == 0x02;
    if is_64bit {
        let addr = ((bar_hi as u64) << 32) | ((bar_lo & 0xFFFF_FFF0) as u64);
        if addr == 0 {
            return (PciBar::None, true);
        }
        (PciBar::Memory64 { addr, prefetchable }, true)
    } else {
        let addr = bar_lo & 0xFFFF_FFF0;
        if addr == 0 {
            return (PciBar::None, false);
        }
        (PciBar::Memory32 { addr, prefetchable }, false)
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
    // The active ECAM window may not be mapped/backed under every
    // emulator/hypervisor (see probe_ecam_base); a fault here just means
    // "no device", matching the 0xFFFF_FFFF convention callers already
    // treat as an absent vendor/device id.
    #[cfg(target_arch = "aarch64")]
    {
        crate::arch::aarch64::exceptions::safe_probe_read_u32(addr).unwrap_or(0xFFFF_FFFF)
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        core::ptr::read_volatile(addr as *const u32)
    }
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
    #[cfg(target_arch = "aarch64")]
    {
        // Read the containing aligned dword through the fault-tolerant
        // probe (see read_config_u32) and extract the requested halfword.
        let aligned = addr & !0b11;
        let shift = (addr & 0b10) * 8;
        let word = crate::arch::aarch64::exceptions::safe_probe_read_u32(aligned)
            .unwrap_or(0xFFFF_FFFF);
        ((word >> shift) & 0xFFFF) as u16
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        core::ptr::read_volatile(addr as *const u16)
    }
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
            let bar_lo = read_config_u32(bus, slot, func, offset);
            let needs_high = bar_lo != 0
                && bar_lo != 0xFFFF_FFFF
                && (bar_lo & 1) == 0
                && ((bar_lo >> 1) & 0x03) == 0x02
                && bar_idx + 1 < 6;
            let bar_hi = if needs_high {
                read_config_u32(bus, slot, func, offset + 4)
            } else {
                0
            };

            let (decoded, consumed_two) = decode_bar(bar_lo, bar_hi);
            bars[bar_idx] = decoded;
            if consumed_two {
                bar_idx += 1; // 64-bit BAR occupies this slot and the next
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

/// Extracts a PCI-to-PCI bridge's secondary bus number (the bus on the far
/// side of the bridge) from the raw dword at config offset `0x18`.
#[inline]
pub fn bridge_secondary_bus(reg_0x18: u32) -> u8 {
    ((reg_0x18 >> 8) & 0xFF) as u8
}

/// Scans the entire PCI bus hierarchy — starting at bus 0 and following every
/// PCI-to-PCI bridge to its secondary bus — and returns all detected
/// functions. A `visited` bitmap keeps a mis-programmed or looping bridge
/// topology from recursing forever.
pub fn scan_pci_bus() -> Vec<PciDevice> {
    let mut devices = Vec::new();
    let mut visited = [false; 256];
    scan_bus_recursive(0, &mut devices, &mut visited);
    devices
}

fn scan_bus_recursive(bus: u8, devices: &mut Vec<PciDevice>, visited: &mut [bool; 256]) {
    if visited[bus as usize] {
        return;
    }
    visited[bus as usize] = true;

    for slot in 0..32 {
        if probe_device(bus, slot, 0).is_none() {
            continue;
        }
        let is_multifunction = unsafe {
            let header_type = (read_config_u32(bus, slot, 0, 0x0C) >> 16) & 0xFF;
            (header_type & 0x80) != 0
        };
        let last_func = if is_multifunction { 7 } else { 0 };

        for func in 0..=last_func {
            let Some(dev) = probe_device(bus, slot, func) else {
                continue;
            };
            // PCI-to-PCI bridge (class 0x06, subclass 0x04): descend.
            let secondary = if dev.class == 0x06 && dev.subclass == 0x04 {
                let sec = unsafe { bridge_secondary_bus(read_config_u32(bus, slot, func, 0x18)) };
                Some(sec)
            } else {
                None
            };
            devices.push(dev);
            if let Some(sec) = secondary {
                if sec != 0 && sec != bus {
                    scan_bus_recursive(sec, devices, visited);
                }
            }
        }
    }
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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecam_offset_layout() {
        // bus in bits 27:20, slot in 19:15, func in 14:12, reg in 11:0.
        assert_eq!(ecam_offset(0, 0, 0, 0), 0);
        assert_eq!(ecam_offset(1, 0, 0, 0), 1 << 20);
        assert_eq!(ecam_offset(0, 3, 0, 0), 3 << 15);
        assert_eq!(ecam_offset(0, 0, 5, 0), 5 << 12);
        assert_eq!(ecam_offset(0, 0, 0, 0x3C), 0x3C);
        assert_eq!(ecam_offset(0, 0, 0, 0x1FFF) & 0xFFF, 0xFFF); // reg masked to 12 bits
        assert_eq!(ecam_offset(2, 6, 1, 0x10), (2 << 20) | (6 << 15) | (1 << 12) | 0x10);
    }

    #[test]
    fn decode_bar_io_and_empty() {
        assert_eq!(decode_bar(0, 0), (PciBar::None, false));
        assert_eq!(decode_bar(0xFFFF_FFFF, 0), (PciBar::None, false));
        assert_eq!(decode_bar(0xC001, 0), (PciBar::Io { port: 0xC000 }, false));
    }

    #[test]
    fn decode_bar_memory32() {
        // 32-bit, non-prefetchable, base 0x1000_0000
        let (bar, two) = decode_bar(0x1000_0000, 0);
        assert_eq!(bar, PciBar::Memory32 { addr: 0x1000_0000, prefetchable: false });
        assert!(!two);
        // prefetchable bit (0x08)
        let (bar, _) = decode_bar(0x1000_0008, 0);
        assert_eq!(bar, PciBar::Memory32 { addr: 0x1000_0000, prefetchable: true });
    }

    #[test]
    fn decode_bar_memory64_consumes_two_slots() {
        // type bits 2:1 == 0b10 -> 64-bit. lo = 0x8000_0004, hi = 0x0000_0001
        let (bar, two) = decode_bar(0x8000_0004, 0x0000_0001);
        assert_eq!(
            bar,
            PciBar::Memory64 { addr: 0x1_8000_0000, prefetchable: false }
        );
        assert!(two);
    }

    #[test]
    fn decode_bar_unprogrammed_memory_is_none() {
        // The BAR type nibble is hardwired, so an unassigned 64-bit MMIO BAR
        // still reads as 0x0000_0004 (type 0b10) with all address bits zero —
        // as on `qemu -M virt -kernel`. It must decode to None (nothing to
        // map) while still consuming the second slot.
        assert_eq!(decode_bar(0x0000_0004, 0), (PciBar::None, true));
        // Unassigned 64-bit prefetchable MMIO BAR: 0x0000_000C.
        assert_eq!(decode_bar(0x0000_000C, 0), (PciBar::None, true));
        // Unassigned 32-bit MMIO BAR: 0x0000_0000 is already covered; the
        // non-zero-type 32-bit form does not exist (bits 2:1 == 0 means 32-bit
        // and bit 0 == 0 means memory, so 0x0 is the only unassigned encoding).
    }

    #[test]
    fn bridge_secondary_bus_extraction() {
        // config 0x18: [primary | secondary | subordinate | latency]
        // secondary bus is byte 1.
        assert_eq!(bridge_secondary_bus(0x0000_0100), 1);
        assert_eq!(bridge_secondary_bus(0x00FF_0A00), 0x0A);
        assert_eq!(bridge_secondary_bus(0x1234_5678), 0x56);
    }
}
