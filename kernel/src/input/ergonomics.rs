//! Input ergonomics: keyboard layouts with dead keys, typematic auto-repeat,
//! keyboard-LED bitmaps, and a pointer acceleration curve.
//!
//! Everything here is pure state/logic so it can be unit-tested off-target; the
//! wiring into the live event path lives in [`super`].

use crate::input::KeyCode;

// ── Keyboard layouts ───────────────────────────────────────────────────────

/// Selectable keyboard layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardLayout {
    /// US QWERTY — the historical default, unchanged behaviour.
    Us,
    /// Spanish QWERTY (`ñ`, `ç`, dead accents, `AltGr` third level).
    Es,
}

impl KeyboardLayout {
    /// Parses a layout name from a config string (`keyboard_layout=es`).
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "us" | "en" | "en-us" | "us-qwerty" => Some(Self::Us),
            "es" | "es-es" | "spanish" | "espanol" | "español" => Some(Self::Es),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Us => "us",
            Self::Es => "es",
        }
    }
}

/// A pending dead key: it produces no character on its own and combines with
/// the next base letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadKey {
    Acute,
    Grave,
    Circumflex,
    Diaeresis,
    Tilde,
}

/// Outcome of resolving one key press against a layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyResolve {
    /// A finished character.
    Char(char),
    /// A dead key; hold it and combine with the next [`KeyResolve::Char`].
    Dead(DeadKey),
    /// No printable output (function keys, bare modifiers, …).
    None,
}

/// Combines a held dead key with a base character. Returns `None` when the pair
/// has no precomposed form (the caller then emits the base char unmodified,
/// optionally preceded by the standalone accent).
pub fn combine_dead(dead: DeadKey, base: char) -> Option<char> {
    let table: &[(char, char)] = match dead {
        DeadKey::Acute => &[
            ('a', 'á'), ('e', 'é'), ('i', 'í'), ('o', 'ó'), ('u', 'ú'), ('y', 'ý'),
            ('A', 'Á'), ('E', 'É'), ('I', 'Í'), ('O', 'Ó'), ('U', 'Ú'),
        ],
        DeadKey::Grave => &[
            ('a', 'à'), ('e', 'è'), ('i', 'ì'), ('o', 'ò'), ('u', 'ù'),
            ('A', 'À'), ('E', 'È'), ('I', 'Ì'), ('O', 'Ò'), ('U', 'Ù'),
        ],
        DeadKey::Circumflex => &[
            ('a', 'â'), ('e', 'ê'), ('i', 'î'), ('o', 'ô'), ('u', 'û'),
            ('A', 'Â'), ('E', 'Ê'), ('I', 'Î'), ('O', 'Ô'), ('U', 'Û'),
        ],
        DeadKey::Diaeresis => &[
            ('a', 'ä'), ('e', 'ë'), ('i', 'ï'), ('o', 'ö'), ('u', 'ü'),
            ('A', 'Ä'), ('E', 'Ë'), ('I', 'Ï'), ('O', 'Ö'), ('U', 'Ü'),
        ],
        DeadKey::Tilde => &[
            ('a', 'ã'), ('n', 'ñ'), ('o', 'õ'),
            ('A', 'Ã'), ('N', 'Ñ'), ('O', 'Õ'),
        ],
    };
    table.iter().find(|(b, _)| *b == base).map(|(_, c)| *c)
}

/// The stand-alone glyph for a dead key when it does not combine (e.g. dead
/// acute followed by space).
pub fn dead_key_glyph(dead: DeadKey) -> char {
    match dead {
        DeadKey::Acute => '´',
        DeadKey::Grave => '`',
        DeadKey::Circumflex => '^',
        DeadKey::Diaeresis => '¨',
        DeadKey::Tilde => '~',
    }
}

/// Resolves a key press to a character / dead key for the given layout and
/// modifier state. `caps` is the Caps Lock latch; `altgr` is the AltGr / right
/// Alt third-level shift.
pub fn resolve_key(
    layout: KeyboardLayout,
    key: KeyCode,
    shift: bool,
    caps: bool,
    altgr: bool,
) -> KeyResolve {
    match layout {
        KeyboardLayout::Us => resolve_us(key, shift, caps),
        KeyboardLayout::Es => resolve_es(key, shift, caps, altgr),
    }
}

fn letter(base_lower: char, shift: bool, caps: bool) -> KeyResolve {
    let upper = shift ^ caps;
    let ch = if upper {
        // `to_uppercase` (not `to_ascii_uppercase`) so `ñ`/`ç` case-fold too.
        base_lower.to_uppercase().next().unwrap_or(base_lower)
    } else {
        base_lower
    };
    KeyResolve::Char(ch)
}

/// US QWERTY — a faithful port of the original `KeyboardState::key_to_char`.
fn resolve_us(key: KeyCode, shift: bool, caps: bool) -> KeyResolve {
    use KeyCode::*;
    let c = |ch: char| KeyResolve::Char(ch);
    match key {
        KeyA => return letter('a', shift, caps),
        KeyB => return letter('b', shift, caps),
        KeyC => return letter('c', shift, caps),
        KeyD => return letter('d', shift, caps),
        KeyE => return letter('e', shift, caps),
        KeyF => return letter('f', shift, caps),
        KeyG => return letter('g', shift, caps),
        KeyH => return letter('h', shift, caps),
        KeyI => return letter('i', shift, caps),
        KeyJ => return letter('j', shift, caps),
        KeyK => return letter('k', shift, caps),
        KeyL => return letter('l', shift, caps),
        KeyM => return letter('m', shift, caps),
        KeyN => return letter('n', shift, caps),
        KeyO => return letter('o', shift, caps),
        KeyP => return letter('p', shift, caps),
        KeyQ => return letter('q', shift, caps),
        KeyR => return letter('r', shift, caps),
        KeyS => return letter('s', shift, caps),
        KeyT => return letter('t', shift, caps),
        KeyU => return letter('u', shift, caps),
        KeyV => return letter('v', shift, caps),
        KeyW => return letter('w', shift, caps),
        KeyX => return letter('x', shift, caps),
        KeyY => return letter('y', shift, caps),
        KeyZ => return letter('z', shift, caps),

        Num0 => return c(if shift { ')' } else { '0' }),
        Num1 => return c(if shift { '!' } else { '1' }),
        Num2 => return c(if shift { '@' } else { '2' }),
        Num3 => return c(if shift { '#' } else { '3' }),
        Num4 => return c(if shift { '$' } else { '4' }),
        Num5 => return c(if shift { '%' } else { '5' }),
        Num6 => return c(if shift { '^' } else { '6' }),
        Num7 => return c(if shift { '&' } else { '7' }),
        Num8 => return c(if shift { '*' } else { '8' }),
        Num9 => return c(if shift { '(' } else { '9' }),

        Space => return c(' '),
        Tab => return c('\t'),
        Enter => return c('\n'),
        Backspace => return c('\x08'),

        Minus => return c(if shift { '_' } else { '-' }),
        Equal => return c(if shift { '+' } else { '=' }),
        LeftBracket => return c(if shift { '{' } else { '[' }),
        RightBracket => return c(if shift { '}' } else { ']' }),
        Backslash => return c(if shift { '|' } else { '\\' }),
        Semicolon => return c(if shift { ':' } else { ';' }),
        Apostrophe => return c(if shift { '"' } else { '\'' }),
        Grave => return c(if shift { '~' } else { '`' }),
        Comma => return c(if shift { '<' } else { ',' }),
        Dot => return c(if shift { '>' } else { '.' }),
        Slash => return c(if shift { '?' } else { '/' }),

        _ => {}
    }
    KeyResolve::None
}

/// Spanish QWERTY. Physical keys are named by their US legend; the ES glyph is
/// what that physical key produces on a Spanish keyboard.
fn resolve_es(key: KeyCode, shift: bool, caps: bool, altgr: bool) -> KeyResolve {
    use KeyCode::*;
    let c = |ch: char| KeyResolve::Char(ch);
    let d = |dk: DeadKey| KeyResolve::Dead(dk);

    // AltGr (third level) first.
    if altgr {
        return match key {
            Num1 => c('|'),
            Num2 => c('@'),
            Num3 => c('#'),
            Num4 => c('~'),
            KeyE => c('€'),
            Grave => c('\\'),        // key left of "1"
            LeftBracket => c('['),
            RightBracket => c(']'),
            Backslash => c('}'),
            Semicolon => c('~'),     // AltGr on the "ñ" key
            Apostrophe => c('{'),
            _ => KeyResolve::None,
        };
    }

    match key {
        // Letters carry through unchanged; accents arrive via dead keys.
        KeyA => letter('a', shift, caps),
        KeyB => letter('b', shift, caps),
        KeyC => letter('c', shift, caps),
        KeyD => letter('d', shift, caps),
        KeyE => letter('e', shift, caps),
        KeyF => letter('f', shift, caps),
        KeyG => letter('g', shift, caps),
        KeyH => letter('h', shift, caps),
        KeyI => letter('i', shift, caps),
        KeyJ => letter('j', shift, caps),
        KeyK => letter('k', shift, caps),
        KeyL => letter('l', shift, caps),
        KeyM => letter('m', shift, caps),
        KeyN => letter('n', shift, caps),
        KeyO => letter('o', shift, caps),
        KeyP => letter('p', shift, caps),
        KeyQ => letter('q', shift, caps),
        KeyR => letter('r', shift, caps),
        KeyS => letter('s', shift, caps),
        KeyT => letter('t', shift, caps),
        KeyU => letter('u', shift, caps),
        KeyV => letter('v', shift, caps),
        KeyW => letter('w', shift, caps),
        KeyX => letter('x', shift, caps),
        KeyY => letter('y', shift, caps),
        KeyZ => letter('z', shift, caps),

        // "ñ" sits on the US semicolon key and behaves as a letter.
        Semicolon => letter('ñ', shift, caps),
        // "ç" sits on the US backslash key.
        Backslash => letter('ç', shift, caps),

        Num0 => c(if shift { '=' } else { '0' }),
        Num1 => c(if shift { '!' } else { '1' }),
        Num2 => c(if shift { '"' } else { '2' }),
        Num3 => c(if shift { '·' } else { '3' }),
        Num4 => c(if shift { '$' } else { '4' }),
        Num5 => c(if shift { '%' } else { '5' }),
        Num6 => c(if shift { '&' } else { '6' }),
        Num7 => c(if shift { '/' } else { '7' }),
        Num8 => c(if shift { '(' } else { '8' }),
        Num9 => c(if shift { ')' } else { '9' }),

        Space => c(' '),
        Tab => c('\t'),
        Enter => c('\n'),
        Backspace => c('\x08'),

        // Dead accents: acute (US apostrophe) and grave/circumflex (US "[").
        Apostrophe => {
            if shift {
                d(DeadKey::Diaeresis)
            } else {
                d(DeadKey::Acute)
            }
        }
        LeftBracket => {
            if shift {
                d(DeadKey::Circumflex)
            } else {
                d(DeadKey::Grave)
            }
        }

        Minus => c(if shift { '_' } else { '\'' }),
        Equal => c(if shift { '¿' } else { '¡' }),
        RightBracket => c(if shift { '*' } else { '+' }),
        Comma => c(if shift { ';' } else { ',' }),
        Dot => c(if shift { ':' } else { '.' }),
        Slash => c(if shift { '_' } else { '-' }),
        Grave => c(if shift { 'ª' } else { 'º' }),

        _ => KeyResolve::None,
    }
}

// ── Keyboard LEDs ──────────────────────────────────────────────────────────

/// HID keyboard Output-report LED bits (USB HID Usage Page 0x08).
pub mod led_bit {
    pub const NUM_LOCK: u8 = 1 << 0;
    pub const CAPS_LOCK: u8 = 1 << 1;
    pub const SCROLL_LOCK: u8 = 1 << 2;
}

/// Packs the three lock latches into a HID LED Output-report byte.
pub fn led_bitmap(num: bool, caps: bool, scroll: bool) -> u8 {
    (if num { led_bit::NUM_LOCK } else { 0 })
        | (if caps { led_bit::CAPS_LOCK } else { 0 })
        | (if scroll { led_bit::SCROLL_LOCK } else { 0 })
}

// ── Typematic auto-repeat ──────────────────────────────────────────────────

/// Tracks the currently held repeatable key and when its next repeat is due,
/// all in timer ticks so it composes with either the input poll or the timer
/// interrupt.
#[derive(Debug, Clone, Copy)]
pub struct AutoRepeat {
    key: Option<KeyCode>,
    fire_at: u64,
    delay_ticks: u64,
    interval_ticks: u64,
}

impl AutoRepeat {
    pub const fn new(delay_ticks: u64, interval_ticks: u64) -> Self {
        Self {
            key: None,
            fire_at: 0,
            delay_ticks,
            interval_ticks,
        }
    }

    pub fn set_timing(&mut self, delay_ticks: u64, interval_ticks: u64) {
        self.delay_ticks = delay_ticks;
        self.interval_ticks = interval_ticks.max(1);
    }

    pub fn held_key(&self) -> Option<KeyCode> {
        self.key
    }

    /// A key went down. A *different* repeatable key takes over and restarts the
    /// initial delay; a press of the key already held (a hardware echo or our
    /// own synthetic repeat fed back in) leaves the schedule untouched.
    pub fn on_press(&mut self, key: KeyCode, now: u64) {
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        self.fire_at = now.saturating_add(self.delay_ticks);
    }

    /// A key came up. Only clears the repeat if it is the tracked one.
    pub fn on_release(&mut self, key: KeyCode) {
        if self.key == Some(key) {
            self.key = None;
        }
    }

    pub fn clear(&mut self) {
        self.key = None;
    }

    /// Returns the held key when a repeat is due at `now`, advancing the
    /// schedule by one interval. Returns `None` otherwise.
    pub fn tick(&mut self, now: u64) -> Option<KeyCode> {
        let key = self.key?;
        if now >= self.fire_at {
            self.fire_at = now.saturating_add(self.interval_ticks.max(1));
            Some(key)
        } else {
            None
        }
    }
}

// ── Pointer acceleration ───────────────────────────────────────────────────

/// Pointer sensitivity and acceleration curve, all in percent so the whole
/// computation stays integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointerAccel {
    /// Base gain in percent (`100` = 1.0x, unchanged motion).
    pub sensitivity_pct: u32,
    /// `|delta|` at or below this is not accelerated (only scaled by
    /// `sensitivity_pct`).
    pub threshold: u32,
    /// Extra gain in percent added per unit of `|delta|` above `threshold`.
    pub accel_pct: u32,
}

impl PointerAccel {
    pub const DEFAULT: Self = Self {
        sensitivity_pct: 100,
        threshold: 6,
        accel_pct: 40,
    };

    /// Applies the curve to one axis delta, preserving sign and guaranteeing a
    /// non-zero result for a non-zero input so slow motion still registers.
    pub fn apply(&self, delta: i32) -> i32 {
        if delta == 0 {
            return 0;
        }
        let mag = delta.unsigned_abs() as u64;
        let over = mag.saturating_sub(self.threshold as u64);
        let gain = self.sensitivity_pct as u64 + over * self.accel_pct as u64;
        let scaled = (mag * gain / 100).max(1);
        let out = scaled.min(i32::MAX as u64) as i32;
        if delta < 0 {
            -out
        } else {
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── layouts ────────────────────────────────────────────────────────────

    fn ch(r: KeyResolve) -> Option<char> {
        match r {
            KeyResolve::Char(c) => Some(c),
            _ => None,
        }
    }

    #[test]
    fn us_layout_is_unchanged() {
        let l = KeyboardLayout::Us;
        assert_eq!(ch(resolve_key(l, KeyCode::KeyA, false, false, false)), Some('a'));
        assert_eq!(ch(resolve_key(l, KeyCode::KeyA, true, false, false)), Some('A'));
        assert_eq!(ch(resolve_key(l, KeyCode::KeyA, false, true, false)), Some('A'));
        assert_eq!(ch(resolve_key(l, KeyCode::KeyA, true, true, false)), Some('a'));
        assert_eq!(ch(resolve_key(l, KeyCode::Num2, true, false, false)), Some('@'));
        assert_eq!(ch(resolve_key(l, KeyCode::Semicolon, false, false, false)), Some(';'));
        assert_eq!(ch(resolve_key(l, KeyCode::Semicolon, true, false, false)), Some(':'));
        assert_eq!(ch(resolve_key(l, KeyCode::Slash, true, false, false)), Some('?'));
        assert_eq!(resolve_key(l, KeyCode::F1, false, false, false), KeyResolve::None);
    }

    #[test]
    fn es_layout_special_glyphs() {
        let l = KeyboardLayout::Es;
        // ñ / Ñ on the US semicolon key
        assert_eq!(ch(resolve_key(l, KeyCode::Semicolon, false, false, false)), Some('ñ'));
        assert_eq!(ch(resolve_key(l, KeyCode::Semicolon, true, false, false)), Some('Ñ'));
        // ç / Ç on the US backslash key
        assert_eq!(ch(resolve_key(l, KeyCode::Backslash, false, false, false)), Some('ç'));
        assert_eq!(ch(resolve_key(l, KeyCode::Backslash, true, false, false)), Some('Ç'));
        // ; and : are Shift on the comma/period keys
        assert_eq!(ch(resolve_key(l, KeyCode::Comma, true, false, false)), Some(';'));
        assert_eq!(ch(resolve_key(l, KeyCode::Dot, true, false, false)), Some(':'));
    }

    #[test]
    fn es_layout_altgr_third_level() {
        let l = KeyboardLayout::Es;
        assert_eq!(ch(resolve_key(l, KeyCode::Num2, false, false, true)), Some('@'));
        assert_eq!(ch(resolve_key(l, KeyCode::Num3, false, false, true)), Some('#'));
        assert_eq!(ch(resolve_key(l, KeyCode::KeyE, false, false, true)), Some('€'));
        assert_eq!(ch(resolve_key(l, KeyCode::Num4, false, false, true)), Some('~'));
        assert_eq!(ch(resolve_key(l, KeyCode::Grave, false, false, true)), Some('\\'));
    }

    #[test]
    fn es_dead_acute_composes_vowels() {
        let l = KeyboardLayout::Es;
        let dead = resolve_key(l, KeyCode::Apostrophe, false, false, false);
        assert_eq!(dead, KeyResolve::Dead(DeadKey::Acute));
        assert_eq!(combine_dead(DeadKey::Acute, 'a'), Some('á'));
        assert_eq!(combine_dead(DeadKey::Acute, 'O'), Some('Ó'));
        assert_eq!(combine_dead(DeadKey::Acute, 'z'), None);
        assert_eq!(combine_dead(DeadKey::Diaeresis, 'u'), Some('ü'));
        assert_eq!(combine_dead(DeadKey::Tilde, 'n'), Some('ñ'));
        assert_eq!(dead_key_glyph(DeadKey::Acute), '´');
    }

    #[test]
    fn layout_name_round_trips() {
        assert_eq!(KeyboardLayout::from_name("ES"), Some(KeyboardLayout::Es));
        assert_eq!(KeyboardLayout::from_name(" us "), Some(KeyboardLayout::Us));
        assert_eq!(KeyboardLayout::from_name("dvorak"), None);
        assert_eq!(KeyboardLayout::Es.name(), "es");
    }

    // ── LEDs ──────────────────────────────────────────────────────────────

    #[test]
    fn led_bitmap_packs_hid_bits() {
        assert_eq!(led_bitmap(false, false, false), 0);
        assert_eq!(led_bitmap(true, false, false), 0b001);
        assert_eq!(led_bitmap(false, true, false), 0b010);
        assert_eq!(led_bitmap(true, true, true), 0b111);
    }

    // ── auto-repeat ───────────────────────────────────────────────────────

    #[test]
    fn auto_repeat_waits_delay_then_repeats_at_rate() {
        let mut ar = AutoRepeat::new(50, 3);
        ar.on_press(KeyCode::KeyA, 1000);
        // Nothing before the initial delay elapses.
        assert_eq!(ar.tick(1040), None);
        assert_eq!(ar.tick(1049), None);
        // First repeat at delay boundary.
        assert_eq!(ar.tick(1050), Some(KeyCode::KeyA));
        // Then every `interval` ticks.
        assert_eq!(ar.tick(1052), None);
        assert_eq!(ar.tick(1053), Some(KeyCode::KeyA));
        assert_eq!(ar.tick(1100), Some(KeyCode::KeyA));
    }

    #[test]
    fn auto_repeat_stops_on_matching_release_only() {
        let mut ar = AutoRepeat::new(50, 3);
        ar.on_press(KeyCode::KeyA, 0);
        ar.on_release(KeyCode::KeyB); // unrelated key
        assert_eq!(ar.tick(100), Some(KeyCode::KeyA));
        ar.on_release(KeyCode::KeyA);
        assert_eq!(ar.tick(200), None);
        assert_eq!(ar.held_key(), None);
    }

    #[test]
    fn auto_repeat_newest_key_wins() {
        let mut ar = AutoRepeat::new(50, 3);
        ar.on_press(KeyCode::KeyA, 0);
        ar.on_press(KeyCode::KeyB, 10);
        assert_eq!(ar.tick(1000), Some(KeyCode::KeyB));
        // Releasing the older key does not disturb the active repeat.
        ar.on_release(KeyCode::KeyA);
        assert_eq!(ar.tick(2000), Some(KeyCode::KeyB));
    }

    // ── pointer acceleration ──────────────────────────────────────────────

    #[test]
    fn accel_identity_below_threshold_with_unit_gain() {
        let p = PointerAccel { sensitivity_pct: 100, threshold: 6, accel_pct: 40 };
        assert_eq!(p.apply(0), 0);
        assert_eq!(p.apply(3), 3);
        assert_eq!(p.apply(-5), -5);
        assert_eq!(p.apply(6), 6);
    }

    #[test]
    fn accel_amplifies_large_deltas() {
        let p = PointerAccel { sensitivity_pct: 100, threshold: 6, accel_pct: 50 };
        // mag 10: gain = 100 + (10-6)*50 = 300 -> 10*300/100 = 30
        assert_eq!(p.apply(10), 30);
        assert_eq!(p.apply(-10), -30);
    }

    #[test]
    fn accel_sensitivity_scales_and_never_zeroes_motion() {
        let slow = PointerAccel { sensitivity_pct: 50, threshold: 100, accel_pct: 0 };
        assert_eq!(slow.apply(4), 2);
        // A 1px twitch at 50% would round to 0; clamped up to 1.
        assert_eq!(slow.apply(1), 1);
        assert_eq!(slow.apply(-1), -1);

        let fast = PointerAccel { sensitivity_pct: 250, threshold: 100, accel_pct: 0 };
        assert_eq!(fast.apply(4), 10);
    }
}
