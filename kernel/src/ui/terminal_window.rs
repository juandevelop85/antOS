//! Embedded Terminal Window component for antOS Sovereign Desktop.
//!
//! Provides an active virtual terminal emulator (VTE) with circular line buffer,
//! vertical scrolling, ANSI color escape sequence parsing, interactive prompt,
//! and bidirectional tie-in to the sovereign userspace shell (`/bin/init`).

use super::color::{palette, Color};
use super::rect::Rect;
use super::surface::Surface;
use crate::console::font::{FONT_HEIGHT, FONT_WIDTH};
use alloc::string::String;
use alloc::vec::Vec;

/// A single rendered line of text in the terminal window with associated color attribute.
#[derive(Debug, Clone)]
pub struct TerminalLine {
    pub text: String,
    pub color: Color,
}

pub struct TerminalWindow {
    rect: Rect,
    title: &'static str,
    focused: bool,
    lines: Vec<TerminalLine>,
    current_line: String,
    current_color: Color,
    max_history: usize,
    scroll_offset: usize,
    last_command: String,
}

impl TerminalWindow {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        let mut win = TerminalWindow {
            rect: Rect::new(x, y, width, height),
            title: "Terminal 1 · antOS Sovereign Shell (EL0)",
            focused: true,
            lines: Vec::new(),
            current_line: String::new(),
            current_color: palette::TEXT_MAIN,
            max_history: 256,
            scroll_offset: 0,
            last_command: String::new(),
        };

        win.lines.push(TerminalLine {
            text: String::from("antOS Kernel 0.1.0-baremetal · AArch64 / x86_64 Dual Architecture"),
            color: palette::ACCENT_BLUE,
        });
        win.lines.push(TerminalLine {
            text: String::from("HAL Status: MMU 4KiB active · GICv2 online · PCIe xHCI USB active"),
            color: palette::TEXT_MUTED,
        });
        win.lines.push(TerminalLine {
            text: String::from("Graphics: Framebuffer GOP/Limine 1024x768x32bpp active"),
            color: palette::ACCENT_CYAN,
        });
        win.lines.push(TerminalLine {
            text: String::from(
                "Input Subsystem: xHCI USB Keyboard/Mouse & VirtIO event queue active",
            ),
            color: palette::ACCENT_GREEN,
        });
        win.lines.push(TerminalLine {
            text: String::from(
                "------------------------------------------------------------------------",
            ),
            color: palette::STATUSBAR_BORDER,
        });

        win
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
        &self.current_line
    }

    pub fn last_command(&self) -> &str {
        &self.last_command
    }

    pub fn insert_char(&mut self, c: char) {
        if c.is_ascii() && !c.is_control() {
            let max_cols = ((self.rect.width.saturating_sub(32)) / FONT_WIDTH as u32) as usize;
            if self.current_line.len() >= max_cols {
                self.lines.push(TerminalLine {
                    text: core::mem::take(&mut self.current_line),
                    color: self.current_color,
                });
                if self.lines.len() > self.max_history {
                    self.lines.remove(0);
                }
            }
            self.current_line.push(c);
        }
    }

    pub fn delete_char(&mut self) {
        self.current_line.pop();
    }

    pub fn submit_command(&mut self) -> String {
        let cmd = core::mem::take(&mut self.current_line);
        self.last_command = cmd.clone();
        self.lines.push(TerminalLine {
            text: cmd.clone(),
            color: palette::TEXT_BRIGHT,
        });
        if self.lines.len() > self.max_history {
            self.lines.remove(0);
        }
        cmd
    }

    /// Writes a character into the terminal buffer, interpreting control and formatting characters.
    pub fn write_char(&mut self, c: char) {
        match c {
            '\n' => {
                self.lines.push(TerminalLine {
                    text: core::mem::take(&mut self.current_line),
                    color: self.current_color,
                });
                if self.lines.len() > self.max_history {
                    self.lines.remove(0);
                }
            }
            '\r' => {
                // Ignore CR if followed by LF, or clear current line
            }
            '\x08' | '\x7f' => {
                self.current_line.pop();
            }
            other if other.is_ascii() && !other.is_control() => {
                self.insert_char(other);
            }
            _ => {}
        }
    }

    /// Writes raw text bytes/string into the terminal window, parsing basic ANSI color escapes.
    pub fn write_str(&mut self, s: &str) {
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                // Check for ANSI escape sequence
                if chars.peek() == Some(&'[') {
                    chars.next(); // consume '['
                    let mut seq = String::new();
                    while let Some(&next_c) = chars.peek() {
                        chars.next();
                        if next_c.is_ascii_alphabetic() {
                            // Terminating byte
                            if next_c == 'm' {
                                // SGR color code
                                self.current_color = match seq.as_str() {
                                    "0" | "" => palette::TEXT_MAIN,
                                    "1;31" | "31" => palette::ACCENT_RED,
                                    "1;32" | "32" => palette::ACCENT_GREEN,
                                    "1;33" | "33" => palette::ACCENT_YELLOW,
                                    "1;34" | "34" => palette::ACCENT_BLUE,
                                    "1;36" | "36" => palette::ACCENT_CYAN,
                                    "1;37" | "37" | "1" => palette::TEXT_BRIGHT,
                                    _ => self.current_color,
                                };
                            } else if next_c == 'J' {
                                self.lines.clear();
                                self.current_line.clear();
                            }
                            break;
                        } else {
                            seq.push(next_c);
                        }
                    }
                    continue;
                }
            }
            self.write_char(c);
        }
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_offset < self.lines.len() {
            self.scroll_offset += 1;
        }
    }

    pub fn scroll_down(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
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
        let available_height = (r.height as usize).saturating_sub(44);
        let line_spacing = FONT_HEIGHT as i32 + 3;
        let visible_lines = available_height / (FONT_HEIGHT + 3);
        let left_padding = r.x + 16;
        let mut line_y = r.y + 36;

        let total_lines = self.lines.len() + 1; // including active current line
        let end_idx = total_lines.saturating_sub(self.scroll_offset);
        let start_idx = end_idx.saturating_sub(visible_lines);

        for (i, line) in self.lines.iter().enumerate() {
            if i >= start_idx && i < end_idx && line_y + (FONT_HEIGHT as i32) < r.bottom() - 8 {
                surface.draw_text(left_padding, line_y, &line.text, line.color, None);
                line_y += line_spacing;
            }
        }

        // 6. Interactive current line with cursor
        if line_y + (FONT_HEIGHT as i32) < r.bottom() - 6 {
            surface.draw_text(
                left_padding,
                line_y,
                &self.current_line,
                self.current_color,
                None,
            );

            let cursor_x = left_padding + (self.current_line.len() * FONT_WIDTH) as i32;
            let cursor_color = if self.focused {
                palette::ACCENT_GREEN
            } else {
                palette::TEXT_MUTED
            };
            surface.draw_char(cursor_x, line_y, '_', cursor_color, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_window_write_and_scroll() {
        let mut term = TerminalWindow::new(0, 0, 800, 600);
        let initial_lines = term.lines.len();

        term.write_str("hola antOS\n");
        assert_eq!(term.lines.len(), initial_lines + 1);
        assert_eq!(term.lines.last().unwrap().text, "hola antOS");

        term.write_str("linea con backspace\x08!");
        assert_eq!(term.input_text(), "linea con backspac!");

        let prev_scroll = term.scroll_offset();
        term.scroll_up();
        assert_eq!(term.scroll_offset(), prev_scroll + 1);
        term.scroll_down();
        assert_eq!(term.scroll_offset(), prev_scroll);
    }

    #[test]
    fn test_terminal_window_ansi_colors() {
        let mut term = TerminalWindow::new(0, 0, 800, 600);
        term.write_str("\x1b[1;32mverde\x1b[0m normal\n");
        assert_eq!(term.lines.last().unwrap().text, "verde normal");

        term.write_str("\x1b[2J");
        assert_eq!(term.lines.len(), 0);
        assert_eq!(term.input_text(), "");
    }

    #[test]
    fn test_terminal_window_submit_and_focus() {
        let mut term = TerminalWindow::new(0, 0, 800, 600);
        term.set_focused(false);
        assert!(!term.is_focused());
        term.set_focused(true);
        assert!(term.is_focused());

        term.insert_char('l');
        term.insert_char('s');
        assert_eq!(term.input_text(), "ls");

        let cmd = term.submit_command();
        assert_eq!(cmd, "ls");
        assert_eq!(term.last_command(), "ls");
        assert_eq!(term.input_text(), "");
    }
}
