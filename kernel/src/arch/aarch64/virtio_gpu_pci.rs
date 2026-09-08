//! VirtIO-GPU over the modern PCI transport (`virtio-gpu-pci`, `1AF4:1050`).
//!
//! Shares the 2D command set with the MMIO driver ([`super::virtio_gpu`]); only
//! the transport differs — a modern `virtio_pci_common_cfg` register block plus
//! a notify window, both located via the PCI vendor capabilities parsed in
//! [`super::virtio_pci`].

use core::ptr::{addr_of, addr_of_mut};

use crate::drivers::pci::{self, PciDevice};
use crate::sync::SpinLock;

use super::virtio_gpu::{
    VirtioGpuCtrlHdr, VirtioGpuRect, VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING,
    VIRTIO_GPU_CMD_RESOURCE_CREATE_2D, VIRTIO_GPU_CMD_RESOURCE_FLUSH, VIRTIO_GPU_CMD_SET_SCANOUT,
    VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D, VIRTIO_GPU_FORMAT_B8G8R8A8_UNORM, VIRTIO_GPU_RESP_OK_NODATA,
};
use super::virtio_pci::{self, VirtioPciRegions};

// virtio_pci_common_cfg field offsets (virtio spec §4.1.4.3).
const CFG_DEVICE_FEATURE_SELECT: u64 = 0x00;
const CFG_DEVICE_FEATURE: u64 = 0x04;
const CFG_DRIVER_FEATURE_SELECT: u64 = 0x08;
const CFG_DRIVER_FEATURE: u64 = 0x0C;
const CFG_NUM_QUEUES: u64 = 0x12;
const CFG_DEVICE_STATUS: u64 = 0x14;
const CFG_QUEUE_SELECT: u64 = 0x16;
const CFG_QUEUE_SIZE: u64 = 0x18;
const CFG_QUEUE_ENABLE: u64 = 0x1C;
const CFG_QUEUE_NOTIFY_OFF: u64 = 0x1E;
const CFG_QUEUE_DESC: u64 = 0x20;
const CFG_QUEUE_DRIVER: u64 = 0x28;
const CFG_QUEUE_DEVICE: u64 = 0x30;

const STATUS_ACKNOWLEDGE: u8 = 1;
const STATUS_DRIVER: u8 = 2;
const STATUS_DRIVER_OK: u8 = 4;
const STATUS_FEATURES_OK: u8 = 8;

const QUEUE_SIZE: usize = 16;
const VRING_DESC_F_NEXT: u16 = 1;
const VRING_DESC_F_WRITE: u16 = 2;

#[repr(C, align(16))]
#[derive(Clone, Copy, Default)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

#[repr(C, align(4096))]
struct VirtQueue {
    desc: [VirtqDesc; QUEUE_SIZE],
    avail_flags: u16,
    avail_idx: u16,
    avail_ring: [u16; QUEUE_SIZE],
    avail_used_event: u16,
    _pad: [u8; 4096 - (QUEUE_SIZE * 16 + 4 + QUEUE_SIZE * 2)],
    used_flags: u16,
    used_idx: u16,
    used_ring: [VirtqUsedElem; QUEUE_SIZE],
    used_avail_event: u16,
    _pad2: [u8; 4096 - (4 + QUEUE_SIZE * 8 + 2)],
}

static mut PCI_VRING: VirtQueue = VirtQueue {
    desc: [VirtqDesc {
        addr: 0,
        len: 0,
        flags: 0,
        next: 0,
    }; QUEUE_SIZE],
    avail_flags: 0,
    avail_idx: 0,
    avail_ring: [0; QUEUE_SIZE],
    avail_used_event: 0,
    _pad: [0; 4096 - (QUEUE_SIZE * 16 + 4 + QUEUE_SIZE * 2)],
    used_flags: 0,
    used_idx: 0,
    used_ring: [VirtqUsedElem { id: 0, len: 0 }; QUEUE_SIZE],
    used_avail_event: 0,
    _pad2: [0; 4096 - (4 + QUEUE_SIZE * 8 + 2)],
};

static mut PCI_CMD_BUF: [u8; 256] = [0; 256];
static mut PCI_RESP_BUF: VirtioGpuCtrlHdr = VirtioGpuCtrlHdr {
    ctrl_type: 0,
    flags: 0,
    fence_id: 0,
    ctx_id: 0,
    padding: 0,
};

#[repr(align(4096))]
struct BackingBuffer([u8; 1024 * 768 * 4]);
static mut PCI_FB_MEM: BackingBuffer = BackingBuffer([0; 1024 * 768 * 4]);

#[repr(C)]
#[derive(Clone, Copy)]
struct CmdCreate2d {
    hdr: VirtioGpuCtrlHdr,
    resource_id: u32,
    format: u32,
    width: u32,
    height: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MemEntry {
    addr: u64,
    length: u32,
    padding: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CmdAttachBacking {
    hdr: VirtioGpuCtrlHdr,
    resource_id: u32,
    nr_entries: u32,
    entry: MemEntry,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CmdSetScanout {
    hdr: VirtioGpuCtrlHdr,
    r: VirtioGpuRect,
    scanout_id: u32,
    resource_id: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CmdTransfer2d {
    hdr: VirtioGpuCtrlHdr,
    r: VirtioGpuRect,
    offset: u64,
    resource_id: u32,
    padding: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CmdFlush {
    hdr: VirtioGpuCtrlHdr,
    r: VirtioGpuRect,
    resource_id: u32,
    padding: u32,
}

pub struct VirtioGpuPci {
    notify: u64,
    width: usize,
    height: usize,
    stride: usize,
    buffer_ptr: *mut u8,
    buffer_len: usize,
    avail_idx: u16,
    used_idx: u16,
}

unsafe impl Send for VirtioGpuPci {}
unsafe impl Sync for VirtioGpuPci {}

pub static VIRTIO_GPU_PCI: SpinLock<Option<VirtioGpuPci>> = SpinLock::new(None);

#[inline]
unsafe fn r8(a: u64) -> u8 {
    core::ptr::read_volatile(a as *const u8)
}
#[inline]
unsafe fn w8(a: u64, v: u8) {
    core::ptr::write_volatile(a as *mut u8, v)
}
#[inline]
unsafe fn r16(a: u64) -> u16 {
    core::ptr::read_volatile(a as *const u16)
}
#[inline]
unsafe fn w16(a: u64, v: u16) {
    core::ptr::write_volatile(a as *mut u16, v)
}
#[inline]
unsafe fn r32(a: u64) -> u32 {
    core::ptr::read_volatile(a as *const u32)
}
#[inline]
unsafe fn w32(a: u64, v: u32) {
    core::ptr::write_volatile(a as *mut u32, v)
}
#[inline]
unsafe fn w64(a: u64, v: u64) {
    w32(a, v as u32);
    w32(a + 4, (v >> 32) as u32);
}

/// Finds a `virtio-gpu-pci` function on the PCIe bus.
pub fn probe() -> Option<PciDevice> {
    pci::scan_pci_bus().into_iter().find(|d| {
        d.vendor_id == virtio_pci::VIRTIO_VENDOR_ID
            && (d.device_id == virtio_pci::VIRTIO_GPU_DEVICE_ID
                || d.device_id == virtio_pci::VIRTIO_GPU_DEVICE_ID_LEGACY)
    })
}

impl VirtioGpuPci {
    /// Brings up a `virtio-gpu-pci` device found by [`probe`] and binds scanout 0
    /// to a `width`x`height` linear framebuffer.
    ///
    /// # Safety
    /// Programs the device's MMIO register blocks and a DMA virtqueue.
    pub unsafe fn init(dev: &PciDevice, width: usize, height: usize) -> Result<Self, &'static str> {
        dev.enable_bus_mastering();
        let regions: VirtioPciRegions =
            virtio_pci::discover_regions(dev).ok_or("no virtio pci capabilities")?;
        let common = regions.common_cfg;
        let notify = regions.notify_base;

        // Reset, then ACKNOWLEDGE | DRIVER.
        w8(common + CFG_DEVICE_STATUS, 0);
        w8(common + CFG_DEVICE_STATUS, STATUS_ACKNOWLEDGE);
        w8(common + CFG_DEVICE_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER);

        // Accept exactly VIRTIO_F_VERSION_1 (feature bit 32).
        w32(common + CFG_DEVICE_FEATURE_SELECT, 1);
        let hi = r32(common + CFG_DEVICE_FEATURE);
        if hi & 1 == 0 {
            return Err("device is not virtio 1.x");
        }
        w32(common + CFG_DRIVER_FEATURE_SELECT, 1);
        w32(common + CFG_DRIVER_FEATURE, 1);
        w32(common + CFG_DRIVER_FEATURE_SELECT, 0);
        w32(common + CFG_DRIVER_FEATURE, 0);

        w8(
            common + CFG_DEVICE_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK,
        );
        if r8(common + CFG_DEVICE_STATUS) & STATUS_FEATURES_OK == 0 {
            return Err("FEATURES_OK rejected");
        }

        if r16(common + CFG_NUM_QUEUES) == 0 {
            return Err("device exposes no virtqueues");
        }

        // Control queue (index 0).
        w16(common + CFG_QUEUE_SELECT, 0);
        let max = r16(common + CFG_QUEUE_SIZE);
        if max == 0 {
            return Err("control queue size is 0");
        }
        w16(common + CFG_QUEUE_SIZE, QUEUE_SIZE as u16);

        core::ptr::write_bytes(addr_of_mut!(PCI_VRING) as *mut u8, 0, core::mem::size_of::<VirtQueue>());
        let ring = addr_of!(PCI_VRING) as u64;
        let desc_pa = ring;
        let avail_pa = ring + core::mem::size_of::<[VirtqDesc; QUEUE_SIZE]>() as u64;
        let used_pa = ring + 4096;

        w64(common + CFG_QUEUE_DESC, desc_pa);
        w64(common + CFG_QUEUE_DRIVER, avail_pa);
        w64(common + CFG_QUEUE_DEVICE, used_pa);
        let notify_off = r16(common + CFG_QUEUE_NOTIFY_OFF) as u64;
        w16(common + CFG_QUEUE_ENABLE, 1);

        w8(
            common + CFG_DEVICE_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK,
        );

        let notify_addr = notify + notify_off * regions.notify_off_multiplier as u64;
        let fb_ptr = addr_of_mut!(PCI_FB_MEM.0) as *mut u8;
        let fb_len = width * height * 4;

        let mut dev = VirtioGpuPci {
            notify: notify_addr,
            width,
            height,
            stride: width,
            buffer_ptr: fb_ptr,
            buffer_len: fb_len,
            avail_idx: 0,
            used_idx: 0,
        };

        dev.exec_or(&CmdCreate2d {
            hdr: hdr(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D),
            resource_id: 1,
            format: VIRTIO_GPU_FORMAT_B8G8R8A8_UNORM,
            width: width as u32,
            height: height as u32,
        })?;
        dev.exec_or(&CmdAttachBacking {
            hdr: hdr(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING),
            resource_id: 1,
            nr_entries: 1,
            entry: MemEntry {
                addr: fb_ptr as u64,
                length: fb_len as u32,
                padding: 0,
            },
        })?;
        dev.exec_or(&CmdSetScanout {
            hdr: hdr(VIRTIO_GPU_CMD_SET_SCANOUT),
            r: rect(width as u32, height as u32),
            scanout_id: 0,
            resource_id: 1,
        })?;

        core::ptr::write_bytes(fb_ptr, 0, fb_len);
        dev.flush(0, 0, width as u32, height as u32);
        Ok(dev)
    }

    pub fn buffer_ptr(&self) -> *mut u8 {
        self.buffer_ptr
    }
    pub fn buffer_len(&self) -> usize {
        self.buffer_len
    }
    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn stride(&self) -> usize {
        self.stride
    }

    pub fn flush(&mut self, x: u32, y: u32, w: u32, h: u32) {
        let r = VirtioGpuRect {
            x,
            y,
            width: w,
            height: h,
        };
        let _ = unsafe {
            self.exec(&CmdTransfer2d {
                hdr: hdr(VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D),
                r,
                offset: 0,
                resource_id: 1,
                padding: 0,
            })
        };
        let _ = unsafe {
            self.exec(&CmdFlush {
                hdr: hdr(VIRTIO_GPU_CMD_RESOURCE_FLUSH),
                r,
                resource_id: 1,
                padding: 0,
            })
        };
    }

    unsafe fn exec_or<T: Copy>(&mut self, cmd: &T) -> Result<(), &'static str> {
        self.exec(cmd).map_err(|_| "virtio-gpu-pci command rejected")
    }

    unsafe fn exec<T: Copy>(&mut self, cmd: &T) -> Result<(), ()> {
        let len = core::mem::size_of::<T>();
        if len > 256 {
            return Err(());
        }
        let cmd_buf = addr_of_mut!(PCI_CMD_BUF) as *mut u8;
        core::ptr::copy_nonoverlapping(cmd as *const T as *const u8, cmd_buf, len);
        PCI_RESP_BUF = VirtioGpuCtrlHdr::default();

        PCI_VRING.desc[0] = VirtqDesc {
            addr: addr_of!(PCI_CMD_BUF) as u64,
            len: len as u32,
            flags: VRING_DESC_F_NEXT,
            next: 1,
        };
        PCI_VRING.desc[1] = VirtqDesc {
            addr: addr_of_mut!(PCI_RESP_BUF) as u64,
            len: core::mem::size_of::<VirtioGpuCtrlHdr>() as u32,
            flags: VRING_DESC_F_WRITE,
            next: 0,
        };

        let slot = (self.avail_idx as usize) % QUEUE_SIZE;
        PCI_VRING.avail_ring[slot] = 0;
        self.avail_idx = self.avail_idx.wrapping_add(1);
        core::arch::asm!("dmb sy", options(nomem, nostack));
        PCI_VRING.avail_idx = self.avail_idx;
        core::arch::asm!("dmb sy", options(nomem, nostack));

        w16(self.notify, 0); // notify queue 0

        let mut spin = 2_000_000u32;
        let used = addr_of!(PCI_VRING.used_idx);
        while core::ptr::read_volatile(used) == self.used_idx && spin > 0 {
            core::hint::spin_loop();
            spin -= 1;
        }
        if spin == 0 {
            return Err(());
        }
        self.used_idx = self.used_idx.wrapping_add(1);
        core::arch::asm!("dmb sy", options(nomem, nostack));

        if PCI_RESP_BUF.ctrl_type == VIRTIO_GPU_RESP_OK_NODATA {
            Ok(())
        } else {
            Err(())
        }
    }
}

fn hdr(ctrl_type: u32) -> VirtioGpuCtrlHdr {
    VirtioGpuCtrlHdr {
        ctrl_type,
        flags: 0,
        fence_id: 0,
        ctx_id: 0,
        padding: 0,
    }
}

fn rect(width: u32, height: u32) -> VirtioGpuRect {
    VirtioGpuRect {
        x: 0,
        y: 0,
        width,
        height,
    }
}

/// Flushes a region if a `virtio-gpu-pci` device is bound.
pub fn flush_screen_pci(x: usize, y: usize, w: usize, h: usize) {
    if let Some(gpu) = VIRTIO_GPU_PCI.lock().as_mut() {
        gpu.flush(x as u32, y as u32, w as u32, h as u32);
    }
}
