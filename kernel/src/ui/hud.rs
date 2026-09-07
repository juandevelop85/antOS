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
}

impl IntentHud {
    pub fn new() -> Self {
        IntentHud { visible: true }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Renders the floating intent HUD modal onto `surface`.
    pub fn render(&self, surface: &mut Surface, current_input: &str) {
        if !self.visible {
            return;
        }

        let screen_w = surface.width();
        let hud_w = 660.min(screen_w.saturating_sub(40));
        let hud_h = 130;
        let hud_x = ((screen_w.saturating_sub(hud_w)) / 2) as i32;
        let hud_y = 48;

        let hud_rect = Rect::new(hud_x, hud_y, hud_w, hud_h);

        // 1. Drop shadow around HUD modal
        surface.draw_shadow(hud_rect, 10);

        // 2. Modal panel background (#0d1117)
        surface.fill_rect(hud_rect, palette::ANTOS_BG);

        // 3. Highlighted border outline (#58a6ff, 2px)
        surface.draw_rect_outline(hud_rect, 2, palette::ACCENT_BLUE);

        // 4. Header Bar
        let title_bar = Rect::new(hud_x, hud_y, hud_w, 28);
        surface.fill_rect(title_bar, palette::WINDOW_TITLEBAR);
        surface.draw_line_h(hud_x, hud_y + 27, hud_w, palette::STATUSBAR_BORDER);

        surface.draw_text(
            hud_x + 14,
            hud_y + 6,
            "HUD de Intenciones · antOS Developer Shell",
            palette::ACCENT_BLUE,
            None,
        );

        // 5. Input Field
        let input_box = Rect::new(hud_x + 14, hud_y + 38, hud_w - 28, 44);
        surface.fill_rect(input_box, palette::WINDOW_BG);
        surface.draw_rect_outline(input_box, 1, palette::WINDOW_BORDER);

        let prompt = "> ";
        surface.draw_text(hud_x + 24, hud_y + 52, prompt, palette::ACCENT_CYAN, None);

        let input_text = if current_input.is_empty() {
            "iniciar workspace de desarrollo --isolated --target aarch64"
        } else {
            current_input
        };

        let text_color = if current_input.is_empty() {
            palette::TEXT_MUTED
        } else {
            palette::TEXT_BRIGHT
        };

        let input_x = hud_x + 24 + (prompt.len() * FONT_WIDTH) as i32;
        surface.draw_text(input_x, hud_y + 52, input_text, text_color, None);

        // Cursor
        let cursor_x = input_x + (input_text.len() * FONT_WIDTH) as i32 + 2;
        surface.draw_char(cursor_x, hud_y + 52, '_', palette::ACCENT_CYAN, None);

        // 6. Footer badges / shortcuts
        let footer_y = hud_y + 94;
        let mut badge_x = hud_x + 20;

        let badges = [
            ("[Enter] Ejecutar", palette::ACCENT_GREEN),
            ("[Tab] Completar", palette::ACCENT_BLUE),
            ("[Esc] Ocultar", palette::TEXT_MUTED),
            ("[Ctrl+P] Comandos", palette::ACCENT_YELLOW),
        ];

        for (label, color) in badges {
            surface.draw_text(badge_x, footer_y, label, color, None);
            badge_x += (label.len() * FONT_WIDTH + 18) as i32;
        }
    }
}
