//! Mouse cursor representation and rendering for antOS Desktop Shell.
//!
//! Provides a 12x18 arrow pointer with dark outline and bright fill,
//! maintaining subpixel/integer coordinates, button states, and
//! rendering directly onto a `Surface` with boundary clipping.

use super::color::{palette, Color};
use super::rect::{Point, Rect};
use super::surface::Surface;

pub const CURSOR_WIDTH: usize = 12;
pub const CURSOR_HEIGHT: usize = 18;

// 12x18 pointer bitmap pattern:
// '.' = transparent, '#' = dark border outline, 'X' = bright fill
const CURSOR_BITMAP: [&str; CURSOR_HEIGHT] = [
    "#...........",
    "##..........",
    "#X#.........",
    "#XX#........",
    "#XXX#.......",
    "#XXXX#......",
    "#XXXXX#.....",
    "#XXXXXX#....",
    "#XXXXXXX#...",
    "#XXXXXXXX#..",
    "#XXXXX#####.",
    "#XX#XX#.....",
    "#X#.#XX#....",
    "##..#XX#....",
    "#....#XX#...",
    ".....#XX#...",
    "......##....",
    "............",
];

/// State of the interactive mouse cursor on the screen.
#[derive(Debug, Clone, Copy)]
pub struct MouseCursor {
    pub x: i32,
    pub y: i32,
    pub left_pressed: bool,
    pub right_pressed: bool,
    pub middle_pressed: bool,
}

impl MouseCursor {
    /// Creates a cursor initialized at the specified screen coordinates.
    pub const fn new(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            left_pressed: false,
            right_pressed: false,
            middle_pressed: false,
        }
    }

    /// Returns the current point coordinate of the cursor hotspot.
    #[inline]
    pub fn point(&self) -> Point {
        Point::new(self.x, self.y)
    }

    /// Displaces the cursor by relative amounts `(dx, dy)` after applying the
    /// global pointer sensitivity / acceleration curve, clamping to
    /// `(0..max_w, 0..max_h)`. Absolute positioning ([`Self::move_abs`]) is
    /// deliberately left untouched by the curve.
    pub fn move_rel(&mut self, dx: i32, dy: i32, max_w: i32, max_h: i32) {
        let accel = crate::input::pointer_accel();
        let dx = accel.apply(dx);
        let dy = accel.apply(dy);
        self.x = (self.x + dx).clamp(0, (max_w - 1).max(0));
        self.y = (self.y + dy).clamp(0, (max_h - 1).max(0));
    }

    /// Sets the cursor from absolute input coordinates (e.g. tablet device 0..32767).
    pub fn move_abs(&mut self, abs_x: u32, abs_y: u32, max_w: i32, max_h: i32) {
        if max_w > 0 && max_h > 0 {
            // Scale from 0..32767 to screen dimensions
            let sx = ((abs_x as u64 * max_w as u64) / 32768) as i32;
            let sy = ((abs_y as u64 * max_h as u64) / 32768) as i32;
            self.x = sx.clamp(0, max_w - 1);
            self.y = sy.clamp(0, max_h - 1);
        }
    }

    /// Bounding rectangle of the cursor sprite on the screen.
    pub fn bounds(&self) -> Rect {
        Rect::new(self.x, self.y, CURSOR_WIDTH as u32, CURSOR_HEIGHT as u32)
    }

    /// Renders the mouse pointer arrow onto the target `surface`.
    pub fn render(&self, surface: &mut Surface) {
        let outline_color = Color::rgb(1, 4, 9);
        let fill_color = if self.left_pressed {
            palette::ACCENT_CYAN
        } else {
            palette::TEXT_BRIGHT
        };

        for (row, line) in CURSOR_BITMAP.iter().enumerate() {
            let py = self.y + row as i32;
            for (col, ch) in line.chars().enumerate() {
                let px = self.x + col as i32;
                match ch {
                    '#' => surface.put_pixel(px, py, outline_color),
                    'X' => surface.put_pixel(px, py, fill_color),
                    _ => {}
                }
            }
        }
    }
}
