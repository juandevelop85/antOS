//! VirtIO Input MMIO Driver for AArch64 & bare-metal kernel.
//!
//! Implements support for VirtIO Input devices (keyboards, mice, tablets)
//! over MMIO (Device ID 18 / 0x12). For each device it:
//!   * negotiates the device status handshake (legacy MMIO v1 and modern v2),
//!   * reads the `EV_BITS` / `ABS_INFO` config blocks to classify the device
//!     (keyboard / mouse / tablet) and calibrate its absolute axes,
//!   * configures the event queue (queue 0) with pre-allocated buffers,
//!   * configures the status queue (queue 1) for `EV_LED` output reports,
//!   * decodes the incoming Linux `EV_*` stream through an
//!     [`EvdevAccumulator`] so relative motion and absolute position each
//!     surface as one coalesced `InputEvent` per `SYN_REPORT` packet, and
//!     enqueues them into `GLOBAL_INPUT_QUEUE`.

use crate::input::{
    push_event, AbsAxisInfo, EvdevAccumulator, EV_ABS, EV_KEY, EV_REL, LED_CAPSL, LED_NUML,
    LED_SCROLLL,
};
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
const MMIO_QUEUE_ALIGN: usize = 0x03c; // Version 1 only
const MMIO_QUEUE_PFN: usize = 0x040; // Version 1 only
const MMIO_QUEUE_READY: usize = 0x044; // Version 2 only
const MMIO_QUEUE_NOTIFY: usize = 0x050;
const MMIO_STATUS: usize = 0x070;
const MMIO_QUEUE_DESC_LOW: usize = 0x080; // Version 2 only
const MMIO_QUEUE_DESC_HIGH: usize = 0x084; // Version 2 only
const MMIO_QUEUE_DRIVER_LOW: usize = 0x090; // Version 2 only
const MMIO_QUEUE_DRIVER_HIGH: usize = 0x094; // Version 2 only
const MMIO_QUEUE_DEVICE_LOW: usize = 0x0a0; // Version 2 only
const MMIO_QUEUE_DEVICE_HIGH: usize = 0x0a4; // Version 2 only

// Config space (0x100+)
const MMIO_CONFIG_SELECT: usize = 0x100;
const MMIO_CONFIG_SUBSEL: usize = 0x101;
const MMIO_CONFIG_SIZE: usize = 0x102;
const MMIO_CONFIG_DATA: usize = 0x108;

// VirtIO Input config `select` values (§5.8.5.1 of the VirtIO spec).
const VIRTIO_INPUT_CFG_ID_NAME: u8 = 0x01;
const VIRTIO_INPUT_CFG_EV_BITS: u8 = 0x11;
const VIRTIO_INPUT_CFG_ABS_INFO: u8 = 0x12;

// Device status flags
const STATUS_ACKNOWLEDGE: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;

// VIRTIO_F_VERSION_1 is feature bit 32 -> bit 0 of the high feature word.
const VIRTIO_F_VERSION_1_HI: u32 = 1;

const QUEUE_SIZE: usize = 16;
const VRING_DESC_F_WRITE: u16 = 2;

const EVENT_QUEUE: u32 = 0;
const STATUS_QUEUE: u32 = 1;

/// One `struct virtio_input_event` (Linux evdev triple over the wire).
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

const PAD1: usize = 4096 - (QUEUE_SIZE * 16 + 2 + 2 + QUEUE_SIZE * 2 + 2);
const PAD2: usize = 4096 - (2 + 2 + QUEUE_SIZE * 8 + 2);

#[repr(C, align(4096))]
struct VirtQueueBuffer {
    descriptors: [VirtqDesc; QUEUE_SIZE],
    avail_flags: u16,
    avail_idx: u16,
    avail_ring: [u16; QUEUE_SIZE],
    avail_event: u16,
    _pad1: [u8; PAD1],
    used_flags: u16,
    used_idx: u16,
    used_ring: [VirtqUsedElem; QUEUE_SIZE],
    used_event: u16,
    _pad2: [u8; PAD2],
}

impl VirtQueueBuffer {
    const fn zeroed() -> Self {
        Self {
            descriptors: [VirtqDesc { addr: 0, len: 0, flags: 0, next: 0 }; QUEUE_SIZE],
            avail_flags: 0,
            avail_idx: 0,
            avail_ring: [0; QUEUE_SIZE],
            avail_event: 0,
            _pad1: [0; PAD1],
            used_flags: 0,
            used_idx: 0,
            used_ring: [VirtqUsedElem { id: 0, len: 0 }; QUEUE_SIZE],
            used_event: 0,
            _pad2: [0; PAD2],
        }
    }
}

/// Up to this many VirtIO-Input devices are driven at once (keyboard + mouse +
/// tablet + spare).
const MAX_DEVICES: usize = 4;

const EMPTY_VRING: VirtQueueBuffer = VirtQueueBuffer::zeroed();
const EMPTY_EVENT: VirtioInputRawEvent = VirtioInputRawEvent { event_type: 0, code: 0, value: 0 };

static mut EVENT_VRINGS: [VirtQueueBuffer; MAX_DEVICES] = [EMPTY_VRING; MAX_DEVICES];
static mut STATUS_VRINGS: [VirtQueueBuffer; MAX_DEVICES] = [EMPTY_VRING; MAX_DEVICES];
static mut EVENT_BUFS: [[VirtioInputRawEvent; QUEUE_SIZE]; MAX_DEVICES] =
    [[EMPTY_EVENT; QUEUE_SIZE]; MAX_DEVICES];
static mut STATUS_BUFS: [[VirtioInputRawEvent; QUEUE_SIZE]; MAX_DEVICES] =
    [[EMPTY_EVENT; QUEUE_SIZE]; MAX_DEVICES];

/// Coarse device role, derived from the `EV_BITS` config block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Keyboard,
    Mouse,
    Tablet,
    Unknown,
}

impl DeviceKind {
    fn label(self) -> &'static str {
        match self {
            DeviceKind::Keyboard => "teclado",
            DeviceKind::Mouse => "raton",
            DeviceKind::Tablet => "tablet",
            DeviceKind::Unknown => "hid",
        }
    }
}

/// A discovered and initialized VirtIO Input MMIO device.
pub struct VirtioInputDevice {
    mmio_base: u64,
    #[allow(dead_code)]
    version: u32,
    device_index: usize,
    kind: DeviceKind,
    last_used_idx: u16,
    event_avail_idx: u16,
    status_avail_idx: u16,
    status_next_buf: usize,
    accum: EvdevAccumulator,
    name: [u8; 32],
    name_len: usize,
}

impl VirtioInputDevice {
    /// Initializes the VirtIO Input MMIO device at `mmio_base`, using the
    /// `device_index`-th slice of the static ring/buffer pools.
    ///
    /// # Safety
    /// `mmio_base` must be the base of a mapped VirtIO MMIO transport window,
    /// and `device_index` must be unique per live device (`< MAX_DEVICES`);
    /// each index owns a disjoint slice of the static ring/buffer pools.
    pub unsafe fn init(mmio_base: u64, device_index: usize) -> Result<Self, ()> {
        if device_index >= MAX_DEVICES {
            return Err(());
        }

        let magic = read32(mmio_base, MMIO_MAGIC_VALUE);
        let version = read32(mmio_base, MMIO_VERSION);
        let device_id = read32(mmio_base, MMIO_DEVICE_ID);
        if magic != 0x7472_6976 || device_id != 18 {
            return Err(());
        }

        // 1. Reset, then ACKNOWLEDGE | DRIVER.
        write32(mmio_base, MMIO_STATUS, 0);
        write32(mmio_base, MMIO_STATUS, STATUS_ACKNOWLEDGE);
        write32(mmio_base, MMIO_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER);

        // 2. Feature negotiation. On modern (v2) MMIO we accept only
        //    VIRTIO_F_VERSION_1 and must latch FEATURES_OK; legacy (v1) has no
        //    such step.
        if version >= 2 {
            write32(mmio_base, MMIO_DEVICE_FEATURES_SEL, 1);
            let dev_hi = read32(mmio_base, MMIO_DEVICE_FEATURES);
            write32(mmio_base, MMIO_DRIVER_FEATURES_SEL, 1);
            write32(mmio_base, MMIO_DRIVER_FEATURES, dev_hi & VIRTIO_F_VERSION_1_HI);
            write32(mmio_base, MMIO_DEVICE_FEATURES_SEL, 0);
            write32(mmio_base, MMIO_DRIVER_FEATURES_SEL, 0);
            write32(mmio_base, MMIO_DRIVER_FEATURES, 0);

            write32(
                mmio_base,
                MMIO_STATUS,
                STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK,
            );
            if read32(mmio_base, MMIO_STATUS) & STATUS_FEATURES_OK == 0 {
                return Err(());
            }
        }

        // 3. Classify the device and read absolute-axis calibration.
        let has_rel = config_block_nonzero(mmio_base, VIRTIO_INPUT_CFG_EV_BITS, EV_REL as u8);
        let has_abs = config_block_nonzero(mmio_base, VIRTIO_INPUT_CFG_EV_BITS, EV_ABS as u8);
        let kind = if has_abs {
            DeviceKind::Tablet
        } else if has_rel {
            DeviceKind::Mouse
        } else {
            let has_key = config_block_nonzero(mmio_base, VIRTIO_INPUT_CFG_EV_BITS, EV_KEY as u8);
            if has_key {
                DeviceKind::Keyboard
            } else {
                DeviceKind::Unknown
            }
        };

        let mut accum = EvdevAccumulator::new();
        if has_abs {
            if let Some(info) = read_abs_info(mmio_base, crate::input::ABS_X as u8) {
                accum.abs_x_info = info;
            }
            if let Some(info) = read_abs_info(mmio_base, crate::input::ABS_Y as u8) {
                accum.abs_y_info = info;
            }
        }

        // 4. Configure the event queue (0) and the status queue (1).
        setup_queue(
            mmio_base,
            version,
            EVENT_QUEUE,
            core::ptr::addr_of_mut!(EVENT_VRINGS[device_index]),
        )?;
        setup_queue(
            mmio_base,
            version,
            STATUS_QUEUE,
            core::ptr::addr_of_mut!(STATUS_VRINGS[device_index]),
        )
        .ok(); // A device without a status queue is still usable (no LEDs).

        // 5. Publish inbound buffers on the event queue.
        let event_vring = &mut *core::ptr::addr_of_mut!(EVENT_VRINGS[device_index]);
        for i in 0..QUEUE_SIZE {
            event_vring.descriptors[i] = VirtqDesc {
                addr: core::ptr::addr_of!(EVENT_BUFS[device_index][i]) as u64,
                len: core::mem::size_of::<VirtioInputRawEvent>() as u32,
                flags: VRING_DESC_F_WRITE,
                next: 0,
            };
            event_vring.avail_ring[i] = i as u16;
        }
        event_vring.avail_idx = QUEUE_SIZE as u16;
        core::arch::asm!("dmb sy", options(nomem, nostack));

        // 6. DRIVER_OK, then kick the event queue.
        let final_status = if version >= 2 {
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK
        } else {
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_DRIVER_OK
        };
        write32(mmio_base, MMIO_STATUS, final_status);
        write32(mmio_base, MMIO_QUEUE_NOTIFY, EVENT_QUEUE);

        // 7. Device name (best effort).
        let mut name = [0u8; 32];
        write8(mmio_base, MMIO_CONFIG_SELECT, VIRTIO_INPUT_CFG_ID_NAME);
        write8(mmio_base, MMIO_CONFIG_SUBSEL, 0);
        let size = read8(mmio_base, MMIO_CONFIG_SIZE) as usize;
        let name_len = size.min(31);
        for (i, slot) in name.iter_mut().take(name_len).enumerate() {
            *slot = read8(mmio_base, MMIO_CONFIG_DATA + i);
        }

        Ok(Self {
            mmio_base,
            version,
            device_index,
            kind,
            last_used_idx: 0,
            event_avail_idx: QUEUE_SIZE as u16,
            status_avail_idx: 0,
            status_next_buf: 0,
            accum,
            name,
            name_len,
        })
    }

    /// Role inferred from the device's `EV_BITS`.
    pub fn kind(&self) -> DeviceKind {
        self.kind
    }

    /// Device name reported by VirtIO config space.
    pub fn name_str(&self) -> &str {
        if self.name_len == 0 {
            "VirtIO Input"
        } else {
            core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("VirtIO Input")
        }
    }

    /// Drains newly used event descriptors, feeds the evdev accumulator with
    /// the current screen resolution, and forwards coalesced events to the
    /// kernel queue. Returns the number of raw evdev triples consumed.
    pub fn poll_events(&mut self, screen: (u32, u32)) -> usize {
        let vring = unsafe { &mut *core::ptr::addr_of_mut!(EVENT_VRINGS[self.device_index]) };
        let used_idx = unsafe { core::ptr::read_volatile(&vring.used_idx) };
        let mut processed = 0usize;

        while self.last_used_idx != used_idx {
            let slot = (self.last_used_idx as usize) % QUEUE_SIZE;
            let desc_id = (vring.used_ring[slot].id as usize) % QUEUE_SIZE;
            let raw = unsafe { EVENT_BUFS[self.device_index][desc_id] };

            if let Some(events) = self.accum.feed(raw.event_type, raw.code, raw.value, screen) {
                for ev in events {
                    push_event(ev);
                }
            }

            let avail_slot = (self.event_avail_idx as usize) % QUEUE_SIZE;
            vring.avail_ring[avail_slot] = desc_id as u16;
            self.event_avail_idx = self.event_avail_idx.wrapping_add(1);
            unsafe {
                core::ptr::write_volatile(&mut vring.avail_idx, self.event_avail_idx);
            }

            self.last_used_idx = self.last_used_idx.wrapping_add(1);
            processed += 1;
        }

        if processed > 0 {
            unsafe {
                core::arch::asm!("dmb sy", options(nomem, nostack));
                write32(self.mmio_base, MMIO_QUEUE_NOTIFY, EVENT_QUEUE);
            }
        }

        processed
    }

    /// Pushes the keyboard-LED state (Caps / Num / Scroll Lock) to the device
    /// through the status queue. No-op if the device exposed no status queue.
    pub fn set_leds(&mut self, caps: bool, num: bool, scroll: bool) {
        if self.kind != DeviceKind::Keyboard {
            return;
        }
        let updates = [
            (LED_CAPSL, caps),
            (LED_NUML, num),
            (LED_SCROLLL, scroll),
        ];
        let vring = unsafe { &mut *core::ptr::addr_of_mut!(STATUS_VRINGS[self.device_index]) };

        for (led, on) in updates {
            let (ev_type, code, value) = crate::input::encode_led_event(led, on);
            let buf_idx = self.status_next_buf % QUEUE_SIZE;
            self.status_next_buf = self.status_next_buf.wrapping_add(1);

            unsafe {
                STATUS_BUFS[self.device_index][buf_idx] = VirtioInputRawEvent {
                    event_type: ev_type,
                    code,
                    value,
                };
            }
            vring.descriptors[buf_idx] = VirtqDesc {
                addr: unsafe { core::ptr::addr_of!(STATUS_BUFS[self.device_index][buf_idx]) as u64 },
                len: core::mem::size_of::<VirtioInputRawEvent>() as u32,
                flags: 0, // device-readable
                next: 0,
            };
            let avail_slot = (self.status_avail_idx as usize) % QUEUE_SIZE;
            vring.avail_ring[avail_slot] = buf_idx as u16;
            self.status_avail_idx = self.status_avail_idx.wrapping_add(1);
        }

        unsafe {
            core::arch::asm!("dmb sy", options(nomem, nostack));
            core::ptr::write_volatile(&mut vring.avail_idx, self.status_avail_idx);
            write32(self.mmio_base, MMIO_QUEUE_NOTIFY, STATUS_QUEUE);
        }
    }
}

#[inline]
unsafe fn read32(base: u64, off: usize) -> u32 {
    core::ptr::read_volatile((base + off as u64) as *const u32)
}
#[inline]
unsafe fn write32(base: u64, off: usize, val: u32) {
    core::ptr::write_volatile((base + off as u64) as *mut u32, val);
}
#[inline]
unsafe fn read8(base: u64, off: usize) -> u8 {
    core::ptr::read_volatile((base + off as u64) as *const u8)
}
#[inline]
unsafe fn write8(base: u64, off: usize, val: u8) {
    core::ptr::write_volatile((base + off as u64) as *mut u8, val);
}

/// Selects a config block and returns `true` if it reports a non-zero size
/// (i.e. the device advertises at least one code in that `EV_*` class).
unsafe fn config_block_nonzero(base: u64, select: u8, subsel: u8) -> bool {
    write8(base, MMIO_CONFIG_SELECT, select);
    write8(base, MMIO_CONFIG_SUBSEL, subsel);
    read8(base, MMIO_CONFIG_SIZE) != 0
}

/// Reads a `struct virtio_input_absinfo` for axis `axis` (`min`, `max` in the
/// first two little-endian u32 fields).
unsafe fn read_abs_info(base: u64, axis: u8) -> Option<AbsAxisInfo> {
    write8(base, MMIO_CONFIG_SELECT, VIRTIO_INPUT_CFG_ABS_INFO);
    write8(base, MMIO_CONFIG_SUBSEL, axis);
    if read8(base, MMIO_CONFIG_SIZE) < 8 {
        return None;
    }
    let mut le = [0u8; 8];
    for (i, b) in le.iter_mut().enumerate() {
        *b = read8(base, MMIO_CONFIG_DATA + i);
    }
    let min = i32::from_le_bytes([le[0], le[1], le[2], le[3]]);
    let max = i32::from_le_bytes([le[4], le[5], le[6], le[7]]);
    if max <= min {
        return None;
    }
    Some(AbsAxisInfo { min, max })
}

/// Programs the selected virtqueue's ring addresses (legacy PFN or modern
/// split addresses) and marks it ready.
unsafe fn setup_queue(
    base: u64,
    version: u32,
    queue: u32,
    vring_ptr: *mut VirtQueueBuffer,
) -> Result<(), ()> {
    write32(base, MMIO_QUEUE_SEL, queue);
    if read32(base, MMIO_QUEUE_NUM_MAX) == 0 {
        return Err(());
    }
    write32(base, MMIO_QUEUE_NUM, QUEUE_SIZE as u32);
    core::ptr::write_bytes(vring_ptr as *mut u8, 0, core::mem::size_of::<VirtQueueBuffer>());

    let ring_paddr = vring_ptr as u64;
    if version >= 2 {
        let desc = ring_paddr;
        let avail = ring_paddr + core::mem::size_of::<[VirtqDesc; QUEUE_SIZE]>() as u64;
        let used = ring_paddr + 4096;
        write32(base, MMIO_QUEUE_DESC_LOW, desc as u32);
        write32(base, MMIO_QUEUE_DESC_HIGH, (desc >> 32) as u32);
        write32(base, MMIO_QUEUE_DRIVER_LOW, avail as u32);
        write32(base, MMIO_QUEUE_DRIVER_HIGH, (avail >> 32) as u32);
        write32(base, MMIO_QUEUE_DEVICE_LOW, used as u32);
        write32(base, MMIO_QUEUE_DEVICE_HIGH, (used >> 32) as u32);
        write32(base, MMIO_QUEUE_READY, 1);
    } else {
        write32(base, MMIO_GUEST_PAGE_SIZE, 4096);
        write32(base, MMIO_QUEUE_ALIGN, 4096);
        write32(base, MMIO_QUEUE_PFN, (ring_paddr >> 12) as u32);
    }
    Ok(())
}

pub struct VirtioInputManager {
    devices: [Option<VirtioInputDevice>; MAX_DEVICES],
    count: usize,
}

impl VirtioInputManager {
    pub const fn empty() -> Self {
        Self {
            devices: [None, None, None, None],
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

    pub fn poll_all(&mut self, screen: (u32, u32)) -> usize {
        let mut total = 0;
        for dev in self.devices.iter_mut().flatten() {
            total += dev.poll_events(screen);
        }
        total
    }

    /// Broadcasts a keyboard-LED state to every registered keyboard device.
    pub fn set_leds(&mut self, caps: bool, num: bool, scroll: bool) {
        for dev in self.devices.iter_mut().flatten() {
            dev.set_leds(caps, num, scroll);
        }
    }

    pub fn count(&self) -> usize {
        self.count
    }
}

pub static VIRTIO_INPUTS: SpinLock<VirtioInputManager> = SpinLock::new(VirtioInputManager::empty());

/// Scans the AArch64 VirtIO MMIO transport window (`0x0a00_0000..0x0a00_4000`,
/// 32 slots of 0x200) for VirtIO-Input devices (device id 18) and initializes
/// every one found, up to `MAX_DEVICES`.
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
            if magic != 0x7472_6976 || device_id != 18 {
                continue;
            }
            if let Ok(dev) = VirtioInputDevice::init(base, discovered) {
                crate::println!(
                    "    virtio-input dispositivo {}: {} ('{}') en {:#x}",
                    discovered,
                    dev.kind().label(),
                    dev.name_str(),
                    base
                );
                VIRTIO_INPUTS.lock().register(dev);
                discovered += 1;
            }
        }
    }

    discovered
}

/// Polls all registered VirtIO input devices, scaling absolute coordinates to
/// `(screen_w, screen_h)`.
pub fn poll_virtio_inputs(screen_w: u32, screen_h: u32) -> usize {
    VIRTIO_INPUTS.lock().poll_all((screen_w, screen_h))
}

/// Broadcasts the keyboard-LED state to every VirtIO-Input keyboard.
pub fn set_keyboard_leds(caps: bool, num: bool, scroll: bool) {
    VIRTIO_INPUTS.lock().set_leds(caps, num, scroll);
}
