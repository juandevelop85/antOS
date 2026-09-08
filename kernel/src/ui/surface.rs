//! Off-screen 2D rendering surface with double-buffering (*backbuffer*),
//! clipping support, geometric drawing primitives and atomic presentation.

use super::color::{palette, Color};
use super::rect::Rect;
use crate::console::font::{self, FONT_WIDTH};
use crate::console::framebuffer::Color as FbColor;
use crate::console::Framebuffer;

pub const DEFAULT_SURFACE_WIDTH: u32 = 1920;
pub const DEFAULT_SURFACE_HEIGHT: u32 = 1080;

#[repr(align(4096))]
struct SurfaceMemory([u32; (DEFAULT_SURFACE_WIDTH * DEFAULT_SURFACE_HEIGHT) as usize]);
static mut DESKTOP_BACKBUFFER: SurfaceMemory = SurfaceMemory([0; (DEFAULT_SURFACE_WIDTH * DEFAULT_SURFACE_HEIGHT) as usize]);

/// An off-screen pixel buffer for flicker-free double buffering.
pub struct Surface {
    buffer: *mut u32,
    width: u32,
    height: u32,
    stride: u32,
    clip_rect: Rect,
}

unsafe impl Send for Surface {}
unsafe impl Sync for Surface {}

impl Surface {
    /// Creates a `Surface` backed by the global statically allocated desktop memory.
    pub fn new_desktop(width: u32, height: u32) -> Self {
        let w = width.min(DEFAULT_SURFACE_WIDTH);
        let h = height.min(DEFAULT_SURFACE_HEIGHT);
        let ptr = unsafe { core::ptr::addr_of_mut!(DESKTOP_BACKBUFFER.0) as *mut u32 };

        let mut s = Surface {
            buffer: ptr,
            width: w,
            height: h,
            stride: w,
            clip_rect: Rect::new(0, 0, w, h),
        };
        s.clear(palette::ANTOS_BG);
        s
    }

    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn set_clip(&mut self, clip: Rect) {
        let screen = Rect::new(0, 0, self.width, self.height);
        self.clip_rect = screen.intersection(&clip).unwrap_or(Rect::ZERO);
    }

    pub fn reset_clip(&mut self) {
        self.clip_rect = Rect::new(0, 0, self.width, self.height);
    }

    /// Clears the entire surface with a uniform solid color.
    pub fn clear(&mut self, color: Color) {
        let pixel = color.to_u32_argb();
        let total = (self.width * self.height) as usize;
        unsafe {
            for i in 0..total {
                *self.buffer.add(i) = pixel;
            }
        }
    }

    /// Sets a single pixel taking clipping into account.
    #[inline]
    pub fn put_pixel(&mut self, x: i32, y: i32, color: Color) {
        if x < self.clip_rect.x
            || x >= self.clip_rect.right()
            || y < self.clip_rect.y
            || y >= self.clip_rect.bottom()
        {
            return;
        }

        let idx = (y as u32 * self.stride + x as u32) as usize;
        unsafe {
            let dst_ptr = self.buffer.add(idx);
            if color.a == 255 {
                *dst_ptr = color.to_u32_argb();
            } else {
                let existing = *dst_ptr;
                let bg = Color::rgba(
                    ((existing >> 16) & 0xff) as u8,
                    ((existing >> 8) & 0xff) as u8,
                    (existing & 0xff) as u8,
                    255,
                );
                *dst_ptr = color.blend_over(bg).to_u32_argb();
            }
        }
    }

    /// Fills an axis-aligned rectangle with a solid or alpha-blended color.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        let Some(clipped) = self.clip_rect.intersection(&rect) else {
            return;
        };

        if color.a == 255 {
            let pixel = color.to_u32_argb();
            for y in clipped.y..clipped.bottom() {
                let row_offset = (y as u32 * self.stride + clipped.x as u32) as usize;
                unsafe {
                    for x in 0..clipped.width as usize {
                        *self.buffer.add(row_offset + x) = pixel;
                    }
                }
            }
        } else {
            for y in clipped.y..clipped.bottom() {
                for x in clipped.x..clipped.right() {
                    self.put_pixel(x, y, color);
                }
            }
        }
    }

    /// Draws a rectangular border outline of specified thickness.
    pub fn draw_rect_outline(&mut self, rect: Rect, thickness: u32, color: Color) {
        if rect.is_empty() || thickness == 0 {
            return;
        }

        let t = thickness as i32;

        // Top edge
        self.fill_rect(Rect::new(rect.x, rect.y, rect.width, thickness), color);
        // Bottom edge
        self.fill_rect(Rect::new(rect.x, rect.bottom() - t, rect.width, thickness), color);
        // Left edge
        self.fill_rect(Rect::new(rect.x, rect.y + t, thickness, rect.height.saturating_sub(thickness * 2)), color);
        // Right edge
        self.fill_rect(Rect::new(rect.right() - t, rect.y + t, thickness, rect.height.saturating_sub(thickness * 2)), color);
    }

    /// Draws a horizontal line segment.
    pub fn draw_line_h(&mut self, x: i32, y: i32, length: u32, color: Color) {
        self.fill_rect(Rect::new(x, y, length, 1), color);
    }

    /// Draws a vertical line segment.
    pub fn draw_line_v(&mut self, x: i32, y: i32, length: u32, color: Color) {
        self.fill_rect(Rect::new(x, y, 1, length), color);
    }

    /// Draws a subtle drop shadow under a window or modal panel.
    pub fn draw_shadow(&mut self, rect: Rect, radius: u32) {
        let r = radius as i32;
        let shadow_rect = Rect::new(rect.x + 4, rect.y + 4, rect.width + (radius * 2), rect.height + (radius * 2));
        let Some(clipped) = self.clip_rect.intersection(&shadow_rect) else {
            return;
        };

        for y in clipped.y..clipped.bottom() {
            for x in clipped.x..clipped.right() {
                if !rect.contains_point(super::rect::Point::new(x, y)) {
                    let dist_x = if x < rect.x { rect.x - x } else if x >= rect.right() { x - rect.right() + 1 } else { 0 };
                    let dist_y = if y < rect.y { rect.y - y } else if y >= rect.bottom() { y - rect.bottom() + 1 } else { 0 };
                    let dist = dist_x.max(dist_y);
                    if dist <= r {
                        let alpha = (140 * (r - dist + 1)) / (r + 1);
                        self.put_pixel(x, y, Color::rgba(0, 0, 0, alpha as u8));
                    }
                }
            }
        }
    }

    /// Renders a single bitmap glyph at coordinates `(x, y)`.
    pub fn draw_char(&mut self, x: i32, y: i32, c: char, fg: Color, bg: Option<Color>) {
        let glyph = font::get_glyph(c);

        for (row_idx, &row_byte) in glyph.iter().enumerate() {
            let py = y + row_idx as i32;

            for col_idx in 0..FONT_WIDTH {
                let px = x + col_idx as i32;
                let is_fg = (row_byte & (0x80 >> col_idx)) != 0;

                if is_fg {
                    self.put_pixel(px, py, fg);
                } else if let Some(bg_col) = bg {
                    self.put_pixel(px, py, bg_col);
                }
            }
        }
    }

    /// Renders an ASCII / UTF-8 string horizontally.
    pub fn draw_text(&mut self, mut x: i32, y: i32, text: &str, fg: Color, bg: Option<Color>) {
        for c in text.chars() {
            self.draw_char(x, y, c, fg, bg);
            x += FONT_WIDTH as i32;
        }
    }

    /// Copies the rendered off-screen backbuffer to the active physical `Framebuffer`.
    ///
    /// This atomic transfer prevents any visual tearing or flickering.
    pub fn present(&self, fb: &mut Framebuffer) {
        let copy_w = self.width.min(fb.width() as u32);
        let copy_h = self.height.min(fb.height() as u32);

        for y in 0..copy_h as usize {
            let src_row = y * self.stride as usize;
            for x in 0..copy_w as usize {
                let p = unsafe { *self.buffer.add(src_row + x) };
                let r = ((p >> 16) & 0xff) as u8;
                let g = ((p >> 8) & 0xff) as u8;
                let b = (p & 0xff) as u8;
                fb.put_pixel(x, y, FbColor::new(r, g, b));
            }
        }

        #[cfg(target_arch = "aarch64")]
        crate::arch::aarch64::virtio_gpu::flush_screen(0, 0, copy_w as usize, copy_h as usize);
    }
}
