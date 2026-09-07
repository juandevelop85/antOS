//! 2D Desktop Shell Compositor for antOS bare-metal graphics.
//!
//! Orchestrates the multi-layered rendering of background wallpaper,
//! status bar, embedded terminal window, and floating intent HUD
//! with double buffering to eliminate screen tearing.

use super::color::palette;
use super::hud::IntentHud;
use super::statusbar::StatusBar;
use super::surface::Surface;
use super::terminal_window::TerminalWindow;
use crate::console::Framebuffer;
use crate::sync::SpinLock;

pub struct DesktopCompositor {
    status_bar: StatusBar,
    hud: IntentHud,
    terminal: TerminalWindow,
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

        DesktopCompositor {
            status_bar: StatusBar::new(),
            hud: IntentHud::new(),
            terminal: TerminalWindow::new(term_x, term_y, term_w, term_h),
            arch_name,
            workspace_name: "[ws: default]",
        }
    }

    pub fn set_workspace(&mut self, ws: &'static str) {
        self.workspace_name = ws;
    }

    pub fn hud_mut(&mut self) -> &mut IntentHud {
        &mut self.hud
    }

    pub fn terminal_mut(&mut self) -> &mut TerminalWindow {
        &mut self.terminal
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

        // 3. Render Terminal Window
        self.terminal.render(surface);

        // 4. Render Floating Intent HUD (overlays terminal)
        self.hud.render(surface, "");

        // 5. Render Top Status Bar (pinned at top layer)
        self.status_bar.render(
            surface,
            self.arch_name,
            self.workspace_name,
            "Terminal 1 · antOS Sovereign Shell",
            heap_used,
            heap_total,
            ticks,
        );
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

/// Renders the current desktop state to the given framebuffer.
pub fn render_desktop(fb: &mut Framebuffer, heap_used: usize, heap_total: usize, ticks: u64) {
    let mut guard = COMPOSITOR.lock();
    if let Some(comp) = guard.as_mut() {
        comp.render_to_framebuffer(fb, heap_used, heap_total, ticks);
    }
}
