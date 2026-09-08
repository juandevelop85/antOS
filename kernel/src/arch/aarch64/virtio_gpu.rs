//! VirtIO GPU MMIO Driver for AArch64 bare-metal kernel.
//!
//! Controls QEMU / UTM VirtIO-GPU devices mapped over MMIO (compatible "virtio,mmio").
//! Supports both Version 1 (legacy MMIO) and Version 2 (modern MMIO).
//! Configures 2D resources, attaches linear framebuffer backing pages,
//! binds scanout display 0, and handles GPU flush requests.

use crate::sync::SpinLock;

// VirtIO MMIO Register Offsets
#[allow(dead_code)]
const MMIO_MAGIC_VALUE: usize = 0x000;
const MMIO_VERSION: usize = 0x004;
const MMIO_DEVICE_ID: usize = 0x008;
#[allow(dead_code)]
const MMIO_VENDOR_ID: usize = 0x00c;
#[allow(dead_code)]
const MMIO_DEVICE_FEATURES: usize = 0x010;
#[allow(dead_code)]
const MMIO_DEVICE_FEATURES_SEL: usize = 0x014;
#[allow(dead_code)]
const MMIO_DRIVER_FEATURES: usize = 0x020;
#[allow(dead_code)]
const MMIO_DRIVER_FEATURES_SEL: usize = 0x024;
const MMIO_GUEST_PAGE_SIZE: usize = 0x028; // Version 1 only
const MMIO_QUEUE_SEL: usize = 0x030;
const MMIO_QUEUE_NUM_MAX: usize = 0x034;
const MMIO_QUEUE_NUM: usize = 0x038;
const MMIO_QUEUE_ALIGN: usize = 0x03c;    // Version 1 only
const MMIO_QUEUE_PFN: usize = 0x040;      // Version 1 only
const MMIO_QUEUE_READY: usize = 0x044;    // Version 2 only
const MMIO_QUEUE_NOTIFY: usize = 0x050;
const MMIO_STATUS: usize = 0x070;
const MMIO_QUEUE_DESC_LOW: usize = 0x080;  // Version 2 only
const MMIO_QUEUE_DESC_HIGH: usize = 0x084; // Version 2 only
const MMIO_QUEUE_DRIVER_LOW: usize = 0x090;// Version 2 only
const MMIO_QUEUE_DRIVER_HIGH: usize = 0x094;// Version 2 only
const MMIO_QUEUE_DEVICE_LOW: usize = 0x0a0;// Version 2 only
const MMIO_QUEUE_DEVICE_HIGH: usize = 0x0a4;// Version 2 only

// Device status flags
const STATUS_ACKNOWLEDGE: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;

// VirtIO GPU Command Types
pub const VIRTIO_GPU_CMD_RESOURCE_CREATE_2D: u32 = 0x0101;
#[allow(dead_code)]
const VIRTIO_GPU_CMD_RESOURCE_UNREF: u32 = 0x0102;
pub const VIRTIO_GPU_CMD_SET_SCANOUT: u32 = 0x0103;
pub const VIRTIO_GPU_CMD_RESOURCE_FLUSH: u32 = 0x0104;
pub const VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D: u32 = 0x0105;
pub const VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING: u32 = 0x0106;

pub const VIRTIO_GPU_RESP_OK_NODATA: u32 = 0x1100;
pub const VIRTIO_GPU_FORMAT_B8G8R8A8_UNORM: u32 = 1;

const QUEUE_SIZE: usize = 16;

#[repr(C, align(16))]
#[derive(Clone, Copy, Default)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

const VRING_DESC_F_NEXT: u16 = 1;
const VRING_DESC_F_WRITE: u16 = 2;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

#[repr(C, align(4096))]
struct VirtQueueBuffer {
    descriptors: [VirtqDesc; QUEUE_SIZE],
    avail_flags: u16,
    avail_idx: u16,
    avail_ring: [u16; QUEUE_SIZE],
    avail_event: u16,
    _pad1: [u8; 4096 - (QUEUE_SIZE * 16 + 2 + 2 + QUEUE_SIZE * 2 + 2)],
    used_flags: u16,
    used_idx: u16,
    used_ring: [VirtqUsedElem; QUEUE_SIZE],
    used_event: u16,
    _pad2: [u8; 4096 - (2 + 2 + QUEUE_SIZE * 8 + 2)],
}

static mut GPU_VRING: VirtQueueBuffer = VirtQueueBuffer {
    descriptors: [VirtqDesc { addr: 0, len: 0, flags: 0, next: 0 }; QUEUE_SIZE],
    avail_flags: 0,
    avail_idx: 0,
    avail_ring: [0; QUEUE_SIZE],
    avail_event: 0,
    _pad1: [0; 4096 - (QUEUE_SIZE * 16 + 2 + 2 + QUEUE_SIZE * 2 + 2)],
    used_flags: 0,
    used_idx: 0,
    used_ring: [VirtqUsedElem { id: 0, len: 0 }; QUEUE_SIZE],
    used_event: 0,
    _pad2: [0; 4096 - (2 + 2 + QUEUE_SIZE * 8 + 2)],
};

static mut GPU_CMD_BUF: [u8; 256] = [0; 256];
static mut GPU_RESP_BUF: VirtioGpuCtrlHdr = VirtioGpuCtrlHdr {
    ctrl_type: 0,
    flags: 0,
    fence_id: 0,
    ctx_id: 0,
    padding: 0,
};

// GPU Command Packets
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct VirtioGpuCtrlHdr {
    pub ctrl_type: u32,
    pub flags: u32,
    pub fence_id: u64,
    pub ctx_id: u32,
    pub padding: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct VirtioGpuRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

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

/// Static buffer for VirtIO GPU Framebuffer backing: 1024x768 @ 32bpp = 3 MiB
/// Placed in .bss (covered by RAM MMU mapping 0x4000_0000..0x8000_0000)
#[repr(align(4096))]
struct BackingBuffer([u8; 1024 * 768 * 4]);
static mut VIRTIO_FB_MEM: BackingBuffer = BackingBuffer([0; 1024 * 768 * 4]);

pub struct VirtioGpu {
    mmio_base: u64,
    width: usize,
    height: usize,
    stride: usize,
    buffer_ptr: *mut u8,
    buffer_len: usize,
    last_avail_idx: u16,
    last_used_idx: u16,
}

unsafe impl Send for VirtioGpu {}
unsafe impl Sync for VirtioGpu {}

pub static VIRTIO_GPU: SpinLock<Option<VirtioGpu>> = SpinLock::new(None);

impl VirtioGpu {
    pub unsafe fn init(mmio_base: u64, width: usize, height: usize) -> Result<Self, ()> {
        let magic = core::ptr::read_volatile(mmio_base as *const u32);
        let version = core::ptr::read_volatile((mmio_base + MMIO_VERSION as u64) as *const u32);
        let device_id = core::ptr::read_volatile((mmio_base + MMIO_DEVICE_ID as u64) as *const u32);

        if magic != 0x74726976 || device_id != 16 {
            return Err(());
        }

        // Reset device
        core::ptr::write_volatile((mmio_base + MMIO_STATUS as u64) as *mut u32, 0);

        // Acknowledge and Driver
        core::ptr::write_volatile((mmio_base + MMIO_STATUS as u64) as *mut u32, STATUS_ACKNOWLEDGE);
        core::ptr::write_volatile((mmio_base + MMIO_STATUS as u64) as *mut u32, STATUS_ACKNOWLEDGE | STATUS_DRIVER);

        // Feature negotiation
        if version == 2 {
            core::ptr::write_volatile((mmio_base + MMIO_DEVICE_FEATURES_SEL as u64) as *mut u32, 1);
            let dev_f1 = core::ptr::read_volatile((mmio_base + MMIO_DEVICE_FEATURES as u64) as *const u32);
            core::ptr::write_volatile((mmio_base + MMIO_DRIVER_FEATURES_SEL as u64) as *mut u32, 1);
            core::ptr::write_volatile((mmio_base + MMIO_DRIVER_FEATURES as u64) as *mut u32, dev_f1 & 1); // VIRTIO_F_VERSION_1

            core::ptr::write_volatile((mmio_base + MMIO_DEVICE_FEATURES_SEL as u64) as *mut u32, 0);
            core::ptr::write_volatile((mmio_base + MMIO_DRIVER_FEATURES_SEL as u64) as *mut u32, 0);
            core::ptr::write_volatile((mmio_base + MMIO_DRIVER_FEATURES as u64) as *mut u32, 0);

            core::ptr::write_volatile((mmio_base + MMIO_STATUS as u64) as *mut u32,
                STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK);
        }

        // Setup Queue 0 (control queue)
        core::ptr::write_volatile((mmio_base + MMIO_QUEUE_SEL as u64) as *mut u32, 0);
        let q_max = core::ptr::read_volatile((mmio_base + MMIO_QUEUE_NUM_MAX as u64) as *const u32);
        if q_max == 0 {
            return Err(());
        }

        core::ptr::write_volatile((mmio_base + MMIO_QUEUE_NUM as u64) as *mut u32, QUEUE_SIZE as u32);

        // Zero out the VRing memory
        core::ptr::write_bytes(core::ptr::addr_of_mut!(GPU_VRING) as *mut u8, 0, core::mem::size_of::<VirtQueueBuffer>());

        let ring_paddr = core::ptr::addr_of!(GPU_VRING) as u64;

        if version == 1 {
            // Version 1 (Legacy MMIO)
            core::ptr::write_volatile((mmio_base + MMIO_GUEST_PAGE_SIZE as u64) as *mut u32, 4096);
            core::ptr::write_volatile((mmio_base + MMIO_QUEUE_ALIGN as u64) as *mut u32, 4096);
            let pfn = (ring_paddr >> 12) as u32;
            core::ptr::write_volatile((mmio_base + MMIO_QUEUE_PFN as u64) as *mut u32, pfn);
        } else {
            // Version 2 (Modern MMIO)
            let desc_paddr = ring_paddr;
            let avail_paddr = ring_paddr + core::mem::size_of::<[VirtqDesc; QUEUE_SIZE]>() as u64;
            let used_paddr = ring_paddr + 4096;

            core::ptr::write_volatile((mmio_base + MMIO_QUEUE_DESC_LOW as u64) as *mut u32, desc_paddr as u32);
            core::ptr::write_volatile((mmio_base + MMIO_QUEUE_DESC_HIGH as u64) as *mut u32, (desc_paddr >> 32) as u32);
            core::ptr::write_volatile((mmio_base + MMIO_QUEUE_DRIVER_LOW as u64) as *mut u32, avail_paddr as u32);
            core::ptr::write_volatile((mmio_base + MMIO_QUEUE_DRIVER_HIGH as u64) as *mut u32, (avail_paddr >> 32) as u32);
            core::ptr::write_volatile((mmio_base + MMIO_QUEUE_DEVICE_LOW as u64) as *mut u32, used_paddr as u32);
            core::ptr::write_volatile((mmio_base + MMIO_QUEUE_DEVICE_HIGH as u64) as *mut u32, (used_paddr >> 32) as u32);
            core::ptr::write_volatile((mmio_base + MMIO_QUEUE_READY as u64) as *mut u32, 1);
        }

        // Set DRIVER_OK
        let final_status = if version == 2 {
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK
        } else {
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_DRIVER_OK
        };
        core::ptr::write_volatile((mmio_base + MMIO_STATUS as u64) as *mut u32, final_status);

        let fb_ptr = core::ptr::addr_of_mut!(VIRTIO_FB_MEM.0) as *mut u8;
        let fb_len = width * height * 4;
        let fb_paddr = fb_ptr as u64;

        let mut dev = VirtioGpu {
            mmio_base,
            width,
            height,
            stride: width,
            buffer_ptr: fb_ptr,
            buffer_len: fb_len,
            last_avail_idx: 0,
            last_used_idx: 0,
        };

        // Command 1: RESOURCE_CREATE_2D (Resource ID 1)
        let create_cmd = CmdCreate2d {
            hdr: VirtioGpuCtrlHdr {
                ctrl_type: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                padding: 0,
            },
            resource_id: 1,
            format: VIRTIO_GPU_FORMAT_B8G8R8A8_UNORM,
            width: width as u32,
            height: height as u32,
        };
        dev.exec_cmd(&create_cmd)?;

        // Command 2: RESOURCE_ATTACH_BACKING
        let attach_cmd = CmdAttachBacking {
            hdr: VirtioGpuCtrlHdr {
                ctrl_type: VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                padding: 0,
            },
            resource_id: 1,
            nr_entries: 1,
            entry: MemEntry {
                addr: fb_paddr,
                length: fb_len as u32,
                padding: 0,
            },
        };
        dev.exec_cmd(&attach_cmd)?;

        // Command 3: SET_SCANOUT
        let scanout_cmd = CmdSetScanout {
            hdr: VirtioGpuCtrlHdr {
                ctrl_type: VIRTIO_GPU_CMD_SET_SCANOUT,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                padding: 0,
            },
            r: VirtioGpuRect {
                x: 0,
                y: 0,
                width: width as u32,
                height: height as u32,
            },
            scanout_id: 0,
            resource_id: 1,
        };
        dev.exec_cmd(&scanout_cmd)?;

        // Initial clear and flush
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

    pub fn flush(&mut self, x: u32, y: u32, width: u32, height: u32) {
        let rect = VirtioGpuRect { x, y, width, height };

        // 1. Transfer to host 2D
        let transfer_cmd = CmdTransfer2d {
            hdr: VirtioGpuCtrlHdr {
                ctrl_type: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                padding: 0,
            },
            r: rect,
            offset: 0,
            resource_id: 1,
            padding: 0,
        };
        let _ = unsafe { self.exec_cmd(&transfer_cmd) };

        // 2. Resource flush
        let flush_cmd = CmdFlush {
            hdr: VirtioGpuCtrlHdr {
                ctrl_type: VIRTIO_GPU_CMD_RESOURCE_FLUSH,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                padding: 0,
            },
            r: rect,
            resource_id: 1,
            padding: 0,
        };
        let _ = unsafe { self.exec_cmd(&flush_cmd) };
    }

    unsafe fn exec_cmd<T: Copy>(&mut self, cmd: &T) -> Result<(), ()> {
        let cmd_len = core::mem::size_of::<T>();
        if cmd_len > 256 {
            return Err(());
        }

        let cmd_buf_ptr = core::ptr::addr_of_mut!(GPU_CMD_BUF) as *mut u8;
        core::ptr::copy_nonoverlapping(cmd as *const T as *const u8, cmd_buf_ptr, cmd_len);
        GPU_RESP_BUF = VirtioGpuCtrlHdr::default();

        let desc_req = 0;
        let desc_resp = 1;

        GPU_VRING.descriptors[desc_req] = VirtqDesc {
            addr: core::ptr::addr_of!(GPU_CMD_BUF) as u64,
            len: cmd_len as u32,
            flags: VRING_DESC_F_NEXT,
            next: desc_resp as u16,
        };

        GPU_VRING.descriptors[desc_resp] = VirtqDesc {
            addr: core::ptr::addr_of_mut!(GPU_RESP_BUF) as u64,
            len: core::mem::size_of::<VirtioGpuCtrlHdr>() as u32,
            flags: VRING_DESC_F_WRITE,
            next: 0,
        };

        let avail_slot = (self.last_avail_idx as usize) % QUEUE_SIZE;
        GPU_VRING.avail_ring[avail_slot] = desc_req as u16;
        self.last_avail_idx = self.last_avail_idx.wrapping_add(1);

        core::arch::asm!("dmb sy", options(nomem, nostack));
        GPU_VRING.avail_idx = self.last_avail_idx;
        core::arch::asm!("dmb sy", options(nomem, nostack));

        // Notify queue 0
        core::ptr::write_volatile((self.mmio_base + MMIO_QUEUE_NOTIFY as u64) as *mut u32, 0);

        // Wait for response with timeout
        let mut timeout = 2_000_000;
        let used_idx_ptr = core::ptr::addr_of!(GPU_VRING.used_idx);
        while core::ptr::read_volatile(used_idx_ptr) == self.last_used_idx && timeout > 0 {
            core::hint::spin_loop();
            timeout -= 1;
        }

        if timeout == 0 {
            return Err(());
        }

        self.last_used_idx = self.last_used_idx.wrapping_add(1);
        core::arch::asm!("dmb sy", options(nomem, nostack));

        if GPU_RESP_BUF.ctrl_type == VIRTIO_GPU_RESP_OK_NODATA {
            Ok(())
        } else {
            Err(())
        }
    }
}

/// Flushes the entire screen or a modified rectangle to whichever VirtIO-GPU
/// transport is active (MMIO or PCIe).
pub fn flush_screen(x: usize, y: usize, width: usize, height: usize) {
    {
        let mut guard = VIRTIO_GPU.lock();
        if let Some(gpu) = guard.as_mut() {
            gpu.flush(x as u32, y as u32, width as u32, height as u32);
        }
    }
    super::virtio_gpu_pci::flush_screen_pci(x, y, width, height);
}

/// Scans standard AArch64 VirtIO MMIO slots (0x0a00_0000..0x0a00_4000) for a GPU device.
pub fn probe_virtio_gpu() -> Option<u64> {
    for i in 0..32 {
        let base = 0x0a00_0000 + (i as u64) * 0x200;
        unsafe {
            let magic = core::ptr::read_volatile(base as *const u32);
            let device_id = core::ptr::read_volatile((base + MMIO_DEVICE_ID as u64) as *const u32);
            if magic == 0x74726976 && device_id == 16 {
                return Some(base);
            }
        }
    }
    None
}
