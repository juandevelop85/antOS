//! xHCI Context Structures (Input Control, Slot, and Endpoint Contexts).
//!
//! Supports both 32-byte and 64-byte context sizes dynamically based on `HCCPARAMS1.CSZ`.

/// Helper for accessing and configuring an Input Control Context.
pub struct InputControlContext<'a> {
    data: &'a mut [u32],
}

impl<'a> InputControlContext<'a> {
    pub fn new(slice: &'a mut [u32]) -> Self {
        Self { data: slice }
    }

    pub fn set_drop_flags(&mut self, flags: u32) {
        if !self.data.is_empty() {
            self.data[0] = flags;
        }
    }

    pub fn set_add_flags(&mut self, flags: u32) {
        if self.data.len() > 1 {
            self.data[1] = flags;
        }
    }
}

/// Helper for accessing and configuring a Slot Context.
pub struct SlotContext<'a> {
    data: &'a mut [u32],
}

impl<'a> SlotContext<'a> {
    pub fn new(slice: &'a mut [u32]) -> Self {
        Self { data: slice }
    }

    /// Sets Route String (19:0), Speed (23:20), and Context Entries (31:27).
    pub fn set_info(&mut self, route_string: u32, speed: u8, context_entries: u8) {
        if !self.data.is_empty() {
            self.data[0] = (route_string & 0xF_FFFF)
                | (((speed as u32) & 0xF) << 20)
                | (((context_entries as u32) & 0x1F) << 27);
        }
    }

    /// Sets Root Hub Port Number (23:16) and Number of Ports (31:24).
    pub fn set_port_info(&mut self, root_hub_port: u8, num_ports: u8) {
        if self.data.len() > 1 {
            self.data[1] =
                (((root_hub_port as u32) & 0xFF) << 16) | (((num_ports as u32) & 0xFF) << 24);
        }
    }

    /// Marks the slot as a hub: DWORD0 Hub (26) and Multi-TT (25) bits, plus
    /// DWORD1 Number of Ports (31:24). Read-modify-write so it composes with a
    /// prior [`set_info`] / [`set_port_info`] on the same context.
    pub fn set_hub(&mut self, is_hub: bool, num_ports: u8, multi_tt: bool) {
        if !self.data.is_empty() {
            let mut d0 = self.data[0] & !((1 << 25) | (1 << 26));
            if is_hub {
                d0 |= 1 << 26;
            }
            if multi_tt {
                d0 |= 1 << 25;
            }
            self.data[0] = d0;
        }
        if self.data.len() > 1 {
            self.data[1] = (self.data[1] & 0x00FF_FFFF) | (((num_ports as u32) & 0xFF) << 24);
        }
    }

    /// Sets the Transaction Translator fields in DWORD2: Parent Hub Slot ID
    /// (7:0), Parent Port Number (15:8), and TT Think Time (17:16). Needed when a
    /// Full/Low-speed device sits behind a High-speed hub.
    pub fn set_tt_info(&mut self, parent_hub_slot: u8, parent_port: u8, think_time: u8) {
        if self.data.len() > 2 {
            self.data[2] = (self.data[2] & !0x0003_FFFF)
                | ((parent_hub_slot as u32) & 0xFF)
                | (((parent_port as u32) & 0xFF) << 8)
                | (((think_time as u32) & 0x3) << 16);
        }
    }
}

/// Helper for accessing and configuring an Endpoint Context.
pub struct EndpointContext<'a> {
    data: &'a mut [u32],
}

impl<'a> EndpointContext<'a> {
    pub fn new(slice: &'a mut [u32]) -> Self {
        Self { data: slice }
    }

    /// Sets Interval (23:16).
    pub fn set_interval(&mut self, interval: u8) {
        if !self.data.is_empty() {
            self.data[0] = ((interval as u32) & 0xFF) << 16;
        }
    }

    /// Sets Error Count CErr (2:1), Endpoint Type (5:3), and Max Packet Size (31:16).
    pub fn set_type_and_max_packet(&mut self, ep_type: u8, max_packet_size: u16, cerr: u8) {
        if self.data.len() > 1 {
            self.data[1] = (((cerr as u32) & 0x3) << 1)
                | (((ep_type as u32) & 0x7) << 3)
                | (((max_packet_size as u32) & 0xFFFF) << 16);
        }
    }

    /// Sets TR Dequeue Pointer (64-bit) with Dequeue Cycle State DCS (bit 0).
    pub fn set_tr_dequeue_pointer(&mut self, ptr: u64, dcs: bool) {
        if self.data.len() > 3 {
            let val = ptr | (if dcs { 1 } else { 0 });
            self.data[2] = val as u32;
            self.data[3] = (val >> 32) as u32;
        }
    }

    /// Sets Average TRB Length (15:0) and Max ESIT Payload (31:16).
    pub fn set_transfer_info(&mut self, avg_trb_len: u16, max_esit_payload: u16) {
        if self.data.len() > 4 {
            self.data[4] = (avg_trb_len as u32) | (((max_esit_payload as u32) & 0xFFFF) << 16);
        }
    }
}

/// Dynamic context slice indexer based on 32-byte or 64-byte context size.
pub struct ContextView<'a> {
    raw: &'a mut [u8],
    ctx_stride: usize,
}

impl<'a> ContextView<'a> {
    pub fn new(raw: &'a mut [u8], csz_64: bool) -> Self {
        let ctx_stride = if csz_64 { 64 } else { 32 };
        Self { raw, ctx_stride }
    }

    fn get_u32_slice(&mut self, index: usize) -> Option<&mut [u32]> {
        let start = index * self.ctx_stride;
        let end = start + self.ctx_stride;
        if end > self.raw.len() {
            return None;
        }
        let u32_count = self.ctx_stride / 4;
        let ptr = self.raw[start..end].as_mut_ptr() as *mut u32;
        Some(unsafe { core::slice::from_raw_parts_mut(ptr, u32_count) })
    }

    /// Obtains the Input Control Context (index 0 of Input Context).
    pub fn input_control(&mut self) -> Option<InputControlContext<'_>> {
        self.get_u32_slice(0).map(InputControlContext::new)
    }

    /// Obtains the Slot Context (index 1 of Input Context, or index 0 of Device Context).
    pub fn slot_context(&mut self, is_input: bool) -> Option<SlotContext<'_>> {
        let idx = if is_input { 1 } else { 0 };
        self.get_u32_slice(idx).map(SlotContext::new)
    }

    /// Obtains the Endpoint Context for DCI (Device Context Index 1..=31).
    pub fn endpoint_context(&mut self, dci: usize, is_input: bool) -> Option<EndpointContext<'_>> {
        let idx = if is_input { dci + 1 } else { dci };
        self.get_u32_slice(idx).map(EndpointContext::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn slot_context_route_string_and_hub_bits_compose() {
        let mut dwords = [0u32; 8];
        let mut slot = SlotContext::new(&mut dwords);
        // Device four tiers deep, HighSpeed (code 3), one context entry.
        slot.set_info(0x4321, 3, 1);
        slot.set_port_info(2, 0);
        slot.set_hub(true, 7, true);

        assert_eq!(dwords[0] & 0xF_FFFF, 0x4321, "route string preserved");
        assert_eq!((dwords[0] >> 20) & 0xF, 3, "speed preserved");
        assert_eq!((dwords[0] >> 27) & 0x1F, 1, "context entries preserved");
        assert_ne!(dwords[0] & (1 << 26), 0, "hub bit set");
        assert_ne!(dwords[0] & (1 << 25), 0, "multi-TT bit set");
        assert_eq!((dwords[1] >> 16) & 0xFF, 2, "root hub port preserved");
        assert_eq!((dwords[1] >> 24) & 0xFF, 7, "number of ports set");
    }

    #[test_case]
    fn slot_context_tt_info_packs_dword2() {
        let mut dwords = [0u32; 8];
        let mut slot = SlotContext::new(&mut dwords);
        slot.set_tt_info(5, 3, 1);
        assert_eq!(dwords[2] & 0xFF, 5);
        assert_eq!((dwords[2] >> 8) & 0xFF, 3);
        assert_eq!((dwords[2] >> 16) & 0x3, 1);
    }
}
