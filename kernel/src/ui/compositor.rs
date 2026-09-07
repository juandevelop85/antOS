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
}

impl DesktopCompositor {
    pub fn new(arch_name: &'static str) -> Self {
        // Default layout positioned for 1024x768 screens
        let term_x = 48;
        let term_y = 196;
        let term_w = 928;
        let term_h = 540;

        let mut hud = IntentHud::new();
        hud.set_focused(false);

        let mut term = TerminalWindow::new(term_x, term_y, term_w, term_h);
        term.set_focused(true);

        DesktopCompositor {
            status_bar: StatusBar::new(),
            hud,
            terminal: term,
            cursor: MouseCursor::new(512, 384),
            focus: FocusTarget::Terminal,
            keyboard_state: KeyboardState::new(),
            arch_name,
            workspace_name: "[ws: default]",
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
        // 1. Clear desktop with official dark theme background (#0d1117)
        surface.clear(palette::ANTOS_BG);

        // 2. Subtle grid / pattern accent lines for sovereign desktop aesthetic
        let w = surface.width();
        let h = surface.height();
        for y in (28..h).step_by(64) {
            surface.draw_line_h(0, y as i32, w, palette::WINDOW_BG);
        }

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

/// Renders the current desktop state to the given framebuffer.
pub fn render_desktop(fb: &mut Framebuffer, heap_used: usize, heap_total: usize, ticks: u64) {
    let mut guard = COMPOSITOR.lock();
    if let Some(comp) = guard.as_mut() {
        comp.render_to_framebuffer(fb, heap_used, heap_total, ticks);
    }
}
