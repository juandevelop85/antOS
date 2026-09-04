//! Framebuffer Graphics Console with Monospace Bitmap Font and ANSI Escapes.
//!
//! Provides text rendering directly on the UEFI/BIOS linear framebuffer,
//! with cursor positioning, control character processing, automatic scrolling,
//! and full ANSI escape sequence emulation.

pub mod ansi;
pub mod font;
pub mod framebuffer;

use crate::sync::SpinLock;
pub use ansi::{AnsiAction, AnsiParser};
pub use framebuffer::{Color, Framebuffer};

/// Text console rendered directly on the graphical framebuffer.
pub struct Console {
    framebuffer: Framebuffer,
    cursor_col: usize,
    cursor_row: usize,
    max_cols: usize,
    max_rows: usize,
    fg_color: Color,
    bg_color: Color,
    bold: bool,
    ansi_parser: AnsiParser,
}

impl Console {
    /// Creates a new `Console` instance taking ownership of `framebuffer`.
    pub fn new(mut framebuffer: Framebuffer) -> Self {
        let max_cols = framebuffer.width() / font::FONT_WIDTH;
        let max_rows = framebuffer.height() / font::FONT_HEIGHT;

        // Start with a clean dark slate
        framebuffer.clear(Color::BLACK);

        Console {
            framebuffer,
            cursor_col: 0,
            cursor_row: 0,
            max_cols,
            max_rows,
            fg_color: Color::LIGHT_GRAY,
            bg_color: Color::BLACK,
            bold: false,
            ansi_parser: AnsiParser::new(),
        }
    }

    pub fn cursor_position(&self) -> (usize, usize) {
        (self.cursor_col, self.cursor_row)
    }

    pub fn dimensions(&self) -> (usize, usize) {
        (self.max_cols, self.max_rows)
    }

    /// Clears the entire console screen with current background color and resets cursor.
    pub fn clear(&mut self) {
        self.framebuffer.clear(self.bg_color);
        self.cursor_col = 0;
        self.cursor_row = 0;
    }

    /// Advances the cursor to the next row, scrolling the framebuffer if needed.
    pub fn newline(&mut self) {
        if self.cursor_row + 1 < self.max_rows {
            self.cursor_row += 1;
        } else {
            self.framebuffer.scroll_up(font::FONT_HEIGHT, self.bg_color);
            self.cursor_row = self.max_rows.saturating_sub(1);
        }
    }

    /// Writes a single Unicode character, interpreting ANSI escape sequences.
    pub fn write_char(&mut self, c: char) {
        let mut parser = core::mem::replace(&mut self.ansi_parser, AnsiParser::new());
        parser.process(c, |action| {
            self.handle_action(action);
        });
        self.ansi_parser = parser;
    }

    fn handle_action(&mut self, action: AnsiAction) {
        match action {
            AnsiAction::SetForeground(color) => self.fg_color = color,
            AnsiAction::SetBackground(color) => self.bg_color = color,
            AnsiAction::ResetAttributes => {
                self.fg_color = Color::LIGHT_GRAY;
                self.bg_color = Color::BLACK;
                self.bold = false;
            }
            AnsiAction::SetBold(bold) => self.bold = bold,
            AnsiAction::ClearScreen => self.clear(),
            AnsiAction::SetCursor { col, row } => {
                self.cursor_col = col.min(self.max_cols.saturating_sub(1));
                self.cursor_row = row.min(self.max_rows.saturating_sub(1));
            }
            AnsiAction::ClearLine => {
                let start_x = self.cursor_col * font::FONT_WIDTH;
                let width = (self.max_cols.saturating_sub(self.cursor_col)) * font::FONT_WIDTH;
                let y = self.cursor_row * font::FONT_HEIGHT;
                self.framebuffer.draw_rect(start_x, y, width, font::FONT_HEIGHT, self.bg_color);
            }
            AnsiAction::PrintChar(ch) => {
                self.put_char(ch);
            }
        }
    }

    fn put_char(&mut self, ch: char) {
        match ch {
            '\n' => {
                self.cursor_col = 0;
                self.newline();
            }
            '\r' => {
                self.cursor_col = 0;
            }
            '\t' => {
                let tab_size = 4;
                let next_col = ((self.cursor_col + tab_size) / tab_size) * tab_size;
                self.cursor_col = next_col.min(self.max_cols.saturating_sub(1));
            }
            '\x08' => {
                // Backspace
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                    let x = self.cursor_col * font::FONT_WIDTH;
                    let y = self.cursor_row * font::FONT_HEIGHT;
                    self.framebuffer.draw_char(x, y, ' ', self.fg_color, self.bg_color);
                }
            }
            _ => {
                if self.cursor_col >= self.max_cols {
                    self.cursor_col = 0;
                    self.newline();
                }

                let fg = if self.bold {
                    self.fg_color.to_bright()
                } else {
                    self.fg_color
                };
                let x = self.cursor_col * font::FONT_WIDTH;
                let y = self.cursor_row * font::FONT_HEIGHT;
                self.framebuffer.draw_char(x, y, ch, fg, self.bg_color);

                self.cursor_col += 1;
                if self.cursor_col >= self.max_cols {
                    self.cursor_col = 0;
                    self.newline();
                }
            }
        }
    }
}

impl core::fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.chars() {
            self.write_char(c);
        }
        Ok(())
    }
}

/// Global system console instance.
pub static CONSOLE: SpinLock<Option<Console>> = SpinLock::new(None);

/// Initializes the global framebuffer text console.
///
/// # Safety
/// `buffer` must point to valid framebuffer memory of at least `buffer_len` bytes.
pub unsafe fn init(
    buffer: *mut u8,
    buffer_len: usize,
    info: bootloader_api::info::FrameBufferInfo,
) {
    let fb = Framebuffer::new(buffer, buffer_len, info);
    let console = Console::new(fb);
    *CONSOLE.lock() = Some(console);
}

/// Multiplex target: formats arguments to the graphical console if initialized.
#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    use core::fmt::Write;
    let mut guard = CONSOLE.lock();
    if let Some(console) = guard.as_mut() {
        let _ = console.write_fmt(args);
    }
}
