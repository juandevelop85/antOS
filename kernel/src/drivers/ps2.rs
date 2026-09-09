//! PS/2 mouse (8042 auxiliary device) driver for x86_64.
//!
//! The 8042 controller multiplexes a keyboard on the first port (IRQ 1) and a
//! mouse on the second (IRQ 12). This module brings up the mouse — optionally
//! negotiating the IntelliMouse 4-byte wheel protocol — and decodes its 3- or
//! 4-byte movement packets into [`InputEvent`]s. The packet decoder is a pure
//! state machine so it can be unit-tested off-target.

use alloc::vec::Vec;

use crate::input::{InputEvent, MouseButton};

/// 8042 data port.
pub const PS2_DATA: u16 = 0x60;
/// 8042 status (read) / command (write) port.
pub const PS2_STATUS_CMD: u16 = 0x64;

// 8042 controller / mouse commands, used only by the x86_64 bring-up.
#[cfg(target_arch = "x86_64")]
const CMD_ENABLE_AUX: u8 = 0xA8;
#[cfg(target_arch = "x86_64")]
const CMD_READ_CONFIG: u8 = 0x20;
#[cfg(target_arch = "x86_64")]
const CMD_WRITE_CONFIG: u8 = 0x60;
#[cfg(target_arch = "x86_64")]
const CMD_WRITE_TO_AUX: u8 = 0xD4;
#[cfg(target_arch = "x86_64")]
const MOUSE_SET_SAMPLE_RATE: u8 = 0xF3;
#[cfg(target_arch = "x86_64")]
const MOUSE_GET_DEVICE_ID: u8 = 0xF2;
#[cfg(target_arch = "x86_64")]
const MOUSE_SET_DEFAULTS: u8 = 0xF6;
#[cfg(target_arch = "x86_64")]
const MOUSE_ENABLE_REPORTING: u8 = 0xF4;
#[cfg(target_arch = "x86_64")]
const MOUSE_ACK: u8 = 0xFA;

/// The mouse reporting protocol currently negotiated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseProtocol {
    /// Standard 3-byte packets, 3 buttons.
    Standard,
    /// IntelliMouse: 4-byte packets with a scroll wheel.
    Wheel,
    /// IntelliMouse Explorer: 4-byte packets, wheel + buttons 4/5.
    WheelFiveButtons,
}

impl MouseProtocol {
    /// Bytes per movement packet.
    pub fn packet_len(&self) -> usize {
        match self {
            MouseProtocol::Standard => 3,
            _ => 4,
        }
    }

    /// Derives the protocol from a `GET_DEVICE_ID` response.
    pub fn from_device_id(id: u8) -> Self {
        match id {
            3 => MouseProtocol::Wheel,
            4 => MouseProtocol::WheelFiveButtons,
            _ => MouseProtocol::Standard,
        }
    }
}

/// Decodes the byte stream from a PS/2 mouse into input events.
#[derive(Debug, Clone)]
pub struct Ps2MouseDecoder {
    protocol: MouseProtocol,
    packet: [u8; 4],
    index: usize,
    prev_buttons: u8,
}

impl Ps2MouseDecoder {
    pub const fn new(protocol: MouseProtocol) -> Self {
        Self {
            protocol,
            packet: [0; 4],
            index: 0,
            prev_buttons: 0,
        }
    }

    pub fn set_protocol(&mut self, protocol: MouseProtocol) {
        self.protocol = protocol;
        self.index = 0;
    }

    /// Feeds one byte. Returns the events for a completed packet, or an empty
    /// vector while a packet is still being assembled (or after a resync).
    pub fn feed(&mut self, byte: u8) -> Vec<InputEvent> {
        // The first byte of every packet has bit 3 set; anything else means we
        // are out of phase, so drop it and wait for a valid header.
        if self.index == 0 && byte & 0x08 == 0 {
            return Vec::new();
        }

        self.packet[self.index] = byte;
        self.index += 1;

        if self.index < self.protocol.packet_len() {
            return Vec::new();
        }
        self.index = 0;
        self.decode_packet()
    }

    fn decode_packet(&mut self) -> Vec<InputEvent> {
        let mut events = Vec::new();
        let p = self.packet;

        // ── buttons ────────────────────────────────────────────────────────
        let mut buttons = p[0] & 0x07; // L=1, R=2, M=4
        if self.protocol == MouseProtocol::WheelFiveButtons {
            buttons |= (p[3] >> 1) & 0x18; // bits 4/5 of p[3] -> bits 3/4
        }
        const BUTTON_MAP: [(u8, MouseButton); 5] = [
            (0x01, MouseButton::Left),
            (0x02, MouseButton::Right),
            (0x04, MouseButton::Middle),
            (0x08, MouseButton::Other(4)),
            (0x10, MouseButton::Other(5)),
        ];
        for (mask, btn) in BUTTON_MAP {
            let was = self.prev_buttons & mask != 0;
            let now = buttons & mask != 0;
            if now && !was {
                events.push(InputEvent::MouseButtonPress(btn));
            } else if was && !now {
                events.push(InputEvent::MouseButtonRelease(btn));
            }
        }
        self.prev_buttons = buttons;

        // ── motion ─────────────────────────────────────────────────────────
        // X/Y are 9-bit two's complement: byte value plus a sign bit in p[0].
        // The overflow bits (0x40 X, 0x80 Y) mean the delta didn't fit; treat
        // that as no motion rather than a wild jump.
        let dx = if p[0] & 0x40 != 0 {
            0
        } else {
            p[1] as i32 - if p[0] & 0x10 != 0 { 256 } else { 0 }
        };
        let dy_raw = if p[0] & 0x80 != 0 {
            0
        } else {
            p[2] as i32 - if p[0] & 0x20 != 0 { 256 } else { 0 }
        };
        // PS/2 reports +Y as "up"; screen coordinates grow downward.
        let dy = -dy_raw;
        if dx != 0 || dy != 0 {
            events.push(InputEvent::MouseMove { dx, dy });
        }

        // ── wheel (4-byte protocols) ──────────────────────────────────────
        if self.protocol != MouseProtocol::Standard {
            // Low nibble of p[3] is a 4-bit signed Z delta.
            let z = ((p[3] & 0x0F) as i8) << 4 >> 4;
            if z != 0 {
                events.push(InputEvent::Scroll {
                    delta_x: 0,
                    delta_y: -(z as i32),
                });
            }
        }

        events
    }
}

// ── Live 8042 bring-up ────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
mod hw {
    use super::*;
    use crate::port::{inb, outb};
    use crate::sync::SpinLock;

    /// Global decoder fed from the IRQ 12 handler.
    pub static MOUSE: SpinLock<Ps2MouseDecoder> =
        SpinLock::new(Ps2MouseDecoder::new(MouseProtocol::Standard));

    fn wait_write() {
        for _ in 0..100_000 {
            if unsafe { inb(PS2_STATUS_CMD) } & 0x02 == 0 {
                return;
            }
        }
    }

    fn wait_read() {
        for _ in 0..100_000 {
            if unsafe { inb(PS2_STATUS_CMD) } & 0x01 != 0 {
                return;
            }
        }
    }

    unsafe fn cmd(byte: u8) {
        wait_write();
        outb(PS2_STATUS_CMD, byte);
    }

    unsafe fn write_data(byte: u8) {
        wait_write();
        outb(PS2_DATA, byte);
    }

    unsafe fn read_data() -> u8 {
        wait_read();
        inb(PS2_DATA)
    }

    /// Sends a command to the mouse and returns whether it ACKed.
    unsafe fn mouse_cmd(byte: u8) -> bool {
        cmd(CMD_WRITE_TO_AUX);
        write_data(byte);
        read_data() == MOUSE_ACK
    }

    unsafe fn set_sample_rate(rate: u8) -> bool {
        mouse_cmd(MOUSE_SET_SAMPLE_RATE) && mouse_cmd(rate)
    }

    /// Performs the IntelliMouse / Explorer "knock" sequences and reads back the
    /// device id to learn which protocol the mouse supports.
    unsafe fn detect_protocol() -> MouseProtocol {
        // Wheel: 200, 100, 80.
        let _ = set_sample_rate(200);
        let _ = set_sample_rate(100);
        let _ = set_sample_rate(80);
        // 5 buttons: 200, 200, 80.
        let _ = set_sample_rate(200);
        let _ = set_sample_rate(200);
        let _ = set_sample_rate(80);

        let mut id = 0u8;
        if mouse_cmd(MOUSE_GET_DEVICE_ID) {
            id = read_data();
        }
        MouseProtocol::from_device_id(id)
    }

    /// Brings up the PS/2 mouse: enables the aux port, turns on the IRQ 12
    /// interrupt in the controller config byte, negotiates the wheel protocol
    /// and starts data reporting. Returns the negotiated protocol.
    ///
    /// # Safety
    /// Touches the 8042 I/O ports; call once during boot.
    pub unsafe fn init() -> MouseProtocol {
        cmd(CMD_ENABLE_AUX);

        // Config byte: set bit 1 (enable aux IRQ 12), clear bit 5 (aux clock).
        cmd(CMD_READ_CONFIG);
        let mut config = read_data();
        config |= 1 << 1;
        config &= !(1 << 5);
        cmd(CMD_WRITE_CONFIG);
        write_data(config);

        let _ = mouse_cmd(MOUSE_SET_DEFAULTS);
        let protocol = detect_protocol();
        // Re-assert a sane sample rate after the detection knocks.
        let _ = set_sample_rate(100);
        let _ = mouse_cmd(MOUSE_ENABLE_REPORTING);

        MOUSE.lock().set_protocol(protocol);
        protocol
    }

    /// Feeds one byte from the IRQ 12 handler into the decoder and pushes any
    /// resulting input events.
    pub fn handle_byte(byte: u8) {
        let events = MOUSE.lock().feed(byte);
        for ev in events {
            crate::input::push_event(ev);
        }
    }
}

#[cfg(target_arch = "x86_64")]
pub use hw::{handle_byte, init, MOUSE};

#[cfg(test)]
mod tests {
    use super::*;

    fn decoder(p: MouseProtocol) -> Ps2MouseDecoder {
        Ps2MouseDecoder::new(p)
    }

    #[test]
    fn standard_three_byte_move_and_buttons() {
        let mut d = decoder(MouseProtocol::Standard);
        // header: sync bit + left button; dx=+5, dy=+3 (screen: up -> -3).
        assert!(d.feed(0x09).is_empty());
        assert!(d.feed(5).is_empty());
        let ev = d.feed(3);
        assert!(ev.contains(&InputEvent::MouseButtonPress(MouseButton::Left)));
        assert!(ev.contains(&InputEvent::MouseMove { dx: 5, dy: -3 }));

        // Release the button, negative X (header sync + X sign bit 0x10),
        // dx = 250 - 256 = -6.
        assert!(d.feed(0x08 | 0x10).is_empty());
        assert!(d.feed(250).is_empty());
        let ev = d.feed(0);
        assert!(ev.contains(&InputEvent::MouseButtonRelease(MouseButton::Left)));
        assert!(ev.contains(&InputEvent::MouseMove { dx: -6, dy: 0 }));
    }

    #[test]
    fn overflow_bits_suppress_motion() {
        let mut d = decoder(MouseProtocol::Standard);
        d.feed(0x08 | 0x40 | 0x80); // both overflow bits
        d.feed(0xFF);
        let ev = d.feed(0xFF);
        assert!(!ev.iter().any(|e| matches!(e, InputEvent::MouseMove { .. })));
    }

    #[test]
    fn wheel_protocol_decodes_four_bytes_and_scroll() {
        let mut d = decoder(MouseProtocol::Wheel);
        // header sync only, no motion, wheel = -1 (0x0F -> -1) -> scroll +1.
        d.feed(0x08);
        d.feed(0);
        d.feed(0);
        let ev = d.feed(0x0F);
        assert_eq!(
            ev,
            alloc::vec![InputEvent::Scroll {
                delta_x: 0,
                delta_y: 1
            }]
        );

        // wheel = +1 -> scroll -1.
        d.feed(0x08);
        d.feed(0);
        d.feed(0);
        let ev = d.feed(0x01);
        assert_eq!(
            ev,
            alloc::vec![InputEvent::Scroll {
                delta_x: 0,
                delta_y: -1
            }]
        );
    }

    #[test]
    fn five_button_protocol_reports_buttons_four_and_five() {
        let mut d = decoder(MouseProtocol::WheelFiveButtons);
        // p[3] bit 4 set -> button 4 pressed.
        d.feed(0x08);
        d.feed(0);
        d.feed(0);
        let ev = d.feed(0x10);
        assert!(ev.contains(&InputEvent::MouseButtonPress(MouseButton::Other(4))));

        d.feed(0x08);
        d.feed(0);
        d.feed(0);
        let ev = d.feed(0x20); // release 4, press 5
        assert!(ev.contains(&InputEvent::MouseButtonRelease(MouseButton::Other(4))));
        assert!(ev.contains(&InputEvent::MouseButtonPress(MouseButton::Other(5))));
    }

    #[test]
    fn desync_byte_is_dropped_until_a_valid_header() {
        let mut d = decoder(MouseProtocol::Standard);
        // A stray byte without bit 3 is ignored.
        assert!(d.feed(0x00).is_empty());
        assert!(d.feed(0x04).is_empty());
        // Now a real packet lands cleanly.
        d.feed(0x08);
        d.feed(2);
        let ev = d.feed(0);
        assert_eq!(ev, alloc::vec![InputEvent::MouseMove { dx: 2, dy: 0 }]);
    }

    #[test]
    fn protocol_from_device_id() {
        assert_eq!(MouseProtocol::from_device_id(0), MouseProtocol::Standard);
        assert_eq!(MouseProtocol::from_device_id(3), MouseProtocol::Wheel);
        assert_eq!(
            MouseProtocol::from_device_id(4),
            MouseProtocol::WheelFiveButtons
        );
        assert_eq!(MouseProtocol::Standard.packet_len(), 3);
        assert_eq!(MouseProtocol::Wheel.packet_len(), 4);
    }
}
