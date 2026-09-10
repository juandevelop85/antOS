//! USB HID Mouse and Tablet Decoder.
//!
//! Handles both relative Boot Mouse reports and absolute USB Tablet reports,
//! emitting `InputEvent::MouseMove`, `InputEvent::MouseAbsolute`,
//! `InputEvent::MouseButtonPress`, and `InputEvent::MouseButtonRelease`.

use crate::input::{InputEvent, MouseButton};
use alloc::vec::Vec;

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

        // Check if report format is absolute tablet
        if self.is_absolute || report.len() >= 8 {
            if report.len() >= 8 {
                // VirtualBox / Standard 8-byte USB Tablet report:
                // [buttons, wheel, pan, padding, x_low, x_high, y_low, y_high]
                let raw_x = u16::from_le_bytes([report[4], report[5]]) as u32;
                let raw_y = u16::from_le_bytes([report[6], report[7]]) as u32;

                events.push(InputEvent::MouseAbsolute { x: raw_x, y: raw_y });

                let wheel = report[1] as i8 as i32;
                let pan = report[2] as i8 as i32;
                if wheel != 0 || pan != 0 {
                    events.push(InputEvent::Scroll {
                        delta_x: pan,
                        delta_y: wheel,
                    });
                }
            } else if report.len() >= 5 {
                // Compact 5-byte USB Tablet report without wheels/padding:
                // [buttons, x_low, x_high, y_low, y_high]
                let raw_x = u16::from_le_bytes([report[1], report[2]]) as u32;
                let raw_y = u16::from_le_bytes([report[3], report[4]]) as u32;

                events.push(InputEvent::MouseAbsolute { x: raw_x, y: raw_y });
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_vbox_tablet_8byte_report() {
        let mut mouse = UsbHidMouse::new();
        mouse.is_absolute = true;

        // Middle of screen horizontally (0x4000 = 16384), lower-right vertically (0x6000 = 24576)
        let report = [0x01, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x60];
        let events = mouse.process_report(&report);

        assert_eq!(events.len(), 2);
        assert_eq!(events[0], InputEvent::MouseButtonPress(MouseButton::Left));
        assert_eq!(events[1], InputEvent::MouseAbsolute { x: 16384, y: 24576 });

        // Release button, move to origin
        let report2 = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let events2 = mouse.process_report(&report2);

        assert_eq!(events2.len(), 2);
        assert_eq!(
            events2[0],
            InputEvent::MouseButtonRelease(MouseButton::Left)
        );
        assert_eq!(events2[1], InputEvent::MouseAbsolute { x: 0, y: 0 });
    }

    #[test_case]
    fn test_relative_mouse_report() {
        let mut mouse = UsbHidMouse::new();
        mouse.is_absolute = false;

        let report = [0x02, 10u8, (-5i8) as u8, 1u8];
        let events = mouse.process_report(&report);

        assert_eq!(events.len(), 3);
        assert_eq!(events[0], InputEvent::MouseButtonPress(MouseButton::Right));
        assert_eq!(events[1], InputEvent::MouseMove { dx: 10, dy: -5 });
        assert_eq!(
            events[2],
            InputEvent::Scroll {
                delta_x: 0,
                delta_y: 1
            }
        );
    }
}
