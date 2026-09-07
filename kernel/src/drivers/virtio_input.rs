//! VirtIO Input MMIO Driver for AArch64 & bare-metal kernel.
//!
//! Implements support for VirtIO Input devices (keyboards, mice, tablets)
//! over MMIO (Device ID 18 / 0x12).
//! Configures Virtqueue 0 (eventq) with pre-allocated buffer descriptors,
//! retrieves incoming input events, decodes Linux EV_* packets, and
//! enqueues them into the kernel's global input queue.

use crate::input::{decode_linux_ev, push_event};
use crate::sync::SpinLock;

// VirtIO MMIO Register Offsets
const MMIO_MAGIC_VALUE: usize = 0x000;
const MMIO_VERSION: usize = 0x004;
const MMIO_DEVICE_ID: usize = 0x008;
const MMIO_DEVICE_FEATURES: usize = 0x010;
const MMIO_DEVICE_FEATURES_SEL: usize = 0x014;
const MMIO_DRIVER_FEATURES: usize = 0x020;
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

// Config space offsets (0x100+)
const MMIO_CONFIG_SELECT: usize = 0x100;
const MMIO_CONFIG_SUBSEL: usize = 0x101;
const MMIO_CONFIG_SIZE: usize = 0x102;
const MMIO_CONFIG_DATA: usize = 0x108;

const VIRTIO_INPUT_CFG_ID_NAME: u8 = 0x01;

// Device status flags
const STATUS_ACKNOWLEDGE: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;

const QUEUE_SIZE: usize = 16;
const VRING_DESC_F_WRITE: u16 = 2;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct VirtioInputRawEvent {
    pub event_type: u16,
    pub code: u16,
    pub value: u32,
}

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

const MAX_DEVICES: usize = 2;

static mut INPUT_VRINGS: [VirtQueueBuffer; MAX_DEVICES] = [
    VirtQueueBuffer {
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
    },
    VirtQueueBuffer {
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
    },
];

static mut INPUT_EVENT_BUFS: [[VirtioInputRawEvent; QUEUE_SIZE]; MAX_DEVICES] = [
    [VirtioInputRawEvent { event_type: 0, code: 0, value: 0 }; QUEUE_SIZE],
    [VirtioInputRawEvent { event_type: 0, code: 0, value: 0 }; QUEUE_SIZE],
];

/// A discovered and initialized VirtIO Input Device.
pub struct VirtioInputDevice {
    mmio_base: u64,
    #[allow(dead_code)]
    version: u32,
    device_index: usize,
    last_used_idx: u16,
    avail_idx: u16,
    name: [u8; 32],
    name_len: usize,
}

impl VirtioInputDevice {
    /// Initializes a VirtIO Input MMIO device at the given base address.
    pub unsafe fn init(mmio_base: u64, device_index: usize) -> Result<Self, ()> {
        if device_index >= MAX_DEVICES {
            return Err(());
        }

        let magic = core::ptr::read_volatile((mmio_base + MMIO_MAGIC_VALUE as u64) as *const u32);
        let version = core::ptr::read_volatile((mmio_base + MMIO_VERSION as u64) as *const u32);
        let device_id = core::ptr::read_volatile((mmio_base + MMIO_DEVICE_ID as u64) as *const u32);

        // magic == "virt" (0x74726976) and device_id == 18 (Input)
        if magic != 0x74726976 || device_id != 18 {
            return Err(());
        }

        // 1. Reset device
        core::ptr::write_volatile((mmio_base + MMIO_STATUS as u64) as *mut u32, 0);

        // 2. Acknowledge and Driver status
        core::ptr::write_volatile((mmio_base + MMIO_STATUS as u64) as *mut u32, STATUS_ACKNOWLEDGE);
        core::ptr::write_volatile((mmio_base + MMIO_STATUS as u64) as *mut u32, STATUS_ACKNOWLEDGE | STATUS_DRIVER);

        // 3. Feature negotiation
        if version == 2 {
            core::ptr::write_volatile((mmio_base + MMIO_DEVICE_FEATURES_SEL as u64) as *mut u32, 1);
            let dev_f1 = core::ptr::read_volatile((mmio_base + MMIO_DEVICE_FEATURES as u64) as *const u32);
            core::ptr::write_volatile((mmio_base + MMIO_DRIVER_FEATURES_SEL as u64) as *mut u32, 1);
            core::ptr::write_volatile((mmio_base + MMIO_DRIVER_FEATURES as u64) as *mut u32, dev_f1 & 1);

            core::ptr::write_volatile((mmio_base + MMIO_DEVICE_FEATURES_SEL as u64) as *mut u32, 0);
            core::ptr::write_volatile((mmio_base + MMIO_DRIVER_FEATURES_SEL as u64) as *mut u32, 0);
            core::ptr::write_volatile((mmio_base + MMIO_DRIVER_FEATURES as u64) as *mut u32, 0);

            core::ptr::write_volatile((mmio_base + MMIO_STATUS as u64) as *mut u32,
                STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK);
        }

        // 4. Setup Queue 0 (eventq)
        core::ptr::write_volatile((mmio_base + MMIO_QUEUE_SEL as u64) as *mut u32, 0);
        let q_max = core::ptr::read_volatile((mmio_base + MMIO_QUEUE_NUM_MAX as u64) as *const u32);
        if q_max == 0 {
            return Err(());
        }

        core::ptr::write_volatile((mmio_base + MMIO_QUEUE_NUM as u64) as *mut u32, QUEUE_SIZE as u32);

        // Zero out the VRing memory for this device
        let vring_ptr = core::ptr::addr_of_mut!(INPUT_VRINGS[device_index]);
        core::ptr::write_bytes(vring_ptr as *mut u8, 0, core::mem::size_of::<VirtQueueBuffer>());

        let ring_paddr = vring_ptr as u64;

        if version == 1 {
            core::ptr::write_volatile((mmio_base + MMIO_GUEST_PAGE_SIZE as u64) as *mut u32, 4096);
            core::ptr::write_volatile((mmio_base + MMIO_QUEUE_ALIGN as u64) as *mut u32, 4096);
            let pfn = (ring_paddr >> 12) as u32;
            core::ptr::write_volatile((mmio_base + MMIO_QUEUE_PFN as u64) as *mut u32, pfn);
        } else {
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

        // 5. Populate initial available ring with pre-allocated buffer descriptors
        let vring = &mut *vring_ptr;
        for i in 0..QUEUE_SIZE {
            let buf_paddr = core::ptr::addr_of!(INPUT_EVENT_BUFS[device_index][i]) as u64;
            vring.descriptors[i] = VirtqDesc {
                addr: buf_paddr,
                len: core::mem::size_of::<VirtioInputRawEvent>() as u32,
                flags: VRING_DESC_F_WRITE,
                next: 0,
            };
            vring.avail_ring[i] = i as u16;
        }

        vring.avail_idx = QUEUE_SIZE as u16;
        core::arch::asm!("dmb sy", options(nomem, nostack));

        // 6. Set DRIVER_OK
        let final_status = if version == 2 {
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK
        } else {
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_DRIVER_OK
        };
        core::ptr::write_volatile((mmio_base + MMIO_STATUS as u64) as *mut u32, final_status);

        // Notify device that buffers are available in Queue 0
        core::ptr::write_volatile((mmio_base + MMIO_QUEUE_NOTIFY as u64) as *mut u32, 0);

        // Read device name from config space
        let mut name = [0u8; 32];
        core::ptr::write_volatile((mmio_base + MMIO_CONFIG_SELECT as u64) as *mut u8, VIRTIO_INPUT_CFG_ID_NAME);
        core::ptr::write_volatile((mmio_base + MMIO_CONFIG_SUBSEL as u64) as *mut u8, 0);
        let size = core::ptr::read_volatile((mmio_base + MMIO_CONFIG_SIZE as u64) as *const u8) as usize;
        let copy_len = if size < 32 { size } else { 31 };
        for i in 0..copy_len {
            name[i] = core::ptr::read_volatile((mmio_base + MMIO_CONFIG_DATA as u64 + i as u64) as *const u8);
        }
        let name_len = copy_len;

        Ok(Self {
            mmio_base,
            version,
            device_index,
            last_used_idx: 0,
            avail_idx: QUEUE_SIZE as u16,
            name,
            name_len,
        })
    }

    /// Device name reported by VirtIO config space.
    pub fn name_str(&self) -> &str {
        if self.name_len == 0 {
            "VirtIO Input"
        } else {
            core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("VirtIO Input")
        }
    }

    /// Polls and consumes newly available used descriptors, forwarding events to the kernel queue.
    pub fn poll_events(&mut self) -> usize {
        let vring_ptr = unsafe { core::ptr::addr_of_mut!(INPUT_VRINGS[self.device_index]) };
        let vring = unsafe { &mut *vring_ptr };

        let used_idx = unsafe { core::ptr::read_volatile(&vring.used_idx) };
        let mut processed = 0;

        while self.last_used_idx != used_idx {
            let slot = (self.last_used_idx as usize) % QUEUE_SIZE;
            let used_elem = vring.used_ring[slot];
            let desc_id = (used_elem.id as usize) % QUEUE_SIZE;

            let raw_event = unsafe { INPUT_EVENT_BUFS[self.device_index][desc_id] };

            if let Some(event) = decode_linux_ev(raw_event.event_type, raw_event.code, raw_event.value) {
                push_event(event);
            }

            // Replenish descriptor back to the avail ring
            let avail_slot = (self.avail_idx as usize) % QUEUE_SIZE;
            vring.avail_ring[avail_slot] = desc_id as u16;
            self.avail_idx = self.avail_idx.wrapping_add(1);

            unsafe {
                core::ptr::write_volatile(&mut vring.avail_idx, self.avail_idx);
            }

            self.last_used_idx = self.last_used_idx.wrapping_add(1);
            processed += 1;
        }

        if processed > 0 {
            unsafe {
                core::arch::asm!("dmb sy", options(nomem, nostack));
                core::ptr::write_volatile((self.mmio_base + MMIO_QUEUE_NOTIFY as u64) as *mut u32, 0);
            }
        }

        processed
    }
}

pub struct VirtioInputManager {
    devices: [Option<VirtioInputDevice>; MAX_DEVICES],
    count: usize,
}

impl VirtioInputManager {
    pub const fn empty() -> Self {
        Self {
            devices: [None, None],
            count: 0,
        }
    }

    pub fn register(&mut self, device: VirtioInputDevice) -> bool {
        if self.count < MAX_DEVICES {
            self.devices[self.count] = Some(device);
            self.count += 1;
            true
        } else {
            false
        }
    }

    pub fn poll_all(&mut self) -> usize {
        let mut total = 0;
        for dev in self.devices.iter_mut().flatten() {
            total += dev.poll_events();
        }
        total
    }

    pub fn count(&self) -> usize {
        self.count
    }
}

pub static VIRTIO_INPUTS: SpinLock<VirtioInputManager> = SpinLock::new(VirtioInputManager::empty());

/// Scans standard AArch64 VirtIO MMIO slots (0x0a00_0000..0x0a00_4000) for VirtIO-Input devices.
pub fn probe_and_init_virtio_inputs() -> usize {
    let mut discovered = 0;

    for i in 0..32 {
        if discovered >= MAX_DEVICES {
            break;
        }

        let base = 0x0a00_0000 + (i as u64) * 0x200;
        unsafe {
            let magic = core::ptr::read_volatile(base as *const u32);
            let device_id = core::ptr::read_volatile((base + MMIO_DEVICE_ID as u64) as *const u32);

            if magic == 0x74726976 && device_id == 18 {
                if let Ok(dev) = VirtioInputDevice::init(base, discovered) {
                    VIRTIO_INPUTS.lock().register(dev);
                    discovered += 1;
                }
            }
        }
    }

    discovered
}

/// Polls all registered VirtIO input devices.
pub fn poll_virtio_inputs() -> usize {
    VIRTIO_INPUTS.lock().poll_all()
}
