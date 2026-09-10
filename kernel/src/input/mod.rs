//! antOS Kernel Input Subsystem.
//!
//! Provides typed input events, modifier tracking, ASCII decoding,
//! circular lock-protected event queue, and scancode decoders.

pub mod ergonomics;
pub mod queue;
pub use ergonomics::{AutoRepeat, KeyboardLayout, PointerAccel};
pub use queue::*;

use core::sync::atomic::{AtomicU8, Ordering};

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
    NumLock,
    ScrollLock,

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
    /// Right Alt held — the AltGr third-level shift, tracked apart from `alt`.
    pub altgr: bool,
    pub super_key: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
    pub scroll_lock: bool,
}

impl Modifiers {
    pub const fn empty() -> Self {
        Self {
            shift: false,
            ctrl: false,
            alt: false,
            altgr: false,
            super_key: false,
            caps_lock: false,
            num_lock: false,
            scroll_lock: false,
        }
    }
}

/// Maintains active keyboard state, modifiers, and converts key presses to characters.
#[derive(Debug, Clone, Default)]
pub struct KeyboardState {
    pub modifiers: Modifiers,
    /// A dead key awaiting its base letter (Spanish layout accents).
    pending_dead: Option<ergonomics::DeadKey>,
}

impl KeyboardState {
    pub const fn new() -> Self {
        Self {
            modifiers: Modifiers::empty(),
            pending_dead: None,
        }
    }

    /// Updates internal modifier states on key press or release.
    pub fn update(&mut self, event: &InputEvent) {
        match *event {
            InputEvent::KeyPress(key) => match key {
                KeyCode::LeftShift | KeyCode::RightShift => self.modifiers.shift = true,
                KeyCode::LeftCtrl | KeyCode::RightCtrl => self.modifiers.ctrl = true,
                KeyCode::LeftAlt => self.modifiers.alt = true,
                KeyCode::RightAlt => {
                    self.modifiers.alt = true;
                    self.modifiers.altgr = true;
                }
                KeyCode::LeftSuper | KeyCode::RightSuper => self.modifiers.super_key = true,
                KeyCode::CapsLock => {
                    self.modifiers.caps_lock = !self.modifiers.caps_lock;
                    publish_led_state(self.modifiers);
                }
                KeyCode::NumLock => {
                    self.modifiers.num_lock = !self.modifiers.num_lock;
                    publish_led_state(self.modifiers);
                }
                KeyCode::ScrollLock => {
                    self.modifiers.scroll_lock = !self.modifiers.scroll_lock;
                    publish_led_state(self.modifiers);
                }
                _ => {}
            },
            InputEvent::KeyRelease(key) => match key {
                KeyCode::LeftShift | KeyCode::RightShift => self.modifiers.shift = false,
                KeyCode::LeftCtrl | KeyCode::RightCtrl => self.modifiers.ctrl = false,
                KeyCode::LeftAlt => self.modifiers.alt = false,
                KeyCode::RightAlt => {
                    self.modifiers.alt = false;
                    self.modifiers.altgr = false;
                }
                KeyCode::LeftSuper | KeyCode::RightSuper => self.modifiers.super_key = false,
                _ => {}
            },
            _ => {}
        }
    }

    /// Converts a key code to a character for the active keyboard layout,
    /// resolving Spanish dead-key accent sequences across two presses.
    pub fn key_to_char(&mut self, key: KeyCode) -> Option<char> {
        use ergonomics::KeyResolve;
        let layout = keyboard_layout();
        let m = self.modifiers;
        let resolved = ergonomics::resolve_key(layout, key, m.shift, m.caps_lock, m.altgr);

        if let Some(dead) = self.pending_dead.take() {
            return match resolved {
                KeyResolve::Char(base) => {
                    Some(ergonomics::combine_dead(dead, base).unwrap_or(base))
                }
                // A second dead key or a non-printing key: emit the standalone
                // accent glyph and let this press resolve on the next call.
                _ => {
                    if let KeyResolve::Dead(next) = resolved {
                        self.pending_dead = Some(next);
                    }
                    Some(ergonomics::dead_key_glyph(dead))
                }
            };
        }

        match resolved {
            KeyResolve::Char(c) => Some(c),
            KeyResolve::Dead(dead) => {
                self.pending_dead = Some(dead);
                None
            }
            KeyResolve::None => None,
        }
    }
}

/// Tracks modifier state (shift/caps/ctrl) for the ASCII bridge below. Shared
/// by whichever driver fed `GLOBAL_INPUT_QUEUE` — PS/2 (x86_64) or
/// VirtIO-Input (AArch64) — since both funnel through the same typed queue.
static ASCII_BRIDGE_STATE: crate::sync::SpinLock<KeyboardState> =
    crate::sync::SpinLock::new(KeyboardState::new());

#[cfg(target_arch = "aarch64")]
#[inline]
fn render_compositor_if_active(comp: &mut crate::ui::DesktopCompositor) {
    if let Some(c) = crate::console::CONSOLE.lock().as_mut() {
        comp.present_best(
            c.framebuffer_mut(),
            crate::allocator::used(),
            crate::AARCH64_HEAP_SIZE,
            crate::arch::aarch64::timer::ticks(),
        );
    }
}

#[cfg(not(target_arch = "aarch64"))]
#[inline]
fn render_compositor_if_active(comp: &mut crate::ui::DesktopCompositor) {
    #[cfg(target_arch = "x86_64")]
    if let Some(c) = crate::console::CONSOLE.lock().as_mut() {
        comp.present_best(
            c.framebuffer_mut(),
            crate::allocator::used(),
            crate::memory::HEAP_SIZE,
            crate::task::timer::ticks(),
        );
    }
}

// ── Runtime input settings (layout, pointer curve, typematic timing) ────────

/// Live, mutable input ergonomics configuration. Written by `/etc/antos.conf`
/// parsing at boot and by the settings syscall; read by the decode/cursor path.
#[derive(Debug, Clone, Copy)]
pub struct InputSettings {
    pub layout: KeyboardLayout,
    pub pointer: PointerAccel,
    pub repeat_delay_ticks: u64,
    pub repeat_interval_ticks: u64,
}

impl InputSettings {
    pub const DEFAULT: Self = Self {
        layout: KeyboardLayout::Us,
        pointer: PointerAccel::DEFAULT,
        // 100 Hz timer: ~500 ms initial delay, ~30 repeats/s.
        repeat_delay_ticks: 50,
        repeat_interval_ticks: 3,
    };
}

pub static INPUT_SETTINGS: crate::sync::SpinLock<InputSettings> =
    crate::sync::SpinLock::new(InputSettings::DEFAULT);

static AUTO_REPEAT: crate::sync::SpinLock<AutoRepeat> =
    crate::sync::SpinLock::new(AutoRepeat::new(50, 3));

/// LED bitmap the keyboard *should* show, and the last one actually pushed to
/// the devices. `0xFF` forces the first real sync.
static KEYBOARD_LEDS_DESIRED: AtomicU8 = AtomicU8::new(0);
static KEYBOARD_LEDS_SYNCED: AtomicU8 = AtomicU8::new(0xFF);

/// Current keyboard layout.
pub fn keyboard_layout() -> KeyboardLayout {
    INPUT_SETTINGS.lock().layout
}

/// Selects the active keyboard layout at runtime (no reboot).
pub fn set_keyboard_layout(layout: KeyboardLayout) {
    INPUT_SETTINGS.lock().layout = layout;
}

/// Current pointer acceleration curve.
pub fn pointer_accel() -> PointerAccel {
    INPUT_SETTINGS.lock().pointer
}

/// Adjusts the global pointer sensitivity in percent (`100` = 1.0x). The change
/// takes effect on the next relative motion.
pub fn set_pointer_sensitivity(pct: u32) {
    INPUT_SETTINGS.lock().pointer.sensitivity_pct = pct.clamp(10, 1000);
}

/// Applies `key = value` lines from `/etc/antos.conf` that concern input:
/// `keyboard_layout`, `pointer_sensitivity`, `repeat_delay_ms`, `repeat_rate_hz`.
pub fn apply_config(conf: &str) {
    let mut s = INPUT_SETTINGS.lock();
    for line in conf.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "keyboard_layout" => {
                if let Some(l) = KeyboardLayout::from_name(value) {
                    s.layout = l;
                }
            }
            "pointer_sensitivity" => {
                if let Ok(v) = value.parse::<u32>() {
                    s.pointer.sensitivity_pct = v.clamp(10, 1000);
                }
            }
            "pointer_accel" => {
                if let Ok(v) = value.parse::<u32>() {
                    s.pointer.accel_pct = v.min(1000);
                }
            }
            "repeat_delay_ms" => {
                if let Ok(v) = value.parse::<u64>() {
                    s.repeat_delay_ticks = (v / 10).max(1);
                }
            }
            "repeat_rate_hz" => {
                if let Ok(v) = value.parse::<u64>() {
                    s.repeat_interval_ticks = (100 / v.max(1)).max(1);
                }
            }
            _ => {}
        }
    }
}

/// Recomputes the desired keyboard-LED bitmap from the lock latches.
fn publish_led_state(m: Modifiers) {
    let bitmap = ergonomics::led_bitmap(m.num_lock, m.caps_lock, m.scroll_lock);
    KEYBOARD_LEDS_DESIRED.store(bitmap, Ordering::Relaxed);
}

/// Pushes the keyboard-LED state to every keyboard (USB HID and VirtIO-Input)
/// when it has changed. Cheap no-op otherwise. Safe to call from any poll loop;
/// must not hold `ASCII_BRIDGE_STATE`.
pub fn sync_keyboard_leds() {
    let desired = KEYBOARD_LEDS_DESIRED.load(Ordering::Relaxed);
    if desired == KEYBOARD_LEDS_SYNCED.swap(desired, Ordering::Relaxed) {
        return;
    }
    #[cfg(target_arch = "aarch64")]
    crate::drivers::virtio_input::set_keyboard_leds(
        desired & ergonomics::led_bit::CAPS_LOCK != 0,
        desired & ergonomics::led_bit::NUM_LOCK != 0,
        desired & ergonomics::led_bit::SCROLL_LOCK != 0,
    );
    crate::drivers::usb::set_keyboard_leds(desired);
}

/// Whether a key participates in typematic auto-repeat (everything except bare
/// modifiers and the lock keys).
fn is_repeatable(key: KeyCode) -> bool {
    !matches!(
        key,
        KeyCode::LeftShift
            | KeyCode::RightShift
            | KeyCode::LeftCtrl
            | KeyCode::RightCtrl
            | KeyCode::LeftAlt
            | KeyCode::RightAlt
            | KeyCode::LeftSuper
            | KeyCode::RightSuper
            | KeyCode::CapsLock
            | KeyCode::NumLock
            | KeyCode::ScrollLock
    )
}

/// Feeds a key event into the auto-repeat state machine.
fn note_key_for_repeat(event: &InputEvent, now: u64) {
    match *event {
        InputEvent::KeyPress(key) if is_repeatable(key) => {
            AUTO_REPEAT.lock().on_press(key, now);
        }
        InputEvent::KeyRelease(key) => {
            AUTO_REPEAT.lock().on_release(key);
        }
        _ => {}
    }
}

/// Emits a synthetic `KeyPress` for the held key when a typematic repeat is due.
/// Call from the input poll and/or the timer tick.
pub fn service_auto_repeat(now: u64) {
    let repeat = {
        let mut ar = AUTO_REPEAT.lock();
        let s = INPUT_SETTINGS.lock();
        ar.set_timing(s.repeat_delay_ticks, s.repeat_interval_ticks);
        ar.tick(now)
    };
    if let Some(key) = repeat {
        push_event(InputEvent::KeyPress(key));
    }
}

/// Last input-event total announced by [`poll_rx_report`].
static RX_REPORT_LAST: AtomicU8 = AtomicU8::new(0);
static RX_REPORT_TOTAL: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Emits a terminal line the first time input events arrive and whenever the
/// running total grows, so a headless smoke test (T28.10) can confirm the
/// kernel received injected keystrokes / mouse motion. Cheap no-op otherwise.
pub fn poll_rx_report() {
    // Once the graphical desktop owns the framebuffer, any `println!` corrupts
    // it — and a moving pointer would flood the log with one line per event.
    // So: announce the very first event (enough for the T28.10 smoke test to
    // detect input), then stay silent unless we are still on the text console,
    // where a coarse every-100-events heartbeat is harmless.
    let total = queue::total_events();
    let prev = RX_REPORT_TOTAL.swap(total, Ordering::Relaxed);
    if total <= prev {
        return;
    }
    let first = RX_REPORT_LAST.swap(1, Ordering::Relaxed) == 0;
    if first {
        crate::println!("input-rx: primer evento recibido · total={}", total);
        return;
    }
    if !crate::ui::compositor::is_desktop_active() && total / 100 != prev / 100 {
        crate::println!("input-rx: total={}", total);
    }
}

/// Current tick count on whichever timer this architecture runs.
#[inline]
fn now_ticks() -> u64 {
    #[cfg(target_arch = "aarch64")]
    {
        crate::arch::aarch64::timer::ticks()
    }
    #[cfg(target_arch = "x86_64")]
    {
        crate::task::timer::ticks()
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        0
    }
}

/// Human-readable snapshot of the input ergonomics settings for `SYS_SYSINFO`.
pub fn settings_report() -> alloc::string::String {
    use core::fmt::Write as _;
    let s = *INPUT_SETTINGS.lock();
    let leds = KEYBOARD_LEDS_DESIRED.load(Ordering::Relaxed);
    let mut out = alloc::string::String::new();
    let _ = write!(
        out,
        "input: layout={} sens={}% accel={}%/u thr={} repeat={}t/{}t leds={:03b}",
        s.layout.name(),
        s.pointer.sensitivity_pct,
        s.pointer.accel_pct,
        s.pointer.threshold,
        s.repeat_delay_ticks,
        s.repeat_interval_ticks,
        leds
    );
    out
}

/// Drains as many queued key-press events as fit in `buf`, decoding them to
/// UTF-8 via [`KeyboardState::key_to_char`]. Non-blocking: returns `0`
/// immediately if nothing is queued. Used by `SYS_READ` (T26.5) so the
/// userspace shell can poll `fd 0` the same way on every architecture.
pub fn drain_ascii(buf: &mut [u8]) -> usize {
    crate::drivers::usb::poll();
    poll_rx_report();
    let now = now_ticks();
    service_auto_repeat(now);
    let mut state = ASCII_BRIDGE_STATE.lock();
    let mut written = 0usize;
    let mut desktop_dirty = false;
    let (screen_w, screen_h) = crate::console::resolution();
    #[cfg(target_arch = "aarch64")]
    crate::drivers::virtio_input::poll_virtio_inputs(screen_w, screen_h);
    while written < buf.len() {
        let Some(event) = pop_event() else { break };
        state.update(&event);
        note_key_for_repeat(&event, now);

        if crate::ui::compositor::is_desktop_active() {
            let mut comp_guard = crate::ui::COMPOSITOR.lock();
            if let Some(comp) = comp_guard.as_mut() {
                match event {
                    InputEvent::MouseMove { .. }
                    | InputEvent::MouseAbsolute { .. }
                    | InputEvent::MouseButtonPress(_)
                    | InputEvent::MouseButtonRelease(_)
                    | InputEvent::Scroll { .. } => {
                        if comp.handle_event(event, screen_w, screen_h) {
                            desktop_dirty = true;
                        }
                        continue;
                    }
                    InputEvent::KeyPress(key) => {
                        if key == KeyCode::LeftSuper
                            || key == KeyCode::RightSuper
                            || (key == KeyCode::KeyK && comp.keyboard_state().modifiers.ctrl)
                            || (key == KeyCode::Escape && comp.hud().is_visible())
                        {
                            if comp.handle_event(event, screen_w, screen_h) {
                                desktop_dirty = true;
                            }
                            continue;
                        }

                        if comp.focus() == crate::ui::FocusTarget::Hud {
                            if comp.handle_event(event, screen_w, screen_h) {
                                desktop_dirty = true;
                            }
                            continue;
                        }
                    }
                    _ => {}
                }
            }
        }

        if let InputEvent::KeyPress(key) = event {
            if let Some(ch) = state.key_to_char(key) {
                let mut utf8 = [0u8; 4];
                for &byte in ch.encode_utf8(&mut utf8).as_bytes() {
                    if written < buf.len() {
                        buf[written] = byte;
                        written += 1;
                    }
                }
            }
        }
    }

    drop(state);
    sync_keyboard_leds();

    if desktop_dirty {
        let mut comp_guard = crate::ui::COMPOSITOR.lock();
        if let Some(comp) = comp_guard.as_mut() {
            render_compositor_if_active(comp);
        }
    }

    written
}

/// Linux Input Subsystem Event Types.
pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_REL: u16 = 0x02;
pub const EV_ABS: u16 = 0x03;
pub const EV_LED: u16 = 0x11;

/// `EV_SYN` codes.
pub const SYN_REPORT: u16 = 0x00;

pub const REL_X: u16 = 0x00;
pub const REL_Y: u16 = 0x01;
pub const REL_WHEEL: u16 = 0x08;
pub const REL_HWHEEL: u16 = 0x06;

pub const ABS_X: u16 = 0x00;
pub const ABS_Y: u16 = 0x01;

pub const BTN_LEFT: u16 = 0x110;
pub const BTN_RIGHT: u16 = 0x111;
pub const BTN_MIDDLE: u16 = 0x112;
pub const BTN_SIDE: u16 = 0x113;
pub const BTN_EXTRA: u16 = 0x114;

/// `EV_LED` codes (used on the VirtIO-Input status queue).
pub const LED_NUML: u16 = 0x00;
pub const LED_CAPSL: u16 = 0x01;
pub const LED_SCROLLL: u16 = 0x02;

/// Decodes a single Linux `EV_KEY` code/value into the matching `InputEvent`
/// (mouse button transitions or key press/release). `value` follows evdev
/// semantics: `0` = release, `1` = press, `2` = auto-repeat (treated as press).
pub fn decode_key_or_button(code: u16, value: u32) -> Option<InputEvent> {
    let pressed = value != 0;
    let button = match code {
        BTN_LEFT => Some(MouseButton::Left),
        BTN_RIGHT => Some(MouseButton::Right),
        BTN_MIDDLE => Some(MouseButton::Middle),
        BTN_SIDE => Some(MouseButton::Other(3)),
        BTN_EXTRA => Some(MouseButton::Other(4)),
        _ => None,
    };

    if let Some(btn) = button {
        return Some(if pressed {
            InputEvent::MouseButtonPress(btn)
        } else {
            InputEvent::MouseButtonRelease(btn)
        });
    }

    let key = linux_code_to_key(code);
    Some(if pressed {
        InputEvent::KeyPress(key)
    } else {
        InputEvent::KeyRelease(key)
    })
}

/// Decodes Linux `EV_*` events (standard VirtIO-Input format) into a single
/// `InputEvent`. Kept for callers that want an event-at-a-time view; the
/// coalescing [`EvdevAccumulator`] is the path VirtIO-Input actually uses,
/// since relative motion and absolute position must be aggregated per
/// `SYN_REPORT` packet rather than emitted axis-by-axis.
pub fn decode_linux_ev(event_type: u16, code: u16, value: u32) -> Option<InputEvent> {
    match event_type {
        EV_KEY => decode_key_or_button(code, value),
        EV_REL => {
            let val = value as i32;
            match code {
                REL_X => Some(InputEvent::MouseMove { dx: val, dy: 0 }),
                REL_Y => Some(InputEvent::MouseMove { dx: 0, dy: val }),
                REL_WHEEL => Some(InputEvent::Scroll {
                    delta_x: 0,
                    delta_y: val,
                }),
                REL_HWHEEL => Some(InputEvent::Scroll {
                    delta_x: val,
                    delta_y: 0,
                }),
                _ => None,
            }
        }
        EV_ABS => match code {
            ABS_X => Some(InputEvent::MouseAbsolute { x: value, y: 0 }),
            ABS_Y => Some(InputEvent::MouseAbsolute { x: 0, y: value }),
            _ => None,
        },
        _ => None,
    }
}

/// Calibration for one absolute axis, taken from a VirtIO-Input
/// `VIRTIO_INPUT_CFG_ABS_INFO` block. Defaults to the 0..32767 range that
/// QEMU's `virtio-tablet` and the VirtualBox USB tablet both report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbsAxisInfo {
    pub min: i32,
    pub max: i32,
}

impl Default for AbsAxisInfo {
    fn default() -> Self {
        Self { min: 0, max: 32767 }
    }
}

impl AbsAxisInfo {
    /// Maps a raw absolute reading onto `0..screen_dim` pixels, clamped.
    pub fn to_screen(&self, raw: i32, screen_dim: u32) -> u32 {
        if screen_dim == 0 {
            return 0;
        }
        let span = (self.max - self.min).max(1) as i64;
        let rel = i64::from((raw - self.min).max(0)).min(span);
        ((rel * i64::from(screen_dim - 1)) / span) as u32
    }
}

/// Aggregates the Linux evdev stream of one device between `SYN_REPORT`
/// barriers. Relative motion, wheel deltas and absolute position each surface
/// as a *single* coalesced `InputEvent` per packet; key and button
/// transitions pass through in order. This mirrors how evdev groups a
/// complete input state change between synchronisation points and prevents
/// the cursor from jumping to `(x, 0)` then `(0, y)` on absolute devices.
#[derive(Default)]
pub struct EvdevAccumulator {
    keys: alloc::vec::Vec<InputEvent>,
    rel_dx: i32,
    rel_dy: i32,
    wheel: i32,
    hwheel: i32,
    abs_x: Option<i32>,
    abs_y: Option<i32>,
    last_abs_x: i32,
    last_abs_y: i32,
    pub abs_x_info: AbsAxisInfo,
    pub abs_y_info: AbsAxisInfo,
}

impl EvdevAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one raw evdev triple. Returns the coalesced events when the
    /// triple is a `SYN_REPORT`; otherwise `None` while it keeps aggregating.
    /// `screen` is the current framebuffer resolution, used to scale absolute
    /// coordinates.
    pub fn feed(
        &mut self,
        event_type: u16,
        code: u16,
        value: u32,
        screen: (u32, u32),
    ) -> Option<alloc::vec::Vec<InputEvent>> {
        match event_type {
            EV_KEY => {
                if let Some(ev) = decode_key_or_button(code, value) {
                    self.keys.push(ev);
                }
                None
            }
            EV_REL => {
                let v = value as i32;
                match code {
                    REL_X => self.rel_dx += v,
                    REL_Y => self.rel_dy += v,
                    REL_WHEEL => self.wheel += v,
                    REL_HWHEEL => self.hwheel += v,
                    _ => {}
                }
                None
            }
            EV_ABS => {
                match code {
                    ABS_X => self.abs_x = Some(value as i32),
                    ABS_Y => self.abs_y = Some(value as i32),
                    _ => {}
                }
                None
            }
            EV_SYN if code == SYN_REPORT => Some(self.flush(screen)),
            _ => None,
        }
    }

    fn flush(&mut self, screen: (u32, u32)) -> alloc::vec::Vec<InputEvent> {
        let mut out = core::mem::take(&mut self.keys);

        if self.rel_dx != 0 || self.rel_dy != 0 {
            out.push(InputEvent::MouseMove {
                dx: self.rel_dx,
                dy: self.rel_dy,
            });
        }
        if self.wheel != 0 || self.hwheel != 0 {
            out.push(InputEvent::Scroll {
                delta_x: self.hwheel,
                delta_y: self.wheel,
            });
        }
        if self.abs_x.is_some() || self.abs_y.is_some() {
            let rx = self.abs_x.unwrap_or(self.last_abs_x);
            let ry = self.abs_y.unwrap_or(self.last_abs_y);
            self.last_abs_x = rx;
            self.last_abs_y = ry;
            // Emit the canonical 0..=32767 range that `InputEvent::MouseAbsolute`
            // carries (the same the USB-HID path normalises to). `Cursor::move_abs`
            // scales that to screen pixels; scaling to pixels *here* as well
            // double-applied it and squashed a full sweep into ~1/32 of the
            // screen. `to_screen(_, 32768)` yields `raw * 32767 / span`.
            let _ = screen;
            out.push(InputEvent::MouseAbsolute {
                x: self.abs_x_info.to_screen(rx, 32768),
                y: self.abs_y_info.to_screen(ry, 32768),
            });
        }

        self.rel_dx = 0;
        self.rel_dy = 0;
        self.wheel = 0;
        self.hwheel = 0;
        self.abs_x = None;
        self.abs_y = None;
        out
    }
}

/// Encodes an `EV_LED` evdev triple for the VirtIO-Input status queue.
/// `on` lights the LED, `!on` clears it.
pub fn encode_led_event(led: u16, on: bool) -> (u16, u16, u32) {
    (EV_LED, led, if on { 1 } else { 0 })
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
        69 => KeyCode::NumLock,
        70 => KeyCode::ScrollLock,
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

#[cfg(test)]
mod evdev_tests {
    use super::*;

    const SCREEN: (u32, u32) = (1280, 720);

    #[test_case]
    fn abs_axis_info_scales_and_clamps() {
        let info = AbsAxisInfo { min: 0, max: 32767 };
        assert_eq!(info.to_screen(0, 1280), 0);
        assert_eq!(info.to_screen(32767, 1280), 1279);
        assert_eq!(info.to_screen(16384, 1280), 639); // ~midpoint
        assert_eq!(info.to_screen(-50, 1280), 0); // below min -> clamped
        assert_eq!(info.to_screen(999_999, 1280), 1279); // above max -> clamped
        assert_eq!(info.to_screen(100, 0), 0); // zero screen dimension
    }

    #[test_case]
    fn abs_axis_info_honours_nonzero_min() {
        let info = AbsAxisInfo {
            min: 100,
            max: 1124,
        }; // span 1024
        assert_eq!(info.to_screen(100, 1025), 0);
        assert_eq!(info.to_screen(1124, 1025), 1024);
        assert_eq!(info.to_screen(612, 1025), 512);
    }

    #[test_case]
    fn key_and_button_transitions() {
        assert_eq!(
            decode_key_or_button(BTN_LEFT, 1),
            Some(InputEvent::MouseButtonPress(MouseButton::Left))
        );
        assert_eq!(
            decode_key_or_button(BTN_LEFT, 0),
            Some(InputEvent::MouseButtonRelease(MouseButton::Left))
        );
        assert_eq!(
            decode_key_or_button(BTN_EXTRA, 1),
            Some(InputEvent::MouseButtonPress(MouseButton::Other(4)))
        );
        // KEY_A == 30 in the Linux keymap
        assert_eq!(
            decode_key_or_button(30, 1),
            Some(InputEvent::KeyPress(KeyCode::KeyA))
        );
        // value 2 is auto-repeat -> still a press
        assert_eq!(
            decode_key_or_button(30, 2),
            Some(InputEvent::KeyPress(KeyCode::KeyA))
        );
        assert_eq!(
            decode_key_or_button(30, 0),
            Some(InputEvent::KeyRelease(KeyCode::KeyA))
        );
    }

    #[test_case]
    fn accumulator_coalesces_relative_motion_on_syn() {
        let mut acc = EvdevAccumulator::new();
        assert!(acc.feed(EV_REL, REL_X, 5i32 as u32, SCREEN).is_none());
        assert!(acc.feed(EV_REL, REL_Y, (-3i32) as u32, SCREEN).is_none());
        assert!(acc.feed(EV_REL, REL_X, 2i32 as u32, SCREEN).is_none());
        let out = acc
            .feed(EV_SYN, SYN_REPORT, 0, SCREEN)
            .expect("syn flushes");
        assert_eq!(out, alloc::vec![InputEvent::MouseMove { dx: 7, dy: -3 }]);

        // Deltas reset after the flush.
        let out2 = acc
            .feed(EV_SYN, SYN_REPORT, 0, SCREEN)
            .expect("syn flushes");
        assert!(out2.is_empty());
    }

    #[test_case]
    fn accumulator_emits_single_absolute_per_packet() {
        // T31.16: este test nunca había compilado (era `#[test]` sin arnés
        // no_std) hasta ahora, y quedó desincronizado con el comentario que
        // hay justo encima de `feed`: `MouseAbsolute` lleva el rango
        // canónico 0..=32767, nunca píxeles de pantalla — escalar aquí
        // TAMBIÉN duplicaba el escalado que ya hace `Cursor::move_abs`. La
        // aserción original esperaba `x: 1279` (el resultado de escalar dos
        // veces); la corrección es esperar el valor canónico sin escalar.
        let mut acc = EvdevAccumulator::new();
        acc.abs_x_info = AbsAxisInfo { min: 0, max: 32767 };
        acc.abs_y_info = AbsAxisInfo { min: 0, max: 32767 };

        assert!(acc.feed(EV_ABS, ABS_X, 32767, SCREEN).is_none());
        assert!(acc.feed(EV_ABS, ABS_Y, 0, SCREEN).is_none());
        let out = acc
            .feed(EV_SYN, SYN_REPORT, 0, SCREEN)
            .expect("syn flushes");
        assert_eq!(
            out,
            alloc::vec![InputEvent::MouseAbsolute { x: 32767, y: 0 }]
        );
    }

    #[test_case]
    fn accumulator_reuses_last_axis_when_packet_updates_only_one() {
        // T31.16: mismo motivo que el test de arriba — `feed` ya no escala
        // al tamaño de pantalla que se le pasa (de ahí que estas llamadas
        // usen `(1001, 1001)`, un valor que la implementación actual ignora
        // por completo), sino al rango canónico 0..=32767. Los valores
        // esperados son `AbsAxisInfo { min: 0, max: 1000 }.to_screen(raw,
        // 32768)` para cada `raw` fed abajo.
        let mut acc = EvdevAccumulator::new();
        acc.abs_x_info = AbsAxisInfo { min: 0, max: 1000 };
        acc.abs_y_info = AbsAxisInfo { min: 0, max: 1000 };

        acc.feed(EV_ABS, ABS_X, 500, (1001, 1001));
        acc.feed(EV_ABS, ABS_Y, 200, (1001, 1001));
        let first = acc.feed(EV_SYN, SYN_REPORT, 0, (1001, 1001)).unwrap();
        assert_eq!(
            first,
            alloc::vec![InputEvent::MouseAbsolute { x: 16383, y: 6553 }]
        );

        // Second packet only reports a new X; Y must carry over.
        acc.feed(EV_ABS, ABS_X, 750, (1001, 1001));
        let second = acc.feed(EV_SYN, SYN_REPORT, 0, (1001, 1001)).unwrap();
        assert_eq!(
            second,
            alloc::vec![InputEvent::MouseAbsolute { x: 24575, y: 6553 }]
        );
    }

    #[test_case]
    fn accumulator_orders_keys_then_motion_then_scroll() {
        let mut acc = EvdevAccumulator::new();
        acc.feed(EV_KEY, BTN_LEFT, 1, SCREEN);
        acc.feed(EV_REL, REL_X, 4i32 as u32, SCREEN);
        acc.feed(EV_REL, REL_WHEEL, 1i32 as u32, SCREEN);
        acc.feed(EV_REL, REL_HWHEEL, (-1i32) as u32, SCREEN);
        let out = acc.feed(EV_SYN, SYN_REPORT, 0, SCREEN).unwrap();
        assert_eq!(
            out,
            alloc::vec![
                InputEvent::MouseButtonPress(MouseButton::Left),
                InputEvent::MouseMove { dx: 4, dy: 0 },
                InputEvent::Scroll {
                    delta_x: -1,
                    delta_y: 1
                },
            ]
        );
    }

    #[test_case]
    fn led_event_encoding() {
        assert_eq!(encode_led_event(LED_CAPSL, true), (EV_LED, LED_CAPSL, 1));
        assert_eq!(encode_led_event(LED_NUML, false), (EV_LED, LED_NUML, 0));
    }

    #[test_case]
    fn config_parsing_updates_input_settings() {
        apply_config(
            "# comment\nkeyboard_layout = es\npointer_sensitivity=175\nrepeat_delay_ms=300\nrepeat_rate_hz=25\nunknown=1\n",
        );
        let s = *INPUT_SETTINGS.lock();
        assert_eq!(s.layout, KeyboardLayout::Es);
        assert_eq!(s.pointer.sensitivity_pct, 175);
        assert_eq!(s.repeat_delay_ticks, 30); // 300 ms / 10 ms per tick
        assert_eq!(s.repeat_interval_ticks, 4); // 100 Hz / 25 Hz
                                                // Restore the default so other tests see a clean layout.
        apply_config("keyboard_layout=us\npointer_sensitivity=100\n");
    }

    #[test_case]
    fn led_bitmap_tracks_lock_latches() {
        let mut m = Modifiers::empty();
        m.caps_lock = true;
        publish_led_state(m);
        assert_eq!(
            KEYBOARD_LEDS_DESIRED.load(Ordering::Relaxed),
            ergonomics::led_bit::CAPS_LOCK
        );
        m.num_lock = true;
        publish_led_state(m);
        assert_eq!(
            KEYBOARD_LEDS_DESIRED.load(Ordering::Relaxed),
            ergonomics::led_bit::CAPS_LOCK | ergonomics::led_bit::NUM_LOCK
        );
        publish_led_state(Modifiers::empty());
    }
}
