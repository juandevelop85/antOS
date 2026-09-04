//! Linear framebuffer abstraction and primitive drawing operations.
//!
//! Supports 24/32-bit RGB/BGR and 8-bit grayscale pixel layouts with
//! bounds-checked pixel manipulation, fast memory scrolling, and character rendering.

use bootloader_api::info::{FrameBufferInfo, PixelFormat};
use super::font::{self, FONT_WIDTH};

/// 24-bit RGB Color.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0 };
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255 };
    pub const RED: Color = Color { r: 205, g: 49, b: 49 };
    pub const GREEN: Color = Color { r: 13, g: 188, b: 121 };
    pub const YELLOW: Color = Color { r: 229, g: 229, b: 16 };
    pub const BLUE: Color = Color { r: 36, g: 114, b: 200 };
    pub const MAGENTA: Color = Color { r: 188, g: 63, b: 188 };
    pub const CYAN: Color = Color { r: 17, g: 168, b: 205 };
    pub const LIGHT_GRAY: Color = Color { r: 204, g: 204, b: 204 };
    pub const DARK_GRAY: Color = Color { r: 102, g: 102, b: 102 };
    pub const BRIGHT_RED: Color = Color { r: 241, g: 76, b: 76 };
    pub const BRIGHT_GREEN: Color = Color { r: 35, g: 209, b: 139 };
    pub const BRIGHT_YELLOW: Color = Color { r: 245, g: 245, b: 67 };
    pub const BRIGHT_BLUE: Color = Color { r: 59, g: 142, b: 234 };
    pub const BRIGHT_MAGENTA: Color = Color { r: 214, g: 112, b: 214 };
    pub const BRIGHT_CYAN: Color = Color { r: 41, g: 184, b: 219 };
    pub const BRIGHT_WHITE: Color = Color { r: 255, g: 255, b: 255 };

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b }
    }

    /// Returns high-intensity (bold/bright) variant of standard colors.
    pub fn to_bright(self) -> Self {
        match self {
            Self::BLACK => Self::DARK_GRAY,
            Self::RED => Self::BRIGHT_RED,
            Self::GREEN => Self::BRIGHT_GREEN,
            Self::YELLOW => Self::BRIGHT_YELLOW,
            Self::BLUE => Self::BRIGHT_BLUE,
            Self::MAGENTA => Self::BRIGHT_MAGENTA,
            Self::CYAN => Self::BRIGHT_CYAN,
            Self::LIGHT_GRAY => Self::BRIGHT_WHITE,
            other => Color {
                r: other.r.saturating_add(50),
                g: other.g.saturating_add(50),
                b: other.b.saturating_add(50),
            },
        }
    }
}

/// Linear framebuffer driver.
pub struct Framebuffer {
    buffer: *mut u8,
    buffer_len: usize,
    width: usize,
    height: usize,
    stride: usize,
    bytes_per_pixel: usize,
    pixel_format: PixelFormat,
}

unsafe impl Send for Framebuffer {}

impl Framebuffer {
    /// Constructs a new `Framebuffer` from a raw memory buffer and bootloader metadata.
    ///
    /// # Safety
    /// `buffer` must point to valid framebuffer memory of at least `buffer_len` bytes.
    pub unsafe fn new(buffer: *mut u8, buffer_len: usize, info: FrameBufferInfo) -> Self {
        Framebuffer {
            buffer,
            buffer_len,
            width: info.width,
            height: info.height,
            stride: info.stride,
            bytes_per_pixel: info.bytes_per_pixel,
            pixel_format: info.pixel_format,
        }
    }

    #[inline]
    pub fn width(&self) -> usize {
        self.width
    }

    #[inline]
    pub fn height(&self) -> usize {
        self.height
    }

    /// Sets a single pixel to the specified color with bounds checking.
    #[inline]
    pub fn put_pixel(&mut self, x: usize, y: usize, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }

        let pixel_offset = (y * self.stride + x) * self.bytes_per_pixel;
        if pixel_offset + self.bytes_per_pixel > self.buffer_len {
            return;
        }

        unsafe {
            let ptr = self.buffer.add(pixel_offset);
            match self.pixel_format {
                PixelFormat::Rgb => {
                    *ptr = color.r;
                    *ptr.add(1) = color.g;
                    *ptr.add(2) = color.b;
                    if self.bytes_per_pixel >= 4 {
                        *ptr.add(3) = 0xFF;
                    }
                }
                PixelFormat::Bgr => {
                    *ptr = color.b;
                    *ptr.add(1) = color.g;
                    *ptr.add(2) = color.r;
                    if self.bytes_per_pixel >= 4 {
                        *ptr.add(3) = 0xFF;
                    }
                }
                PixelFormat::U8 => {
                    let gray = ((color.r as u16 + color.g as u16 + color.b as u16) / 3) as u8;
                    *ptr = gray;
                }
                _ => {}
            }
        }
    }

    /// Fills a rectangular region of pixels with a given color.
    pub fn draw_rect(&mut self, x: usize, y: usize, width: usize, height: usize, color: Color) {
        let max_x = (x + width).min(self.width);
        let max_y = (y + height).min(self.height);

        for py in y..max_y {
            for px in x..max_x {
                self.put_pixel(px, py, color);
            }
        }
    }

    /// Clears the entire framebuffer display with a uniform color.
    pub fn clear(&mut self, color: Color) {
        if color == Color::BLACK {
            unsafe {
                core::ptr::write_bytes(self.buffer, 0, self.buffer_len);
            }
        } else {
            self.draw_rect(0, 0, self.width, self.height, color);
        }
    }

    /// Scrolls the display vertically by `pixels` rows, shifting contents upward.
    ///
    /// Newly exposed rows at the bottom are filled with `bg_color`.
    pub fn scroll_up(&mut self, pixels: usize, bg_color: Color) {
        if pixels == 0 {
            return;
        }
        if pixels >= self.height {
            self.clear(bg_color);
            return;
        }

        let bytes_per_row = self.stride * self.bytes_per_pixel;
        let rows_to_copy = self.height - pixels;
        let bytes_to_copy = rows_to_copy * bytes_per_row;

        unsafe {
            let src = self.buffer.add(pixels * bytes_per_row);
            let dst = self.buffer;
            core::ptr::copy(src, dst, bytes_to_copy);
        }

        // Clear the bottom rows with fast memset when black
        if bg_color == Color::BLACK {
            let offset = (self.height - pixels) * bytes_per_row;
            let clear_bytes = pixels * bytes_per_row;
            if offset + clear_bytes <= self.buffer_len {
                unsafe {
                    core::ptr::write_bytes(self.buffer.add(offset), 0, clear_bytes);
                }
            }
        } else {
            self.draw_rect(0, self.height - pixels, self.width, pixels, bg_color);
        }
    }

    /// Renders an 8x16 bitmap character glyph at pixel coordinates `(x, y)`.
    pub fn draw_char(&mut self, x: usize, y: usize, c: char, fg: Color, bg: Color) {
        let glyph = font::get_glyph(c);

        for (row_idx, &row_byte) in glyph.iter().enumerate() {
            let py = y + row_idx;
            if py >= self.height {
                break;
            }

            for col_idx in 0..FONT_WIDTH {
                let px = x + col_idx;
                if px >= self.width {
                    break;
                }

                let is_fg = (row_byte & (0x80 >> col_idx)) != 0;
                let color = if is_fg { fg } else { bg };
                self.put_pixel(px, py, color);
            }
        }
    }
}
