//! UI Geometry, Color representations and Desktop Layout primitives for antOS.
//!
//! Provides pure coordinate arithmetic, rectangle intersection, clipping calculations,
//! bounding box unions, and color blending routines used by the antOS compositor.

use serde::{Deserialize, Serialize};

/// 2D Coordinate Point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const ZERO: Point = Point { x: 0, y: 0 };

    pub const fn new(x: i32, y: i32) -> Self {
        Point { x, y }
    }
}

/// 2D Dimension Size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const ZERO: Size = Size {
        width: 0,
        height: 0,
    };

    pub const fn new(width: u32, height: u32) -> Self {
        Size { width, height }
    }
}

/// 2D Axis-Aligned Rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const ZERO: Rect = Rect {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    };

    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[inline]
    pub fn right(&self) -> i32 {
        self.x.saturating_add(self.width as i32)
    }

    #[inline]
    pub fn bottom(&self) -> i32 {
        self.y.saturating_add(self.height as i32)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Checks whether `point` lies inside this rectangle (inclusive left/top, exclusive right/bottom).
    pub fn contains_point(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    /// Checks whether another rectangle intersects this one.
    pub fn intersects(&self, other: &Rect) -> bool {
        if self.is_empty() || other.is_empty() {
            return false;
        }
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    /// Computes the intersection (clipping region) between two rectangles.
    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = self.right().min(other.right());
        let y2 = self.bottom().min(other.bottom());

        if x2 > x1 && y2 > y1 {
            Some(Rect::new(x1, y1, (x2 - x1) as u32, (y2 - y1) as u32))
        } else {
            None
        }
    }

    /// Computes the minimum bounding rectangle enclosing both rectangles.
    pub fn union(&self, other: &Rect) -> Rect {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }

        let x1 = self.x.min(other.x);
        let y1 = self.y.min(other.y);
        let x2 = self.right().max(other.right());
        let y2 = self.bottom().max(other.bottom());

        Rect::new(x1, y1, (x2 - x1) as u32, (y2 - y1) as u32)
    }

    /// Insets the rectangle by `dx` and `dy`.
    pub fn inset(&self, dx: i32, dy: i32) -> Rect {
        let double_dx = (dx * 2) as u32;
        let double_dy = (dy * 2) as u32;

        let new_w = self.width.saturating_sub(double_dx);
        let new_h = self.height.saturating_sub(double_dy);

        Rect::new(self.x + dx, self.y + dy, new_w, new_h)
    }

    /// Offsets the rectangle by `(dx, dy)`.
    pub fn offset(&self, dx: i32, dy: i32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.width, self.height)
    }
}

/// RGBA Color representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

    pub const fn to_u32_argb(self) -> u32 {
        ((self.a as u32) << 24) | ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    pub const fn to_u32_bgr(self) -> u32 {
        ((self.a as u32) << 24) | ((self.b as u32) << 16) | ((self.g as u32) << 8) | (self.r as u32)
    }

    /// Blends a foreground color over a background color using standard alpha blending.
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

/// Official antOS Dark Theme Color Palette.
pub mod palette {
    use super::Color;

    pub const ANTOS_BG: Color = Color::rgb(13, 17, 23); // #0d1117 (Deep Slate)
    pub const STATUSBAR_BG: Color = Color::rgb(22, 27, 34); // #161b22 (Top Bar)
    pub const STATUSBAR_BORDER: Color = Color::rgb(48, 54, 61); // #30363d
    pub const ACCENT_BLUE: Color = Color::rgb(88, 166, 255); // #58a6ff (Primary Accent)
    pub const ACCENT_CYAN: Color = Color::rgb(56, 189, 248); // #38bdf8 (Logo Accent)
    pub const ACCENT_GREEN: Color = Color::rgb(63, 185, 80); // #3fb950 (HAL OK)
    pub const ACCENT_YELLOW: Color = Color::rgb(210, 153, 34); // #d29922 (Heap / Mem)
    pub const ACCENT_RED: Color = Color::rgb(248, 81, 73); // #f85149 (Close btn)
    pub const WINDOW_BG: Color = Color::rgb(15, 20, 27); // #0f141b (Terminal canvas)
    pub const WINDOW_TITLEBAR: Color = Color::rgb(33, 38, 45); // #21262d
    pub const WINDOW_BORDER: Color = Color::rgb(48, 54, 61); // #30363d
    pub const WINDOW_BORDER_ACTIVE: Color = Color::rgb(88, 166, 255); // #58a6ff
    pub const TEXT_MAIN: Color = Color::rgb(201, 209, 217); // #c9d1d9
    pub const TEXT_MUTED: Color = Color::rgb(139, 148, 158); // #8b949e
    pub const TEXT_BRIGHT: Color = Color::rgb(240, 246, 252); // #f0f6fc
    pub const SHADOW_COLOR: Color = Color::rgba(0, 0, 0, 140);
}

/// Metadata describing desktop layout boundaries for multi-window composition.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DesktopLayout {
    pub screen: Rect,
    pub status_bar: Rect,
    pub intent_hud: Rect,
    pub terminal_window: Rect,
}

impl DesktopLayout {
    /// Computes standard centered layout geometries for a given screen resolution.
    pub fn compute(width: u32, height: u32) -> Self {
        let screen = Rect::new(0, 0, width, height);
        let status_bar = Rect::new(0, 0, width, 28);

        // Modal Intent HUD: centered horizontally, floating near top
        let hud_w = 640.min(width.saturating_sub(40));
        let hud_h = 140.min(height.saturating_sub(80));
        let hud_x = ((width.saturating_sub(hud_w)) / 2) as i32;
        let hud_y = 56;
        let intent_hud = Rect::new(hud_x, hud_y, hud_w, hud_h);

        // Terminal window: centered in lower region of screen
        let term_y = (hud_y + hud_h as i32 + 24).min((height as i32) - 100);
        let term_w = 920.min(width.saturating_sub(40));
        let term_h = (height as i32 - term_y - 20).max(200) as u32;
        let term_x = ((width.saturating_sub(term_w)) / 2) as i32;
        let terminal_window = Rect::new(term_x, term_y, term_w, term_h);

        DesktopLayout {
            screen,
            status_bar,
            intent_hud,
            terminal_window,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_contains_point() {
        let r = Rect::new(10, 20, 100, 50);
        assert!(r.contains_point(Point::new(10, 20)));
        assert!(r.contains_point(Point::new(50, 45)));
        assert!(r.contains_point(Point::new(109, 69)));
        assert!(!r.contains_point(Point::new(110, 70))); // Exclusive right/bottom
        assert!(!r.contains_point(Point::new(9, 20)));
        assert!(!r.contains_point(Point::new(50, 19)));
    }

    #[test]
    fn test_rect_intersection_and_clipping() {
        let r1 = Rect::new(10, 10, 50, 50);
        let r2 = Rect::new(30, 30, 50, 50);

        assert!(r1.intersects(&r2));
        let inter = r1.intersection(&r2).expect("Must intersect");
        assert_eq!(inter, Rect::new(30, 30, 30, 30));

        let non_overlapping = Rect::new(100, 100, 20, 20);
        assert!(!r1.intersects(&non_overlapping));
        assert!(r1.intersection(&non_overlapping).is_none());
    }

    #[test]
    fn test_rect_union_bounding_box() {
        let r1 = Rect::new(10, 20, 30, 40); // right: 40, bottom: 60
        let r2 = Rect::new(25, 35, 45, 50); // right: 70, bottom: 85

        let u = r1.union(&r2);
        assert_eq!(u, Rect::new(10, 20, 60, 65));
    }

    #[test]
    fn test_rect_inset_and_offset() {
        let r = Rect::new(20, 30, 100, 80);
        let inset = r.inset(5, 10);
        assert_eq!(inset, Rect::new(25, 40, 90, 60));

        let off = r.offset(15, -10);
        assert_eq!(off, Rect::new(35, 20, 100, 80));
    }

    #[test]
    fn test_color_alpha_blending() {
        let black = Color::rgb(0, 0, 0);
        let _white = Color::rgb(255, 255, 255);
        let semi_white = Color::rgba(255, 255, 255, 128);

        let blended = semi_white.blend_over(black);
        // Expect approximately 128 for RGB
        assert!((blended.r as i32 - 128).abs() <= 1);
        assert!((blended.g as i32 - 128).abs() <= 1);
        assert!((blended.b as i32 - 128).abs() <= 1);
        assert_eq!(blended.a, 255);
    }

    #[test]
    fn test_desktop_layout_computation() {
        let layout = DesktopLayout::compute(1024, 768);
        assert_eq!(layout.screen, Rect::new(0, 0, 1024, 768));
        assert_eq!(layout.status_bar, Rect::new(0, 0, 1024, 28));

        // HUD must be centered horizontally
        assert_eq!(layout.intent_hud.width, 640);
        assert_eq!(layout.intent_hud.x, ((1024 - 640) / 2) as i32);
        assert_eq!(layout.intent_hud.y, 56);

        // Terminal window must fit within screen bounds
        assert!(layout.terminal_window.right() <= 1024);
        assert!(layout.terminal_window.bottom() <= 768);
        assert!(layout.terminal_window.y > layout.intent_hud.bottom());
    }
}
