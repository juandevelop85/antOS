//! Embedded Terminal Window component for antOS Sovereign Desktop.
//!
//! Provides a desktop terminal window with title bar, window controls,
//! drop shadow, and ANSI/VT100 monospace text rendering.

use super::color::palette;
use super::rect::Rect;
use super::surface::Surface;
use crate::console::font::{FONT_HEIGHT, FONT_WIDTH};

pub struct TerminalWindow {
    rect: Rect,
    title: &'static str,
}

impl TerminalWindow {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        TerminalWindow {
            rect: Rect::new(x, y, width, height),
            title: "Terminal 1 · antOS Sovereign Shell (EL0)",
        }
    }

    pub fn rect(&self) -> Rect {
        self.rect
    }

    pub fn set_rect(&mut self, rect: Rect) {
        self.rect = rect;
    }

    /// Renders the terminal window and its content lines onto `surface`.
    pub fn render(&self, surface: &mut Surface) {
        let r = self.rect;

        // 1. Drop shadow
        surface.draw_shadow(r, 12);

        // 2. Window background canvas (#0f141b)
        surface.fill_rect(r, palette::WINDOW_BG);

        // 3. Window border (#30363d or active accent)
        surface.draw_rect_outline(r, 1, palette::WINDOW_BORDER);

        // 4. Title Bar (#21262d, 28px height)
        let title_bar = Rect::new(r.x, r.y, r.width, 28);
        surface.fill_rect(title_bar, palette::WINDOW_TITLEBAR);
        surface.draw_line_h(r.x, r.y + 27, r.width, palette::STATUSBAR_BORDER);

        // Window control buttons (Left side macOS/Unix style or Right side)
        // Red close button dot
        let dot_y = r.y + 9;
        let mut btn_x = r.x + 14;

        // [x] Close
        surface.draw_text(btn_x, dot_y, "[x]", palette::ACCENT_RED, None);
        btn_x += (3 * FONT_WIDTH + 8) as i32;

        // [-] Minimize
        surface.draw_text(btn_x, dot_y, "[-]", palette::ACCENT_YELLOW, None);
        btn_x += (3 * FONT_WIDTH + 8) as i32;

        // [+] Maximize
        surface.draw_text(btn_x, dot_y, "[+]", palette::ACCENT_GREEN, None);

        // Title text centered or next to controls
        let title_x = r.x + (14 + 18 * FONT_WIDTH) as i32;
        surface.draw_text(title_x, r.y + 6, self.title, palette::TEXT_BRIGHT, None);

        // 5. Terminal Text Area
        let mut line_y = r.y + 38;
        let line_spacing = FONT_HEIGHT as i32 + 3;
        let left_padding = r.x + 16;

        let lines = [
            ("antOS Kernel 0.1.0-baremetal · AArch64 / x86_64 Dual Architecture", palette::ACCENT_BLUE),
            ("HAL Status: MMU 4KiB active · GICv2 distributor online · Virtual Timer 100Hz", palette::TEXT_MUTED),
            ("Storage Subsystem: VirtIO-blk / devfs / tarfs ramdisk mounted at /", palette::TEXT_MUTED),
            ("Graphics: Framebuffer linear renderer active · VirtIO-GPU 1024x768x32bpp", palette::ACCENT_CYAN),
            ("Userspace: Supervisor EL1 -> User EL0 transition verified cleanly via SVC", palette::ACCENT_GREEN),
            ("------------------------------------------------------------------------", palette::STATUSBAR_BORDER),
            ("antOS:~$ antos status --verbose", palette::TEXT_BRIGHT),
            ("  [ok] Sistema Operativo Soberano listo para desarrollo nativo.", palette::ACCENT_GREEN),
            ("  [ok] Compositor 2D y Desktop Shell inicializados con doble buffer.", palette::ACCENT_CYAN),
            ("antOS:~$ _", palette::ACCENT_CYAN),
        ];

        for (text, color) in lines {
            if line_y + (FONT_HEIGHT as i32) < r.bottom() - 8 {
                surface.draw_text(left_padding, line_y, text, color, None);
                line_y += line_spacing;
            }
        }
    }
}
