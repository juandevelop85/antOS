//! antOS Kernel Input Subsystem.
//!
//! Provides typed input events, modifier tracking, ASCII decoding,
//! circular lock-protected event queue, and scancode decoders.

pub mod queue;
pub use queue::*;

/// Identifiers for keyboard physical keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeyCode {
    // Letters
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,

    // Digits
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,

    // Function keys
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,

    // Control & Editing
    Escape,
    Enter,
    Tab,
    Backspace,
    Space,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,

    // Navigation
    Up,
    Down,
    Left,
    Right,

    // Modifiers
    LeftShift,
    RightShift,
    LeftCtrl,
    RightCtrl,
    LeftAlt,
    RightAlt,
    LeftSuper,
    RightSuper,
    CapsLock,

    // Punctuation & Symbols
    Minus,
    Equal,
    LeftBracket,
    RightBracket,
    Backslash,
    Semicolon,
    Apostrophe,
    Grave,
    Comma,
    Dot,
    Slash,

    // Fallback
    Unknown(u16),
}

/// Identifiers for mouse buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u8),
}

/// Typed input event produced by input devices (keyboard, mouse, touchpad).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    KeyPress(KeyCode),
    KeyRelease(KeyCode),
    MouseMove { dx: i32, dy: i32 },
    MouseAbsolute { x: u32, y: u32 },
    MouseButtonPress(MouseButton),
    MouseButtonRelease(MouseButton),
    Scroll { delta_x: i32, delta_y: i32 },
}

/// Active modifier keys state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_key: bool,
    pub caps_lock: bool,
}

impl Modifiers {
    pub const fn empty() -> Self {
        Self {
            shift: false,
            ctrl: false,
            alt: false,
            super_key: false,
            caps_lock: false,
        }
    }
}

/// Maintains active keyboard state, modifiers, and converts key presses to characters.
#[derive(Debug, Clone, Default)]
pub struct KeyboardState {
    pub modifiers: Modifiers,
}

impl KeyboardState {
    pub const fn new() -> Self {
        Self {
            modifiers: Modifiers::empty(),
        }
    }

    /// Updates internal modifier states on key press or release.
    pub fn update(&mut self, event: &InputEvent) {
        match *event {
            InputEvent::KeyPress(key) => match key {
                KeyCode::LeftShift | KeyCode::RightShift => self.modifiers.shift = true,
                KeyCode::LeftCtrl | KeyCode::RightCtrl => self.modifiers.ctrl = true,
                KeyCode::LeftAlt | KeyCode::RightAlt => self.modifiers.alt = true,
                KeyCode::LeftSuper | KeyCode::RightSuper => self.modifiers.super_key = true,
                KeyCode::CapsLock => self.modifiers.caps_lock = !self.modifiers.caps_lock,
                _ => {}
            },
            InputEvent::KeyRelease(key) => match key {
                KeyCode::LeftShift | KeyCode::RightShift => self.modifiers.shift = false,
                KeyCode::LeftCtrl | KeyCode::RightCtrl => self.modifiers.ctrl = false,
                KeyCode::LeftAlt | KeyCode::RightAlt => self.modifiers.alt = false,
                KeyCode::LeftSuper | KeyCode::RightSuper => self.modifiers.super_key = false,
                _ => {}
            },
            _ => {}
        }
    }

    /// Converts a key code to an ASCII char given the current modifier state.
    pub fn key_to_char(&self, key: KeyCode) -> Option<char> {
        let is_upper = self.modifiers.shift ^ self.modifiers.caps_lock;
        let shift = self.modifiers.shift;

        match key {
            KeyCode::KeyA => Some(if is_upper { 'A' } else { 'a' }),
            KeyCode::KeyB => Some(if is_upper { 'B' } else { 'b' }),
            KeyCode::KeyC => Some(if is_upper { 'C' } else { 'c' }),
            KeyCode::KeyD => Some(if is_upper { 'D' } else { 'd' }),
            KeyCode::KeyE => Some(if is_upper { 'E' } else { 'e' }),
            KeyCode::KeyF => Some(if is_upper { 'F' } else { 'f' }),
            KeyCode::KeyG => Some(if is_upper { 'G' } else { 'g' }),
            KeyCode::KeyH => Some(if is_upper { 'H' } else { 'h' }),
            KeyCode::KeyI => Some(if is_upper { 'I' } else { 'i' }),
            KeyCode::KeyJ => Some(if is_upper { 'J' } else { 'j' }),
            KeyCode::KeyK => Some(if is_upper { 'K' } else { 'k' }),
            KeyCode::KeyL => Some(if is_upper { 'L' } else { 'l' }),
            KeyCode::KeyM => Some(if is_upper { 'M' } else { 'm' }),
            KeyCode::KeyN => Some(if is_upper { 'N' } else { 'n' }),
            KeyCode::KeyO => Some(if is_upper { 'O' } else { 'o' }),
            KeyCode::KeyP => Some(if is_upper { 'P' } else { 'p' }),
            KeyCode::KeyQ => Some(if is_upper { 'Q' } else { 'q' }),
            KeyCode::KeyR => Some(if is_upper { 'R' } else { 'r' }),
            KeyCode::KeyS => Some(if is_upper { 'S' } else { 's' }),
            KeyCode::KeyT => Some(if is_upper { 'T' } else { 't' }),
            KeyCode::KeyU => Some(if is_upper { 'U' } else { 'u' }),
            KeyCode::KeyV => Some(if is_upper { 'V' } else { 'v' }),
            KeyCode::KeyW => Some(if is_upper { 'W' } else { 'w' }),
            KeyCode::KeyX => Some(if is_upper { 'X' } else { 'x' }),
            KeyCode::KeyY => Some(if is_upper { 'Y' } else { 'y' }),
            KeyCode::KeyZ => Some(if is_upper { 'Z' } else { 'z' }),

            KeyCode::Num0 => Some(if shift { ')' } else { '0' }),
            KeyCode::Num1 => Some(if shift { '!' } else { '1' }),
            KeyCode::Num2 => Some(if shift { '@' } else { '2' }),
            KeyCode::Num3 => Some(if shift { '#' } else { '3' }),
            KeyCode::Num4 => Some(if shift { '$' } else { '4' }),
            KeyCode::Num5 => Some(if shift { '%' } else { '5' }),
            KeyCode::Num6 => Some(if shift { '^' } else { '6' }),
            KeyCode::Num7 => Some(if shift { '&' } else { '7' }),
            KeyCode::Num8 => Some(if shift { '*' } else { '8' }),
            KeyCode::Num9 => Some(if shift { '(' } else { '9' }),

            KeyCode::Space => Some(' '),
            KeyCode::Tab => Some('\t'),
            KeyCode::Enter => Some('\n'),
            KeyCode::Backspace => Some('\x08'),

            KeyCode::Minus => Some(if shift { '_' } else { '-' }),
            KeyCode::Equal => Some(if shift { '+' } else { '=' }),
            KeyCode::LeftBracket => Some(if shift { '{' } else { '[' }),
            KeyCode::RightBracket => Some(if shift { '}' } else { ']' }),
            KeyCode::Backslash => Some(if shift { '|' } else { '\\' }),
            KeyCode::Semicolon => Some(if shift { ':' } else { ';' }),
            KeyCode::Apostrophe => Some(if shift { '"' } else { '\'' }),
            KeyCode::Grave => Some(if shift { '~' } else { '`' }),
            KeyCode::Comma => Some(if shift { '<' } else { ',' }),
            KeyCode::Dot => Some(if shift { '>' } else { '.' }),
            KeyCode::Slash => Some(if shift { '?' } else { '/' }),

            _ => None,
        }
    }
}

/// Tracks modifier state (shift/caps/ctrl) for the ASCII bridge below. Shared
/// by whichever driver fed `GLOBAL_INPUT_QUEUE` — PS/2 (x86_64) or
/// VirtIO-Input (AArch64) — since both funnel through the same typed queue.
static ASCII_BRIDGE_STATE: crate::sync::SpinLock<KeyboardState> =
    crate::sync::SpinLock::new(KeyboardState::new());

/// Drains as many queued key-press events as fit in `buf`, decoding them to
/// ASCII via [`KeyboardState::key_to_char`]. Non-blocking: returns `0`
/// immediately if nothing is queued. Used by `SYS_READ` (T26.5) so the
/// userspace shell can poll `fd 0` the same way on every architecture.
pub fn drain_ascii(buf: &mut [u8]) -> usize {
    crate::drivers::usb::poll();
    let mut state = ASCII_BRIDGE_STATE.lock();
    let mut written = 0usize;
    while written < buf.len() {
        let Some(event) = pop_event() else { break };
        state.update(&event);
        if let InputEvent::KeyPress(key) = event {
            if let Some(ch) = state.key_to_char(key) {
                if ch.is_ascii() {
                    buf[written] = ch as u8;
                    written += 1;
                }
            }
        }
    }
    written
}

/// Linux Input Subsystem Event Types.
pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_REL: u16 = 0x02;
pub const EV_ABS: u16 = 0x03;

pub const REL_X: u16 = 0x00;
pub const REL_Y: u16 = 0x01;
pub const REL_WHEEL: u16 = 0x08;

pub const ABS_X: u16 = 0x00;
pub const ABS_Y: u16 = 0x01;

pub const BTN_LEFT: u16 = 0x110;
pub const BTN_RIGHT: u16 = 0x111;
pub const BTN_MIDDLE: u16 = 0x112;

/// Decodes Linux EV_* events (standard VirtIO-Input format) into `InputEvent`.
pub fn decode_linux_ev(event_type: u16, code: u16, value: u32) -> Option<InputEvent> {
    match event_type {
        EV_KEY => {
            if code == BTN_LEFT {
                return if value != 0 {
                    Some(InputEvent::MouseButtonPress(MouseButton::Left))
                } else {
                    Some(InputEvent::MouseButtonRelease(MouseButton::Left))
                };
            } else if code == BTN_RIGHT {
                return if value != 0 {
                    Some(InputEvent::MouseButtonPress(MouseButton::Right))
                } else {
                    Some(InputEvent::MouseButtonRelease(MouseButton::Right))
                };
            } else if code == BTN_MIDDLE {
                return if value != 0 {
                    Some(InputEvent::MouseButtonPress(MouseButton::Middle))
                } else {
                    Some(InputEvent::MouseButtonRelease(MouseButton::Middle))
                };
            }

            let key = linux_code_to_key(code);
            if value == 0 {
                Some(InputEvent::KeyRelease(key))
            } else {
                Some(InputEvent::KeyPress(key))
            }
        }
        EV_REL => {
            let val = value as i32;
            match code {
                REL_X => Some(InputEvent::MouseMove { dx: val, dy: 0 }),
                REL_Y => Some(InputEvent::MouseMove { dx: 0, dy: val }),
                REL_WHEEL => Some(InputEvent::Scroll {
                    delta_x: 0,
                    delta_y: val,
                }),
                _ => None,
            }
        }
        EV_ABS => match code {
            ABS_X => Some(InputEvent::MouseAbsolute {
                x: value,
                y: 0,
            }),
            ABS_Y => Some(InputEvent::MouseAbsolute {
                x: 0,
                y: value,
            }),
            _ => None,
        },
        _ => None,
    }
}

/// Converts Linux input event codes (`KEY_*`) to `KeyCode`.
pub fn linux_code_to_key(code: u16) -> KeyCode {
    match code {
        1 => KeyCode::Escape,
        2 => KeyCode::Num1,
        3 => KeyCode::Num2,
        4 => KeyCode::Num3,
        5 => KeyCode::Num4,
        6 => KeyCode::Num5,
        7 => KeyCode::Num6,
        8 => KeyCode::Num7,
        9 => KeyCode::Num8,
        10 => KeyCode::Num9,
        11 => KeyCode::Num0,
        12 => KeyCode::Minus,
        13 => KeyCode::Equal,
        14 => KeyCode::Backspace,
        15 => KeyCode::Tab,
        16 => KeyCode::KeyQ,
        17 => KeyCode::KeyW,
        18 => KeyCode::KeyE,
        19 => KeyCode::KeyR,
        20 => KeyCode::KeyT,
        21 => KeyCode::KeyY,
        22 => KeyCode::KeyU,
        23 => KeyCode::KeyI,
        24 => KeyCode::KeyO,
        25 => KeyCode::KeyP,
        26 => KeyCode::LeftBracket,
        27 => KeyCode::RightBracket,
        28 => KeyCode::Enter,
        29 => KeyCode::LeftCtrl,
        30 => KeyCode::KeyA,
        31 => KeyCode::KeyS,
        32 => KeyCode::KeyD,
        33 => KeyCode::KeyF,
        34 => KeyCode::KeyG,
        35 => KeyCode::KeyH,
        36 => KeyCode::KeyJ,
        37 => KeyCode::KeyK,
        38 => KeyCode::KeyL,
        39 => KeyCode::Semicolon,
        40 => KeyCode::Apostrophe,
        41 => KeyCode::Grave,
        42 => KeyCode::LeftShift,
        43 => KeyCode::Backslash,
        44 => KeyCode::KeyZ,
        45 => KeyCode::KeyX,
        46 => KeyCode::KeyC,
        47 => KeyCode::KeyV,
        48 => KeyCode::KeyB,
        49 => KeyCode::KeyN,
        50 => KeyCode::KeyM,
        51 => KeyCode::Comma,
        52 => KeyCode::Dot,
        53 => KeyCode::Slash,
        54 => KeyCode::RightShift,
        56 => KeyCode::LeftAlt,
        57 => KeyCode::Space,
        58 => KeyCode::CapsLock,
        59 => KeyCode::F1,
        60 => KeyCode::F2,
        61 => KeyCode::F3,
        62 => KeyCode::F4,
        63 => KeyCode::F5,
        64 => KeyCode::F6,
        65 => KeyCode::F7,
        66 => KeyCode::F8,
        67 => KeyCode::F9,
        68 => KeyCode::F10,
        87 => KeyCode::F11,
        88 => KeyCode::F12,
        97 => KeyCode::RightCtrl,
        100 => KeyCode::RightAlt,
        103 => KeyCode::Up,
        105 => KeyCode::Left,
        106 => KeyCode::Right,
        108 => KeyCode::Down,
        110 => KeyCode::Insert,
        111 => KeyCode::Delete,
        102 => KeyCode::Home,
        107 => KeyCode::End,
        104 => KeyCode::PageUp,
        109 => KeyCode::PageDown,
        125 => KeyCode::LeftSuper,
        126 => KeyCode::RightSuper,
        other => KeyCode::Unknown(other),
    }
}

/// Decodes raw PS/2 Set 1 scancodes into `InputEvent`.
pub fn decode_ps2_set1(scancode: u8) -> Option<InputEvent> {
    let is_release = (scancode & 0x80) != 0;
    let code = scancode & 0x7F;

    let key = match code {
        0x01 => KeyCode::Escape,
        0x02 => KeyCode::Num1,
        0x03 => KeyCode::Num2,
        0x04 => KeyCode::Num3,
        0x05 => KeyCode::Num4,
        0x06 => KeyCode::Num5,
        0x07 => KeyCode::Num6,
        0x08 => KeyCode::Num7,
        0x09 => KeyCode::Num8,
        0x0A => KeyCode::Num9,
        0x0B => KeyCode::Num0,
        0x0C => KeyCode::Minus,
        0x0D => KeyCode::Equal,
        0x0E => KeyCode::Backspace,
        0x0F => KeyCode::Tab,
        0x10 => KeyCode::KeyQ,
        0x11 => KeyCode::KeyW,
        0x12 => KeyCode::KeyE,
        0x13 => KeyCode::KeyR,
        0x14 => KeyCode::KeyT,
        0x15 => KeyCode::KeyY,
        0x16 => KeyCode::KeyU,
        0x17 => KeyCode::KeyI,
        0x18 => KeyCode::KeyO,
        0x19 => KeyCode::KeyP,
        0x1A => KeyCode::LeftBracket,
        0x1B => KeyCode::RightBracket,
        0x1C => KeyCode::Enter,
        0x1D => KeyCode::LeftCtrl,
        0x1E => KeyCode::KeyA,
        0x1F => KeyCode::KeyS,
        0x20 => KeyCode::KeyD,
        0x21 => KeyCode::KeyF,
        0x22 => KeyCode::KeyG,
        0x23 => KeyCode::KeyH,
        0x24 => KeyCode::KeyJ,
        0x25 => KeyCode::KeyK,
        0x26 => KeyCode::KeyL,
        0x27 => KeyCode::Semicolon,
        0x28 => KeyCode::Apostrophe,
        0x29 => KeyCode::Grave,
        0x2A => KeyCode::LeftShift,
        0x2B => KeyCode::Backslash,
        0x2C => KeyCode::KeyZ,
        0x2D => KeyCode::KeyX,
        0x2E => KeyCode::KeyC,
        0x2F => KeyCode::KeyV,
        0x30 => KeyCode::KeyB,
        0x31 => KeyCode::KeyN,
        0x32 => KeyCode::KeyM,
        0x33 => KeyCode::Comma,
        0x34 => KeyCode::Dot,
        0x35 => KeyCode::Slash,
        0x36 => KeyCode::RightShift,
        0x38 => KeyCode::LeftAlt,
        0x39 => KeyCode::Space,
        0x3A => KeyCode::CapsLock,
        0x3B => KeyCode::F1,
        0x3C => KeyCode::F2,
        0x3D => KeyCode::F3,
        0x3E => KeyCode::F4,
        0x3F => KeyCode::F5,
        0x40 => KeyCode::F6,
        0x41 => KeyCode::F7,
        0x42 => KeyCode::F8,
        0x43 => KeyCode::F9,
        0x44 => KeyCode::F10,
        0x57 => KeyCode::F11,
        0x58 => KeyCode::F12,
        other => KeyCode::Unknown(other as u16),
    };

    if is_release {
        Some(InputEvent::KeyRelease(key))
    } else {
        Some(InputEvent::KeyPress(key))
    }
}
