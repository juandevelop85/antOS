//! USB HID Report Descriptor parser and a generic, usage-driven report decoder.
//!
//! The boot decoders in [`super::keyboard`] / [`super::mouse`] assume the fixed
//! 8-byte Boot Protocol layout. A device left in Report Protocol (NKRO
//! keyboards, mice with Report IDs, digitizers, gamepads) emits whatever its
//! Report Descriptor dictates, which those decoders misread. This module parses
//! the descriptor into [`ReportField`]s and decodes arbitrary Input reports into
//! [`InputEvent`]s without per-model code.

use alloc::vec::Vec;

use crate::drivers::usb::hid::keyboard::hid_usage_to_key;
use crate::input::{InputEvent, KeyCode, MouseButton};

// ── Short-item type / tag encoding (HID 1.11 §6.2.2) ────────────────────────

const ITEM_TYPE_MAIN: u8 = 0;
const ITEM_TYPE_GLOBAL: u8 = 1;
const ITEM_TYPE_LOCAL: u8 = 2;

const MAIN_INPUT: u8 = 0x8;
const MAIN_OUTPUT: u8 = 0x9;
const MAIN_FEATURE: u8 = 0xB;
const MAIN_COLLECTION: u8 = 0xA;
const MAIN_END_COLLECTION: u8 = 0xC;

const GLOBAL_USAGE_PAGE: u8 = 0x0;
const GLOBAL_LOGICAL_MIN: u8 = 0x1;
const GLOBAL_LOGICAL_MAX: u8 = 0x2;
const GLOBAL_PHYSICAL_MIN: u8 = 0x3;
const GLOBAL_PHYSICAL_MAX: u8 = 0x4;
const GLOBAL_UNIT_EXPONENT: u8 = 0x5;
const GLOBAL_UNIT: u8 = 0x6;
const GLOBAL_REPORT_SIZE: u8 = 0x7;
const GLOBAL_REPORT_ID: u8 = 0x8;
const GLOBAL_REPORT_COUNT: u8 = 0x9;
const GLOBAL_PUSH: u8 = 0xA;
const GLOBAL_POP: u8 = 0xB;

const LOCAL_USAGE: u8 = 0x0;
const LOCAL_USAGE_MIN: u8 = 0x1;
const LOCAL_USAGE_MAX: u8 = 0x2;

// ── Usage pages / IDs we act on ─────────────────────────────────────────────

pub const PAGE_GENERIC_DESKTOP: u16 = 0x01;
pub const PAGE_KEYBOARD: u16 = 0x07;
pub const PAGE_BUTTON: u16 = 0x09;
pub const PAGE_CONSUMER: u16 = 0x0C;

pub const USAGE_POINTER: u16 = 0x01;
pub const USAGE_MOUSE: u16 = 0x02;
pub const USAGE_JOYSTICK: u16 = 0x04;
pub const USAGE_GAMEPAD: u16 = 0x05;
pub const USAGE_KEYBOARD: u16 = 0x06;
pub const USAGE_X: u16 = 0x30;
pub const USAGE_Y: u16 = 0x31;
pub const USAGE_Z: u16 = 0x32;
pub const USAGE_WHEEL: u16 = 0x38;
pub const USAGE_AC_PAN: u16 = 0x0238;
pub const USAGE_CONSUMER_CONTROL: u16 = 0x01;

/// Main-item data flags shared by Input / Output / Feature (HID 1.11 §6.2.2.5).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FieldFlags {
    pub constant: bool,
    pub variable: bool,
    pub relative: bool,
    pub wrap: bool,
    pub null_state: bool,
}

impl FieldFlags {
    fn from_bits(v: u32) -> Self {
        Self {
            constant: v & (1 << 0) != 0,
            variable: v & (1 << 1) != 0,
            relative: v & (1 << 2) != 0,
            wrap: v & (1 << 3) != 0,
            null_state: v & (1 << 6) != 0,
        }
    }
}

/// Which report class a field belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainKind {
    Input,
    Output,
    Feature,
}

/// One Main item's worth of the report layout: `report_count` consecutive
/// fields of `bit_size` bits starting at `bit_offset` within the report payload
/// (the byte after the Report ID prefix, when the descriptor uses one).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportField {
    pub report_id: u8,
    pub kind: MainKind,
    pub bit_offset: usize,
    pub bit_size: usize,
    pub report_count: usize,
    pub usage_page: u16,
    /// Explicit usages listed before this Main item (may be shorter than
    /// `report_count`; the last entry repeats for the remaining fields).
    pub usages: Vec<u32>,
    pub usage_min: u32,
    pub usage_max: u32,
    pub logical_min: i32,
    pub logical_max: i32,
    pub flags: FieldFlags,
}

impl ReportField {
    /// Effective `(usage_page, usage_id)` for the `index`-th field of this item.
    pub fn usage_at(&self, index: usize) -> (u16, u16) {
        // Extended (32-bit) usages carry their own page in the high word.
        let split = |u: u32| -> (u16, u16) {
            if u > 0xFFFF {
                ((u >> 16) as u16, u as u16)
            } else {
                (self.usage_page, u as u16)
            }
        };
        if self.usage_min != 0 || self.usage_max != 0 {
            let u = self.usage_min + index as u32;
            let capped = if self.usage_max != 0 {
                u.min(self.usage_max)
            } else {
                u
            };
            return split(capped);
        }
        if self.usages.is_empty() {
            return (self.usage_page, 0);
        }
        let i = index.min(self.usages.len() - 1);
        split(self.usages[i])
    }
}

#[derive(Clone, Copy)]
struct GlobalItems {
    usage_page: u16,
    logical_min: i32,
    logical_max: i32,
    report_size: usize,
    report_id: u8,
    report_count: usize,
}

impl GlobalItems {
    const fn new() -> Self {
        Self {
            usage_page: 0,
            logical_min: 0,
            logical_max: 0,
            report_size: 0,
            report_id: 0,
            report_count: 0,
        }
    }
}

/// Reads a short item's data field as an unsigned value.
fn item_data_unsigned(bytes: &[u8]) -> u32 {
    let mut v = 0u32;
    for (i, &b) in bytes.iter().take(4).enumerate() {
        v |= (b as u32) << (i * 8);
    }
    v
}

/// Reads a short item's data field, sign-extended from its byte width.
fn item_data_signed(bytes: &[u8]) -> i32 {
    let v = item_data_unsigned(bytes);
    match bytes.len() {
        1 => v as u8 as i8 as i32,
        2 => v as u16 as i16 as i32,
        _ => v as i32,
    }
}

/// Parses a raw HID Report Descriptor into a flat list of [`ReportField`]s in
/// declaration order. Malformed or truncated input yields whatever parsed
/// cleanly up to that point.
pub fn parse_report_descriptor(data: &[u8]) -> Vec<ReportField> {
    let mut fields = Vec::new();

    let mut global = GlobalItems::new();
    let mut global_stack: Vec<GlobalItems> = Vec::new();

    let mut usages: Vec<u32> = Vec::new();
    let mut usage_min: u32 = 0;
    let mut usage_max: u32 = 0;

    // Running bit offset per (report_id, kind).
    let mut offsets: Vec<((u8, u8), usize)> = Vec::new();
    let offset_key = |rid: u8, kind: u8| (rid, kind);

    let mut i = 0usize;
    while i < data.len() {
        let prefix = data[i];
        i += 1;

        // Long items: prefix 0xFE, [bDataSize][bLongItemTag][data...].
        if prefix == 0xFE {
            if i + 1 >= data.len() {
                break;
            }
            let size = data[i] as usize;
            i += 2 + size;
            continue;
        }

        let b_size = match prefix & 0x03 {
            0 => 0,
            1 => 1,
            2 => 2,
            _ => 4,
        };
        let b_type = (prefix >> 2) & 0x03;
        let b_tag = (prefix >> 4) & 0x0F;

        if i + b_size > data.len() {
            break;
        }
        let item = &data[i..i + b_size];
        i += b_size;

        match b_type {
            ITEM_TYPE_GLOBAL => match b_tag {
                GLOBAL_USAGE_PAGE => global.usage_page = item_data_unsigned(item) as u16,
                GLOBAL_LOGICAL_MIN => global.logical_min = item_data_signed(item),
                GLOBAL_LOGICAL_MAX => global.logical_max = item_data_signed(item),
                GLOBAL_PHYSICAL_MIN | GLOBAL_PHYSICAL_MAX | GLOBAL_UNIT | GLOBAL_UNIT_EXPONENT => {}
                GLOBAL_REPORT_SIZE => global.report_size = item_data_unsigned(item) as usize,
                GLOBAL_REPORT_ID => global.report_id = item_data_unsigned(item) as u8,
                GLOBAL_REPORT_COUNT => global.report_count = item_data_unsigned(item) as usize,
                GLOBAL_PUSH => global_stack.push(global),
                GLOBAL_POP => {
                    if let Some(g) = global_stack.pop() {
                        global = g;
                    }
                }
                _ => {}
            },
            ITEM_TYPE_LOCAL => match b_tag {
                LOCAL_USAGE => usages.push(item_data_unsigned(item)),
                LOCAL_USAGE_MIN => usage_min = item_data_unsigned(item),
                LOCAL_USAGE_MAX => usage_max = item_data_unsigned(item),
                _ => {}
            },
            ITEM_TYPE_MAIN => {
                match b_tag {
                    MAIN_INPUT | MAIN_OUTPUT | MAIN_FEATURE => {
                        let (kind, kcode) = match b_tag {
                            MAIN_INPUT => (MainKind::Input, 0u8),
                            MAIN_OUTPUT => (MainKind::Output, 1u8),
                            _ => (MainKind::Feature, 2u8),
                        };
                        let flags = FieldFlags::from_bits(item_data_unsigned(item));
                        let total_bits = global.report_size * global.report_count;

                        let key = offset_key(global.report_id, kcode);
                        let slot = offsets.iter_mut().find(|(k, _)| *k == key);
                        let bit_offset = match slot {
                            Some((_, o)) => {
                                let cur = *o;
                                *o += total_bits;
                                cur
                            }
                            None => {
                                offsets.push((key, total_bits));
                                0
                            }
                        };

                        fields.push(ReportField {
                            report_id: global.report_id,
                            kind,
                            bit_offset,
                            bit_size: global.report_size,
                            report_count: global.report_count,
                            usage_page: global.usage_page,
                            usages: usages.clone(),
                            usage_min,
                            usage_max,
                            logical_min: global.logical_min,
                            logical_max: global.logical_max,
                            flags,
                        });
                    }
                    MAIN_COLLECTION | MAIN_END_COLLECTION => {}
                    _ => {}
                }
                // Local items are cleared after every Main item.
                usages.clear();
                usage_min = 0;
                usage_max = 0;
            }
            _ => {}
        }
    }

    fields
}

/// Scans a Report Descriptor for the usage attached to the first
/// `Collection (Application)` item — this is where a keyboard declares
/// `Generic Desktop / Keyboard` and a mouse `Generic Desktop / Mouse`, and the
/// parser above drops it because it precedes (and is cleared by) the Collection
/// Main item. Returns `(usage_page, usage_id)`.
pub fn application_collection_usage(data: &[u8]) -> Option<(u16, u16)> {
    let mut usage_page = 0u16;
    let mut last_usage: Option<u32> = None;

    let mut i = 0usize;
    while i < data.len() {
        let prefix = data[i];
        i += 1;
        if prefix == 0xFE {
            if i >= data.len() {
                break;
            }
            i += 2 + data[i] as usize;
            continue;
        }
        let b_size = match prefix & 0x03 {
            0 => 0,
            1 => 1,
            2 => 2,
            _ => 4,
        };
        let b_type = (prefix >> 2) & 0x03;
        let b_tag = (prefix >> 4) & 0x0F;
        if i + b_size > data.len() {
            break;
        }
        let val = item_data_unsigned(&data[i..i + b_size]);
        i += b_size;

        match (b_type, b_tag) {
            (ITEM_TYPE_GLOBAL, GLOBAL_USAGE_PAGE) => usage_page = val as u16,
            (ITEM_TYPE_LOCAL, LOCAL_USAGE) => last_usage = Some(val),
            (ITEM_TYPE_MAIN, MAIN_COLLECTION) => {
                // Collection data 0x01 == Application.
                if val == 0x01 {
                    let u = last_usage.unwrap_or(0);
                    let (page, id) = if u > 0xFFFF {
                        ((u >> 16) as u16, u as u16)
                    } else {
                        (usage_page, u as u16)
                    };
                    return Some((page, id));
                }
                last_usage = None;
            }
            (ITEM_TYPE_MAIN, _) => last_usage = None,
            _ => {}
        }
    }
    None
}

/// Device role inferred from the top-level Application collection usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidRole {
    Keyboard,
    Mouse,
    Other,
}

/// A parsed HID device: its Input field layout plus the state needed to diff
/// successive reports into press/release events.
#[derive(Debug, Clone)]
pub struct HidDevice {
    fields: Vec<ReportField>,
    uses_report_id: bool,
    pub role: HidRole,
    prev_keys: Vec<u16>,
    prev_buttons: u32,
}

impl HidDevice {
    /// Builds a device model from a raw Report Descriptor.
    pub fn from_descriptor(data: &[u8]) -> Self {
        let fields = parse_report_descriptor(data);
        let uses_report_id = fields.iter().any(|f| f.report_id != 0);
        // The Application-collection usage is the authoritative role signal;
        // fall back to inspecting the fields when a device omits it.
        let role = match application_collection_usage(data) {
            Some((PAGE_GENERIC_DESKTOP, USAGE_KEYBOARD)) => HidRole::Keyboard,
            Some((PAGE_GENERIC_DESKTOP, USAGE_MOUSE))
            | Some((PAGE_GENERIC_DESKTOP, USAGE_POINTER)) => HidRole::Mouse,
            _ => classify(&fields),
        };
        Self {
            fields,
            uses_report_id,
            role,
            prev_keys: Vec::new(),
            prev_buttons: 0,
        }
    }

    /// `true` when the parser found no usable Input fields (fall back to Boot).
    pub fn is_empty(&self) -> bool {
        !self.fields.iter().any(|f| f.kind == MainKind::Input)
    }

    pub fn uses_report_id(&self) -> bool {
        self.uses_report_id
    }

    /// Total Input payload length in bytes for `report_id` (excluding the ID
    /// prefix), rounded up — used to size `GET_REPORT` requests.
    pub fn input_len_bytes(&self, report_id: u8) -> usize {
        let bits = self
            .fields
            .iter()
            .filter(|f| f.kind == MainKind::Input && f.report_id == report_id)
            .map(|f| f.bit_offset + f.bit_size * f.report_count)
            .max()
            .unwrap_or(0);
        bits.div_ceil(8)
    }

    /// Decodes one raw Input report into typed events, updating internal diff
    /// state. Accepts the report exactly as delivered on the interrupt endpoint
    /// (with the Report ID prefix byte when the descriptor uses one).
    pub fn decode(&mut self, report: &[u8]) -> Vec<InputEvent> {
        let mut events = Vec::new();
        if report.is_empty() {
            return events;
        }

        let (report_id, payload) = if self.uses_report_id {
            (report[0], &report[1..])
        } else {
            (0u8, report)
        };

        let mut dx = 0i32;
        let mut dy = 0i32;
        let mut wheel = 0i32;
        let mut pan = 0i32;
        let mut abs_x: Option<u32> = None;
        let mut abs_y: Option<u32> = None;
        let mut buttons = 0u32;
        let mut have_buttons = false;
        let mut cur_keys: Vec<u16> = Vec::new();
        let mut have_keys = false;

        for field in self
            .fields
            .iter()
            .filter(|f| f.kind == MainKind::Input && f.report_id == report_id)
        {
            if field.flags.constant {
                continue;
            }

            let signed = field.logical_min < 0;

            if field.flags.variable {
                for idx in 0..field.report_count {
                    let bit = field.bit_offset + idx * field.bit_size;
                    let raw = extract_bits(payload, bit, field.bit_size);
                    let (page, usage) = field.usage_at(idx);

                    match page {
                        PAGE_BUTTON => {
                            have_buttons = true;
                            if (1..=32).contains(&usage) && raw != 0 {
                                buttons |= 1 << (usage - 1);
                            }
                        }
                        PAGE_KEYBOARD => {
                            have_keys = true;
                            if raw != 0 && usage != 0 {
                                cur_keys.push(usage);
                            }
                        }
                        PAGE_GENERIC_DESKTOP => {
                            let val = if signed {
                                sign_extend(raw, field.bit_size)
                            } else {
                                raw as i32
                            };
                            match usage {
                                USAGE_X if field.flags.relative => dx += val,
                                USAGE_Y if field.flags.relative => dy += val,
                                USAGE_WHEEL if field.flags.relative => wheel += val,
                                USAGE_X => abs_x = Some(normalize_abs(raw, field.logical_max)),
                                USAGE_Y => abs_y = Some(normalize_abs(raw, field.logical_max)),
                                USAGE_Z | USAGE_WHEEL => wheel += val,
                                _ => {}
                            }
                        }
                        PAGE_CONSUMER => {
                            let val = if signed {
                                sign_extend(raw, field.bit_size)
                            } else {
                                raw as i32
                            };
                            if usage == USAGE_AC_PAN {
                                pan += val;
                            }
                        }
                        _ => {}
                    }
                }
            } else {
                // Array field: each slot holds a usage *index* selecting one
                // usage from the [usage_min, usage_max] range (keyboards) or the
                // explicit usage list.
                have_keys = have_keys || field.usage_page == PAGE_KEYBOARD;
                for idx in 0..field.report_count {
                    let bit = field.bit_offset + idx * field.bit_size;
                    let code = extract_bits(payload, bit, field.bit_size);
                    if code == 0 {
                        continue;
                    }
                    let (page, usage) = if field.usage_min != 0 || field.usage_max != 0 {
                        (field.usage_page, code as u16)
                    } else {
                        field.usage_at(code as usize)
                    };
                    if page == PAGE_KEYBOARD {
                        cur_keys.push(usage);
                    }
                }
            }
        }

        // Keyboard: diff the active usage set against the previous report.
        if have_keys {
            for &k in &self.prev_keys {
                if !cur_keys.contains(&k) {
                    events.push(InputEvent::KeyRelease(key_from_usage(k)));
                }
            }
            for &k in &cur_keys {
                if !self.prev_keys.contains(&k) {
                    events.push(InputEvent::KeyPress(key_from_usage(k)));
                }
            }
            self.prev_keys = cur_keys;
        }

        // Buttons: diff the bitmask.
        if have_buttons {
            let changed = buttons ^ self.prev_buttons;
            for b in 0..8u32 {
                let mask = 1 << b;
                if changed & mask != 0 {
                    let btn = button_from_index(b as u8);
                    if buttons & mask != 0 {
                        events.push(InputEvent::MouseButtonPress(btn));
                    } else {
                        events.push(InputEvent::MouseButtonRelease(btn));
                    }
                }
            }
            self.prev_buttons = buttons;
        }

        if dx != 0 || dy != 0 {
            events.push(InputEvent::MouseMove { dx, dy });
        }
        if let (Some(x), Some(y)) = (abs_x, abs_y) {
            events.push(InputEvent::MouseAbsolute { x, y });
        } else if let Some(x) = abs_x {
            events.push(InputEvent::MouseAbsolute { x, y: 0 });
        } else if let Some(y) = abs_y {
            events.push(InputEvent::MouseAbsolute { x: 0, y });
        }
        if wheel != 0 || pan != 0 {
            events.push(InputEvent::Scroll {
                delta_x: pan,
                delta_y: wheel,
            });
        }

        events
    }
}

/// Rescales a raw absolute axis reading onto the canonical `0..32767` range the
/// cursor layer expects, using the descriptor's real Logical Maximum instead of
/// a hardcoded `0x7FFF`.
fn normalize_abs(raw: u32, logical_max: i32) -> u32 {
    if logical_max <= 0 || logical_max == 32767 {
        return raw;
    }
    ((raw as u64 * 32767) / logical_max as u64) as u32
}

fn classify(fields: &[ReportField]) -> HidRole {
    // Prefer the first Generic-Desktop top-level usage seen.
    for f in fields {
        if f.usage_page == PAGE_GENERIC_DESKTOP {
            for &u in &f.usages {
                match u as u16 {
                    USAGE_KEYBOARD => return HidRole::Keyboard,
                    USAGE_MOUSE | USAGE_POINTER => return HidRole::Mouse,
                    _ => {}
                }
            }
        }
    }
    // Otherwise infer from what the fields actually carry.
    let has_keys = fields.iter().any(|f| f.usage_page == PAGE_KEYBOARD);
    let has_axes = fields.iter().any(|f| {
        f.usage_page == PAGE_GENERIC_DESKTOP
            && f.usages.iter().any(|&u| {
                let u = u as u16;
                u == USAGE_X || u == USAGE_Y
            })
    });
    if has_keys && !has_axes {
        HidRole::Keyboard
    } else if has_axes {
        HidRole::Mouse
    } else {
        HidRole::Other
    }
}

fn key_from_usage(usage: u16) -> KeyCode {
    hid_usage_to_key(usage as u8)
}

fn button_from_index(index: u8) -> MouseButton {
    match index {
        0 => MouseButton::Left,
        1 => MouseButton::Right,
        2 => MouseButton::Middle,
        n => MouseButton::Other(n + 1),
    }
}

/// Extracts `bit_size` bits (little-endian bit order) starting at `bit_offset`.
fn extract_bits(data: &[u8], bit_offset: usize, bit_size: usize) -> u32 {
    let mut val = 0u32;
    let n = bit_size.min(32);
    for i in 0..n {
        let bit = bit_offset + i;
        let byte = bit / 8;
        let shift = bit % 8;
        if byte < data.len() && (data[byte] >> shift) & 1 != 0 {
            val |= 1 << i;
        }
    }
    val
}

/// Sign-extends an `bits`-wide value held in the low bits of `val`.
fn sign_extend(val: u32, bits: usize) -> i32 {
    if bits == 0 || bits >= 32 {
        return val as i32;
    }
    let shift = 32 - bits;
    ((val << shift) as i32) >> shift
}

#[cfg(test)]
mod tests {
    use super::*;

    /// USB tablet layout matching the VirtualBox / QEMU `usb-tablet` field log:
    /// a full byte of buttons, a wheel byte, a pan byte, then 16-bit absolute
    /// X and Y — X at bit 32, Y at bit 48.
    fn tablet_descriptor() -> Vec<u8> {
        alloc::vec![
            0x05, 0x01, // Usage Page (Generic Desktop)
            0x09, 0x02, // Usage (Mouse)
            0xA1, 0x01, // Collection (Application)
            0x09, 0x01, //   Usage (Pointer)
            0xA1, 0x00, //   Collection (Physical)
            0x05, 0x09, //     Usage Page (Button)
            0x19, 0x01, //     Usage Minimum (1)
            0x29, 0x08, //     Usage Maximum (8)
            0x15, 0x00, //     Logical Minimum (0)
            0x25, 0x01, //     Logical Maximum (1)
            0x95, 0x08, //     Report Count (8)
            0x75, 0x01, //     Report Size (1)
            0x81, 0x02, //     Input (Data,Var,Abs)   -> buttons, bits 0..8
            0x05, 0x01, //     Usage Page (Generic Desktop)
            0x09, 0x38, //     Usage (Wheel)
            0x15, 0x81, //     Logical Minimum (-127)
            0x25, 0x7F, //     Logical Maximum (127)
            0x75, 0x08, //     Report Size (8)
            0x95, 0x01, //     Report Count (1)
            0x81, 0x06, //     Input (Data,Var,Rel)   -> wheel, bits 8..16
            0x0B, 0x38, 0x02, 0x0C, 0x00, // Usage (AC Pan, page 0x0C ext)
            0x81, 0x06, //     Input (Data,Var,Rel)   -> pan, bits 16..24
            0x75, 0x08, //     Report Size (8)
            0x95, 0x01, //     Report Count (1)
            0x81, 0x01, //     Input (Const)          -> padding, bits 24..32
            0x05, 0x01, //     Usage Page (Generic Desktop)
            0x09, 0x30, //     Usage (X)
            0x09, 0x31, //     Usage (Y)
            0x15, 0x00, //     Logical Minimum (0)
            0x26, 0xFF, 0x7F, // Logical Maximum (32767)
            0x75, 0x10, //     Report Size (16)
            0x95, 0x02, //     Report Count (2)
            0x81, 0x02, //     Input (Data,Var,Abs)   -> X bit 32, Y bit 48
            0xC0, //         End Collection
            0xC0, //       End Collection
        ]
    }

    fn boot_keyboard_descriptor() -> Vec<u8> {
        alloc::vec![
            0x05, 0x01, // Usage Page (Generic Desktop)
            0x09, 0x06, // Usage (Keyboard)
            0xA1, 0x01, // Collection (Application)
            0x05, 0x07, //   Usage Page (Keyboard/Keypad)
            0x19, 0xE0, //   Usage Minimum (0xE0)
            0x29, 0xE7, //   Usage Maximum (0xE7)
            0x15, 0x00, //   Logical Minimum (0)
            0x25, 0x01, //   Logical Maximum (1)
            0x75, 0x01, //   Report Size (1)
            0x95, 0x08, //   Report Count (8)
            0x81, 0x02, //   Input (Data,Var,Abs)   -> 8 modifier bits, 0..8
            0x95, 0x01, //   Report Count (1)
            0x75, 0x08, //   Report Size (8)
            0x81, 0x01, //   Input (Const)          -> reserved byte, 8..16
            0x05, 0x07, //   Usage Page (Keyboard/Keypad)
            0x19, 0x00, //   Usage Minimum (0)
            0x29, 0x65, //   Usage Maximum (0x65)
            0x15, 0x00, //   Logical Minimum (0)
            0x25, 0x65, //   Logical Maximum (0x65)
            0x75, 0x08, //   Report Size (8)
            0x95, 0x06, //   Report Count (6)
            0x81, 0x00, //   Input (Data,Array)     -> 6 key slots, 16..64
            0xC0, //       End Collection
        ]
    }

    #[test_case]
    fn tablet_x_y_are_16bit_absolute_at_bit_32_and_48() {
        let fields = parse_report_descriptor(&tablet_descriptor());
        let xy = fields
            .iter()
            .find(|f| {
                f.usage_page == PAGE_GENERIC_DESKTOP
                    && f.usages.contains(&(USAGE_X as u32))
                    && f.flags.variable
            })
            .expect("X/Y field present");
        assert_eq!(xy.bit_size, 16);
        assert_eq!(xy.report_count, 2);
        assert_eq!(xy.bit_offset, 32);
        assert!(!xy.flags.relative, "tablet axes are absolute");
        assert_eq!(xy.logical_max, 32767);
        assert_eq!(xy.usage_at(0), (PAGE_GENERIC_DESKTOP, USAGE_X));
        assert_eq!(xy.usage_at(1), (PAGE_GENERIC_DESKTOP, USAGE_Y));
        // Y therefore starts at bit 48.
        assert_eq!(xy.bit_offset + xy.bit_size, 48);
    }

    #[test_case]
    fn tablet_decodes_absolute_position_and_buttons() {
        let mut dev = HidDevice::from_descriptor(&tablet_descriptor());
        assert_eq!(dev.role, HidRole::Mouse);
        assert!(!dev.uses_report_id());

        // buttons=0x01, wheel=0, pan=0, pad=0, X=0x4000, Y=0x2000.
        let report = [0x01, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x20];
        let events = dev.decode(&report);
        assert!(events.contains(&InputEvent::MouseButtonPress(MouseButton::Left)));
        assert!(events.contains(&InputEvent::MouseAbsolute {
            x: 0x4000,
            y: 0x2000
        }));
    }

    #[test_case]
    fn tablet_rescales_when_logical_max_is_not_0x7fff() {
        // logical_max 4095 -> raw 4095 maps to 32767.
        assert_eq!(normalize_abs(4095, 4095), 32767);
        assert_eq!(normalize_abs(0, 4095), 0);
        // Identity when the device already uses the canonical range.
        assert_eq!(normalize_abs(1234, 32767), 1234);
    }

    #[test_case]
    fn application_usage_survives_the_collection_that_clears_it() {
        // 05 01 09 06 a1 01 ... -> Generic Desktop / Keyboard.
        assert_eq!(
            application_collection_usage(&boot_keyboard_descriptor()),
            Some((PAGE_GENERIC_DESKTOP, USAGE_KEYBOARD))
        );
        // Tablet descriptor: Generic Desktop / Mouse.
        assert_eq!(
            application_collection_usage(&tablet_descriptor()),
            Some((PAGE_GENERIC_DESKTOP, USAGE_MOUSE))
        );
        assert_eq!(application_collection_usage(&[]), None);
    }

    #[test_case]
    fn keyboard_role_comes_from_the_application_usage_even_without_boot_protocol() {
        let dev = HidDevice::from_descriptor(&boot_keyboard_descriptor());
        assert_eq!(dev.role, HidRole::Keyboard);
    }

    #[test_case]
    fn boot_keyboard_descriptor_parses_modifiers_and_key_array() {
        let fields = parse_report_descriptor(&boot_keyboard_descriptor());
        let mods = &fields[0];
        assert!(mods.flags.variable);
        assert_eq!(mods.bit_offset, 0);
        assert_eq!(mods.bit_size, 1);
        assert_eq!(mods.report_count, 8);
        assert_eq!(mods.usage_min, 0xE0);
        assert_eq!(mods.usage_max, 0xE7);

        let keys = fields
            .iter()
            .find(|f| !f.flags.variable && !f.flags.constant)
            .unwrap();
        assert_eq!(keys.bit_offset, 16);
        assert_eq!(keys.bit_size, 8);
        assert_eq!(keys.report_count, 6);
    }

    #[test_case]
    fn non_boot_keyboard_emits_press_and_release_with_modifiers() {
        let mut dev = HidDevice::from_descriptor(&boot_keyboard_descriptor());
        assert_eq!(dev.role, HidRole::Keyboard);

        // LeftShift (0xE1 -> modifier bit 1) down, key 'A' (usage 0x04) in slot 0.
        let down = [0b0000_0010, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00];
        let ev = dev.decode(&down);
        assert!(ev.contains(&InputEvent::KeyPress(KeyCode::LeftShift)));
        assert!(ev.contains(&InputEvent::KeyPress(KeyCode::KeyA)));

        // Release everything.
        let up = [0x00; 8];
        let ev = dev.decode(&up);
        assert!(ev.contains(&InputEvent::KeyRelease(KeyCode::LeftShift)));
        assert!(ev.contains(&InputEvent::KeyRelease(KeyCode::KeyA)));
    }

    #[test_case]
    fn report_id_prefix_shifts_the_payload() {
        // Two report IDs: ID 1 = 8 relative button bits + 8-bit X; ID 2 unused.
        let desc = alloc::vec![
            0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, //
            0x85, 0x01, //   Report ID (1)
            0x05, 0x09, 0x19, 0x01, 0x29, 0x08, 0x15, 0x00, 0x25, 0x01, //
            0x75, 0x01, 0x95, 0x08, 0x81, 0x02, // 8 button bits
            0x05, 0x01, 0x09, 0x30, 0x15, 0x81, 0x25, 0x7F, //
            0x75, 0x08, 0x95, 0x01, 0x81, 0x06, // rel X, signed 8-bit
            0xC0,
        ];
        let mut dev = HidDevice::from_descriptor(&desc);
        assert!(dev.uses_report_id());

        // report id 1, no buttons, X = -2.
        let report = [0x01, 0x00, (-2i8) as u8];
        let ev = dev.decode(&report);
        assert_eq!(ev, alloc::vec![InputEvent::MouseMove { dx: -2, dy: 0 }]);

        // Wrong report id -> nothing.
        assert!(dev.decode(&[0x02, 0xFF, 0xFF]).is_empty());
    }

    #[test_case]
    fn extract_and_sign_extend_helpers() {
        // bits 4..12 of 0xAB 0xCD = 0b1101_1010 -> 0xDA
        let data = [0xAB, 0xCD];
        assert_eq!(extract_bits(&data, 4, 8), 0xDA);
        assert_eq!(sign_extend(0xFF, 8), -1);
        assert_eq!(sign_extend(0x7F, 8), 127);
        assert_eq!(sign_extend(0x01, 8), 1);
        assert_eq!(sign_extend(0x1FF, 9), -1);
    }

    #[test_case]
    fn push_pop_restores_global_state() {
        let desc = alloc::vec![
            0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, //
            0x75, 0x08, 0x95, 0x01, //   Report Size 8, Count 1
            0xA4, //   Push
            0x75, 0x10, 0x95, 0x02, //   Report Size 16, Count 2
            0x09, 0x30, 0x81, 0x02, //   Input -> 16-bit field
            0xB4, //   Pop  (back to size 8, count 1)
            0x09, 0x31, 0x81, 0x02, //   Input -> 8-bit field
            0xC0,
        ];
        let fields = parse_report_descriptor(&desc);
        assert_eq!(fields[0].bit_size, 16);
        assert_eq!(fields[0].report_count, 2);
        assert_eq!(fields[1].bit_size, 8);
        assert_eq!(fields[1].report_count, 1);
        // Second field starts right after the first (32 bits).
        assert_eq!(fields[1].bit_offset, 32);
    }
}
