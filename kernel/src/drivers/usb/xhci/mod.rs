//! xHCI (eXtensible Host Controller Interface) Driver.
//!
//! Provides xHCI host controller initialization, DMA rings management,
//! and RootHub port status discovery.

pub mod registers;
pub mod ring;

use alloc::vec::Vec;
use registers::{XhciRegisters, usbcmd, usbsts, portsc, iman, HcParams};
use ring::{CommandRing, EventRing, EventRingSegmentEntry, RING_SIZE};
use crate::drivers::pci::{self, PciDevice};

/// Static DMA pool for xHCI controller structures aligned to 4096 bytes.
#[repr(C, align(4096))]
pub struct XhciDmaPool {
    pub dcbaa: [u64; 256],
    pub erst: [EventRingSegmentEntry; 1],
    pub command_ring: CommandRing,
    pub event_ring: EventRing,
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
        }
    }
}

static mut XHCI_DMA: XhciDmaPool = XhciDmaPool::new();

/// Information about an xHCI RootHub port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortInfo {
    pub port_number: u8,
    pub connected: bool,
    pub enabled: bool,
    pub speed_name: &'static str,
}

/// Active xHCI Host Controller instance.
pub struct XhciController {
    pub registers: XhciRegisters,
    pub pci_device: PciDevice,
    pub params: HcParams,
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
                },
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
        #[cfg(target_arch = "aarch64")]
        let dcbaa_phys = crate::arch::aarch64::mmu::kernel_virt_to_phys(
            core::ptr::addr_of!((*dma_pool).dcbaa) as u64
        );
        #[cfg(not(target_arch = "aarch64"))]
        let dcbaa_phys = core::ptr::addr_of!((*dma_pool).dcbaa) as u64;

        #[cfg(target_arch = "aarch64")]
        let cmd_ring_phys = crate::arch::aarch64::mmu::kernel_virt_to_phys(
            core::ptr::addr_of!((*dma_pool).command_ring) as u64
        );
        #[cfg(not(target_arch = "aarch64"))]
        let cmd_ring_phys = core::ptr::addr_of!((*dma_pool).command_ring) as u64;

        #[cfg(target_arch = "aarch64")]
        let event_ring_phys = crate::arch::aarch64::mmu::kernel_virt_to_phys(
            core::ptr::addr_of!((*dma_pool).event_ring) as u64
        );
        #[cfg(not(target_arch = "aarch64"))]
        let event_ring_phys = core::ptr::addr_of!((*dma_pool).event_ring) as u64;

        #[cfg(target_arch = "aarch64")]
        let erst_phys = crate::arch::aarch64::mmu::kernel_virt_to_phys(
            core::ptr::addr_of!((*dma_pool).erst) as u64
        );
        #[cfg(not(target_arch = "aarch64"))]
        let erst_phys = core::ptr::addr_of!((*dma_pool).erst) as u64;

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
}
