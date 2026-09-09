//! Non-Volatile Memory Express (NVMe 1.0+) Storage Driver.
//!
//! Provides PCI BAR0 MMIO access, controller reset and capability negotiation,
//! Admin Submission / Completion Queue setup, Namespace discovery (`Identify Controller`
//! and `Identify Namespace`), I/O Queue pair creation, and 4096-byte block read/write operations.

#![allow(clippy::field_reassign_with_default)]

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{compiler_fence, Ordering};

use crate::drivers::pci::PciDevice;
use crate::memory::{FrameAllocator, Mapper, PRESENT, WRITABLE};
use crate::println;

pub const DEFAULT_BLOCK_SIZE: usize = 4096;

// NVMe Admin Opcodes
#[allow(dead_code)]
const NVME_ADMIN_CMD_DELETE_IO_SQ: u8 = 0x00;
const NVME_ADMIN_CMD_CREATE_IO_SQ: u8 = 0x01;
#[allow(dead_code)]
const NVME_ADMIN_CMD_DELETE_IO_CQ: u8 = 0x04;
const NVME_ADMIN_CMD_CREATE_IO_CQ: u8 = 0x05;
const NVME_ADMIN_CMD_IDENTIFY: u8 = 0x06;

// NVMe NVM I/O Opcodes
const NVME_NVM_CMD_WRITE: u8 = 0x01;
const NVME_NVM_CMD_READ: u8 = 0x02;

// Controller Configuration & Status bits
const NVME_CC_EN: u32 = 1 << 0;
const NVME_CC_CSS_NVM: u32 = 0 << 4;
const NVME_CC_MPS_4K: u32 = 0 << 7;
const NVME_CC_IOSQES_64: u32 = 6 << 16;
const NVME_CC_IOCQES_16: u32 = 4 << 20;

const NVME_CSTS_RDY: u32 = 1 << 0;
const NVME_CSTS_CFS: u32 = 1 << 1;

const QUEUE_ENTRIES: u16 = 64;

#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
struct NvmeCmd {
    cdw0: u32,
    nsid: u32,
    reserved0: u64,
    mptr: u64,
    prp1: u64,
    prp2: u64,
    cdw10: u32,
    cdw11: u32,
    cdw12: u32,
    cdw13: u32,
    cdw14: u32,
    cdw15: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
struct NvmeCqe {
    cdw0: u32,
    reserved: u32,
    sqhd: u16,
    sqid: u16,
    cid: u16,
    status: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvmeError {
    ControllerInitFailed,
    ControllerTimeout,
    FatalStatus,
    QueueAllocFailed,
    MmioMapFailed,
    IdentifyFailed,
    IoError,
    BufferTooSmall,
}

/// Represents an active NVMe namespace (logical drive).
pub struct NvmeNamespace {
    pub nsid: u32,
    pub model: String,
    pub serial: String,
    pub block_size: usize,
    pub total_blocks: u64,
    pub read_only: bool,
    controller_ptr: *mut NvmeController,
}

unsafe impl Send for NvmeNamespace {}
unsafe impl Sync for NvmeNamespace {}

impl NvmeNamespace {
    /// Returns total drive size in bytes.
    pub fn capacity_bytes(&self) -> u64 {
        self.total_blocks.saturating_mul(self.block_size as u64)
    }

    /// Reads `count` blocks starting at `lba` into `buf`.
    pub fn read_blocks(&self, lba: u64, count: u16, buf: &mut [u8]) -> Result<(), NvmeError> {
        let total_bytes = (count as usize) * self.block_size;
        if buf.len() < total_bytes {
            return Err(NvmeError::BufferTooSmall);
        }

        unsafe {
            let ctrl = &mut *self.controller_ptr;
            let mut blocks_left = count;
            let mut cur_lba = lba;
            let mut offset = 0;

            while blocks_left > 0 {
                let chunk_blocks = 1u16; // 1 block (4 KiB) per bounce buffer
                let chunk_bytes = (chunk_blocks as usize) * self.block_size;

                ctrl.submit_io_command(
                    self.nsid,
                    cur_lba,
                    chunk_blocks,
                    ctrl.dma_buf_phys,
                    NVME_NVM_CMD_READ,
                )?;

                // Copy from DMA bounce buffer to destination
                core::ptr::copy_nonoverlapping(
                    ctrl.dma_buf_virt,
                    buf.as_mut_ptr().add(offset),
                    chunk_bytes,
                );

                blocks_left -= chunk_blocks;
                cur_lba += chunk_blocks as u64;
                offset += chunk_bytes;
            }
        }
        Ok(())
    }

    /// Writes `count` blocks starting at `lba` from `buf`.
    pub fn write_blocks(&self, lba: u64, count: u16, buf: &[u8]) -> Result<(), NvmeError> {
        if self.read_only {
            return Err(NvmeError::IoError);
        }
        let total_bytes = (count as usize) * self.block_size;
        if buf.len() < total_bytes {
            return Err(NvmeError::BufferTooSmall);
        }

        unsafe {
            let ctrl = &mut *self.controller_ptr;
            let mut blocks_left = count;
            let mut cur_lba = lba;
            let mut offset = 0;

            while blocks_left > 0 {
                let chunk_blocks = 1u16;
                let chunk_bytes = (chunk_blocks as usize) * self.block_size;

                // Copy to DMA bounce buffer
                core::ptr::copy_nonoverlapping(
                    buf.as_ptr().add(offset),
                    ctrl.dma_buf_virt,
                    chunk_bytes,
                );

                ctrl.submit_io_command(
                    self.nsid,
                    cur_lba,
                    chunk_blocks,
                    ctrl.dma_buf_phys,
                    NVME_NVM_CMD_WRITE,
                )?;

                blocks_left -= chunk_blocks;
                cur_lba += chunk_blocks as u64;
                offset += chunk_bytes;
            }
        }
        Ok(())
    }
}

/// NVMe Controller managing Admin and I/O Queues over PCIe.
pub struct NvmeController {
    mmio_base: *mut u8,
    doorbell_stride: usize,
    // Admin Queue
    asq_phys: u64,
    asq_virt: *mut NvmeCmd,
    asq_tail: u16,
    acq_phys: u64,
    acq_virt: *mut NvmeCqe,
    acq_head: u16,
    acq_phase: u16,
    // I/O Queue 1
    iosq_phys: u64,
    iosq_virt: *mut NvmeCmd,
    iosq_tail: u16,
    iocq_phys: u64,
    iocq_virt: *mut NvmeCqe,
    iocq_head: u16,
    iocq_phase: u16,
    // DMA bounce buffer (4 KiB)
    dma_buf_phys: u64,
    dma_buf_virt: *mut u8,
    command_id: u16,
    pub namespaces: Vec<NvmeNamespace>,
}

unsafe impl Send for NvmeController {}
unsafe impl Sync for NvmeController {}

impl NvmeController {
    /// Probes and initializes an NVMe controller discovered via PCI.
    pub fn init(
        pci_dev: &PciDevice,
        mapper: &mut Mapper,
        allocator: &mut FrameAllocator,
        phys_offset: u64,
    ) -> Result<Self, NvmeError> {
        let phys_addr = pci_dev
            .get_bar(0)
            .and_then(|b| b.memory_address())
            .ok_or(NvmeError::ControllerInitFailed)?;

        pci_dev.enable_bus_mastering();

        // Ensure at least 16 KiB MMIO mapping for registers and doorbells
        for i in 0..4 {
            let p = phys_addr + i * 4096;
            let v = p + phys_offset;
            if mapper.translate(v).is_none() {
                unsafe {
                    let _ = mapper.map(v, p, PRESENT | WRITABLE, allocator);
                }
            }
        }
        let mmio_base = (phys_addr + phys_offset) as *mut u8;

        // Allocate Admin Queues (4 KiB each)
        let asq_phys = allocator.allocate().ok_or(NvmeError::QueueAllocFailed)?;
        let asq_v = (asq_phys + phys_offset) as *mut NvmeCmd;
        unsafe {
            core::ptr::write_bytes(asq_v as *mut u8, 0, 4096);
        }

        let acq_phys = allocator.allocate().ok_or(NvmeError::QueueAllocFailed)?;
        let acq_v = (acq_phys + phys_offset) as *mut NvmeCqe;
        unsafe {
            core::ptr::write_bytes(acq_v as *mut u8, 0, 4096);
        }

        // Allocate I/O Queues (4 KiB each)
        let iosq_phys = allocator.allocate().ok_or(NvmeError::QueueAllocFailed)?;
        let iosq_v = (iosq_phys + phys_offset) as *mut NvmeCmd;
        unsafe {
            core::ptr::write_bytes(iosq_v as *mut u8, 0, 4096);
        }

        let iocq_phys = allocator.allocate().ok_or(NvmeError::QueueAllocFailed)?;
        let iocq_v = (iocq_phys + phys_offset) as *mut NvmeCqe;
        unsafe {
            core::ptr::write_bytes(iocq_v as *mut u8, 0, 4096);
        }

        // Allocate DMA bounce buffer (4 KiB)
        let dma_buf_phys = allocator.allocate().ok_or(NvmeError::QueueAllocFailed)?;
        let dma_buf_v = (dma_buf_phys + phys_offset) as *mut u8;
        unsafe {
            core::ptr::write_bytes(dma_buf_v, 0, 4096);
        }

        // Read CAP (Controller Capabilities)
        let cap = unsafe { core::ptr::read_volatile(mmio_base as *const u64) };
        let dstrd = ((cap >> 32) & 0x0F) as usize;
        let doorbell_stride = 4 << dstrd;

        let mut controller = NvmeController {
            mmio_base,
            doorbell_stride,
            asq_phys,
            asq_virt: asq_v,
            asq_tail: 0,
            acq_phys,
            acq_virt: acq_v,
            acq_head: 0,
            acq_phase: 1, // Phase starts at 1
            iosq_phys,
            iosq_virt: iosq_v,
            iosq_tail: 0,
            iocq_phys,
            iocq_virt: iocq_v,
            iocq_head: 0,
            iocq_phase: 1,
            dma_buf_phys,
            dma_buf_virt: dma_buf_v,
            command_id: 0,
            namespaces: Vec::new(),
        };

        controller.reset_and_configure_controller()?;
        controller.setup_io_queues()?;
        controller.discover_namespaces()?;

        Ok(controller)
    }

    /// Resets controller, sets Admin Queue attributes and addresses, and enables controller.
    fn reset_and_configure_controller(&mut self) -> Result<(), NvmeError> {
        unsafe {
            let cc_ptr = self.mmio_base.add(0x14) as *mut u32;
            let csts_ptr = self.mmio_base.add(0x1C) as *const u32;

            // 1. If controller is currently enabled (EN == 1), disable it
            let mut cc = core::ptr::read_volatile(cc_ptr);
            if (cc & NVME_CC_EN) != 0 {
                core::ptr::write_volatile(cc_ptr, cc & !NVME_CC_EN);

                // Wait until RDY == 0
                let mut spins = 1_000_000;
                while (core::ptr::read_volatile(csts_ptr) & NVME_CSTS_RDY) != 0 {
                    core::hint::spin_loop();
                    spins -= 1;
                    if spins == 0 {
                        return Err(NvmeError::ControllerTimeout);
                    }
                }
            }

            // 2. Set Admin Queue Attributes (AQA)
            let aqa_ptr = self.mmio_base.add(0x24) as *mut u32;
            let aqa_val = ((QUEUE_ENTRIES - 1) as u32) | (((QUEUE_ENTRIES - 1) as u32) << 16);
            core::ptr::write_volatile(aqa_ptr, aqa_val);

            // 3. Set Admin Submission Queue Base (ASQ)
            let asq_reg = self.mmio_base.add(0x28) as *mut u64;
            core::ptr::write_volatile(asq_reg, self.asq_phys);

            // 4. Set Admin Completion Queue Base (ACQ)
            let acq_reg = self.mmio_base.add(0x30) as *mut u64;
            core::ptr::write_volatile(acq_reg, self.acq_phys);

            // 5. Configure and Enable Controller (CC)
            cc = NVME_CC_EN
                | NVME_CC_CSS_NVM
                | NVME_CC_MPS_4K
                | NVME_CC_IOSQES_64
                | NVME_CC_IOCQES_16;
            core::ptr::write_volatile(cc_ptr, cc);

            // 6. Wait until RDY == 1
            let mut spins = 1_000_000;
            while (core::ptr::read_volatile(csts_ptr) & NVME_CSTS_RDY) == 0 {
                let csts = core::ptr::read_volatile(csts_ptr);
                if (csts & NVME_CSTS_CFS) != 0 {
                    return Err(NvmeError::FatalStatus);
                }
                core::hint::spin_loop();
                spins -= 1;
                if spins == 0 {
                    return Err(NvmeError::ControllerTimeout);
                }
            }
        }
        Ok(())
    }

    /// Submits a command to Admin Submission Queue and waits for completion.
    fn submit_admin_command(&mut self, mut cmd: NvmeCmd) -> Result<NvmeCqe, NvmeError> {
        self.command_id = self.command_id.wrapping_add(1);
        cmd.cdw0 |= (self.command_id as u32) << 16;

        unsafe {
            // Write command to ASQ
            let slot = (self.asq_tail as usize) % (QUEUE_ENTRIES as usize);
            core::ptr::write_volatile(self.asq_virt.add(slot), cmd);
            compiler_fence(Ordering::SeqCst);

            // Ring Admin SQ Doorbell (y = 0)
            self.asq_tail = (self.asq_tail + 1) % QUEUE_ENTRIES;
            let sq_db = self.mmio_base.add(0x1000) as *mut u32;
            core::ptr::write_volatile(sq_db, self.asq_tail as u32);

            // Wait for completion in ACQ
            let cq_slot = (self.acq_head as usize) % (QUEUE_ENTRIES as usize);
            let cqe_ptr = self.acq_virt.add(cq_slot);

            let mut spins = 5_000_000;
            loop {
                compiler_fence(Ordering::SeqCst);
                let cqe = core::ptr::read_volatile(cqe_ptr);
                let phase = (cqe.status & 1) != 0;
                let expected_phase = self.acq_phase == 1;

                if phase == expected_phase {
                    // Ring Admin CQ Doorbell to acknowledge completion
                    self.acq_head = (self.acq_head + 1) % QUEUE_ENTRIES;
                    if self.acq_head == 0 {
                        self.acq_phase ^= 1;
                    }
                    let cq_db = self.mmio_base.add(0x1000 + self.doorbell_stride) as *mut u32;
                    core::ptr::write_volatile(cq_db, self.acq_head as u32);

                    let raw_status = cqe.status;
                    let status_code = (raw_status >> 1) & 0x7FF;
                    if status_code != 0 {
                        return Err(NvmeError::IoError);
                    }
                    return Ok(cqe);
                }

                core::hint::spin_loop();
                spins -= 1;
                if spins == 0 {
                    return Err(NvmeError::ControllerTimeout);
                }
            }
        }
    }

    /// Creates I/O Completion Queue 1 and I/O Submission Queue 1.
    fn setup_io_queues(&mut self) -> Result<(), NvmeError> {
        // 1. Create I/O CQ 1 (QID = 1 in bits 15:0, QSIZE in bits 31:16)
        let mut create_cq = NvmeCmd::default();
        create_cq.cdw0 = NVME_ADMIN_CMD_CREATE_IO_CQ as u32;
        create_cq.prp1 = self.iocq_phys;
        create_cq.cdw10 = 1 | (((QUEUE_ENTRIES - 1) as u32) << 16);
        create_cq.cdw11 = 1; // Physically contiguous, interrupts disabled
        self.submit_admin_command(create_cq)?;

        // 2. Create I/O SQ 1 (QID = 1 in bits 15:0, QSIZE in bits 31:16, CQID = 1 in bits 31:16 of cdw11)
        let mut create_sq = NvmeCmd::default();
        create_sq.cdw0 = NVME_ADMIN_CMD_CREATE_IO_SQ as u32;
        create_sq.prp1 = self.iosq_phys;
        create_sq.cdw10 = 1 | (((QUEUE_ENTRIES - 1) as u32) << 16);
        create_sq.cdw11 = (1 << 16) | 1; // CQID = 1, physically contiguous
        self.submit_admin_command(create_sq)?;

        Ok(())
    }

    /// Submits a command to I/O Submission Queue 1 and waits for completion.
    pub fn submit_io_command(
        &mut self,
        nsid: u32,
        lba: u64,
        block_count: u16,
        data_phys: u64,
        opcode: u8,
    ) -> Result<(), NvmeError> {
        self.command_id = self.command_id.wrapping_add(1);
        let mut cmd = NvmeCmd::default();
        cmd.cdw0 = (opcode as u32) | ((self.command_id as u32) << 16);
        cmd.nsid = nsid;
        cmd.prp1 = data_phys;
        cmd.cdw10 = (lba & 0xFFFF_FFFF) as u32;
        cmd.cdw11 = ((lba >> 32) & 0xFFFF_FFFF) as u32;
        cmd.cdw12 = (block_count - 1) as u32; // 0-based block count

        unsafe {
            let slot = (self.iosq_tail as usize) % (QUEUE_ENTRIES as usize);
            core::ptr::write_volatile(self.iosq_virt.add(slot), cmd);
            compiler_fence(Ordering::SeqCst);

            // Ring I/O SQ 1 Doorbell: offset = 0x1000 + (2 * 1) * doorbell_stride
            self.iosq_tail = (self.iosq_tail + 1) % QUEUE_ENTRIES;
            let sq_db = self.mmio_base.add(0x1000 + 2 * self.doorbell_stride) as *mut u32;
            core::ptr::write_volatile(sq_db, self.iosq_tail as u32);

            // Wait for completion in I/O CQ 1
            let cq_slot = (self.iocq_head as usize) % (QUEUE_ENTRIES as usize);
            let cqe_ptr = self.iocq_virt.add(cq_slot);

            let mut spins = 5_000_000;
            loop {
                compiler_fence(Ordering::SeqCst);
                let cqe = core::ptr::read_volatile(cqe_ptr);
                let phase = (cqe.status & 1) != 0;
                let expected_phase = self.iocq_phase == 1;

                if phase == expected_phase {
                    // Ring I/O CQ 1 Doorbell: offset = 0x1000 + (2 * 1 + 1) * doorbell_stride
                    self.iocq_head = (self.iocq_head + 1) % QUEUE_ENTRIES;
                    if self.iocq_head == 0 {
                        self.iocq_phase ^= 1;
                    }
                    let cq_db = self.mmio_base.add(0x1000 + 3 * self.doorbell_stride) as *mut u32;
                    core::ptr::write_volatile(cq_db, self.iocq_head as u32);

                    let status_code = (cqe.status >> 1) & 0x7FF;
                    if status_code != 0 {
                        return Err(NvmeError::IoError);
                    }
                    return Ok(());
                }

                core::hint::spin_loop();
                spins -= 1;
                if spins == 0 {
                    return Err(NvmeError::ControllerTimeout);
                }
            }
        }
    }

    /// Discovers controller identification and active namespaces.
    fn discover_namespaces(&mut self) -> Result<(), NvmeError> {
        // 1. Identify Controller (CNS = 1)
        let mut id_ctrl = NvmeCmd::default();
        id_ctrl.cdw0 = NVME_ADMIN_CMD_IDENTIFY as u32;
        id_ctrl.nsid = 0;
        id_ctrl.prp1 = self.dma_buf_phys;
        id_ctrl.cdw10 = 1; // CNS = 1: Identify Controller
        self.submit_admin_command(id_ctrl)?;

        let mut model_str = String::from("NVMe Solid State Drive");
        let mut serial_str = String::new();
        let num_namespaces: u32;

        unsafe {
            let buf = self.dma_buf_virt;

            // Bytes 4..23: Serial Number (20 ASCII chars)
            let mut ser_bytes = [0u8; 20];
            core::ptr::copy_nonoverlapping(buf.add(4), ser_bytes.as_mut_ptr(), 20);
            if let Ok(s) = core::str::from_utf8(&ser_bytes) {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    serial_str = String::from(trimmed);
                }
            }

            // Bytes 24..63: Model Number (40 ASCII chars)
            let mut mod_bytes = [0u8; 40];
            core::ptr::copy_nonoverlapping(buf.add(24), mod_bytes.as_mut_ptr(), 40);
            if let Ok(s) = core::str::from_utf8(&mod_bytes) {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    model_str = String::from(trimmed);
                }
            }

            // Bytes 516..519: Number of Namespaces (NN)
            num_namespaces = core::ptr::read_volatile(buf.add(516) as *const u32);
        }

        // 2. Identify Namespace 1 (CNS = 0, NSID = 1)
        let max_ns = if num_namespaces == 0 {
            1
        } else {
            num_namespaces.min(4)
        };
        for nsid in 1..=max_ns {
            let mut id_ns = NvmeCmd::default();
            id_ns.cdw0 = NVME_ADMIN_CMD_IDENTIFY as u32;
            id_ns.nsid = nsid;
            id_ns.prp1 = self.dma_buf_phys;
            id_ns.cdw10 = 0; // CNS = 0: Identify Namespace
            if self.submit_admin_command(id_ns).is_err() {
                continue;
            }

            unsafe {
                let buf = self.dma_buf_virt;
                // Bytes 0..7: NSZE (Namespace Size in blocks)
                let total_blocks = core::ptr::read_volatile(buf as *const u64);
                if total_blocks == 0 {
                    continue;
                }

                // Byte 27: FLBAS
                let flbas = core::ptr::read_volatile(buf.add(27)) & 0x0F;
                // LBA Format table starts at byte 128 (4 bytes per format)
                let lbaf_offset = 128 + (flbas as usize) * 4;
                let lbads = core::ptr::read_volatile(buf.add(lbaf_offset + 2));
                let block_size = if (9..=14).contains(&lbads) {
                    1usize << lbads
                } else {
                    DEFAULT_BLOCK_SIZE
                };

                let ns = NvmeNamespace {
                    nsid,
                    model: model_str.clone(),
                    serial: serial_str.clone(),
                    block_size,
                    total_blocks,
                    read_only: false,
                    controller_ptr: self as *mut NvmeController,
                };

                println!(
                    "  nvme-ssd     NSID {} · {} · {} bloques ({} MiB, bloque: {} B)",
                    ns.nsid,
                    ns.model,
                    ns.total_blocks,
                    ns.capacity_bytes() / (1024 * 1024),
                    ns.block_size
                );

                self.namespaces.push(ns);
            }
        }

        Ok(())
    }
}
