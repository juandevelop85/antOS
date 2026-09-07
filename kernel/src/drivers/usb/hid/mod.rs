//! USB Human Interface Device (HID) Class Drivers.
//!
//! Provides decoders and state trackers for USB Boot Keyboards,
//! Boot Mice, and absolute USB Tablets.

pub mod keyboard;
pub mod mouse;

pub use keyboard::{UsbHidKeyboard, hid_usage_to_key};
pub use mouse::UsbHidMouse;
