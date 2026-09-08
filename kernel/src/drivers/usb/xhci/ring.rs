//! xHCI Transfer Request Block (TRB) and Ring Structures.
//!
//! Provides TRB bitfield encoding, Command Ring, and Event Ring primitives
//! aligned to xHCI hardware specifications.

pub const TRB_TYPE_NORMAL: u8 = 1;
pub const TRB_TYPE_SETUP_STAGE: u8 = 2;
pub const TRB_TYPE_DATA_STAGE: u8 = 3;
pub const TRB_TYPE_STATUS_STAGE: u8 = 4;
pub const TRB_TYPE_LINK: u8 = 6;
pub const TRB_TYPE_ENABLE_SLOT: u8 = 9;
pub const TRB_TYPE_DISABLE_SLOT: u8 = 10;
pub const TRB_TYPE_ADDRESS_DEVICE: u8 = 11;
pub const TRB_TYPE_CONFIGURE_ENDPOINT: u8 = 12;
pub const TRB_TYPE_EVALUATE_CONTEXT: u8 = 13;
pub const TRB_TYPE_RESET_ENDPOINT: u8 = 14;
pub const TRB_TYPE_SET_TR_DEQUEUE: u8 = 16;
pub const TRB_TYPE_TRANSFER_EVENT: u8 = 32;
pub const TRB_TYPE_COMMAND_COMPLETION_EVENT: u8 = 33;
pub const TRB_TYPE_PORT_STATUS_CHANGE_EVENT: u8 = 34;

/// Standard 16-byte Transfer Request Block (TRB).
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Trb {
    pub parameter: u64,
    pub status: u32,
    pub control: u32,
}

impl Trb {
    pub const fn empty() -> Self {
        Self {
            parameter: 0,
            status: 0,
            control: 0,
        }
    }

    pub const fn new(parameter: u64, status: u32, control: u32) -> Self {
        Self {
            parameter,
            status,
            control,
        }
    }

    #[inline]
    pub fn trb_type(&self) -> u8 {
        ((self.control >> 10) & 0x3F) as u8
    }

    #[inline]
    pub fn cycle_bit(&self) -> bool {
        (self.control & 1) != 0
    }

    /// Constructs a Link TRB pointing to `target` address.
    pub fn make_link(target_phys: u64, toggle_cycle: bool) -> Self {
        let mut control = ((TRB_TYPE_LINK as u32) << 10) | 1; // initial cycle bit = 1
        if toggle_cycle {
            control |= 1 << 1; // TC (Toggle Cycle) bit
        }
        Self::new(target_phys, 0, control)
    }

    /// Constructs an Enable Slot command TRB.
    pub fn make_enable_slot(cycle_bit: bool) -> Self {
        let control = ((TRB_TYPE_ENABLE_SLOT as u32) << 10) | (if cycle_bit { 1 } else { 0 });
        Self::new(0, 0, control)
    }

    /// Constructs an Address Device command TRB.
    pub fn make_address_device(input_ctx_phys: u64, slot_id: u8, bsr: bool) -> Self {
        let mut control = ((TRB_TYPE_ADDRESS_DEVICE as u32) << 10) | ((slot_id as u32) << 24);
        if bsr {
            control |= 1 << 9;
        }
        Self::new(input_ctx_phys, 0, control)
    }

    /// Constructs a Configure Endpoint command TRB.
    pub fn make_configure_endpoint(input_ctx_phys: u64, slot_id: u8) -> Self {
        let control = ((TRB_TYPE_CONFIGURE_ENDPOINT as u32) << 10) | ((slot_id as u32) << 24);
        Self::new(input_ctx_phys, 0, control)
    }

    /// Constructs an Evaluate Context command TRB (used to fold a freshly parsed
    /// Hub Descriptor's port count and the Hub bit into an addressed slot).
    pub fn make_evaluate_context(input_ctx_phys: u64, slot_id: u8) -> Self {
        let control = ((TRB_TYPE_EVALUATE_CONTEXT as u32) << 10) | ((slot_id as u32) << 24);
        Self::new(input_ctx_phys, 0, control)
    }

    /// Constructs a Disable Slot command TRB (detach / hot-unplug path).
    pub fn make_disable_slot(slot_id: u8) -> Self {
        let control = ((TRB_TYPE_DISABLE_SLOT as u32) << 10) | ((slot_id as u32) << 24);
        Self::new(0, 0, control)
    }

    /// Constructs a Reset Endpoint command TRB, clearing a `Halted` endpoint
    /// after a STALL. `dci` is the Device Context Index (1 = EP0).
    pub fn make_reset_endpoint(slot_id: u8, dci: u8) -> Self {
        let control = ((TRB_TYPE_RESET_ENDPOINT as u32) << 10)
            | (((dci as u32) & 0x1F) << 16)
            | ((slot_id as u32) << 24);
        Self::new(0, 0, control)
    }

    /// Constructs a Set TR Dequeue Pointer command TRB, re-seeding an endpoint's
    /// transfer ring after a reset so the controller and driver agree on the
    /// next TRB and cycle state.
    pub fn make_set_tr_dequeue(deq_ptr_phys: u64, dcs: bool, slot_id: u8, dci: u8) -> Self {
        let param = (deq_ptr_phys & !0xF) | (if dcs { 1 } else { 0 });
        let control = ((TRB_TYPE_SET_TR_DEQUEUE as u32) << 10)
            | (((dci as u32) & 0x1F) << 16)
            | ((slot_id as u32) << 24);
        Self::new(param, 0, control)
    }

    /// Constructs a Setup Stage TRB for Control Transfers on Endpoint 0.
    pub fn make_setup_stage(setup_bytes: [u8; 8], trt: u8) -> Self {
        let param = u64::from_le_bytes(setup_bytes);
        let control = ((TRB_TYPE_SETUP_STAGE as u32) << 10) | ((trt as u32) << 16) | (1 << 6); // IDT = 1
        Self::new(param, 8, control)
    }

    /// Constructs a Data Stage TRB for Control Transfers.
    pub fn make_data_stage(buf_phys: u64, len: u32, is_in: bool) -> Self {
        let mut control = ((TRB_TYPE_DATA_STAGE as u32) << 10) | (1 << 2); // ISP = 1
        if is_in {
            control |= 1 << 16; // Dir = IN
        }
        Self::new(buf_phys, len, control)
    }

    /// Constructs a Status Stage TRB for Control Transfers.
    pub fn make_status_stage(is_in: bool) -> Self {
        let mut control = ((TRB_TYPE_STATUS_STAGE as u32) << 10) | (1 << 5); // IOC = 1
        if is_in {
            control |= 1 << 16; // Dir = IN
        }
        Self::new(0, 0, control)
    }

    /// Constructs a Normal TRB for Interrupt / Bulk transfers.
    pub fn make_normal(buf_phys: u64, len: u32) -> Self {
        let control = ((TRB_TYPE_NORMAL as u32) << 10) | (1 << 5) | (1 << 2); // IOC = 1, ISP = 1
        Self::new(buf_phys, len, control)
    }

    /// Constructs a No-Op command TRB.
    pub fn make_noop(cycle_bit: bool) -> Self {
        let control = (23 << 10) | (if cycle_bit { 1 } else { 0 });
        Self::new(0, 0, control)
    }
}

/// Transfer Ring alias sharing identical ring mechanics with CommandRing.
pub type TransferRing = CommandRing;

/// Event Ring Segment Table (ERST) Entry aligned to 64 bytes.
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy, Default)]
pub struct EventRingSegmentEntry {
    pub ring_segment_base_address: u64,
    pub ring_segment_size: u32,
    pub reserved: u32,
}

pub const RING_SIZE: usize = 64;

/// Circular buffer representing an xHCI Command Ring.
#[repr(C, align(64))]
pub struct CommandRing {
    pub trbs: [Trb; RING_SIZE],
    pub enqueue_idx: usize,
    pub cycle_state: bool,
}

impl CommandRing {
    pub const fn new() -> Self {
        Self {
            trbs: [Trb::empty(); RING_SIZE],
            enqueue_idx: 0,
            cycle_state: true,
        }
    }

    /// Initializes the ring's Link TRB at the end to loop back to `phys_base`.
    pub fn init_link(&mut self, phys_base: u64) {
        self.trbs[RING_SIZE - 1] = Trb::make_link(phys_base, true);
    }

    /// Clears every TRB and rewinds the enqueue pointer / cycle state to their
    /// power-on values, then re-installs the Link TRB. Used to recover a control
    /// endpoint whose ring is out of sync with the controller after a STALL,
    /// paired with a Set TR Dequeue Pointer command.
    pub fn reset(&mut self, phys_base: u64) {
        self.trbs = [Trb::empty(); RING_SIZE];
        self.enqueue_idx = 0;
        self.cycle_state = true;
        self.init_link(phys_base);
    }

    /// Pushes a TRB to the command ring and advances the enqueue pointer.
    pub fn push(&mut self, mut trb: Trb) -> Result<usize, &'static str> {
        if self.enqueue_idx >= RING_SIZE - 1 {
            // Need to handle link transition
            let link_idx = RING_SIZE - 1;
            let mut link = self.trbs[link_idx];
            // Toggle link cycle bit if needed
            link.control = (link.control & !1) | (if self.cycle_state { 1 } else { 0 });
            self.trbs[link_idx] = link;
            self.enqueue_idx = 0;
            self.cycle_state = !self.cycle_state;
        }

        // Apply active cycle state to TRB
        trb.control = (trb.control & !1) | (if self.cycle_state { 1 } else { 0 });
        let written_idx = self.enqueue_idx;
        self.trbs[written_idx] = trb;
        self.enqueue_idx += 1;
        Ok(written_idx)
    }
}

/// Circular buffer representing an xHCI Event Ring.
#[repr(C, align(64))]
pub struct EventRing {
    pub trbs: [Trb; RING_SIZE],
    pub dequeue_idx: usize,
    pub cycle_state: bool,
}

impl EventRing {
    pub const fn new() -> Self {
        Self {
            trbs: [Trb::empty(); RING_SIZE],
            dequeue_idx: 0,
            cycle_state: true,
        }
    }

    /// Checks if a new event TRB is available at the dequeue pointer.
    pub fn has_event(&self) -> bool {
        let current = &self.trbs[self.dequeue_idx];
        current.cycle_bit() == self.cycle_state
    }

    /// Pops the next event TRB from the ring if ready.
    pub fn pop(&mut self) -> Option<Trb> {
        if !self.has_event() {
            return None;
        }

        let event = self.trbs[self.dequeue_idx];
        self.dequeue_idx += 1;
        if self.dequeue_idx >= RING_SIZE {
            self.dequeue_idx = 0;
            self.cycle_state = !self.cycle_state;
        }
        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trb_type_and_cycle() {
        let trb = Trb::make_enable_slot(true);
        assert_eq!(trb.trb_type(), TRB_TYPE_ENABLE_SLOT);
        assert!(trb.cycle_bit());

        let link = Trb::make_link(0x1000_0000, true);
        assert_eq!(link.trb_type(), TRB_TYPE_LINK);
        assert_eq!(link.parameter, 0x1000_0000);
    }

    #[test]
    fn test_command_ring_push() {
        let mut ring = CommandRing::new();
        ring.init_link(0x2000);
        let idx0 = ring.push(Trb::make_noop(false)).expect("push");
        assert_eq!(idx0, 0);
        assert!(ring.trbs[0].cycle_bit());
    }
}
