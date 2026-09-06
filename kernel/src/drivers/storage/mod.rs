//! Physical and Virtual Mass Storage Subsystem for antOS.
//!
//! Provides unified device registration, PCI detection, and block operations
//! across AHCI/SATA controllers, NVMe PCIe controllers, and VirtIO-blk devices.

pub mod ahci;
pub mod nvme;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use crate::drivers::pci::PciDevice;
use crate::memory::{FrameAllocator, Mapper};
use crate::println;
use crate::sync::SpinLock;

pub use ahci::{AhciController, AhciDisk};
pub use nvme::{NvmeController, NvmeNamespace};

/// Underlying hardware driver type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverType {
    VirtioBlk,
    AhciSata,
    Nvme,
}

/// Metadata and state describing a detected block device.
#[derive(Debug, Clone)]
pub struct BlockDeviceInfo {
    pub name: String,
    pub driver_type: DriverType,
    pub model: String,
    pub sector_size: usize,
    pub total_sectors: u64,
    pub size_bytes: u64,
    pub read_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageError {
    DeviceNotFound,
    IoError,
    BufferTooSmall,
    ReadOnly,
    Unsupported,
}

/// Global registry of all detected block devices.
pub static BLOCK_DEVICES: SpinLock<Vec<BlockDeviceInfo>> = SpinLock::new(Vec::new());

/// Global storage controllers.
pub static AHCI_CONTROLLERS: SpinLock<Vec<AhciController>> = SpinLock::new(Vec::new());
pub static NVME_CONTROLLERS: SpinLock<Vec<Box<NvmeController>>> = SpinLock::new(Vec::new());

/// Returns a clone of all registered block device metadata.
pub fn list_devices() -> Vec<BlockDeviceInfo> {
    BLOCK_DEVICES.lock().clone()
}

/// Retrieves metadata for a specific block device name (e.g. `/dev/sda`).
pub fn get_device(name: &str) -> Option<BlockDeviceInfo> {
    BLOCK_DEVICES.lock().iter().find(|d| d.name == name).cloned()
}

/// Registers a VirtIO block device into the global device registry.
pub fn register_virtio_device(name: &str, sectors: u64, sector_size: usize, model: &str) {
    let size_bytes = sectors.saturating_mul(sector_size as u64);
    let info = BlockDeviceInfo {
        name: String::from(name),
        driver_type: DriverType::VirtioBlk,
        model: String::from(model),
        sector_size,
        total_sectors: sectors,
        size_bytes,
        read_only: false,
    };
    BLOCK_DEVICES.lock().push(info);
}

/// Detects and initializes all physical AHCI and NVMe mass storage controllers
/// on the PCI bus and registers their storage devices.
pub fn detect_and_init_storage(
    pci_devices: &[PciDevice],
    mapper: &mut Mapper,
    allocator: &mut FrameAllocator,
    phys_offset: u64,
) {
    let mut ahci_disk_idx = 0usize;
    let mut nvme_ctrl_idx = 0usize;

    for dev in pci_devices {
        // 1. Check for Serial ATA AHCI Controller
        if dev.is_ahci_controller() {
            match AhciController::init(dev, mapper, allocator, phys_offset) {
                Ok(ctrl) => {
                    println!(
                        "  ahci         Controlador SATA inicializado en PCI {}:{}.{}",
                        dev.bus, dev.slot, dev.func
                    );

                    for disk in &ctrl.disks {
                        // Letter indexing: 0 -> 'a' (/dev/sda), 1 -> 'b' (/dev/sdb), etc.
                        let disk_letter = (b'a' + (ahci_disk_idx as u8 % 26)) as char;
                        let dev_name = alloc::format!("/dev/sd{}", disk_letter);

                        let info = BlockDeviceInfo {
                            name: dev_name.clone(),
                            driver_type: DriverType::AhciSata,
                            model: disk.model.clone(),
                            sector_size: disk.sector_size,
                            total_sectors: disk.total_sectors,
                            size_bytes: disk.capacity_bytes(),
                            read_only: disk.read_only,
                        };

                        println!(
                            "  storage      Registrado nodo {} · {} ({} MiB)",
                            info.name,
                            info.model,
                            info.size_bytes / (1024 * 1024)
                        );

                        BLOCK_DEVICES.lock().push(info);
                        ahci_disk_idx += 1;
                    }

                    AHCI_CONTROLLERS.lock().push(ctrl);
                }
                Err(e) => {
                    println!(
                        "  advertencia  fallo al inicializar controlador AHCI PCI {}:{}.{}: {:?}",
                        dev.bus, dev.slot, dev.func, e
                    );
                }
            }
        }

        // 2. Check for Non-Volatile Memory (NVMe) Controller
        if dev.is_nvme_controller() {
            match NvmeController::init(dev, mapper, allocator, phys_offset) {
                Ok(ctrl) => {
                    let boxed_ctrl = Box::new(ctrl);
                    println!(
                        "  nvme         Controlador NVMe inicializado en PCI {}:{}.{}",
                        dev.bus, dev.slot, dev.func
                    );

                    for ns in &boxed_ctrl.namespaces {
                        let dev_name = alloc::format!("/dev/nvme{}n{}", nvme_ctrl_idx, ns.nsid);
                        let info = BlockDeviceInfo {
                            name: dev_name.clone(),
                            driver_type: DriverType::Nvme,
                            model: ns.model.clone(),
                            sector_size: ns.block_size,
                            total_sectors: ns.total_blocks,
                            size_bytes: ns.capacity_bytes(),
                            read_only: ns.read_only,
                        };

                        println!(
                            "  storage      Registrado nodo {} · {} ({} MiB)",
                            info.name,
                            info.model,
                            info.size_bytes / (1024 * 1024)
                        );

                        BLOCK_DEVICES.lock().push(info);
                    }

                    NVME_CONTROLLERS.lock().push(boxed_ctrl);
                    nvme_ctrl_idx += 1;
                }
                Err(e) => {
                    println!(
                        "  advertencia  fallo al inicializar controlador NVMe PCI {}:{}.{}: {:?}",
                        dev.bus, dev.slot, dev.func, e
                    );
                }
            }
        }
    }
}

/// Reads blocks from a named block device.
pub fn read_device_blocks(name: &str, lba: u64, count: u16, buf: &mut [u8]) -> Result<(), StorageError> {
    let device = get_device(name).ok_or(StorageError::DeviceNotFound)?;

    match device.driver_type {
        DriverType::VirtioBlk => {
            crate::drivers::virtio_blk::read_blocks(lba, buf).map_err(|_| StorageError::IoError)
        }
        DriverType::AhciSata => {
            let guard = AHCI_CONTROLLERS.lock();
            for ctrl in guard.iter() {
                for disk in &ctrl.disks {
                    let disk_letter = (b'a' + (disk.port_index as u8 % 26)) as char;
                    let target_name = alloc::format!("/dev/sd{}", disk_letter);
                    if target_name == name {
                        return disk.read_sectors(lba, count, buf).map_err(|_| StorageError::IoError);
                    }
                }
            }
            Err(StorageError::DeviceNotFound)
        }
        DriverType::Nvme => {
            let guard = NVME_CONTROLLERS.lock();
            for (ctrl_idx, ctrl) in guard.iter().enumerate() {
                for ns in &ctrl.namespaces {
                    let target_name = alloc::format!("/dev/nvme{}n{}", ctrl_idx, ns.nsid);
                    if target_name == name {
                        return ns.read_blocks(lba, count, buf).map_err(|_| StorageError::IoError);
                    }
                }
            }
            Err(StorageError::DeviceNotFound)
        }
    }
}

/// Writes blocks to a named block device.
pub fn write_device_blocks(name: &str, lba: u64, count: u16, buf: &[u8]) -> Result<(), StorageError> {
    let device = get_device(name).ok_or(StorageError::DeviceNotFound)?;
    if device.read_only {
        return Err(StorageError::ReadOnly);
    }

    match device.driver_type {
        DriverType::VirtioBlk => {
            crate::drivers::virtio_blk::write_blocks(lba, buf).map_err(|_| StorageError::IoError)
        }
        DriverType::AhciSata => {
            let guard = AHCI_CONTROLLERS.lock();
            for ctrl in guard.iter() {
                for disk in &ctrl.disks {
                    let disk_letter = (b'a' + (disk.port_index as u8 % 26)) as char;
                    let target_name = alloc::format!("/dev/sd{}", disk_letter);
                    if target_name == name {
                        return disk.write_sectors(lba, count, buf).map_err(|_| StorageError::IoError);
                    }
                }
            }
            Err(StorageError::DeviceNotFound)
        }
        DriverType::Nvme => {
            let guard = NVME_CONTROLLERS.lock();
            for (ctrl_idx, ctrl) in guard.iter().enumerate() {
                for ns in &ctrl.namespaces {
                    let target_name = alloc::format!("/dev/nvme{}n{}", ctrl_idx, ns.nsid);
                    if target_name == name {
                        return ns.write_blocks(lba, count, buf).map_err(|_| StorageError::IoError);
                    }
                }
            }
            Err(StorageError::DeviceNotFound)
        }
    }
}
