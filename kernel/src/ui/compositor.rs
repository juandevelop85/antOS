//! 2D Desktop Shell Compositor for antOS bare-metal graphics.
//!
//! Orchestrates the multi-layered rendering of background wallpaper,
//! status bar, embedded terminal window, floating intent HUD, and mouse cursor
//! with double buffering to eliminate screen tearing.
//! Routes user input events and manages focus between HUD and Terminal.

use core::sync::atomic::{AtomicBool, Ordering};
use super::color::palette;
use super::cursor::MouseCursor;
use super::hud::IntentHud;
use super::rect::Rect;
use super::statusbar::StatusBar;
use super::surface::Surface;
use super::terminal_window::TerminalWindow;
use crate::console::Framebuffer;
use crate::input::{pop_event, InputEvent, KeyCode, KeyboardState, MouseButton};
use crate::sync::SpinLock;

/// Tracks whether the graphical Desktop Compositor session is currently active.
static DESKTOP_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Returns true if the Desktop Compositor is currently active.
#[inline]
pub fn is_desktop_active() -> bool {
    DESKTOP_ACTIVE.load(Ordering::Relaxed)
}

/// Sets the Desktop Compositor active state.
#[inline]
pub fn set_desktop_active(active: bool) {
    DESKTOP_ACTIVE.store(active, Ordering::Relaxed);
}

/// Represents which UI component currently owns keyboard input focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    None,
    Hud,
    Terminal,
}

pub struct DesktopCompositor {
    status_bar: StatusBar,
    hud: IntentHud,
    terminal: TerminalWindow,
    cursor: MouseCursor,
    focus: FocusTarget,
    keyboard_state: KeyboardState,
    arch_name: &'static str,
    workspace_name: &'static str,
    /// `true` when the next present must repaint the whole screen (terminal
    /// output, HUD, focus or button-state change). Cleared after a full render.
    /// While it stays `false`, a pointer motion only repaints the small
    /// rectangle it swept over — see [`DesktopCompositor::present_best`].
    full_redraw_pending: bool,
    /// Screen rectangle the cursor sprite occupied at the last present, so the
    /// fast path knows which stale pixels to repaint.
    last_cursor_bounds: Rect,
}

impl DesktopCompositor {
    pub fn new(arch_name: &'static str) -> Self {
        let (screen_w, screen_h) = crate::console::resolution();
        let term_x = 48;
        let term_y = 180;
        let term_w = (screen_w.saturating_sub(96)).max(640);
        let term_h = (screen_h.saturating_sub(210)).max(300);

        let mut hud = IntentHud::new();
        hud.set_focused(false);

        let mut term = TerminalWindow::new(term_x, term_y, term_w, term_h);
        term.set_focused(true);

        let cursor = MouseCursor::new((screen_w / 2) as i32, (screen_h / 2) as i32);
        let last_cursor_bounds = cursor.bounds();

        DesktopCompositor {
            status_bar: StatusBar::new(),
            hud,
            terminal: term,
            cursor,
            focus: FocusTarget::Terminal,
            keyboard_state: KeyboardState::new(),
            arch_name,
            workspace_name: "[ws: default]",
            full_redraw_pending: true,
            last_cursor_bounds,
        }
    }

    pub fn set_workspace(&mut self, ws: &'static str) {
        self.workspace_name = ws;
    }

    pub fn cursor(&self) -> &MouseCursor {
        &self.cursor
    }

    pub fn cursor_mut(&mut self) -> &mut MouseCursor {
        &mut self.cursor
    }

    pub fn focus(&self) -> FocusTarget {
        self.focus
    }

    pub fn set_focus(&mut self, target: FocusTarget) {
        self.focus = target;
        self.hud.set_focused(target == FocusTarget::Hud);
        self.terminal.set_focused(target == FocusTarget::Terminal);
    }

    pub fn keyboard_state(&self) -> &KeyboardState {
        &self.keyboard_state
    }

    pub fn hud(&self) -> &IntentHud {
        &self.hud
    }

    pub fn hud_mut(&mut self) -> &mut IntentHud {
        &mut self.hud
    }

    pub fn terminal(&self) -> &TerminalWindow {
        &self.terminal
    }

    pub fn terminal_mut(&mut self) -> &mut TerminalWindow {
        &mut self.terminal
    }

    /// Dispatches an input event to the compositor, moving the cursor or routing keys.
    pub fn handle_event(&mut self, event: InputEvent, screen_w: u32, screen_h: u32) -> bool {
        // Anything other than a bare pointer motion can change pixels outside
        // the cursor sprite (button colour, focus ring, HUD, terminal scroll),
        // so it forces the next present to be a full-screen repaint.
        if !matches!(
            event,
            InputEvent::MouseMove { .. } | InputEvent::MouseAbsolute { .. }
        ) {
            self.full_redraw_pending = true;
        }

        match event {
            InputEvent::MouseMove { dx, dy } => {
                self.cursor.move_rel(dx, dy, screen_w as i32, screen_h as i32);
                true
            }
            InputEvent::MouseAbsolute { x, y } => {
                self.cursor.move_abs(x, y, screen_w as i32, screen_h as i32);
                true
            }
            InputEvent::MouseButtonPress(MouseButton::Left) => {
                self.cursor.left_pressed = true;
                let pt = self.cursor.point();

                // Hit-testing priority: HUD modal (if visible) -> Terminal Window -> Background
                if self.hud.is_visible() && self.hud.bounds(screen_w).contains_point(pt) {
                    self.set_focus(FocusTarget::Hud);
                } else if self.terminal.rect().contains_point(pt) {
                    self.set_focus(FocusTarget::Terminal);
                } else {
                    if self.hud.is_visible() {
                        self.set_focus(FocusTarget::None);
                    }
                }
                true
            }
            InputEvent::MouseButtonRelease(MouseButton::Left) => {
                self.cursor.left_pressed = false;
                true
            }
            InputEvent::MouseButtonPress(MouseButton::Right) => {
                self.cursor.right_pressed = true;
                true
            }
            InputEvent::MouseButtonRelease(MouseButton::Right) => {
                self.cursor.right_pressed = false;
                true
            }
            InputEvent::KeyPress(key) => {
                self.keyboard_state.update(&event);

                // Global shortcut: Super or Ctrl+K toggles Intent HUD
                if key == KeyCode::LeftSuper
                    || key == KeyCode::RightSuper
                    || (key == KeyCode::KeyK && self.keyboard_state.modifiers.ctrl)
                {
                    self.hud.toggle();
                    if self.hud.is_visible() {
                        self.set_focus(FocusTarget::Hud);
                    } else {
                        self.set_focus(FocusTarget::Terminal);
                    }
                    return true;
                }

                // Global shortcut: Escape closes HUD and transfers focus to Terminal
                if key == KeyCode::Escape && self.hud.is_visible() {
                    self.hud.set_visible(false);
                    self.set_focus(FocusTarget::Terminal);
                    return true;
                }

                // Route keystrokes to the focused component
                match self.focus {
                    FocusTarget::Hud => {
                        if key == KeyCode::Backspace {
                            self.hud.delete_char();
                        } else if key == KeyCode::Enter {
                            let text = self.hud.text();
                            if !text.is_empty() {
                                let _ = crate::ipc::channel::send_message(1, 0, text.as_bytes());
                                self.terminal.write_str("\n\x1b[1;36m[agent] intención encolada en canal IPC: \"\x1b[1;37m");
                                self.terminal.write_str(text);
                                self.terminal.write_str("\x1b[1;36m\"\x1b[0m\n");
                                self.hud.clear();
                            }
                        } else if let Some(ch) = self.keyboard_state.key_to_char(key) {
                            self.hud.insert_char(ch);
                        }
                        true
                    }
                    FocusTarget::Terminal => {
                        // Terminal keystrokes are returned through drain_ascii to SYS_READ
                        // and echoed back by the shell via SYS_WRITE -> terminal.write_str
                        false
                    }
                    FocusTarget::None => false,
                }
            }
            InputEvent::KeyRelease(_key) => {
                self.keyboard_state.update(&event);
                true
            }
            InputEvent::Scroll { delta_y, .. } => {
                if delta_y > 0 {
                    self.terminal.scroll_up();
                } else if delta_y < 0 {
                    self.terminal.scroll_down();
                }
                true
            }
            _ => false,
        }
    }


    /// Renders the complete desktop environment into `surface`.
    pub fn render(
        &mut self,
        surface: &mut Surface,
        heap_used: usize,
        heap_total: usize,
        ticks: u64,
    ) {
        // 1. Clear desktop with official dark theme background (#0d1117).
        //    `fill_rect` honours the active clip, so when this pass runs inside
        //    the cursor-move fast path only the damage rectangle is repainted.
        let w = surface.width();
        let h = surface.height();
        surface.fill_rect(Rect::new(0, 0, w, h), palette::ANTOS_BG);

        // 2. Subtle grid / pattern accent lines for sovereign desktop aesthetic
        for y in (28..h).step_by(64) {
            surface.draw_line_h(0, y as i32, w, palette::WINDOW_BG);
        }

        // Dynamically fit terminal window to surface dimensions
        let term_w = (w.saturating_sub(96)).max(640);
        let term_h = (h.saturating_sub(210)).max(300);
        self.terminal.set_rect(Rect::new(48, 180, term_w, term_h));

        // 3. Render Terminal Window (Layer 1)
        self.terminal.render(surface);

        // 4. Render Floating Intent HUD (Layer 2, overlays terminal)
        self.hud.render(surface);

        // 5. Render Top Status Bar (Layer 3, pinned at top layer)
        let focused_title = match self.focus {
            FocusTarget::Hud => "HUD de Intenciones · antOS Developer Shell",
            FocusTarget::Terminal => "Terminal 1 · antOS Sovereign Shell",
            FocusTarget::None => "antOS Sovereign Desktop",
        };

        self.status_bar.render(
            surface,
            self.arch_name,
            self.workspace_name,
            focused_title,
            heap_used,
            heap_total,
            ticks,
        );

        // 6. Render Mouse Cursor (Layer 4, top-most interactive overlay)
        self.cursor.render(surface);
    }

    /// Renders the desktop environment to the backbuffer and atomically presents it to `fb`.
    pub fn render_to_framebuffer(
        &mut self,
        fb: &mut Framebuffer,
        heap_used: usize,
        heap_total: usize,
        ticks: u64,
    ) {
        let mut surface = Surface::new_desktop(fb.width() as u32, fb.height() as u32);
        self.render(&mut surface, heap_used, heap_total, ticks);
        surface.present(fb);
        self.full_redraw_pending = false;
        self.last_cursor_bounds = self.cursor.bounds();
    }

    /// Presents the smallest update that still shows the current state: a full
    /// repaint when something outside the pointer changed, otherwise just the
    /// rectangle the cursor swept over since the last present (plus the pinned
    /// status bar band, so its clock keeps advancing).
    ///
    /// A full-screen software composite runs on every input event on the
    /// AArch64 path; at 1024x768 that is ~2 M scalar pixel ops per frame in the
    /// debug profile, which is what made the pointer feel sluggish. A pointer
    /// motion now touches ~a thousandth of that.
    pub fn present_best(
        &mut self,
        fb: &mut Framebuffer,
        heap_used: usize,
        heap_total: usize,
        ticks: u64,
    ) {
        let screen_w = fb.width() as u32;
        let screen_h = fb.height() as u32;
        let cursor_now = self.cursor.bounds();
        let swept = self.last_cursor_bounds.union(&cursor_now);

        // Grow the damage a couple of pixels each way (the sprite has a 1px
        // outline) and staple on the status-bar band so the clock still ticks.
        let damage = Rect::new(
            (swept.x - 2).max(0),
            0,
            (swept.width + 4).min(screen_w),
            (swept.bottom() as u32 + 2).min(screen_h),
        );

        let full_area = (screen_w as u64) * (screen_h as u64);
        let damage_area = (damage.width as u64) * (damage.height as u64);
        // A partial (span) blit into a framebuffer whose row pitch is padded
        // (VirtualBox GOP) has been observed to smear the cursor trail a few
        // pixels vertically; a full-frame present is exact, so fall back to it
        // there. Tightly-packed framebuffers (QEMU, UTM) keep the fast path.
        let padded_pitch = fb.stride_pixels() != fb.width();
        let must_full =
            self.full_redraw_pending || padded_pitch || damage_area * 2 >= full_area;

        if must_full {
            self.render_to_framebuffer(fb, heap_used, heap_total, ticks);
            return;
        }

        let mut surface = Surface::new_desktop(screen_w, screen_h);
        surface.set_clip(damage);
        self.render(&mut surface, heap_used, heap_total, ticks);
        surface.reset_clip();
        surface.present_rect(fb, damage);
        self.last_cursor_bounds = cursor_now;
    }
}

pub static COMPOSITOR: SpinLock<Option<DesktopCompositor>> = SpinLock::new(None);

/// Initializes the global desktop compositor with the active architecture tag.
pub fn init(arch_name: &'static str) {
    let comp = DesktopCompositor::new(arch_name);
    *COMPOSITOR.lock() = Some(comp);
}

/// Drains pending events from the global input queue and dispatches them to the compositor.
pub fn dispatch_pending_inputs(screen_w: u32, screen_h: u32) -> bool {
    let mut modified = false;
    let mut guard = COMPOSITOR.lock();
    if let Some(comp) = guard.as_mut() {
        while let Some(event) = pop_event() {
            if comp.handle_event(event, screen_w, screen_h) {
                modified = true;
            }
        }
    }
    modified
}

/// Renders the current desktop state to the given framebuffer, choosing a full
/// repaint or a cursor-only damage update as appropriate.
pub fn render_desktop(fb: &mut Framebuffer, heap_used: usize, heap_total: usize, ticks: u64) {
    let mut guard = COMPOSITOR.lock();
    if let Some(comp) = guard.as_mut() {
        comp.present_best(fb, heap_used, heap_total, ticks);
    }
}
