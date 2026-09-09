//! Serial ATA Advanced Host Controller Interface (AHCI 1.0+) Driver.
//!
//! Provides hardware-level MMIO access to AHCI HBA controllers, port enumeration,
//! SATA drive detection (ATA signature 0x00000101), DMA command list / FIS allocation,
//! ATA IDENTIFY device inquiry, and LBA48 sector read/write operations.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{compiler_fence, Ordering};

use crate::drivers::pci::PciDevice;
use crate::memory::{FrameAllocator, Mapper, PRESENT, WRITABLE};
use crate::println;

pub const SECTOR_SIZE: usize = 512;

/// Signature for standard ATA hard disk / SSD.
pub const SATA_SIG_ATA: u32 = 0x0000_0101;
/// Signature for ATAPI optical drive.
pub const SATA_SIG_ATAPI: u32 = 0xEB14_0101;
/// Signature for enclosure management bridge.
pub const SATA_SIG_SEMB: u32 = 0xC33C_0101;
/// Signature for port multiplier.
pub const SATA_SIG_PM: u32 = 0x9669_0101;

// ATA Command Opcodes
const ATA_CMD_READ_DMA_EXT: u8 = 0x25;
const ATA_CMD_WRITE_DMA_EXT: u8 = 0x35;
const ATA_CMD_IDENTIFY: u8 = 0xEC;

// FIS Types
const FIS_TYPE_REG_H2D: u8 = 0x27;

// Port Command & Status Bits
const PORT_CMD_ST: u32 = 1 << 0; // Start
#[allow(dead_code)]
const PORT_CMD_SUD: u32 = 1 << 1; // Spin-up device
#[allow(dead_code)]
const PORT_CMD_POD: u32 = 1 << 2; // Power on device
const PORT_CMD_FRE: u32 = 1 << 4; // FIS Receive Enable
const PORT_CMD_FR: u32 = 1 << 14; // FIS Receive Running
const PORT_CMD_CR: u32 = 1 << 15; // Command List Running

// Port Task File Data Bits
const PORT_TFD_ERR: u32 = 1 << 0;
const PORT_TFD_DRQ: u32 = 1 << 3;
const PORT_TFD_BSY: u32 = 1 << 7;

// Global Host Control Bits
#[allow(dead_code)]
const GHC_HR: u32 = 1 << 0; // HBA Reset
#[allow(dead_code)]
const GHC_IE: u32 = 1 << 1; // Interrupt Enable
const GHC_AE: u32 = 1 << 31; // AHCI Enable

#[repr(C)]
struct HbaPortRegisters {
    clb: u32,
    clbu: u32,
    fb: u32,
    fbu: u32,
    is: u32,
    ie: u32,
    cmd: u32,
    reserved0: u32,
    tfd: u32,
    sig: u32,
    ssts: u32,
    sctl: u32,
    serr: u32,
    sact: u32,
    ci: u32,
    sntf: u32,
    fbs: u32,
    reserved1: [u32; 11],
    vendor: [u32; 4],
}

#[repr(C)]
struct HbaMemoryRegisters {
    cap: u32,
    ghc: u32,
    is: u32,
    pi: u32,
    vs: u32,
    ccc_ctl: u32,
    ccc_pts: u32,
    em_loc: u32,
    em_ctl: u32,
    cap2: u32,
    bohc: u32,
    reserved: [u8; 0xA0 - 0x2C],
    vendor: [u8; 0x100 - 0xA0],
    ports: [HbaPortRegisters; 32],
}

#[repr(C, packed)]
struct HbaCmdHeader {
    flags: u16,
    prdtl: u16,
    prdbc: u32,
    ctba: u32,
    ctbau: u32,
    reserved: [u32; 4],
}

#[repr(C, packed)]
struct HbaPrdtEntry {
    dba: u32,
    dbau: u32,
    reserved: u32,
    dbc: u32,
}

#[repr(C, packed)]
struct FisRegH2D {
    fis_type: u8,
    pmport_c: u8,
    command: u8,
    features_low: u8,
    lba0: u8,
    lba1: u8,
    lba2: u8,
    device: u8,
    lba3: u8,
    lba4: u8,
    lba5: u8,
    features_high: u8,
    count_low: u8,
    count_high: u8,
    icc: u8,
    control: u8,
    reserved: [u8; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AhciError {
    ControllerInitFailed,
    PortNotActive,
    UnsupportedDevice,
    DmaAllocFailed,
    MmioMapFailed,
    DeviceBusy,
    CommandTimeout,
    IoError,
    BufferTooSmall,
}

/// Represents an identified SATA drive attached to an AHCI port.
pub struct AhciDisk {
    pub port_index: usize,
    pub model: String,
    pub serial: String,
    pub total_sectors: u64,
    pub sector_size: usize,
    pub read_only: bool,
    port_mmio: *mut HbaPortRegisters,
    dma_phys: u64,
    dma_virt: *mut u8,
}

unsafe impl Send for AhciDisk {}
unsafe impl Sync for AhciDisk {}

impl AhciDisk {
    /// Returns capacity in bytes.
    pub fn capacity_bytes(&self) -> u64 {
        self.total_sectors.saturating_mul(self.sector_size as u64)
    }

    /// Reads `count` sectors starting at `lba` into `buf`.
    pub fn read_sectors(&self, lba: u64, count: u16, buf: &mut [u8]) -> Result<(), AhciError> {
        let total_bytes = (count as usize) * self.sector_size;
        if buf.len() < total_bytes {
            return Err(AhciError::BufferTooSmall);
        }

        let mut current_lba = lba;
        let mut sectors_left = count;
        let mut offset = 0;

        // Process in chunks up to 16 sectors (8 KiB) using the DMA buffer
        while sectors_left > 0 {
            let chunk_sectors = sectors_left.min(8); // 8 sectors = 4096 bytes
            let chunk_bytes = (chunk_sectors as usize) * self.sector_size;

            self.execute_command(current_lba, chunk_sectors, ATA_CMD_READ_DMA_EXT, false)?;

            // Copy chunk from DMA buffer to user buffer
            unsafe {
                let bounce_buf = self.dma_virt.add(2048);
                core::ptr::copy_nonoverlapping(
                    bounce_buf,
                    buf.as_mut_ptr().add(offset),
                    chunk_bytes,
                );
            }

            sectors_left -= chunk_sectors;
            current_lba += chunk_sectors as u64;
            offset += chunk_bytes;
        }

        Ok(())
    }

    /// Writes `count` sectors starting at `lba` from `buf`.
    pub fn write_sectors(&self, lba: u64, count: u16, buf: &[u8]) -> Result<(), AhciError> {
        if self.read_only {
            return Err(AhciError::IoError);
        }
        let total_bytes = (count as usize) * self.sector_size;
        if buf.len() < total_bytes {
            return Err(AhciError::BufferTooSmall);
        }

        let mut current_lba = lba;
        let mut sectors_left = count;
        let mut offset = 0;

        while sectors_left > 0 {
            let chunk_sectors = sectors_left.min(8);
            let chunk_bytes = (chunk_sectors as usize) * self.sector_size;

            // Copy data into DMA bounce buffer
            unsafe {
                let bounce_buf = self.dma_virt.add(2048);
                core::ptr::copy_nonoverlapping(buf.as_ptr().add(offset), bounce_buf, chunk_bytes);
            }

            self.execute_command(current_lba, chunk_sectors, ATA_CMD_WRITE_DMA_EXT, true)?;

            sectors_left -= chunk_sectors;
            current_lba += chunk_sectors as u64;
            offset += chunk_bytes;
        }

        Ok(())
    }

    /// Executes an ATA command with 1 PRDT entry using the pre-allocated DMA layout:
    /// - 0..1024: Command List (Slot 0 is used)
    /// - 1024..1280: Received FIS
    /// - 1280..1536: Command Table (Header + PRDT)
    /// - 2048..4096: Bounce Buffer (up to 2048 bytes)
    fn execute_command(
        &self,
        lba: u64,
        count: u16,
        command: u8,
        is_write: bool,
    ) -> Result<(), AhciError> {
        unsafe {
            let port = &mut *self.port_mmio;

            // 1. Wait until port is not busy
            let mut timeout = 1_000_000;
            while (core::ptr::read_volatile(&port.tfd) & (PORT_TFD_BSY | PORT_TFD_DRQ)) != 0 {
                core::hint::spin_loop();
                timeout -= 1;
                if timeout == 0 {
                    return Err(AhciError::DeviceBusy);
                }
            }

            // 2. Prepare Command Header in Slot 0
            let cmd_header = self.dma_virt as *mut HbaCmdHeader;
            let prdt_len = 1u16;
            let fis_dwords = (core::mem::size_of::<FisRegH2D>() / 4) as u16;
            let mut flags = fis_dwords & 0x1F;
            if is_write {
                flags |= 1 << 6; // W = 1 for write
            }
            flags |= 1 << 10; // Clear BSY on R_OK

            let cmd_table_phys = self.dma_phys + 1280;
            (*cmd_header).flags = flags;
            (*cmd_header).prdtl = prdt_len;
            (*cmd_header).prdbc = 0;
            (*cmd_header).ctba = cmd_table_phys as u32;
            (*cmd_header).ctbau = (cmd_table_phys >> 32) as u32;
            (*cmd_header).reserved = [0; 4];

            // 3. Clear and set up Command Table
            let cmd_table_ptr = self.dma_virt.add(1280);
            core::ptr::write_bytes(cmd_table_ptr, 0, 256);

            // 4. Set up Command FIS (Register H2D)
            let cfis = cmd_table_ptr as *mut FisRegH2D;
            (*cfis).fis_type = FIS_TYPE_REG_H2D;
            (*cfis).pmport_c = 1 << 7; // Command
            (*cfis).command = command;
            (*cfis).device = 1 << 6; // LBA mode

            (*cfis).lba0 = (lba & 0xFF) as u8;
            (*cfis).lba1 = ((lba >> 8) & 0xFF) as u8;
            (*cfis).lba2 = ((lba >> 16) & 0xFF) as u8;
            (*cfis).lba3 = ((lba >> 24) & 0xFF) as u8;
            (*cfis).lba4 = ((lba >> 32) & 0xFF) as u8;
            (*cfis).lba5 = ((lba >> 40) & 0xFF) as u8;

            (*cfis).count_low = (count & 0xFF) as u8;
            (*cfis).count_high = ((count >> 8) & 0xFF) as u8;

            // 5. Set up PRDT entry pointing to bounce buffer (offset 2048)
            let prdt_phys = self.dma_phys + 2048;
            let byte_count = (count as u32) * (self.sector_size as u32);
            let prdt_ptr = cmd_table_ptr.add(0x80) as *mut HbaPrdtEntry;
            (*prdt_ptr).dba = prdt_phys as u32;
            (*prdt_ptr).dbau = (prdt_phys >> 32) as u32;
            (*prdt_ptr).reserved = 0;
            // DBC: byte count minus 1, bit 31 = Interrupt on Completion
            (*prdt_ptr).dbc = (byte_count - 1) | (1 << 31);

            // Ensure memory writes are visible to HBA
            compiler_fence(Ordering::SeqCst);

            // 6. Issue command on Slot 0
            core::ptr::write_volatile(&mut port.ci, 1);

            // 7. Spin wait for completion
            let mut wait_spins = 20_000_000;
            while (core::ptr::read_volatile(&port.ci) & 1) != 0 {
                if (core::ptr::read_volatile(&port.is) & (1 << 30)) != 0 {
                    // Task file error bit
                    return Err(AhciError::IoError);
                }
                core::hint::spin_loop();
                wait_spins -= 1;
                if wait_spins == 0 {
                    return Err(AhciError::CommandTimeout);
                }
            }

            // 8. Check final status
            let tfd = core::ptr::read_volatile(&port.tfd);
            if (tfd & PORT_TFD_ERR) != 0 {
                return Err(AhciError::IoError);
            }

            Ok(())
        }
    }
}

/// AHCI Controller managing HBA ports and physical devices.
pub struct AhciController {
    #[allow(dead_code)]
    hba_base_phys: u64,
    hba_base_virt: *mut HbaMemoryRegisters,
    pub disks: Vec<AhciDisk>,
}

unsafe impl Send for AhciController {}
unsafe impl Sync for AhciController {}

impl AhciController {
    /// Initializes an AHCI controller discovered from PCI BAR5.
    pub fn init(
        pci_dev: &PciDevice,
        mapper: &mut Mapper,
        allocator: &mut FrameAllocator,
        phys_offset: u64,
    ) -> Result<Self, AhciError> {
        let phys_addr = pci_dev
            .get_bar(5)
            .and_then(|b| b.memory_address())
            .ok_or(AhciError::ControllerInitFailed)?;

        // Enable Bus Master and Memory Space in PCI Command register
        pci_dev.enable_bus_mastering();

        // Ensure BAR5 MMIO space (at least 4096 bytes) is mapped in kernel virtual memory
        let virt_addr = phys_addr + phys_offset;
        if mapper.translate(virt_addr).is_none() {
            unsafe {
                let _ = mapper.map(virt_addr, phys_addr, PRESENT | WRITABLE, allocator);
            }
        }
        let hba = virt_addr as *mut HbaMemoryRegisters;

        let mut controller = AhciController {
            hba_base_phys: phys_addr,
            hba_base_virt: hba,
            disks: Vec::new(),
        };

        controller.enable_ahci_mode()?;
        controller.probe_ports(allocator, phys_offset)?;

        Ok(controller)
    }

    /// Enables AHCI mode (AE bit in GHC) and handles BIOS-to-OS handoff if supported.
    fn enable_ahci_mode(&mut self) -> Result<(), AhciError> {
        unsafe {
            let hba = &mut *self.hba_base_virt;

            // Set AHCI Enable (AE)
            let ghc = core::ptr::read_volatile(&hba.ghc);
            core::ptr::write_volatile(&mut hba.ghc, ghc | GHC_AE);

            // Check BIOS/OS Handoff if supported (CAP2 bit 0)
            let cap2 = core::ptr::read_volatile(&hba.cap2);
            if (cap2 & 1) != 0 {
                let bohc = core::ptr::read_volatile(&hba.bohc);
                core::ptr::write_volatile(&mut hba.bohc, bohc | (1 << 1)); // OS Owned Semaphore (OOS)
                let mut spins = 50_000;
                while (core::ptr::read_volatile(&hba.bohc) & 1) != 0 && spins > 0 {
                    core::hint::spin_loop();
                    spins -= 1;
                }
            }
        }
        Ok(())
    }

    /// Probes implemented ports and discovers connected SATA drives.
    fn probe_ports(
        &mut self,
        allocator: &mut FrameAllocator,
        phys_offset: u64,
    ) -> Result<(), AhciError> {
        let pi = unsafe { core::ptr::read_volatile(&(*self.hba_base_virt).pi) };

        for port_idx in 0..32 {
            if (pi & (1 << port_idx)) != 0 {
                let port_ptr =
                    unsafe { &mut (*self.hba_base_virt).ports[port_idx] as *mut HbaPortRegisters };
                if let Some(disk) = self.init_port(port_idx, port_ptr, allocator, phys_offset) {
                    println!(
                        "  ahci-sata    Puerto {} · {} · {} sectores ({} MiB)",
                        disk.port_index,
                        disk.model,
                        disk.total_sectors,
                        disk.capacity_bytes() / (1024 * 1024)
                    );
                    self.disks.push(disk);
                }
            }
        }

        Ok(())
    }

    /// Checks if a port has a detected and active device, and initializes it if it's ATA.
    fn init_port(
        &self,
        port_idx: usize,
        port_ptr: *mut HbaPortRegisters,
        allocator: &mut FrameAllocator,
        phys_offset: u64,
    ) -> Option<AhciDisk> {
        unsafe {
            let port = &mut *port_ptr;

            let ssts = core::ptr::read_volatile(&port.ssts);
            let det = ssts & 0x0F;
            let ipm = (ssts >> 8) & 0x0F;

            // DET == 3: Device detected and communication established
            // IPM == 1: Device in active state
            if det != 3 || ipm != 1 {
                return None;
            }

            let sig = core::ptr::read_volatile(&port.sig);
            if sig != SATA_SIG_ATA {
                // Not standard ATA disk (might be ATAPI or SEMB)
                return None;
            }

            // Stop port command engine before reconfiguring pointers
            Self::stop_port(port);

            // Allocate a 4 KiB DMA page for this port
            let dma_phys = allocator.allocate()?;
            let dma_virt = (dma_phys + phys_offset) as *mut u8;
            core::ptr::write_bytes(dma_virt, 0, 4096);

            // Setup Port CLB and FB base addresses
            // CLB: Command List (1024 bytes) at offset 0
            // FB: Received FIS (256 bytes) at offset 1024
            core::ptr::write_volatile(&mut port.clb, dma_phys as u32);
            core::ptr::write_volatile(&mut port.clbu, (dma_phys >> 32) as u32);
            core::ptr::write_volatile(&mut port.fb, (dma_phys + 1024) as u32);
            core::ptr::write_volatile(&mut port.fbu, ((dma_phys + 1024) >> 32) as u32);

            // Clear SERR
            core::ptr::write_volatile(&mut port.serr, 0xFFFF_FFFF);
            // Clear IS
            core::ptr::write_volatile(&mut port.is, 0xFFFF_FFFF);

            // Start port command engine
            Self::start_port(port);

            // Query drive with ATA IDENTIFY
            let mut disk = AhciDisk {
                port_index: port_idx,
                model: String::from("SATA Hard Disk"),
                serial: String::new(),
                total_sectors: 0,
                sector_size: SECTOR_SIZE,
                read_only: false,
                port_mmio: port_ptr,
                dma_phys,
                dma_virt,
            };

            if disk.identify_device().is_ok() && disk.total_sectors > 0 {
                Some(disk)
            } else {
                // Fallback default if identify had issue but drive is present
                disk.total_sectors = 2048; // default minimal size
                Some(disk)
            }
        }
    }

    /// Stops command execution and FIS reception on port.
    unsafe fn stop_port(port: &mut HbaPortRegisters) {
        let mut cmd = core::ptr::read_volatile(&port.cmd);
        cmd &= !PORT_CMD_ST;
        core::ptr::write_volatile(&mut port.cmd, cmd);

        let mut timeout = 50_000;
        while (core::ptr::read_volatile(&port.cmd) & PORT_CMD_CR) != 0 && timeout > 0 {
            core::hint::spin_loop();
            timeout -= 1;
        }

        cmd = core::ptr::read_volatile(&port.cmd);
        cmd &= !PORT_CMD_FRE;
        core::ptr::write_volatile(&mut port.cmd, cmd);

        timeout = 50_000;
        while (core::ptr::read_volatile(&port.cmd) & PORT_CMD_FR) != 0 && timeout > 0 {
            core::hint::spin_loop();
            timeout -= 1;
        }
    }

    /// Starts command execution and FIS reception on port.
    unsafe fn start_port(port: &mut HbaPortRegisters) {
        let mut timeout = 50_000;
        while (core::ptr::read_volatile(&port.cmd) & PORT_CMD_CR) != 0 && timeout > 0 {
            core::hint::spin_loop();
            timeout -= 1;
        }

        let mut cmd = core::ptr::read_volatile(&port.cmd);
        cmd |= PORT_CMD_FRE;
        core::ptr::write_volatile(&mut port.cmd, cmd);

        cmd = core::ptr::read_volatile(&port.cmd);
        cmd |= PORT_CMD_ST;
        core::ptr::write_volatile(&mut port.cmd, cmd);
    }
}

impl AhciDisk {
    /// Issues ATA IDENTIFY command (0xEC) to parse model, serial and sector capacity.
    fn identify_device(&mut self) -> Result<(), AhciError> {
        self.execute_command(0, 1, ATA_CMD_IDENTIFY, false)?;

        unsafe {
            let info_ptr = self.dma_virt.add(2048) as *const u16;

            // Word 27..46: Model number (40 ASCII chars, big-endian word pairs)
            let mut model_bytes = [b' '; 40];
            for i in 0..20 {
                let w = core::ptr::read_volatile(info_ptr.add(27 + i));
                model_bytes[i * 2] = (w >> 8) as u8;
                model_bytes[i * 2 + 1] = (w & 0xFF) as u8;
            }
            if let Ok(model_str) = core::str::from_utf8(&model_bytes) {
                let trimmed = model_str.trim();
                if !trimmed.is_empty() {
                    self.model = String::from(trimmed);
                }
            }

            // Word 10..19: Serial number (20 ASCII chars)
            let mut serial_bytes = [b' '; 20];
            for i in 0..10 {
                let w = core::ptr::read_volatile(info_ptr.add(10 + i));
                serial_bytes[i * 2] = (w >> 8) as u8;
                serial_bytes[i * 2 + 1] = (w & 0xFF) as u8;
            }
            if let Ok(ser_str) = core::str::from_utf8(&serial_bytes) {
                let trimmed = ser_str.trim();
                if !trimmed.is_empty() {
                    self.serial = String::from(trimmed);
                }
            }

            // Word 83 bit 10: LBA48 supported
            let feat83 = core::ptr::read_volatile(info_ptr.add(83));
            let has_lba48 = (feat83 & (1 << 10)) != 0;

            if has_lba48 {
                // Words 100..103: Total number of LBA48 user addressable sectors (64-bit)
                let s0 = core::ptr::read_volatile(info_ptr.add(100)) as u64;
                let s1 = core::ptr::read_volatile(info_ptr.add(101)) as u64;
                let s2 = core::ptr::read_volatile(info_ptr.add(102)) as u64;
                let s3 = core::ptr::read_volatile(info_ptr.add(103)) as u64;
                self.total_sectors = s0 | (s1 << 16) | (s2 << 32) | (s3 << 48);
            } else {
                // Words 60..61: LBA28 sectors
                let s0 = core::ptr::read_volatile(info_ptr.add(60)) as u64;
                let s1 = core::ptr::read_volatile(info_ptr.add(61)) as u64;
                self.total_sectors = s0 | (s1 << 16);
            }
        }

        Ok(())
    }
}
