//! Embedded Terminal Window component for antOS Sovereign Desktop.
//!
//! Provides a desktop terminal window with title bar, window controls,
//! drop shadow, interactive prompt input buffer, and ANSI monospace text rendering.

use super::color::palette;
use super::rect::Rect;
use super::surface::Surface;
use crate::console::font::{FONT_HEIGHT, FONT_WIDTH};

pub struct TerminalWindow {
    rect: Rect,
    title: &'static str,
    focused: bool,
    input_buf: [u8; 128],
    input_len: usize,
    last_command_buf: [u8; 128],
    last_command_len: usize,
}

impl TerminalWindow {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        TerminalWindow {
            rect: Rect::new(x, y, width, height),
            title: "Terminal 1 · antOS Sovereign Shell (EL0)",
            focused: false,
            input_buf: [0; 128],
            input_len: 0,
            last_command_buf: [0; 128],
            last_command_len: 0,
        }
    }

    pub fn rect(&self) -> Rect {
        self.rect
    }

    pub fn set_rect(&mut self, rect: Rect) {
        self.rect = rect;
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub fn input_text(&self) -> &str {
        core::str::from_utf8(&self.input_buf[..self.input_len]).unwrap_or("")
    }

    pub fn last_command(&self) -> &str {
        core::str::from_utf8(&self.last_command_buf[..self.last_command_len]).unwrap_or("")
    }

    pub fn insert_char(&mut self, c: char) {
        if self.input_len < self.input_buf.len() && c.is_ascii() {
            self.input_buf[self.input_len] = c as u8;
            self.input_len += 1;
        }
    }

    pub fn delete_char(&mut self) {
        if self.input_len > 0 {
            self.input_len -= 1;
        }
    }

    pub fn submit_command(&mut self) {
        if self.input_len > 0 {
            self.last_command_buf[..self.input_len].copy_from_slice(&self.input_buf[..self.input_len]);
            self.last_command_len = self.input_len;
            self.input_len = 0;
        }
    }

    /// Renders the terminal window and its content lines onto `surface`.
    pub fn render(&self, surface: &mut Surface) {
        let r = self.rect;

        // 1. Drop shadow
        surface.draw_shadow(r, 12);

        // 2. Window background canvas (#0f141b)
        surface.fill_rect(r, palette::WINDOW_BG);

        // 3. Window border (#58a6ff when focused, #30363d when unfocused)
        let border_color = if self.focused {
            palette::ACCENT_BLUE
        } else {
            palette::WINDOW_BORDER
        };
        surface.draw_rect_outline(r, 1, border_color);

        // 4. Title Bar (#21262d, 28px height)
        let title_bar = Rect::new(r.x, r.y, r.width, 28);
        surface.fill_rect(title_bar, palette::WINDOW_TITLEBAR);
        surface.draw_line_h(r.x, r.y + 27, r.width, palette::STATUSBAR_BORDER);

        // Window control buttons [x] [-] [+]
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

        // Title text
        let title_x = r.x + (14 + 18 * FONT_WIDTH) as i32;
        let title_color = if self.focused {
            palette::TEXT_BRIGHT
        } else {
            palette::TEXT_MUTED
        };
        surface.draw_text(title_x, r.y + 6, self.title, title_color, None);

        // 5. Terminal Text Area
        let mut line_y = r.y + 38;
        let line_spacing = FONT_HEIGHT as i32 + 3;
        let left_padding = r.x + 16;

        let lines = [
            ("antOS Kernel 0.1.0-baremetal · AArch64 / x86_64 Dual Architecture", palette::ACCENT_BLUE),
            ("HAL Status: MMU 4KiB active · GICv2 online · VirtIO-Input ready", palette::TEXT_MUTED),
            ("Graphics: Framebuffer linear renderer active · VirtIO-GPU 1024x768x32bpp", palette::ACCENT_CYAN),
            ("Input Subsystem: VirtIO-Input / PS/2 keyboard & mouse event queue active", palette::ACCENT_GREEN),
            ("------------------------------------------------------------------------", palette::STATUSBAR_BORDER),
            ("antOS:~$ antos status --verbose", palette::TEXT_BRIGHT),
            ("  [ok] Sistema Operativo Soberano listo para desarrollo nativo.", palette::ACCENT_GREEN),
            ("  [ok] Compositor 2D, cursor interactivo y enrutador de foco activos.", palette::ACCENT_CYAN),
        ];

        for (text, color) in lines {
            if line_y + (FONT_HEIGHT as i32) < r.bottom() - 36 {
                surface.draw_text(left_padding, line_y, text, color, None);
                line_y += line_spacing;
            }
        }

        // Show last executed command if present
        let last_cmd = self.last_command();
        if !last_cmd.is_empty() && line_y + (FONT_HEIGHT as i32) < r.bottom() - 36 {
            surface.draw_text(left_padding, line_y, "  [exec] ", palette::ACCENT_YELLOW, None);
            surface.draw_text(left_padding + 9 * FONT_WIDTH as i32, line_y, last_cmd, palette::TEXT_BRIGHT, None);
            line_y += line_spacing;
        }

        // Interactive prompt line
        if line_y + (FONT_HEIGHT as i32) < r.bottom() - 8 {
            let prompt = "antOS:~$ ";
            surface.draw_text(left_padding, line_y, prompt, palette::ACCENT_CYAN, None);

            let input_str = self.input_text();
            let text_x = left_padding + (prompt.len() * FONT_WIDTH) as i32;
            surface.draw_text(text_x, line_y, input_str, palette::TEXT_BRIGHT, None);

            // Cursor in prompt line
            let cursor_x = text_x + (input_str.len() * FONT_WIDTH) as i32;
            let cursor_color = if self.focused {
                palette::ACCENT_GREEN
            } else {
                palette::TEXT_MUTED
            };
            surface.draw_char(cursor_x, line_y, '_', cursor_color, None);
        }
    }
}
