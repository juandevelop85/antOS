//! Floating Intent HUD component (estilo antOS-Barra).
//!
//! Provides a modal intention entry panel for entering system commands,
//! developer intents, and natural language development directives.

use super::color::palette;
use super::rect::Rect;
use super::surface::Surface;
use crate::console::font::FONT_WIDTH;

pub struct IntentHud {
    visible: bool,
    focused: bool,
    input_buf: [u8; 128],
    input_len: usize,
}

impl IntentHud {
    pub fn new() -> Self {
        IntentHud {
            visible: true,
            focused: true,
            input_buf: [0; 128],
            input_len: 0,
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub fn text(&self) -> &str {
        core::str::from_utf8(&self.input_buf[..self.input_len]).unwrap_or("")
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

    pub fn clear(&mut self) {
        self.input_len = 0;
    }

    /// Calculates the bounding rectangle of the HUD given the current screen width.
    pub fn bounds(&self, screen_w: u32) -> Rect {
        let hud_w = 660.min(screen_w.saturating_sub(40));
        let hud_h = 130;
        let hud_x = ((screen_w.saturating_sub(hud_w)) / 2) as i32;
        let hud_y = 48;
        Rect::new(hud_x, hud_y, hud_w, hud_h)
    }

    /// Renders the floating intent HUD modal onto `surface`.
    pub fn render(&self, surface: &mut Surface) {
        if !self.visible {
            return;
        }

        let hud_rect = self.bounds(surface.width());
        let hud_x = hud_rect.x;
        let hud_y = hud_rect.y;
        let hud_w = hud_rect.width;

        // 1. Drop shadow around HUD modal
        surface.draw_shadow(hud_rect, 10);

        // 2. Modal panel background (#0d1117)
        surface.fill_rect(hud_rect, palette::ANTOS_BG);

        // 3. Highlighted border outline (#58a6ff when focused, #30363d when unfocused)
        let border_color = if self.focused {
            palette::ACCENT_BLUE
        } else {
            palette::WINDOW_BORDER
        };
        surface.draw_rect_outline(hud_rect, 2, border_color);

        // 4. Header Bar
        let title_bar = Rect::new(hud_x, hud_y, hud_w, 28);
        surface.fill_rect(title_bar, palette::WINDOW_TITLEBAR);
        surface.draw_line_h(hud_x, hud_y + 27, hud_w, palette::STATUSBAR_BORDER);

        surface.draw_text(
            hud_x + 14,
            hud_y + 6,
            "HUD de Intenciones · antOS Developer Shell",
            if self.focused { palette::ACCENT_BLUE } else { palette::TEXT_MUTED },
            None,
        );

        // 5. Input Field
        let input_box = Rect::new(hud_x + 14, hud_y + 38, hud_w - 28, 44);
        surface.fill_rect(input_box, palette::WINDOW_BG);
        surface.draw_rect_outline(input_box, 1, if self.focused { palette::ACCENT_CYAN } else { palette::WINDOW_BORDER });

        let prompt = "> ";
        surface.draw_text(hud_x + 24, hud_y + 52, prompt, palette::ACCENT_CYAN, None);

        let input_text = self.text();
        let display_text = if input_text.is_empty() {
            "iniciar workspace de desarrollo --isolated --target aarch64"
        } else {
            input_text
        };

        let text_color = if input_text.is_empty() {
            palette::TEXT_MUTED
        } else {
            palette::TEXT_BRIGHT
        };

        let input_x = hud_x + 24 + (prompt.len() * FONT_WIDTH) as i32;
        surface.draw_text(input_x, hud_y + 52, display_text, text_color, None);

        // Cursor (rendered only if focused or when typing)
        if self.focused {
            let cursor_offset = if input_text.is_empty() { 0 } else { input_text.len() };
            let cursor_x = input_x + (cursor_offset * FONT_WIDTH) as i32 + 2;
            surface.draw_char(cursor_x, hud_y + 52, '_', palette::ACCENT_CYAN, None);
        }

        // 6. Footer badges / shortcuts
        let footer_y = hud_y + 94;
        let mut badge_x = hud_x + 20;

        let badges = [
            ("[Enter] Ejecutar", palette::ACCENT_GREEN),
            ("[Tab] Completar", palette::ACCENT_BLUE),
            ("[Esc] Ocultar", palette::TEXT_MUTED),
            ("[Ctrl+K] Alternar HUD", palette::ACCENT_YELLOW),
        ];

        for (label, color) in badges {
            surface.draw_text(badge_x, footer_y, label, color, None);
            badge_x += (label.len() * FONT_WIDTH + 18) as i32;
        }
    }
}
