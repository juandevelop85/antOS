//! xHCI (eXtensible Host Controller Interface) Driver.
//!
//! Provides xHCI host controller initialization, DMA rings management,
//! RootHub port status discovery, USB device enumeration, and HID interrupt polling.

pub mod context;
pub mod registers;
pub mod ring;

use crate::drivers::pci::{self, PciDevice};
use crate::drivers::usb::hid::{UsbHidKeyboard, UsbHidMouse};
use crate::drivers::usb::hub;
use alloc::vec::Vec;
use registers::{iman, portsc, usbcmd, usbsts, HcParams, XhciRegisters};
use ring::{CommandRing, EventRing, EventRingSegmentEntry, TransferRing, Trb, RING_SIZE};

/// Maximum number of device slots the driver backs with DMA structures. Raised
/// from 4 so a `usb-hub` plus several downstream devices all fit.
pub const MAX_SLOTS: usize = 16;

/// Interrupt-endpoint rings/buffers carried per slot. A composite device (one
/// USB address exposing e.g. a keyboard and a mouse on separate endpoints)
/// needs one ring/buffer pair per interface, not one per slot.
pub const MAX_EP_SLOTS: usize = 4;

/// Control-transfer bounce buffer size. 512 B swallows the Configuration
/// Descriptor bundle of a 3+ interface composite HID device, which overran the
/// old 256 B buffer and truncated the later interfaces.
pub const CONTROL_BUF_LEN: usize = 512;

/// Picks the lowest free interrupt-endpoint slot given a bitmask of the ones
/// already taken on a device, or `None` when the device has more interrupt
/// interfaces than [`MAX_EP_SLOTS`].
pub fn alloc_ep_slot(used_mask: u8) -> Option<usize> {
    (0..MAX_EP_SLOTS).find(|&i| used_mask & (1 << i) == 0)
}

/// DMA buffers for an individual device slot.
#[repr(C, align(64))]
pub struct SlotDma {
    pub device_context: [u8; 2048],
    pub input_context: [u8; 2112],
    pub ep0_ring: TransferRing,
    pub ep_int_ring: [TransferRing; MAX_EP_SLOTS],
    pub control_buffer: [u8; CONTROL_BUF_LEN],
    pub report_buffer: [[u8; 64]; MAX_EP_SLOTS],
}

impl SlotDma {
    pub const fn new() -> Self {
        Self {
            device_context: [0u8; 2048],
            input_context: [0u8; 2112],
            ep0_ring: TransferRing::new(),
            ep_int_ring: [const { TransferRing::new() }; MAX_EP_SLOTS],
            control_buffer: [0u8; CONTROL_BUF_LEN],
            report_buffer: [[0u8; 64]; MAX_EP_SLOTS],
        }
    }
}

impl Default for SlotDma {
    fn default() -> Self {
        Self::new()
    }
}

/// Static DMA pool for xHCI controller structures aligned to 4096 bytes.
#[repr(C, align(4096))]
pub struct XhciDmaPool {
    pub dcbaa: [u64; 256],
    pub erst: [EventRingSegmentEntry; 1],
    pub command_ring: CommandRing,
    pub event_ring: EventRing,
    pub slots: [SlotDma; MAX_SLOTS],
}

impl XhciDmaPool {
    pub const fn new() -> Self {
        Self {
            dcbaa: [0u64; 256],
            erst: [EventRingSegmentEntry {
                ring_segment_base_address: 0,
                ring_segment_size: 0,
                reserved: 0,
            }; 1],
            command_ring: CommandRing::new(),
            event_ring: EventRing::new(),
            slots: [const { SlotDma::new() }; MAX_SLOTS],
        }
    }
}

impl Default for XhciDmaPool {
    fn default() -> Self {
        Self::new()
    }
}

static mut XHCI_DMA: XhciDmaPool = XhciDmaPool::new();

#[inline]
pub fn virt_to_phys(vaddr: u64) -> u64 {
    #[cfg(target_arch = "aarch64")]
    {
        crate::arch::aarch64::mmu::kernel_virt_to_phys(vaddr)
    }
    #[cfg(target_arch = "x86_64")]
    {
        // The DMA pool is a page-aligned kernel static; walk the page tables
        // for its real physical address rather than assuming identity mapping.
        crate::memory::translate(vaddr).unwrap_or(vaddr)
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        vaddr
    }
}

/// Maps a device MMIO physical base to a CPU-accessible address for the running
/// architecture (identity on AArch64's device window, physical-offset window on
/// x86_64).
#[inline]
pub fn mmio_base(phys: u64) -> usize {
    #[cfg(target_arch = "x86_64")]
    {
        (phys + crate::memory::physical_memory_offset()) as usize
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        phys as usize
    }
}

/// Busy-wait delay in milliseconds, used for USB spec-mandated port timings.
#[inline]
pub fn delay_ms(ms: u64) {
    #[cfg(target_arch = "aarch64")]
    {
        crate::arch::aarch64::timer::delay_ms(ms);
    }
    #[cfg(target_arch = "x86_64")]
    {
        // Prefer the kernel tick clock; fall back to a bounded spin if it is
        // not advancing (e.g. interrupts still masked).
        let start = crate::task::timer::ticks();
        let want = ms * crate::task::timer::TICKS_PER_SECOND / 1000;
        let mut spin = ms.saturating_mul(2_000_000) + 200_000;
        while crate::task::timer::ticks().wrapping_sub(start) < want && spin > 0 {
            spin -= 1;
            core::hint::spin_loop();
        }
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        for _ in 0..(ms * 5_000) {
            core::hint::spin_loop();
        }
    }
}

/// Translates a USB endpoint `bInterval` into the value the xHCI Endpoint
/// Context "Interval" field expects (a `125 µs * 2^Interval` period).
///
/// * High/SuperSpeed interrupt endpoints already express `bInterval` as a
///   `2^(bInterval-1)` exponent, so the field is simply `bInterval - 1`.
/// * Full/Low-speed interrupt endpoints express `bInterval` directly in 1 ms
///   frames; the field becomes `3 + floor(log2(bInterval))`, clamped to the
///   1 ms .. 128 ms range xHCI allows for these speeds.
///
/// Passing the raw `bInterval` through (as the code did before) made a
/// Full-speed keyboard advertising `bInterval = 10` poll every `2^10 * 125 µs`
/// ≈ 128 ms *at best*, and encode to a reserved value at worst — which is why
/// keystrokes never surfaced.
fn encode_interval(speed: u8, b_interval: u8) -> u8 {
    match speed as u32 {
        portsc::SPEED_HIGH | portsc::SPEED_SUPER => b_interval.max(1).saturating_sub(1).min(15),
        _ => {
            let frames = b_interval.max(1) as u32;
            let log2 = 31 - frames.leading_zeros(); // floor(log2(frames))
            (3 + log2).clamp(3, 10) as u8
        }
    }
}

/// Information about an xHCI RootHub port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortInfo {
    pub port_number: u8,
    pub connected: bool,
    pub enabled: bool,
    pub speed_name: &'static str,
}

/// Active USB device slot managed by the kernel.
pub struct SlotDevice {
    pub slot_id: u8,
    pub port: u8,
    pub speed: u8,
    pub is_keyboard: bool,
    pub is_mouse: bool,
    pub is_hub: bool,
    pub ep_int_dci: u8,
    /// Index into the slot's per-endpoint ring/buffer arrays for this interface.
    pub ep_slot: usize,
    pub ep_int_max_packet: u16,
    /// Route string of this device in the bus topology (0 for a root-port device).
    pub route_string: u32,
    /// Slot ID of the parent hub, or 0 when attached to a root port.
    pub parent_hub_slot: u8,
    pub keyboard: UsbHidKeyboard,
    pub mouse: UsbHidMouse,
    /// Parsed Report-Descriptor model, used when the interface is not in Boot
    /// Protocol. `None` on the boot fast path.
    pub hid: Option<crate::drivers::usb::hid::HidDevice>,
    /// When set, decode interrupt reports with [`Self::hid`] instead of the
    /// fixed boot decoders.
    pub use_generic: bool,
    /// Interface number this endpoint belongs to (for `SET_REPORT` LED writes).
    pub iface_number: u8,
    /// Diagnostics: interface class/protocol as reported by the device and a
    /// snapshot of the last interrupt report, surfaced through `SYS_SYSINFO`.
    pub iface_class: u8,
    pub iface_protocol: u8,
    pub report_events: u32,
    pub last_report: [u8; 8],
    pub last_report_len: u8,
}

/// Active xHCI Host Controller instance.
pub struct XhciController {
    pub registers: XhciRegisters,
    pub pci_device: PciDevice,
    pub params: HcParams,
    pub devices: Vec<SlotDevice>,
    /// Counter that throttles the per-port PORTSC hotplug sweep in [`poll`].
    port_scan_divider: u32,
}

impl XhciController {
    /// Discovers an xHCI controller on the PCIe bus and initializes it.
    pub fn probe_and_init() -> Option<Self> {
        #[cfg(target_arch = "aarch64")]
        pci::probe_ecam_base();

        let devices = pci::scan_pci_bus();
        let xhci_dev = devices.into_iter().find(|d| d.is_xhci_controller())?;

        // Extract BAR0 physical MMIO base. A zero address means the BAR is
        // present but unprogrammed (no firmware PCI resource allocation, as on
        // `qemu -M virt -kernel`); there is nothing to map, so skip the
        // controller instead of dereferencing a null MMIO base.
        let bar0_addr = xhci_dev.bars[0].memory_address().filter(|&a| a != 0)?;
        xhci_dev.enable_bus_mastering();

        unsafe {
            let registers = XhciRegisters::new(mmio_base(bar0_addr)).ok()?;
            let mut controller = Self {
                registers,
                pci_device: xhci_dev,
                params: HcParams {
                    max_slots: 0,
                    max_intrs: 0,
                    max_ports: 0,
                    csz_64: false,
                },
                devices: Vec::new(),
                port_scan_divider: 0,
            };
            controller.params = controller.registers.params();

            if controller.init_hardware().is_ok() {
                Some(controller)
            } else {
                None
            }
        }
    }

    /// Highest slot ID the controller and our DMA pool both support.
    #[inline]
    fn slot_ceiling(&self) -> u8 {
        (self.params.max_slots as usize).clamp(1, MAX_SLOTS) as u8
    }

    /// Executes the xHCI controller reset and hardware initialization sequence.
    unsafe fn init_hardware(&mut self) -> Result<(), &'static str> {
        // 1. Wait until Controller Not Ready (CNR) is 0
        let mut timeout = 100_000u32;
        while (self.registers.read_usbsts() & usbsts::CNR) != 0 && timeout > 0 {
            timeout -= 1;
            core::hint::spin_loop();
        }

        // 2. Halt controller if running
        let usbcmd_val = self.registers.read_usbcmd();
        self.registers.write_usbcmd(usbcmd_val & !usbcmd::RS);
        timeout = 100_000;
        while (self.registers.read_usbsts() & usbsts::HCH) == 0 && timeout > 0 {
            timeout -= 1;
            core::hint::spin_loop();
        }

        // 3. Reset host controller
        self.registers.write_usbcmd(usbcmd::HCRST);
        timeout = 100_000;
        while (self.registers.read_usbcmd() & usbcmd::HCRST) != 0 && timeout > 0 {
            timeout -= 1;
            core::hint::spin_loop();
        }
        timeout = 100_000;
        while (self.registers.read_usbsts() & usbsts::CNR) != 0 && timeout > 0 {
            timeout -= 1;
            core::hint::spin_loop();
        }

        // 4. Configure Max Device Slots Enabled
        let max_slots = self.slot_ceiling();
        self.registers.set_config_max_slots(max_slots);

        // 5. Setup DMA buffers (calculate physical addresses)
        let dma_pool = &raw mut XHCI_DMA;
        let dcbaa_phys = virt_to_phys(core::ptr::addr_of!((*dma_pool).dcbaa) as u64);
        let cmd_ring_phys = virt_to_phys(core::ptr::addr_of!((*dma_pool).command_ring) as u64);
        let event_ring_phys = virt_to_phys(core::ptr::addr_of!((*dma_pool).event_ring) as u64);
        let erst_phys = virt_to_phys(core::ptr::addr_of!((*dma_pool).erst) as u64);

        // Initialize Command Ring Link TRB
        (*dma_pool).command_ring.init_link(cmd_ring_phys);

        // Program DCBAAP
        self.registers.set_dcbaap(dcbaa_phys);

        // Program CRCR (pointer | Ring Cycle State bit 1)
        self.registers.set_crcr(cmd_ring_phys | 1);

        // 6. Setup Primary Interrupter Event Ring
        (*dma_pool).erst[0].ring_segment_base_address = event_ring_phys;
        (*dma_pool).erst[0].ring_segment_size = RING_SIZE as u32;

        self.registers.set_interrupter0_erstsz(1);
        self.registers.set_interrupter0_erstba(erst_phys);
        self.registers.set_interrupter0_erdp(event_ring_phys);
        self.registers.set_interrupter0_iman(iman::IE);

        // 7. Start the controller (Run/Stop = 1, Interrupter Enable = 1)
        self.registers.write_usbcmd(usbcmd::RS | usbcmd::INTE);

        timeout = 100_000;
        while (self.registers.read_usbsts() & usbsts::HCH) != 0 && timeout > 0 {
            timeout -= 1;
            core::hint::spin_loop();
        }

        Ok(())
    }

    /// Inspects all RootHub ports and returns a summary list of active connections.
    pub fn inspect_ports(&self) -> Vec<PortInfo> {
        let mut list = Vec::new();
        for port in 1..=self.params.max_ports {
            let sc = self.registers.read_portsc(port);
            let connected = (sc & portsc::CCS) != 0;
            let enabled = (sc & portsc::PED) != 0;
            let speed_code = (sc >> portsc::SPEED_SHIFT) & portsc::SPEED_MASK;
            let speed_name = match speed_code {
                portsc::SPEED_FULL => "Full-speed (12 Mb/s)",
                portsc::SPEED_LOW => "Low-speed (1.5 Mb/s)",
                portsc::SPEED_HIGH => "High-speed (480 Mb/s)",
                portsc::SPEED_SUPER => "SuperSpeed (5 Gb/s)",
                _ => "Unknown Speed",
            };

            list.push(PortInfo {
                port_number: port,
                connected,
                enabled,
                speed_name,
            });
        }
        list
    }

    /// Acknowledges dequeued events by updating the Event Ring Dequeue Pointer (ERDP).
    pub fn update_erdp(&self) {
        let dma = &raw mut XHCI_DMA;
        unsafe {
            let deq_idx = (*dma).event_ring.dequeue_idx;
            let event_ring_phys = virt_to_phys(core::ptr::addr_of!((*dma).event_ring) as u64);
            let erdp = event_ring_phys + (deq_idx as u64 * 16);
            // Clear a latched Interrupt Pending (IMAN.IP is W1C) so a controller
            // that gates further Event TRBs on IP being clear — the VirtualBox
            // model behaves this way — keeps delivering after the first event.
            let iman = self.registers.read_interrupter0_iman();
            self.registers
                .set_interrupter0_iman(iman | iman::IP | iman::IE);
            self.registers.set_interrupter0_erdp(erdp | (1 << 3)); // EHB = 1
        }
    }

    /// Submits a command TRB to the Command Ring and awaits its completion event.
    pub fn send_command_and_wait(&mut self, trb: Trb) -> Result<Trb, &'static str> {
        let dma = &raw mut XHCI_DMA;
        unsafe {
            (*dma).command_ring.push(trb)?;
            self.registers.ring_doorbell(0, 0);

            let mut timeout = 300_000u32;
            while timeout > 0 {
                if let Some(event) = (*dma).event_ring.pop() {
                    self.update_erdp();
                    if event.trb_type() == ring::TRB_TYPE_COMMAND_COMPLETION_EVENT {
                        let code = (event.status >> 24) & 0xFF;
                        if code == 1 {
                            return Ok(event);
                        } else {
                            return Err("Command completion code != Success");
                        }
                    }
                }
                timeout -= 1;
                core::hint::spin_loop();
            }
        }
        Err("Command timeout")
    }

    /// Resets a RootHub port and returns its negotiated speed code.
    pub fn reset_port(&mut self, port: u8) -> Result<u8, &'static str> {
        let mut sc = self.registers.read_portsc(port);
        if (sc & portsc::CCS) == 0 {
            return Err("Port not connected");
        }

        // The device (or the host, e.g. VirtualBox) may already be mid-reset;
        // wait for PR to clear before touching the port again rather than
        // stacking another reset request on top.
        if (sc & portsc::PR) != 0 {
            let mut spin = 60;
            while spin > 0 && (self.registers.read_portsc(port) & portsc::PR) != 0 {
                delay_ms(5);
                spin -= 1;
            }
            sc = self.registers.read_portsc(port);
        }

        if (sc & portsc::PED) != 0 {
            // Already enabled — give the device its reset-recovery window
            // anyway, then report the negotiated speed.
            delay_ms(15);
            let speed = ((self.registers.read_portsc(port) >> portsc::SPEED_SHIFT)
                & portsc::SPEED_MASK) as u8;
            return Ok(speed);
        }

        // Trigger Port Reset preserving Port Power (PP), without writing 1 to PED
        let write_val = (sc & portsc::PP) | portsc::PR;
        self.registers.write_portsc(port, write_val);

        // USB spec: wait 25 ms for port reset signaling
        delay_ms(25);

        let mut timeout = 50;
        while timeout > 0 {
            let current = self.registers.read_portsc(port);
            if (current & portsc::PR) == 0 && (current & portsc::PED) != 0 {
                let speed = ((current >> portsc::SPEED_SHIFT) & portsc::SPEED_MASK) as u8;
                // USB spec: wait 15 ms reset recovery time
                delay_ms(15);
                return Ok(speed);
            }
            delay_ms(5);
            timeout -= 1;
        }

        let final_sc = self.registers.read_portsc(port);
        if (final_sc & portsc::PED) == 0 {
            return Err("Port failed to enable after reset");
        }
        let speed = ((final_sc >> portsc::SPEED_SHIFT) & portsc::SPEED_MASK) as u8;
        Ok(speed)
    }

    /// Submits an Enable Slot command and returns the allocated Slot ID.
    pub fn enable_slot(&mut self) -> Result<u8, &'static str> {
        let trb = Trb::make_enable_slot(false);
        let event = self.send_command_and_wait(trb)?;
        let slot_id = ((event.control >> 24) & 0xFF) as u8;
        if slot_id == 0 || slot_id > self.slot_ceiling() {
            return Err("Invalid slot ID allocated");
        }
        Ok(slot_id)
    }

    /// Prepares Input/Device Contexts and issues the Address Device command.
    ///
    /// `route_string` / `parent_hub_slot` / `parent_port` are 0 for a device on
    /// a root port and carry the topology of a device sitting behind a hub.
    pub fn address_device(
        &mut self,
        slot_id: u8,
        root_port: u8,
        speed: u8,
        route_string: u32,
        parent_hub_slot: u8,
        parent_port: u8,
    ) -> Result<(), &'static str> {
        let slot_idx = (slot_id - 1) as usize;
        let dma = &raw mut XHCI_DMA;

        unsafe {
            let dev_ctx_phys =
                virt_to_phys(core::ptr::addr_of!((*dma).slots[slot_idx].device_context) as u64);
            (*dma).dcbaa[slot_id as usize] = dev_ctx_phys;

            (*dma).slots[slot_idx].input_context.fill(0);
            (*dma).slots[slot_idx].device_context.fill(0);

            let ep0_ring_phys =
                virt_to_phys(core::ptr::addr_of!((*dma).slots[slot_idx].ep0_ring) as u64);
            (*dma).slots[slot_idx].ep0_ring = ring::TransferRing::new();
            (*dma).slots[slot_idx].ep0_ring.init_link(ep0_ring_phys);

            let csz = self.params.csz_64;
            let mut view =
                context::ContextView::new(&mut (*dma).slots[slot_idx].input_context, csz);

            if let Some(mut icc) = view.input_control() {
                icc.set_add_flags((1 << 0) | (1 << 1)); // Slot + EP0
            }

            if let Some(mut slot) = view.slot_context(true) {
                slot.set_info(route_string, speed, 1);
                slot.set_port_info(root_port, 0);
                if parent_hub_slot != 0 {
                    slot.set_tt_info(parent_hub_slot, parent_port, 0);
                }
            }

            // Default EP0 max packet size for Full/Low speed devices before descriptor read is 8 bytes
            let max_pkt: u16 = match speed as u32 {
                portsc::SPEED_HIGH => 64,
                portsc::SPEED_SUPER => 512,
                _ => 8,
            };

            if let Some(mut ep0) = view.endpoint_context(1, true) {
                ep0.set_type_and_max_packet(4, max_pkt, 3);
                ep0.set_tr_dequeue_pointer(ep0_ring_phys, true);
                ep0.set_transfer_info(8, 0);
            }

            let input_ctx_phys =
                virt_to_phys(core::ptr::addr_of!((*dma).slots[slot_idx].input_context) as u64);
            let trb = Trb::make_address_device(input_ctx_phys, slot_id, false);
            self.send_command_and_wait(trb)?;
        }

        Ok(())
    }

    /// Runs a single Control Transfer attempt on Endpoint 0, surfacing the raw
    /// completion code on failure so the caller can decide whether to recover.
    fn control_transfer_once(
        &mut self,
        slot_id: u8,
        setup: [u8; 8],
        data: Option<&mut [u8]>,
        is_in: bool,
    ) -> Result<usize, (u32, &'static str)> {
        let slot_idx = (slot_id - 1) as usize;
        let dma = &raw mut XHCI_DMA;

        let mut data = data;
        let data_len = data.as_ref().map(|d| d.len() as u32).unwrap_or(0);
        let xfer_len = data_len.min(CONTROL_BUF_LEN as u32);
        let trt = if xfer_len == 0 {
            0 // No Data Stage
        } else if is_in {
            3 // IN Data Stage
        } else {
            2 // OUT Data Stage
        };

        unsafe {
            let slot_dma = &mut (*dma).slots[slot_idx];

            // 1. Setup Stage TRB
            let setup_trb = Trb::make_setup_stage(setup, trt);
            slot_dma.ep0_ring.push(setup_trb).map_err(|e| (0, e))?;

            // 2. Data Stage TRB
            if xfer_len > 0 {
                let buf_phys = virt_to_phys(core::ptr::addr_of!(slot_dma.control_buffer) as u64);
                if !is_in {
                    if let Some(ref src) = data {
                        let len = xfer_len as usize;
                        slot_dma.control_buffer[..len].copy_from_slice(&src[..len]);
                    }
                }
                let data_trb = Trb::make_data_stage(buf_phys, xfer_len, is_in);
                slot_dma.ep0_ring.push(data_trb).map_err(|e| (0, e))?;
            }

            // 3. Status Stage TRB
            let status_in = !(xfer_len > 0 && is_in);
            let status_trb = Trb::make_status_stage(status_in);
            slot_dma.ep0_ring.push(status_trb).map_err(|e| (0, e))?;

            // 4. Ring Doorbell for Endpoint 0
            self.registers.ring_doorbell(slot_id, 1);

            // 5. Wait for completion event (up to 500 ms)
            let mut timeout = 500u32;
            while timeout > 0 {
                if let Some(event) = (*dma).event_ring.pop() {
                    self.update_erdp();
                    if event.trb_type() == ring::TRB_TYPE_TRANSFER_EVENT {
                        let ev_slot = ((event.control >> 24) & 0xFF) as u8;
                        let ev_ep = ((event.control >> 16) & 0x1F) as u8;
                        if ev_slot == slot_id && ev_ep == 1 {
                            let code = (event.status >> 24) & 0xFF;
                            if code == 1 || code == 13 {
                                if is_in && xfer_len > 0 {
                                    if let Some(ref mut dest) = data {
                                        let len = (xfer_len as usize).min(dest.len());
                                        dest[..len]
                                            .copy_from_slice(&slot_dma.control_buffer[..len]);
                                    }
                                }
                                return Ok(xfer_len as usize);
                            } else {
                                return Err((code, "Control transfer failed"));
                            }
                        }
                    }
                }
                delay_ms(1);
                timeout -= 1;
            }
        }

        Err((0, "Control transfer timed out"))
    }

    /// Performs a USB Control Transfer on Endpoint 0 with bounded recovery.
    ///
    /// A STALL (completion code 6) halts EP0; the endpoint is reset, its
    /// transfer ring re-seeded via Set TR Dequeue Pointer, and `CLEAR_FEATURE
    /// (ENDPOINT_HALT)` sent before retrying. A USB Transaction Error
    /// (code 4) is retried without the reset dance. Up to two retries.
    pub fn control_transfer(
        &mut self,
        slot_id: u8,
        setup: [u8; 8],
        mut data: Option<&mut [u8]>,
        is_in: bool,
    ) -> Result<usize, &'static str> {
        let mut attempt = 0u32;
        loop {
            match self.control_transfer_once(slot_id, setup, data.as_deref_mut(), is_in) {
                Ok(n) => return Ok(n),
                Err((code, msg)) => {
                    attempt += 1;
                    if attempt > 2 {
                        return Err(msg);
                    }
                    match code {
                        6 => unsafe { self.recover_ep0(slot_id) },
                        4 => delay_ms(2),
                        _ => return Err(msg),
                    }
                }
            }
        }
    }

    /// Clears a halted EP0: Reset Endpoint, rewind the control ring, Set TR
    /// Dequeue Pointer, and a best-effort device-side `CLEAR_FEATURE`.
    unsafe fn recover_ep0(&mut self, slot_id: u8) {
        let dma = &raw mut XHCI_DMA;
        let slot_idx = (slot_id - 1) as usize;

        let _ = self.send_command_and_wait(Trb::make_reset_endpoint(slot_id, 1));

        let ep0_phys = virt_to_phys(core::ptr::addr_of!((*dma).slots[slot_idx].ep0_ring) as u64);
        (*dma).slots[slot_idx].ep0_ring.reset(ep0_phys);
        let _ = self.send_command_and_wait(Trb::make_set_tr_dequeue(ep0_phys, true, slot_id, 1));

        // bmRequestType 0x02 (Host->Dev | Standard | Endpoint),
        // bRequest 0x01 CLEAR_FEATURE, wValue 0 = ENDPOINT_HALT, wIndex 0 = EP0.
        let clear_halt = [0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let _ = self.control_transfer_once(slot_id, clear_halt, None, false);
    }

    /// Configures an Interrupt IN endpoint on its own per-interface ring/buffer.
    ///
    /// `ep_slot` selects the ring/buffer pair (see [`MAX_EP_SLOTS`]);
    /// `context_entries` is the running maximum DCI configured on the slot so a
    /// second, lower-DCI endpoint does not shrink the Slot Context.
    #[allow(clippy::too_many_arguments)]
    pub fn configure_hid_endpoint(
        &mut self,
        slot_id: u8,
        ep_slot: usize,
        speed: u8,
        route_string: u32,
        root_port: u8,
        ep_addr: u8,
        max_packet: u16,
        interval: u8,
        context_entries: u8,
    ) -> Result<u8, &'static str> {
        let slot_idx = (slot_id - 1) as usize;
        let ep_slot = ep_slot.min(MAX_EP_SLOTS - 1);
        let dma = &raw mut XHCI_DMA;

        let ep_num = ep_addr & 0x0F;
        let is_in = (ep_addr & 0x80) != 0;
        let dci = (ep_num * 2) + (if is_in { 1 } else { 0 });

        unsafe {
            (*dma).slots[slot_idx].input_context.fill(0);

            let ep_int_ring_phys = virt_to_phys(core::ptr::addr_of!(
                (*dma).slots[slot_idx].ep_int_ring[ep_slot]
            ) as u64);
            (*dma).slots[slot_idx].ep_int_ring[ep_slot] = ring::TransferRing::new();
            (*dma).slots[slot_idx].ep_int_ring[ep_slot].init_link(ep_int_ring_phys);

            let csz = self.params.csz_64;
            let mut view =
                context::ContextView::new(&mut (*dma).slots[slot_idx].input_context, csz);

            if let Some(mut icc) = view.input_control() {
                icc.set_add_flags((1 << 0) | (1 << dci));
            }

            if let Some(mut slot) = view.slot_context(true) {
                slot.set_info(route_string, speed, context_entries.max(dci));
                // The input context was just zeroed and the Slot add-flag (A0)
                // is set, so the controller re-evaluates the Slot Context on
                // this Configure Endpoint. Re-assert the Root Hub Port Number
                // (DWORD1) — leaving it 0 breaks routing of the interrupt
                // endpoint's transfers, i.e. the actual key/mouse reports.
                slot.set_port_info(root_port, 0);
            }

            if let Some(mut ep_ctx) = view.endpoint_context(dci as usize, true) {
                ep_ctx.set_interval(encode_interval(speed, interval));
                let ep_type = if is_in { 7 } else { 3 }; // 7 = Interrupt IN
                ep_ctx.set_type_and_max_packet(ep_type, max_packet.max(8), 3);
                ep_ctx.set_tr_dequeue_pointer(ep_int_ring_phys, true);
                ep_ctx.set_transfer_info(max_packet.max(8), max_packet.max(8));
            }

            let input_ctx_phys =
                virt_to_phys(core::ptr::addr_of!((*dma).slots[slot_idx].input_context) as u64);
            let trb = Trb::make_configure_endpoint(input_ctx_phys, slot_id);
            self.send_command_and_wait(trb)?;

            // Queue first Normal TRB on this endpoint's Interrupt Ring
            let report_phys = virt_to_phys(core::ptr::addr_of!(
                (*dma).slots[slot_idx].report_buffer[ep_slot]
            ) as u64);
            let normal_trb = Trb::make_normal(report_phys, max_packet.max(8) as u32);
            (*dma).slots[slot_idx].ep_int_ring[ep_slot].push(normal_trb)?;

            // Ring doorbell for interrupt endpoint
            self.registers.ring_doorbell(slot_id, dci);
        }

        Ok(dci)
    }

    /// Enumerates all connected RootHub ports, addressing every device and
    /// configuring its HID endpoints (recursing through any USB hub).
    pub fn enumerate_connected_ports(&mut self) {
        // Power every root port first — a device on an unpowered port reads
        // CCS = 0, so without this a keyboard on a port the controller left
        // unpowered would be invisible.
        for port in 1..=self.params.max_ports {
            let sc = self.registers.read_portsc(port);
            if (sc & portsc::PP) == 0 {
                self.registers.write_portsc(port, sc | portsc::PP);
            }
        }
        delay_ms(20);

        // Diagnostics: raw PORTSC of every root port.
        for port in 1..=self.params.max_ports {
            let sc = self.registers.read_portsc(port);
            if sc != 0 {
                crate::println!(
                    "    usb-debug  portsc[{}]={:#010x} ccs={} ped={} pp={} pr={} spd={}",
                    port,
                    sc,
                    (sc & portsc::CCS != 0) as u8,
                    (sc & portsc::PED != 0) as u8,
                    (sc & portsc::PP != 0) as u8,
                    (sc & portsc::PR != 0) as u8,
                    (sc >> portsc::SPEED_SHIFT) & portsc::SPEED_MASK,
                );
            }
        }

        let ports = self.inspect_ports();
        for p in ports {
            if p.connected {
                self.enumerate_root_port(p.port_number);
            }
        }
    }

    /// Resets one root port and enumerates whatever is attached to it.
    fn enumerate_root_port(&mut self, port_num: u8) {
        let speed = match self.reset_port(port_num) {
            Ok(s) => s,
            Err(e) => {
                crate::println!("    usb-debug  puerto {}: reset fallo: {}", port_num, e);
                return;
            }
        };
        if let Err(e) = self.enumerate_device(port_num, speed, 0, 0, 0) {
            crate::println!(
                "    usb-debug  puerto {}: enumeracion fallo: {}",
                port_num,
                e
            );
        }
    }

    /// Shared device bring-up: Enable Slot, Address Device, descriptor reads,
    /// Set Configuration, then per-interface endpoint configuration. A device
    /// whose class is Hub is handed to [`configure_hub`] instead.
    fn enumerate_device(
        &mut self,
        root_port: u8,
        speed: u8,
        route_string: u32,
        parent_hub_slot: u8,
        parent_port: u8,
    ) -> Result<(), &'static str> {
        let slot_id = self.enable_slot()?;
        self.address_device(
            slot_id,
            root_port,
            speed,
            route_string,
            parent_hub_slot,
            parent_port,
        )?;

        // 1. Device Descriptor (18 bytes) — carries bDeviceClass at offset 4.
        let mut dev_desc_buf = [0u8; 18];
        let get_dev_setup = [0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 18, 0x00];
        let dd = self.control_transfer(slot_id, get_dev_setup, Some(&mut dev_desc_buf), true);
        let device_class = dev_desc_buf[4];
        crate::println!(
            "    usb-debug  slot {} pto {}: devdesc={:?} {:04x}:{:04x} class={:#x} proto={:#x}",
            slot_id,
            root_port,
            dd,
            u16::from_le_bytes([dev_desc_buf[8], dev_desc_buf[9]]),
            u16::from_le_bytes([dev_desc_buf[10], dev_desc_buf[11]]),
            device_class,
            dev_desc_buf[6],
        );

        // 2. Configuration Descriptor: 9-byte header first for wTotalLength,
        //    then the whole bundle (bounded by the 512 B control buffer).
        let mut config_hdr = [0u8; 9];
        let get_cfg_setup = [0x80, 0x06, 0x00, 0x02, 0x00, 0x00, 9, 0x00];
        self.control_transfer(slot_id, get_cfg_setup, Some(&mut config_hdr), true)?;

        let total_len = u16::from_le_bytes([config_hdr[2], config_hdr[3]]) as usize;
        let read_len = total_len.min(CONTROL_BUF_LEN);
        let mut config_buf = alloc::vec![0u8; read_len];
        let get_full_cfg = [
            0x80,
            0x06,
            0x00,
            0x02,
            0x00,
            0x00,
            (read_len & 0xFF) as u8,
            ((read_len >> 8) & 0xFF) as u8,
        ];
        let _ = self.control_transfer(slot_id, get_full_cfg, Some(&mut config_buf), true);

        let (config_val, ifaces) =
            crate::drivers::usb::descriptor::parse_configuration_bundle(&config_buf);
        let config_val = config_val.unwrap_or(1);

        // 3. Set Configuration
        let set_cfg_setup = [0x00, 0x09, config_val, 0x00, 0x00, 0x00, 0x00, 0x00];
        let _ = self.control_transfer(slot_id, set_cfg_setup, None, false);

        // A USB hub is required to report its class at the device level.
        if device_class == hub::CLASS_HUB {
            self.devices.push(SlotDevice {
                slot_id,
                port: root_port,
                speed,
                is_keyboard: false,
                is_mouse: false,
                is_hub: true,
                ep_int_dci: 0,
                ep_slot: 0,
                ep_int_max_packet: 0,
                route_string,
                parent_hub_slot,
                keyboard: UsbHidKeyboard::new(),
                mouse: UsbHidMouse::new(),
                hid: None,
                use_generic: false,
                iface_number: 0,
                iface_class: hub::CLASS_HUB,
                iface_protocol: 0,
                report_events: 0,
                last_report: [0u8; 8],
                last_report_len: 0,
            });
            let tier = hub::route_depth(route_string);
            return self.configure_hub(slot_id, root_port, speed, route_string, tier);
        }

        // Context Entries must cover the highest DCI configured on the slot.
        let mut max_dci = 1u8;
        let mut used_mask = 0u8;

        for iface in ifaces {
            let mut desc_buf = [0u8; 256];
            let get_report_desc = [
                0x81,
                0x06,
                0x00,
                0x22,
                iface.interface_number,
                0x00,
                (desc_buf.len() & 0xFF) as u8,
                ((desc_buf.len() >> 8) & 0xFF) as u8,
            ];
            let rd_len = self.control_transfer(slot_id, get_report_desc, Some(&mut desc_buf), true);
            let rd_valid = rd_len.unwrap_or(0).min(desc_buf.len());

            // Try Boot Protocol. It is only meaningful for a Boot Interface
            // subclass; a subclass-0 device (e.g. the absolute tablet) is left
            // in Report Protocol and decoded generically from its descriptor.
            let is_boot_subclass = iface.interface_subclass
                == crate::drivers::usb::descriptor::SUBCLASS_BOOT_INTERFACE;
            let wants_boot = is_boot_subclass
                || iface.interface_protocol == crate::drivers::usb::descriptor::PROTOCOL_KEYBOARD
                || iface.interface_protocol == crate::drivers::usb::descriptor::PROTOCOL_MOUSE;
            let mut boot_ok = false;
            if wants_boot {
                let set_protocol = [
                    0x21,
                    0x0B,
                    0x00,
                    0x00,
                    iface.interface_number,
                    0x00,
                    0x00,
                    0x00,
                ];
                let sp_ok = self
                    .control_transfer(slot_id, set_protocol, None, false)
                    .is_ok();
                let set_idle = [
                    0x21,
                    0x0A,
                    0x00,
                    0x00,
                    iface.interface_number,
                    0x00,
                    0x00,
                    0x00,
                ];
                let _ = self.control_transfer(slot_id, set_idle, None, false);
                boot_ok = sp_ok && is_boot_subclass;
            }

            let (rd_kbd, rd_mouse) =
                crate::drivers::usb::descriptor::classify_report_descriptor(&desc_buf[..rd_valid]);
            let mut hid_model =
                crate::drivers::usb::hid::HidDevice::from_descriptor(&desc_buf[..rd_valid]);
            let use_generic = !boot_ok && !hid_model.is_empty();

            // T28.4 diagnostics: one line on what the parser made of this
            // interface, so a mis-classified device is visible in the boot log.
            crate::println!(
                "    usb-debug  slot {} iface {}: rd_len={:?} role={:?} rid={} boot_ok={}",
                slot_id,
                iface.interface_number,
                rd_len,
                hid_model.role,
                hid_model.uses_report_id(),
                boot_ok,
            );

            let ep_slot = match alloc_ep_slot(used_mask) {
                Some(s) => s,
                None => {
                    crate::println!("    usb-debug  slot {}: sin ep_slot libre", slot_id);
                    break;
                }
            };

            let max_pkt = iface.ep_max_packet.max(8);
            let ep_num = iface.ep_addr & 0x0F;
            let dci = ep_num * 2 + if iface.ep_addr & 0x80 != 0 { 1 } else { 0 };
            max_dci = max_dci.max(dci);

            match self.configure_hid_endpoint(
                slot_id,
                ep_slot,
                speed,
                route_string,
                root_port,
                iface.ep_addr,
                max_pkt,
                iface.ep_interval,
                max_dci,
            ) {
                Ok(dci) => {
                    used_mask |= 1 << ep_slot;

                    use crate::drivers::usb::hid::HidRole;
                    let (is_kbd, is_mou) = if use_generic {
                        match hid_model.role {
                            HidRole::Keyboard => (true, false),
                            HidRole::Mouse => (false, true),
                            HidRole::Other => (rd_kbd && !rd_mouse, !(rd_kbd && !rd_mouse)),
                        }
                    } else if iface.is_boot_keyboard {
                        (true, false)
                    } else if iface.is_boot_mouse {
                        (false, true)
                    } else if rd_kbd && !rd_mouse {
                        (true, false)
                    } else {
                        (false, true)
                    };

                    let mut mouse = UsbHidMouse::new();
                    if is_mou {
                        mouse.is_absolute = max_pkt >= 5;
                    }

                    // Prime the generic decoder's diff state with the current
                    // report so a device that only emits on change starts sane.
                    if use_generic {
                        let want = hid_model.input_len_bytes(0)
                            + if hid_model.uses_report_id() { 1 } else { 0 };
                        let rlen = want.clamp(1, 64);
                        let mut prime = alloc::vec![0u8; rlen];
                        let get_report = [
                            0xA1,
                            0x01,
                            0x00,
                            0x01,
                            iface.interface_number,
                            0x00,
                            (rlen & 0xFF) as u8,
                            ((rlen >> 8) & 0xFF) as u8,
                        ];
                        if self
                            .control_transfer(slot_id, get_report, Some(&mut prime), true)
                            .is_ok()
                        {
                            let _ = hid_model.decode(&prime);
                        }
                    }

                    self.devices.push(SlotDevice {
                        slot_id,
                        port: root_port,
                        speed,
                        is_keyboard: is_kbd,
                        is_mouse: is_mou,
                        is_hub: false,
                        ep_int_dci: dci,
                        ep_slot,
                        ep_int_max_packet: max_pkt,
                        route_string,
                        parent_hub_slot,
                        keyboard: UsbHidKeyboard::new(),
                        mouse,
                        hid: if use_generic { Some(hid_model) } else { None },
                        use_generic,
                        iface_number: iface.interface_number,
                        iface_class: iface.interface_class,
                        iface_protocol: iface.interface_protocol,
                        report_events: 0,
                        last_report: [0u8; 8],
                        last_report_len: 0,
                    });
                }
                Err(e) => {
                    crate::println!("    usb-debug  slot {}: config_ep fallo: {}", slot_id, e);
                }
            }
        }

        Ok(())
    }

    /// Reads the Hub Descriptor, folds the Hub bit / port count into the Slot
    /// Context, powers every downstream port, and enumerates what is attached.
    fn configure_hub(
        &mut self,
        hub_slot: u8,
        root_port: u8,
        speed: u8,
        route_string: u32,
        tier: usize,
    ) -> Result<(), &'static str> {
        if tier >= hub::MAX_HUB_TIERS {
            return Err("hub topology deeper than the route string allows");
        }

        let mut buf = [0u8; 71];
        let get_hub_desc = [
            hub::REQ_TYPE_HUB_IN,
            hub::REQ_GET_DESCRIPTOR,
            0x00,
            hub::DESC_TYPE_HUB,
            0x00,
            0x00,
            71,
            0x00,
        ];
        let n = self.control_transfer(hub_slot, get_hub_desc, Some(&mut buf), true)?;
        let desc = hub::HubDescriptor::from_bytes(&buf[..n.min(buf.len())])
            .ok_or("invalid hub descriptor")?;

        self.update_hub_slot_context(hub_slot, speed, route_string, &desc)?;

        crate::println!(
            "    usb-hub    slot {} · {} puertos downstream",
            hub_slot,
            desc.num_ports
        );

        // Power all downstream ports, then wait the descriptor's settle time.
        for port in 1..=desc.num_ports {
            let set_power = [
                hub::REQ_TYPE_PORT_OUT,
                hub::REQ_SET_FEATURE,
                (hub::PORT_POWER & 0xFF) as u8,
                (hub::PORT_POWER >> 8) as u8,
                port,
                0x00,
                0x00,
                0x00,
            ];
            let _ = self.control_transfer(hub_slot, set_power, None, false);
        }
        delay_ms(desc.power_on_delay_ms() as u64);

        for port in 1..=desc.num_ports {
            if let Err(e) = self.enumerate_hub_port(hub_slot, root_port, route_string, tier, port) {
                crate::println!("    usb-debug  hub {} puerto {}: {}", hub_slot, port, e);
            }
        }

        Ok(())
    }

    /// Issues a Configure Endpoint command that only rewrites the Slot Context,
    /// setting the Hub bit and downstream port count. Some controller models
    /// reject an A0-only Configure Endpoint; the hub path then degrades
    /// gracefully and root-port devices are unaffected.
    fn update_hub_slot_context(
        &mut self,
        hub_slot: u8,
        speed: u8,
        route_string: u32,
        desc: &hub::HubDescriptor,
    ) -> Result<(), &'static str> {
        let slot_idx = (hub_slot - 1) as usize;
        let dma = &raw mut XHCI_DMA;
        unsafe {
            (*dma).slots[slot_idx].input_context.fill(0);
            let csz = self.params.csz_64;
            let mut view =
                context::ContextView::new(&mut (*dma).slots[slot_idx].input_context, csz);
            if let Some(mut icc) = view.input_control() {
                icc.set_add_flags(1 << 0); // Slot context only
            }
            if let Some(mut slot) = view.slot_context(true) {
                slot.set_info(route_string, speed, 1);
                slot.set_hub(true, desc.num_ports, false);
            }
            let phys =
                virt_to_phys(core::ptr::addr_of!((*dma).slots[slot_idx].input_context) as u64);
            self.send_command_and_wait(Trb::make_configure_endpoint(phys, hub_slot))?;
        }
        Ok(())
    }

    /// Drives one downstream hub port: GET_STATUS, reset, clear change bits, and
    /// enumerate the device with a route string one tier deeper.
    fn enumerate_hub_port(
        &mut self,
        hub_slot: u8,
        root_port: u8,
        hub_route: u32,
        hub_tier: usize,
        port: u8,
    ) -> Result<(), &'static str> {
        let get_status = [
            hub::REQ_TYPE_PORT_IN,
            hub::REQ_GET_STATUS,
            0x00,
            0x00,
            port,
            0x00,
            0x04,
            0x00,
        ];

        let mut st = [0u8; 4];
        self.control_transfer(hub_slot, get_status, Some(&mut st), true)?;
        let status = u16::from_le_bytes([st[0], st[1]]);
        if status & hub::port_status::CONNECTION == 0 {
            return Ok(());
        }

        // Reset the downstream port and poll for completion.
        let set_reset = [
            hub::REQ_TYPE_PORT_OUT,
            hub::REQ_SET_FEATURE,
            (hub::PORT_RESET & 0xFF) as u8,
            (hub::PORT_RESET >> 8) as u8,
            port,
            0x00,
            0x00,
            0x00,
        ];
        self.control_transfer(hub_slot, set_reset, None, false)?;
        delay_ms(50);

        let mut status_after = status;
        let mut tries = 10;
        while tries > 0 {
            let mut s = [0u8; 4];
            if self
                .control_transfer(hub_slot, get_status, Some(&mut s), true)
                .is_ok()
            {
                status_after = u16::from_le_bytes([s[0], s[1]]);
                if status_after & hub::port_status::RESET == 0
                    && status_after & hub::port_status::ENABLE != 0
                {
                    break;
                }
            }
            delay_ms(10);
            tries -= 1;
        }

        // Acknowledge the change bits the reset raised.
        for feat in [
            hub::C_PORT_CONNECTION,
            hub::C_PORT_RESET,
            hub::C_PORT_ENABLE,
        ] {
            let clear = [
                hub::REQ_TYPE_PORT_OUT,
                hub::REQ_CLEAR_FEATURE,
                (feat & 0xFF) as u8,
                (feat >> 8) as u8,
                port,
                0x00,
                0x00,
                0x00,
            ];
            let _ = self.control_transfer(hub_slot, clear, None, false);
        }

        if status_after & hub::port_status::ENABLE == 0 {
            return Err("downstream port did not enable after reset");
        }

        let speed = hub::speed_from_port_status(status_after);
        let child_route = hub::route_string_append(hub_route, hub_tier, port);
        self.enumerate_device(root_port, speed, child_route, hub_slot, port)
    }

    /// Polls the xHCI Event Ring for completed HID interrupt transfers and
    /// RootHub port changes, then re-arms the interrupt transfer rings.
    pub fn poll(&mut self) {
        let dma = &raw mut XHCI_DMA;
        unsafe {
            while let Some(event) = (*dma).event_ring.pop() {
                self.update_erdp();
                match event.trb_type() {
                    ring::TRB_TYPE_TRANSFER_EVENT => {
                        let slot_id = ((event.control >> 24) & 0xFF) as u8;
                        let ep_id = ((event.control >> 16) & 0x1F) as u8;
                        self.handle_transfer_event(slot_id, ep_id);
                    }
                    ring::TRB_TYPE_PORT_STATUS_CHANGE_EVENT => {
                        let port = ((event.parameter >> 24) & 0xFF) as u8;
                        self.service_root_port_change(port);
                    }
                    _ => {}
                }
            }
        }

        // Fallback for controllers that never post PORT_STATUS_CHANGE events
        // (observed under VirtualBox): sweep the root ports for W1C change bits.
        // Hot path — `poll()` runs on every timer tick and every input IRQ — so
        // rate-limit the (slow, per-port) MMIO sweep to a few times a second;
        // hotplug does not need sub-100 ms latency.
        self.port_scan_divider = self.port_scan_divider.wrapping_add(1);
        if self.port_scan_divider.is_multiple_of(16) {
            self.scan_root_port_changes();
        }
    }

    /// Handles a single Transfer Event on an interrupt endpoint: decodes the
    /// report from the interface's own buffer and re-arms its own ring.
    unsafe fn handle_transfer_event(&mut self, slot_id: u8, ep_id: u8) {
        let dma = &raw mut XHCI_DMA;
        let Some(dev) = self
            .devices
            .iter_mut()
            .find(|d| d.slot_id == slot_id && d.ep_int_dci == ep_id)
        else {
            return;
        };

        let slot_idx = (slot_id - 1) as usize;
        let ep_slot = dev.ep_slot.min(MAX_EP_SLOTS - 1);
        let slot_dma = &mut (*dma).slots[slot_idx];
        let report_buf = slot_dma.report_buffer[ep_slot];

        // Diagnostics snapshot (surfaced via `info` / SYS_SYSINFO).
        dev.report_events = dev.report_events.wrapping_add(1);
        let snap = report_buf.len().min(8);
        dev.last_report[..snap].copy_from_slice(&report_buf[..snap]);
        dev.last_report_len = snap as u8;

        if dev.use_generic {
            if let Some(hid) = dev.hid.as_mut() {
                let len = (dev.ep_int_max_packet as usize).min(report_buf.len());
                for ev in hid.decode(&report_buf[..len]) {
                    crate::input::push_event(ev);
                }
            }
        } else if dev.is_keyboard {
            let mut k_rep = [0u8; 8];
            k_rep.copy_from_slice(&report_buf[..8]);
            let events = dev.keyboard.process_report(&k_rep);
            for ev in events {
                crate::input::push_event(ev);
            }
        } else if dev.is_mouse {
            let len = (dev.ep_int_max_packet as usize).min(64);
            let events = dev.mouse.process_report(&report_buf[..len]);
            for ev in events {
                crate::input::push_event(ev);
            }
        }

        // Re-arm this interface's interrupt transfer.
        let report_phys = virt_to_phys(core::ptr::addr_of!(slot_dma.report_buffer[ep_slot]) as u64);
        let norm = Trb::make_normal(report_phys, dev.ep_int_max_packet as u32);
        let _ = slot_dma.ep_int_ring[ep_slot].push(norm);
        self.registers.ring_doorbell(slot_id, dev.ep_int_dci);
    }

    /// Sweeps every root port for a latched Connect Status Change.
    fn scan_root_port_changes(&mut self) {
        for port in 1..=self.params.max_ports {
            let sc = self.registers.read_portsc(port);
            if (sc & portsc::CSC) != 0 {
                self.service_root_port_change(port);
            }
        }
    }

    /// Attaches or detaches whatever is now on `port`, acknowledging the change.
    fn service_root_port_change(&mut self, port: u8) {
        let sc = self.registers.read_portsc(port);
        // Acknowledge the W1C change bits while preserving Port Power.
        self.registers
            .write_portsc(port, (sc & portsc::PP) | portsc::CSC | portsc::PRC);

        let connected = (sc & portsc::CCS) != 0;
        let known = self
            .devices
            .iter()
            .any(|d| d.port == port && d.route_string == 0);

        if connected && !known {
            crate::println!("  usb-hotplug  puerto {} conectado", port);
            self.enumerate_root_port(port);
        } else if !connected && known {
            crate::println!("  usb-hotplug  puerto {} desconectado", port);
            self.detach_root_port(port);
        }
    }

    /// Tears down every slot reached through `port` (the root device and any
    /// devices behind a hub on it): Disable Slot, clear the DCBAA entry, and
    /// drop the [`SlotDevice`] records.
    fn detach_root_port(&mut self, port: u8) {
        let dma = &raw mut XHCI_DMA;

        let mut slot_ids: Vec<u8> = Vec::new();
        for d in self.devices.iter().filter(|d| d.port == port) {
            if !slot_ids.contains(&d.slot_id) {
                slot_ids.push(d.slot_id);
            }
        }

        for slot_id in slot_ids {
            let _ = self.send_command_and_wait(Trb::make_disable_slot(slot_id));
            unsafe {
                (*dma).dcbaa[slot_id as usize] = 0;
            }
        }

        self.devices.retain(|d| d.port != port);
    }

    /// Writes the keyboard-LED bitmap (`Num`/`Caps`/`Scroll` Lock) to every
    /// enumerated keyboard via `SET_REPORT(Output)` on EP0.
    pub fn push_keyboard_leds(&mut self, bitmap: u8) {
        let targets: Vec<(u8, u8)> = self
            .devices
            .iter()
            .filter(|d| d.is_keyboard && !d.is_hub)
            .map(|d| (d.slot_id, d.iface_number))
            .collect();

        for (slot_id, iface) in targets {
            // bmRequestType 0x21 (Host->Dev | Class | Interface),
            // bRequest 0x09 SET_REPORT, wValue 0x0200 (Output report, ID 0).
            let setup = [0x21, 0x09, 0x00, 0x02, iface, 0x00, 0x01, 0x00];
            let mut data = [bitmap];
            let _ = self.control_transfer(slot_id, setup, Some(&mut data), false);
        }
    }
}
