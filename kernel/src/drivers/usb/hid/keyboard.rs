//! USB HID Boot Keyboard Decoder and State Tracker.
//!
//! Parses 8-byte USB HID Boot Keyboard reports conforming to the
//! Device Class Definition for HID 1.11, generating typed `InputEvent`s.

use crate::input::{InputEvent, KeyCode};
use alloc::vec::Vec;

/// Maps standard USB HID Usage IDs (Usage Page 0x07) to `KeyCode`.
pub fn hid_usage_to_key(usage: u8) -> KeyCode {
    match usage {
        // Letters (0x04 - 0x1D)
        0x04 => KeyCode::KeyA,
        0x05 => KeyCode::KeyB,
        0x06 => KeyCode::KeyC,
        0x07 => KeyCode::KeyD,
        0x08 => KeyCode::KeyE,
        0x09 => KeyCode::KeyF,
        0x0A => KeyCode::KeyG,
        0x0B => KeyCode::KeyH,
        0x0C => KeyCode::KeyI,
        0x0D => KeyCode::KeyJ,
        0x0E => KeyCode::KeyK,
        0x0F => KeyCode::KeyL,
        0x10 => KeyCode::KeyM,
        0x11 => KeyCode::KeyN,
        0x12 => KeyCode::KeyO,
        0x13 => KeyCode::KeyP,
        0x14 => KeyCode::KeyQ,
        0x15 => KeyCode::KeyR,
        0x16 => KeyCode::KeyS,
        0x17 => KeyCode::KeyT,
        0x18 => KeyCode::KeyU,
        0x19 => KeyCode::KeyV,
        0x1A => KeyCode::KeyW,
        0x1B => KeyCode::KeyX,
        0x1C => KeyCode::KeyY,
        0x1D => KeyCode::KeyZ,

        // Numbers 1-9, 0 (0x1E - 0x27)
        0x1E => KeyCode::Num1,
        0x1F => KeyCode::Num2,
        0x20 => KeyCode::Num3,
        0x21 => KeyCode::Num4,
        0x22 => KeyCode::Num5,
        0x23 => KeyCode::Num6,
        0x24 => KeyCode::Num7,
        0x25 => KeyCode::Num8,
        0x26 => KeyCode::Num9,
        0x27 => KeyCode::Num0,

        // Control & Editing
        0x28 => KeyCode::Enter,
        0x29 => KeyCode::Escape,
        0x2A => KeyCode::Backspace,
        0x2B => KeyCode::Tab,
        0x2C => KeyCode::Space,
        0x2D => KeyCode::Minus,
        0x2E => KeyCode::Equal,
        0x2F => KeyCode::LeftBracket,
        0x30 => KeyCode::RightBracket,
        0x31 => KeyCode::Backslash,
        0x33 => KeyCode::Semicolon,
        0x34 => KeyCode::Apostrophe,
        0x35 => KeyCode::Grave,
        0x36 => KeyCode::Comma,
        0x37 => KeyCode::Dot,
        0x38 => KeyCode::Slash,
        0x39 => KeyCode::CapsLock,
        0x47 => KeyCode::ScrollLock,
        0x53 => KeyCode::NumLock,

        // Function Keys F1 - F12 (0x3A - 0x45)
        0x3A => KeyCode::F1,
        0x3B => KeyCode::F2,
        0x3C => KeyCode::F3,
        0x3D => KeyCode::F4,
        0x3E => KeyCode::F5,
        0x3F => KeyCode::F6,
        0x40 => KeyCode::F7,
        0x41 => KeyCode::F8,
        0x42 => KeyCode::F9,
        0x43 => KeyCode::F10,
        0x44 => KeyCode::F11,
        0x45 => KeyCode::F12,

        // Editing & Navigation (0x49 - 0x52)
        0x49 => KeyCode::Insert,
        0x4A => KeyCode::Home,
        0x4B => KeyCode::PageUp,
        0x4C => KeyCode::Delete,
        0x4D => KeyCode::End,
        0x4E => KeyCode::PageDown,
        0x4F => KeyCode::Right,
        0x50 => KeyCode::Left,
        0x51 => KeyCode::Down,
        0x52 => KeyCode::Up,

        // Modifier usages (0xE0 - 0xE7). Boot reports carry these in the
        // modifier bitmap byte, but a Report-Protocol keyboard declares them as
        // ordinary Usage-Page-0x07 variable fields, so the generic decoder maps
        // them here too.
        0xE0 => KeyCode::LeftCtrl,
        0xE1 => KeyCode::LeftShift,
        0xE2 => KeyCode::LeftAlt,
        0xE3 => KeyCode::LeftSuper,
        0xE4 => KeyCode::RightCtrl,
        0xE5 => KeyCode::RightShift,
        0xE6 => KeyCode::RightAlt,
        0xE7 => KeyCode::RightSuper,

        other => KeyCode::Unknown(other as u16),
    }
}

/// Tracks previous report to synthesize KeyPress and KeyRelease events.
#[derive(Debug, Clone, Default)]
pub struct UsbHidKeyboard {
    pub prev_report: [u8; 8],
}

impl UsbHidKeyboard {
    pub const fn new() -> Self {
        Self {
            prev_report: [0u8; 8],
        }
    }

    /// Compares `new_report` with `self.prev_report` and emits all KeyPress and KeyRelease events.
    pub fn process_report(&mut self, new_report: &[u8; 8]) -> Vec<InputEvent> {
        let mut events = Vec::new();

        let old_mods = self.prev_report[0];
        let new_mods = new_report[0];

        // Process modifiers: bits 0..7
        const MOD_MAP: [(u8, KeyCode); 8] = [
            (0x01, KeyCode::LeftCtrl),
            (0x02, KeyCode::LeftShift),
            (0x04, KeyCode::LeftAlt),
            (0x08, KeyCode::LeftSuper),
            (0x10, KeyCode::RightCtrl),
            (0x20, KeyCode::RightShift),
            (0x40, KeyCode::RightAlt),
            (0x80, KeyCode::RightSuper),
        ];

        for (mask, key) in MOD_MAP {
            let was_pressed = (old_mods & mask) != 0;
            let is_pressed = (new_mods & mask) != 0;
            if !was_pressed && is_pressed {
                events.push(InputEvent::KeyPress(key));
            } else if was_pressed && !is_pressed {
                events.push(InputEvent::KeyRelease(key));
            }
        }

        // Process key usage codes: bytes 2..8
        let old_keys = &self.prev_report[2..8];
        let new_keys = &new_report[2..8];

        // Released keys: present in old_keys but absent from new_keys
        for &k in old_keys {
            if k != 0 && !new_keys.contains(&k) {
                let code = hid_usage_to_key(k);
                events.push(InputEvent::KeyRelease(code));
            }
        }

        // Pressed keys: present in new_keys but absent from old_keys
        for &k in new_keys {
            if k != 0 && !old_keys.contains(&k) {
                let code = hid_usage_to_key(k);
                events.push(InputEvent::KeyPress(code));
            }
        }

        self.prev_report = *new_report;
        events
    }
}
