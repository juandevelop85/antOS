//! Color representation and antOS Dark Theme palette for kernel graphics.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }

    #[inline]
    pub const fn to_u32_argb(self) -> u32 {
        ((self.a as u32) << 24) | ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    #[inline]
    pub const fn to_u32_bgr(self) -> u32 {
        ((self.a as u32) << 24) | ((self.b as u32) << 16) | ((self.g as u32) << 8) | (self.r as u32)
    }

    /// Alpha blending of `self` (source foreground) over `bg` (destination background).
    #[inline]
    pub fn blend_over(self, bg: Color) -> Color {
        if self.a == 255 {
            return self;
        }
        if self.a == 0 {
            return bg;
        }

        let alpha = self.a as u32;
        let inv_alpha = 255 - alpha;

        let r = ((self.r as u32 * alpha + bg.r as u32 * inv_alpha) / 255) as u8;
        let g = ((self.g as u32 * alpha + bg.g as u32 * inv_alpha) / 255) as u8;
        let b = ((self.b as u32 * alpha + bg.b as u32 * inv_alpha) / 255) as u8;

        Color::rgba(r, g, b, 255)
    }
}

pub mod palette {
    use super::Color;

    pub const ANTOS_BG: Color = Color::rgb(13, 17, 23);               // #0d1117 (Deep Slate Desktop)
    pub const STATUSBAR_BG: Color = Color::rgb(22, 27, 34);           // #161b22 (Top Status Bar)
    pub const STATUSBAR_BORDER: Color = Color::rgb(48, 54, 61);       // #30363d (Bar Divider)
    pub const ACCENT_BLUE: Color = Color::rgb(88, 166, 255);          // #58a6ff (Highlighted Borders / HUD)
    pub const ACCENT_CYAN: Color = Color::rgb(56, 189, 248);          // #38bdf8 (antOS Logo)
    pub const ACCENT_GREEN: Color = Color::rgb(63, 185, 80);          // #3fb950 (HAL status)
    pub const ACCENT_YELLOW: Color = Color::rgb(210, 153, 34);        // #d29922 (Heap / RAM)
    pub const ACCENT_RED: Color = Color::rgb(248, 81, 73);            // #f85149 (Close / Alert)
    pub const WINDOW_BG: Color = Color::rgb(15, 20, 27);              // #0f141b (Terminal canvas)
    pub const WINDOW_TITLEBAR: Color = Color::rgb(33, 38, 45);        // #21262d (Window Title Bar)
    pub const WINDOW_BORDER: Color = Color::rgb(48, 54, 61);          // #30363d (Inactive border)
    pub const WINDOW_BORDER_ACTIVE: Color = Color::rgb(88, 166, 255); // #58a6ff (Active window border)
    pub const TEXT_MAIN: Color = Color::rgb(201, 209, 217);           // #c9d1d9 (Primary Text)
    pub const TEXT_MUTED: Color = Color::rgb(139, 148, 158);          // #8b949e (Dimmed / Secondary)
    pub const TEXT_BRIGHT: Color = Color::rgb(240, 246, 252);         // #f0f6fc (Header text)
    pub const SHADOW_COLOR: Color = Color::rgba(0, 0, 0, 140);        // Drop shadow
}
