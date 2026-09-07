//! USB HID Mouse and Tablet Decoder.
//!
//! Handles both relative Boot Mouse reports and absolute USB Tablet reports,
//! emitting `InputEvent::MouseMove`, `InputEvent::MouseAbsolute`,
//! `InputEvent::MouseButtonPress`, and `InputEvent::MouseButtonRelease`.

use alloc::vec::Vec;
use crate::input::{InputEvent, MouseButton};

/// Tracks mouse buttons and coordinates.
#[derive(Debug, Clone, Default)]
pub struct UsbHidMouse {
    pub prev_buttons: u8,
    pub is_absolute: bool,
}

impl UsbHidMouse {
    pub const fn new() -> Self {
        Self {
            prev_buttons: 0,
            is_absolute: false,
        }
    }

    /// Processes a raw mouse or tablet report.
    pub fn process_report(&mut self, report: &[u8]) -> Vec<InputEvent> {
        let mut events = Vec::new();
        if report.is_empty() {
            return events;
        }

        let buttons = report[0];

        // Process button transitions
        const BUTTON_MAP: [(u8, MouseButton); 3] = [
            (0x01, MouseButton::Left),
            (0x02, MouseButton::Right),
            (0x04, MouseButton::Middle),
        ];

        for (mask, btn) in BUTTON_MAP {
            let was_pressed = (self.prev_buttons & mask) != 0;
            let is_pressed = (buttons & mask) != 0;
            if !was_pressed && is_pressed {
                events.push(InputEvent::MouseButtonPress(btn));
            } else if was_pressed && !is_pressed {
                events.push(InputEvent::MouseButtonRelease(btn));
            }
        }
        self.prev_buttons = buttons;

        // Check if report format is absolute tablet (typically >= 5 bytes with 16-bit coordinates)
        if report.len() >= 5 && self.is_absolute {
            let raw_x = u16::from_le_bytes([report[1], report[2]]) as u32;
            let raw_y = u16::from_le_bytes([report[3], report[4]]) as u32;

            // Map 0..32767 coordinate space to virtual screen or emit raw absolute
            events.push(InputEvent::MouseAbsolute {
                x: raw_x,
                y: raw_y,
            });
        } else if report.len() >= 3 {
            // Relative mouse: byte 1 = dx, byte 2 = dy
            let dx = report[1] as i8 as i32;
            let dy = report[2] as i8 as i32;

            if dx != 0 || dy != 0 {
                events.push(InputEvent::MouseMove { dx, dy });
            }

            if report.len() >= 4 {
                let wheel = report[3] as i8 as i32;
                if wheel != 0 {
                    events.push(InputEvent::Scroll {
                        delta_x: 0,
                        delta_y: wheel,
                    });
                }
            }
        }

        events
    }
}
