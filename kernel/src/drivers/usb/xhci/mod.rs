//! xHCI (eXtensible Host Controller Interface) Driver.
//!
//! Provides xHCI host controller initialization, DMA rings management,
//! RootHub port status discovery, USB device enumeration, and HID interrupt polling.

pub mod context;
pub mod registers;
pub mod ring;

use alloc::vec::Vec;
use registers::{XhciRegisters, usbcmd, usbsts, portsc, iman, HcParams};
use ring::{CommandRing, EventRing, EventRingSegmentEntry, TransferRing, Trb, RING_SIZE};
use crate::drivers::pci::{self, PciDevice};
use crate::drivers::usb::hid::{UsbHidKeyboard, UsbHidMouse};

/// DMA buffers for an individual device slot.
#[repr(C, align(64))]
pub struct SlotDma {
    pub device_context: [u8; 2048],
    pub input_context: [u8; 2112],
    pub ep0_ring: TransferRing,
    pub ep_int_ring: TransferRing,
    pub control_buffer: [u8; 256],
    pub report_buffer: [u8; 64],
}

impl SlotDma {
    pub const fn new() -> Self {
        Self {
            device_context: [0u8; 2048],
            input_context: [0u8; 2112],
            ep0_ring: TransferRing::new(),
            ep_int_ring: TransferRing::new(),
            control_buffer: [0u8; 256],
            report_buffer: [0u8; 64],
        }
    }
}

/// Static DMA pool for xHCI controller structures aligned to 4096 bytes.
#[repr(C, align(4096))]
pub struct XhciDmaPool {
    pub dcbaa: [u64; 256],
    pub erst: [EventRingSegmentEntry; 1],
    pub command_ring: CommandRing,
    pub event_ring: EventRing,
    pub slots: [SlotDma; 4],
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
            slots: [
                SlotDma::new(),
                SlotDma::new(),
                SlotDma::new(),
                SlotDma::new(),
            ],
        }
    }
}

static mut XHCI_DMA: XhciDmaPool = XhciDmaPool::new();

#[inline]
pub fn virt_to_phys(vaddr: u64) -> u64 {
    #[cfg(target_arch = "aarch64")]
    {
        crate::arch::aarch64::mmu::kernel_virt_to_phys(vaddr)
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        vaddr
    }
}

/// Busy-wait delay in milliseconds, used for USB spec-mandated port timings.
#[inline]
pub fn delay_ms(ms: u64) {
    #[cfg(target_arch = "aarch64")]
    {
        crate::arch::aarch64::timer::delay_ms(ms);
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        for _ in 0..(ms * 5_000) {
            core::hint::spin_loop();
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
    pub ep_int_dci: u8,
    pub ep_int_max_packet: u16,
    pub keyboard: UsbHidKeyboard,
    pub mouse: UsbHidMouse,
}

/// Active xHCI Host Controller instance.
pub struct XhciController {
    pub registers: XhciRegisters,
    pub pci_device: PciDevice,
    pub params: HcParams,
    pub devices: Vec<SlotDevice>,
}

impl XhciController {
    /// Discovers an xHCI controller on the PCIe bus and initializes it.
    pub fn probe_and_init() -> Option<Self> {
        #[cfg(target_arch = "aarch64")]
        pci::probe_ecam_base();

        let devices = pci::scan_pci_bus();
        let xhci_dev = devices.into_iter().find(|d| d.is_xhci_controller())?;

        // Extract BAR0 physical MMIO base
        let bar0_addr = xhci_dev.bars[0].memory_address()?;
        xhci_dev.enable_bus_mastering();

        unsafe {
            let registers = XhciRegisters::new(bar0_addr as usize).ok()?;
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
            };
            controller.params = controller.registers.params();

            if controller.init_hardware().is_ok() {
                Some(controller)
            } else {
                None
            }
        }
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
        let max_slots = self.params.max_slots.min(16).max(1);
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
        let sc = self.registers.read_portsc(port);
        if (sc & portsc::CCS) == 0 {

            return Err("Port not connected");
        }

        if (sc & portsc::PED) != 0 {
            let speed = ((sc >> portsc::SPEED_SHIFT) & portsc::SPEED_MASK) as u8;
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

    /// Submits an Enable Slot command and returns the allocated Slot ID (1..=4).
    pub fn enable_slot(&mut self) -> Result<u8, &'static str> {
        let trb = Trb::make_enable_slot(false);
        let event = self.send_command_and_wait(trb)?;
        let slot_id = ((event.control >> 24) & 0xFF) as u8;
        if slot_id == 0 || slot_id > 4 {
            return Err("Invalid slot ID allocated");
        }
        Ok(slot_id)
    }

    /// Prepares Input/Device Contexts and issues the Address Device command.
    pub fn address_device(&mut self, slot_id: u8, port: u8, speed: u8) -> Result<(), &'static str> {
        let slot_idx = (slot_id - 1) as usize;
        let dma = &raw mut XHCI_DMA;

        unsafe {
            let dev_ctx_phys = virt_to_phys(core::ptr::addr_of!((*dma).slots[slot_idx].device_context) as u64);
            (*dma).dcbaa[slot_id as usize] = dev_ctx_phys;

            (*dma).slots[slot_idx].input_context.fill(0);
            (*dma).slots[slot_idx].device_context.fill(0);

            let ep0_ring_phys = virt_to_phys(core::ptr::addr_of!((*dma).slots[slot_idx].ep0_ring) as u64);
            (*dma).slots[slot_idx].ep0_ring = ring::TransferRing::new();
            (*dma).slots[slot_idx].ep0_ring.init_link(ep0_ring_phys);

            let csz = self.params.csz_64;
            let mut view = context::ContextView::new(&mut (*dma).slots[slot_idx].input_context, csz);

            if let Some(mut icc) = view.input_control() {
                icc.set_add_flags((1 << 0) | (1 << 1)); // Slot + EP0
            }

            if let Some(mut slot) = view.slot_context(true) {
                slot.set_info(0, speed, 1);
                slot.set_port_info(port, 0);
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

            let input_ctx_phys = virt_to_phys(core::ptr::addr_of!((*dma).slots[slot_idx].input_context) as u64);
            let trb = Trb::make_address_device(input_ctx_phys, slot_id, false);
            self.send_command_and_wait(trb)?;
        }

        Ok(())
    }

    /// Performs a USB Control Transfer on Endpoint 0.
    pub fn control_transfer(
        &mut self,
        slot_id: u8,
        setup: [u8; 8],
        mut data: Option<&mut [u8]>,
        is_in: bool,
    ) -> Result<usize, &'static str> {
        let slot_idx = (slot_id - 1) as usize;
        let dma = &raw mut XHCI_DMA;

        let data_len = if let Some(ref d) = data { d.len() as u32 } else { 0 };
        let trt = if data_len == 0 {
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
            slot_dma.ep0_ring.push(setup_trb)?;

            // 2. Data Stage TRB
            if data_len > 0 {
                let buf_phys = virt_to_phys(core::ptr::addr_of!(slot_dma.control_buffer) as u64);
                if !is_in {
                    if let Some(ref src) = data {
                        let len = (data_len as usize).min(256);
                        slot_dma.control_buffer[..len].copy_from_slice(&src[..len]);
                    }
                }
                let data_trb = Trb::make_data_stage(buf_phys, data_len.min(256), is_in);
                slot_dma.ep0_ring.push(data_trb)?;
            }

            // 3. Status Stage TRB
            let status_in = if data_len > 0 && is_in { false } else { true };
            let status_trb = Trb::make_status_stage(status_in);
            slot_dma.ep0_ring.push(status_trb)?;

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
                                if is_in && data_len > 0 {
                                    if let Some(ref mut dest) = data {
                                        let len = (data_len as usize).min(dest.len()).min(256);
                                        dest[..len].copy_from_slice(&slot_dma.control_buffer[..len]);
                                    }
                                }
                                return Ok(data_len as usize);
                            } else {
                                return Err("Control transfer failed");
                            }
                        }
                    }
                }
                delay_ms(1);
                timeout -= 1;
            }

        }

        Err("Control transfer timed out")
    }

    /// Configures an Interrupt IN endpoint (e.g. EP 1 IN -> DCI 3).
    pub fn configure_hid_endpoint(
        &mut self,
        slot_id: u8,
        speed: u8,
        ep_addr: u8,
        max_packet: u16,
        interval: u8,
    ) -> Result<u8, &'static str> {
        let slot_idx = (slot_id - 1) as usize;
        let dma = &raw mut XHCI_DMA;

        let ep_num = ep_addr & 0x0F;
        let is_in = (ep_addr & 0x80) != 0;
        let dci = (ep_num * 2) + (if is_in { 1 } else { 0 });

        unsafe {
            (*dma).slots[slot_idx].input_context.fill(0);

            let ep_int_ring_phys = virt_to_phys(core::ptr::addr_of!((*dma).slots[slot_idx].ep_int_ring) as u64);
            (*dma).slots[slot_idx].ep_int_ring = ring::TransferRing::new();
            (*dma).slots[slot_idx].ep_int_ring.init_link(ep_int_ring_phys);

            let csz = self.params.csz_64;
            let mut view = context::ContextView::new(&mut (*dma).slots[slot_idx].input_context, csz);

            if let Some(mut icc) = view.input_control() {
                icc.set_add_flags((1 << 0) | (1 << dci));
            }

            if let Some(mut slot) = view.slot_context(true) {
                slot.set_info(0, speed, dci);
            }

            if let Some(mut ep_ctx) = view.endpoint_context(dci as usize, true) {
                ep_ctx.set_interval(interval.max(3));
                let ep_type = if is_in { 7 } else { 3 }; // 7 = Interrupt IN
                ep_ctx.set_type_and_max_packet(ep_type, max_packet.max(8), 3);
                ep_ctx.set_tr_dequeue_pointer(ep_int_ring_phys, true);
                ep_ctx.set_transfer_info(max_packet.max(8), max_packet.max(8));
            }

            let input_ctx_phys = virt_to_phys(core::ptr::addr_of!((*dma).slots[slot_idx].input_context) as u64);
            let trb = Trb::make_configure_endpoint(input_ctx_phys, slot_id);
            self.send_command_and_wait(trb)?;

            // Queue first Normal TRB on Interrupt Ring
            let report_phys = virt_to_phys(core::ptr::addr_of!((*dma).slots[slot_idx].report_buffer) as u64);
            let normal_trb = Trb::make_normal(report_phys, max_packet.max(8) as u32);
            (*dma).slots[slot_idx].ep_int_ring.push(normal_trb)?;

            // Ring doorbell for interrupt endpoint
            self.registers.ring_doorbell(slot_id, dci);
        }

        Ok(dci)
    }

    /// Enumerates all connected RootHub ports, addresses devices, and configures HID endpoints.
    pub fn enumerate_connected_ports(&mut self) {
        let ports = self.inspect_ports();
        for p in ports {
            if !p.connected {
                continue;
            }

            let port_num = p.port_number;
            let speed = match self.reset_port(port_num) {
                Ok(s) => s,
                Err(e) => {
                    crate::println!("    usb-debug  puerto {}: reset fallo: {}", port_num, e);
                    continue;
                }
            };

            let slot_id = match self.enable_slot() {
                Ok(s) => s,
                Err(e) => {
                    crate::println!("    usb-debug  puerto {}: enable_slot fallo: {}", port_num, e);
                    continue;
                }
            };

            if let Err(e) = self.address_device(slot_id, port_num, speed) {
                crate::println!("    usb-debug  slot {}: address_device fallo: {}", slot_id, e);
                continue;
            }

            // 1. Get Device Descriptor
            let mut dev_desc_buf = [0u8; 18];
            let get_dev_setup = [0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 18, 0x00];
            if let Err(e) = self.control_transfer(slot_id, get_dev_setup, Some(&mut dev_desc_buf), true) {
                crate::println!("    usb-debug  slot {}: get_dev_desc fallo: {}", slot_id, e);
            }

            // 2. Get Configuration Descriptor (first 9 bytes, then full bundle)
            let mut config_hdr = [0u8; 9];
            let get_cfg_setup = [0x80, 0x06, 0x00, 0x02, 0x00, 0x00, 9, 0x00];
            if let Err(e) = self.control_transfer(slot_id, get_cfg_setup, Some(&mut config_hdr), true) {
                crate::println!("    usb-debug  slot {}: get_cfg_hdr fallo: {}", slot_id, e);
                continue;
            }

            let total_len = u16::from_le_bytes([config_hdr[2], config_hdr[3]]) as usize;
            let read_len = total_len.min(256);
            let mut config_buf = alloc::vec![0u8; read_len];
            let get_full_cfg = [0x80, 0x06, 0x00, 0x02, 0x00, 0x00, read_len as u8, 0x00];
            let _ = self.control_transfer(slot_id, get_full_cfg, Some(&mut config_buf), true);

            let (config_val, ifaces) = crate::drivers::usb::descriptor::parse_configuration_bundle(&config_buf);
            let config_val = config_val.unwrap_or(1);

            // 3. Set Configuration
            let set_cfg_setup = [0x00, 0x09, config_val, 0x00, 0x00, 0x00, 0x00, 0x00];
            let _ = self.control_transfer(slot_id, set_cfg_setup, None, false);

            // 4. Configure interfaces
            for iface in ifaces {
                let max_pkt = iface.ep_max_packet.max(8);
                match self.configure_hid_endpoint(slot_id, speed, iface.ep_addr, max_pkt, iface.ep_interval) {
                    Ok(dci) => {
                        let is_kbd = iface.is_boot_keyboard || port_num == 1;
                        let is_mou = (iface.is_boot_mouse || port_num == 2) && !is_kbd;

                        let mut mouse = UsbHidMouse::new();
                        if is_mou {
                            mouse.is_absolute = max_pkt >= 5;
                        }

                        self.devices.push(SlotDevice {
                            slot_id,
                            port: port_num,
                            speed,
                            is_keyboard: is_kbd,
                            is_mouse: is_mou,
                            ep_int_dci: dci,
                            ep_int_max_packet: max_pkt,
                            keyboard: UsbHidKeyboard::new(),
                            mouse,
                        });
                    }
                    Err(e) => {
                        crate::println!("    usb-debug  slot {}: config_ep fallo: {}", slot_id, e);
                    }
                }
            }
        }
    }

    /// Polls the xHCI Event Ring for completed HID interrupt transfers,
    /// pushes input events, and re-arms the transfer rings.
    pub fn poll(&mut self) {
        let dma = &raw mut XHCI_DMA;
        unsafe {
            while let Some(event) = (*dma).event_ring.pop() {
                self.update_erdp();
                if event.trb_type() == ring::TRB_TYPE_TRANSFER_EVENT {
                    let slot_id = ((event.control >> 24) & 0xFF) as u8;
                    let ep_id = ((event.control >> 16) & 0x1F) as u8;

                    if let Some(dev) = self.devices.iter_mut().find(|d| d.slot_id == slot_id && d.ep_int_dci == ep_id) {
                        let slot_idx = (slot_id - 1) as usize;
                        let slot_dma = &mut (*dma).slots[slot_idx];
                        let report_buf = &slot_dma.report_buffer;

                        if dev.is_keyboard {
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

                        // Re-arm interrupt transfer
                        let report_phys = virt_to_phys(core::ptr::addr_of!(slot_dma.report_buffer) as u64);
                        let norm = Trb::make_normal(report_phys, dev.ep_int_max_packet as u32);
                        let _ = slot_dma.ep_int_ring.push(norm);
                        self.registers.ring_doorbell(slot_id, dev.ep_int_dci);
                    }
                }
            }
        }
    }
}
