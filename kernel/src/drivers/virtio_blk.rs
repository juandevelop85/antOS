//! VirtIO Block Device Driver (`virtio-blk`).
//!
//! Implements a legacy VirtIO block driver over PCI I/O ports with Virtqueue
//! descriptor chaining, synchronous block reading/writing, and capacity discovery.

use crate::arch::x86_64::port::{inl, inw, outb, outl, outw};
use crate::drivers::pci::{PciBar, PciDevice};
use crate::sync::SpinLock;

pub const SECTOR_SIZE: usize = 512;
pub const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
pub const VIRTIO_DEV_BLOCK_LEGACY: u16 = 0x1001;
pub const VIRTIO_DEV_BLOCK_MODERN: u16 = 0x1042;

// VirtIO Device Status flags
const STATUS_ACKNOWLEDGE: u8 = 1;
const STATUS_DRIVER: u8 = 2;
const STATUS_DRIVER_OK: u8 = 4;
#[allow(dead_code)]
const STATUS_FAILED: u8 = 128;

// Virtqueue Descriptor Flags
const VRING_DESC_F_NEXT: u16 = 1;
const VRING_DESC_F_WRITE: u16 = 2;

// VirtIO Block Request Types
const VIRTIO_BLK_T_IN: u32 = 0; // Read
const VIRTIO_BLK_T_OUT: u32 = 1; // Write

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct VirtioBlkHeader {
    type_: u32,
    reserved: u32,
    sector: u64,
}

#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Default)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
struct VirtqUsed {
    flags: u16,
    idx: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioError {
    DeviceNotFound,
    InvalidBar,
    QueueSizeTooSmall,
    AllocationFailed,
    IoError,
    BufferMisaligned,
}

/// VirtIO Block Device controller.
pub struct VirtioBlock {
    io_base: u16,
    capacity_sectors: u64,
    queue_size: u16,
    virt_base: *mut u8,
    phys_base: u64,
    last_used_idx: u16,
}

unsafe impl Send for VirtioBlock {}
unsafe impl Sync for VirtioBlock {}

impl VirtioBlock {
    /// Initializes a VirtIO block device using a discovered PCI device.
    ///
    /// # Safety
    /// `virt_base` must be a valid, 4096-aligned pointer to at least 8192 bytes of
    /// contiguous memory whose physical address is `phys_base`.
    pub unsafe fn init(
        device: &PciDevice,
        virt_base: *mut u8,
        phys_base: u64,
    ) -> Result<Self, VirtioError> {
        // Find I/O port BAR
        let io_base = match device.bars[0] {
            PciBar::Io { port } => port,
            _ => return Err(VirtioError::InvalidBar),
        };

        // 1. Enable Bus Master & I/O space on PCI device
        device.enable_bus_mastering();

        // 2. Reset device by writing 0 to DEVICE_STATUS
        outb(io_base + 0x12, 0);

        // 3. Set ACKNOWLEDGE and DRIVER status bits
        outb(io_base + 0x12, STATUS_ACKNOWLEDGE);
        outb(io_base + 0x12, STATUS_ACKNOWLEDGE | STATUS_DRIVER);

        // 4. Feature negotiation (accept default feature set)
        let _device_features = inl(io_base + 0x00);
        outl(io_base + 0x04, 0);

        // 5. Configure Queue 0
        outw(io_base + 0x0E, 0); // Select queue 0
        let queue_size = inw(io_base + 0x0C);
        if queue_size == 0 || queue_size > 256 {
            return Err(VirtioError::QueueSizeTooSmall);
        }

        // Zero the entire 16 KiB allocated buffer
        core::ptr::write_bytes(virt_base, 0, 16384);

        // Tell device the physical page frame number of the virtqueue
        let pfn = (phys_base >> 12) as u32;
        outl(io_base + 0x08, pfn);

        // 6. Set DRIVER_OK status bit
        outb(
            io_base + 0x12,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_DRIVER_OK,
        );

        // 7. Read device capacity (in 512-byte sectors)
        let cap_low = inl(io_base + 0x14);
        let cap_high = inl(io_base + 0x18);
        let capacity_sectors = ((cap_high as u64) << 32) | (cap_low as u64);

        Ok(VirtioBlock {
            io_base,
            capacity_sectors,
            queue_size,
            virt_base,
            phys_base,
            last_used_idx: 0,
        })
    }

    /// Returns the total storage capacity in 512-byte sectors.
    pub fn capacity_sectors(&self) -> u64 {
        self.capacity_sectors
    }

    /// Returns the total capacity in bytes.
    pub fn capacity_bytes(&self) -> u64 {
        self.capacity_sectors * (SECTOR_SIZE as u64)
    }

    /// Reads a single 512-byte sector at the specified LBA.
    pub fn read_sector(
        &mut self,
        sector: u64,
        buf: &mut [u8; SECTOR_SIZE],
    ) -> Result<(), VirtioError> {
        self.perform_io(VIRTIO_BLK_T_IN, sector, buf)
    }

    /// Writes a single 512-byte sector at the specified LBA.
    pub fn write_sector(
        &mut self,
        sector: u64,
        buf: &[u8; SECTOR_SIZE],
    ) -> Result<(), VirtioError> {
        let mut temp = *buf;
        self.perform_io(VIRTIO_BLK_T_OUT, sector, &mut temp)
    }

    /// Reads contiguous 512-byte sectors into `buf`.
    pub fn read_blocks(&mut self, start_sector: u64, buf: &mut [u8]) -> Result<(), VirtioError> {
        if buf.len() % SECTOR_SIZE != 0 {
            return Err(VirtioError::BufferMisaligned);
        }

        let mut sector_buf = [0u8; SECTOR_SIZE];
        for (i, chunk) in buf.chunks_mut(SECTOR_SIZE).enumerate() {
            let sector = start_sector + (i as u64);
            self.read_sector(sector, &mut sector_buf)?;
            chunk.copy_from_slice(&sector_buf);
        }
        Ok(())
    }

    /// Writes contiguous 512-byte sectors from `buf`.
    pub fn write_blocks(&mut self, start_sector: u64, buf: &[u8]) -> Result<(), VirtioError> {
        if buf.len() % SECTOR_SIZE != 0 {
            return Err(VirtioError::BufferMisaligned);
        }

        let mut sector_buf = [0u8; SECTOR_SIZE];
        for (i, chunk) in buf.chunks(SECTOR_SIZE).enumerate() {
            let sector = start_sector + (i as u64);
            sector_buf.copy_from_slice(chunk);
            self.write_sector(sector, &sector_buf)?;
        }
        Ok(())
    }

    fn perform_io(
        &mut self,
        type_: u32,
        sector: u64,
        buf: &mut [u8; SECTOR_SIZE],
    ) -> Result<(), VirtioError> {
        unsafe {
            let q_size = self.queue_size as usize;
            let avail_offset = q_size * 16;
            let avail_idx_offset = avail_offset + 2;
            let avail_ring_offset = avail_offset + 4;
            let used_offset = (avail_offset + 6 + 2 * q_size + 4095) & !4095;
            let used_idx_offset = used_offset + 2;

            let header_offset = (used_offset + 6 + 8 * q_size + 4095) & !4095;
            let status_offset = header_offset + 16;
            let data_offset = (status_offset + 1 + 15) & !15;

            // 1. Setup Request Header
            let header_ptr = self.virt_base.add(header_offset) as *mut VirtioBlkHeader;
            core::ptr::write_unaligned(
                header_ptr,
                VirtioBlkHeader {
                    type_,
                    reserved: 0,
                    sector,
                },
            );

            // 2. Setup Status Byte (init to 0xFF)
            let status_ptr = self.virt_base.add(status_offset);
            core::ptr::write_volatile(status_ptr, 0xFF);

            // If write operation, copy user buffer into bounce buffer
            if type_ == VIRTIO_BLK_T_OUT {
                core::ptr::copy_nonoverlapping(
                    buf.as_ptr(),
                    self.virt_base.add(data_offset),
                    SECTOR_SIZE,
                );
            }

            // 3. Setup Descriptor 0: Header
            let desc_table = self.virt_base as *mut VirtqDesc;
            *desc_table = VirtqDesc {
                addr: self.phys_base + (header_offset as u64),
                len: core::mem::size_of::<VirtioBlkHeader>() as u32,
                flags: VRING_DESC_F_NEXT,
                next: 1,
            };

            // 4. Setup Descriptor 1: Data Buffer
            let data_flags = if type_ == VIRTIO_BLK_T_IN {
                VRING_DESC_F_NEXT | VRING_DESC_F_WRITE
            } else {
                VRING_DESC_F_NEXT
            };
            *desc_table.add(1) = VirtqDesc {
                addr: self.phys_base + (data_offset as u64),
                len: SECTOR_SIZE as u32,
                flags: data_flags,
                next: 2,
            };

            // 5. Setup Descriptor 2: Status
            *desc_table.add(2) = VirtqDesc {
                addr: self.phys_base + (status_offset as u64),
                len: 1,
                flags: VRING_DESC_F_WRITE,
                next: 0,
            };

            // 6. Put head descriptor (0) into Available Ring
            let avail_idx_ptr = self.virt_base.add(avail_idx_offset) as *mut u16;
            let cur_avail_idx = core::ptr::read_volatile(avail_idx_ptr);
            let ring_slot = (cur_avail_idx as usize) % q_size;
            let ring_entry = self.virt_base.add(avail_ring_offset + ring_slot * 2) as *mut u16;
            core::ptr::write_volatile(ring_entry, 0); // Descriptor 0 is head of chain

            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            core::ptr::write_volatile(avail_idx_ptr, cur_avail_idx.wrapping_add(1));
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

            // 7. Notify Device
            outw(self.io_base + 0x10, 0);

            // 8. Spin wait for request completion via Used Ring
            let used_idx_ptr = self.virt_base.add(used_idx_offset) as *const u16;
            let mut spins = 0u64;
            while core::ptr::read_volatile(used_idx_ptr) == self.last_used_idx {
                core::hint::spin_loop();
                spins += 1;
                if spins > 20_000_000 {
                    return Err(VirtioError::IoError);
                }
            }
            self.last_used_idx = core::ptr::read_volatile(used_idx_ptr);

            // 9. Inspect status byte
            let status = core::ptr::read_volatile(status_ptr);
            if status != 0 {
                return Err(VirtioError::IoError);
            }

            // If read operation, copy from bounce buffer back to user
            if type_ == VIRTIO_BLK_T_IN {
                core::ptr::copy_nonoverlapping(
                    self.virt_base.add(data_offset),
                    buf.as_mut_ptr(),
                    SECTOR_SIZE,
                );
            }

            Ok(())
        }
    }
}

/// Global system block device instance.
pub static BLOCK_DEVICE: SpinLock<Option<VirtioBlock>> = SpinLock::new(None);

/// Returns true if a VirtIO block device is currently registered.
pub fn is_available() -> bool {
    BLOCK_DEVICE.lock().is_some()
}

/// Returns total capacity in sectors from global block device.
pub fn capacity_sectors() -> u64 {
    BLOCK_DEVICE
        .lock()
        .as_ref()
        .map(|d| d.capacity_sectors())
        .unwrap_or(0)
}

/// Reads contiguous 512-byte blocks from the global VirtIO block device.
pub fn read_blocks(start_sector: u64, buf: &mut [u8]) -> Result<(), VirtioError> {
    let mut guard = BLOCK_DEVICE.lock();
    if let Some(device) = guard.as_mut() {
        device.read_blocks(start_sector, buf)
    } else {
        Err(VirtioError::DeviceNotFound)
    }
}

/// Writes contiguous 512-byte blocks to the global VirtIO block device.
pub fn write_blocks(start_sector: u64, buf: &[u8]) -> Result<(), VirtioError> {
    let mut guard = BLOCK_DEVICE.lock();
    if let Some(device) = guard.as_mut() {
        device.write_blocks(start_sector, buf)
    } else {
        Err(VirtioError::DeviceNotFound)
    }
}
