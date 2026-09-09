//! ANSI escape sequence state machine.
//!
//! Parses VT100/ANSI Control Sequence Introducer (CSI) sequences for colors,
//! text attributes (bold, reset), screen clearing, and cursor repositioning.

use super::framebuffer::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnsiAction {
    /// Emit a regular or control character.
    PrintChar(char),
    /// Change text foreground color.
    SetForeground(Color),
    /// Change text background color.
    SetBackground(Color),
    /// Reset colors and attributes to defaults.
    ResetAttributes,
    /// Enable or disable bold/bright text rendering.
    SetBold(bool),
    /// Clear the screen and reset cursor to top-left.
    ClearScreen,
    /// Position cursor at specified (column, row), 0-indexed.
    SetCursor { col: usize, row: usize },
    /// Clear line from current cursor position.
    ClearLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Normal,
    Escape,
    Csi,
}

pub struct AnsiParser {
    state: State,
    params: [u16; 8],
    param_count: usize,
    current_num: u16,
    has_num: bool,
}

impl AnsiParser {
    pub const fn new() -> Self {
        AnsiParser {
            state: State::Normal,
            params: [0; 8],
            param_count: 0,
            current_num: 0,
            has_num: false,
        }
    }

    /// Feeds a single character into the state machine, firing `callback`
    /// for every parsed action.
    pub fn process<F>(&mut self, c: char, mut callback: F)
    where
        F: FnMut(AnsiAction),
    {
        match self.state {
            State::Normal => {
                if c == '\x1B' {
                    self.state = State::Escape;
                } else {
                    callback(AnsiAction::PrintChar(c));
                }
            }
            State::Escape => {
                if c == '[' {
                    self.state = State::Csi;
                    self.param_count = 0;
                    self.current_num = 0;
                    self.has_num = false;
                } else {
                    // Unknown escape sequence, return to normal
                    self.state = State::Normal;
                    callback(AnsiAction::PrintChar(c));
                }
            }
            State::Csi => {
                match c {
                    '0'..='9' => {
                        let digit = (c as u8 - b'0') as u16;
                        self.current_num =
                            self.current_num.saturating_mul(10).saturating_add(digit);
                        self.has_num = true;
                    }
                    ';' => {
                        if self.param_count < self.params.len() {
                            self.params[self.param_count] = self.current_num;
                            self.param_count += 1;
                        }
                        self.current_num = 0;
                        self.has_num = false;
                    }
                    // SGR (Select Graphic Rendition)
                    'm' => {
                        self.push_current_num();
                        if self.param_count == 0 {
                            callback(AnsiAction::ResetAttributes);
                        } else {
                            for i in 0..self.param_count {
                                self.apply_sgr(self.params[i], &mut callback);
                            }
                        }
                        self.state = State::Normal;
                    }
                    // Erase in Display
                    'J' => {
                        self.push_current_num();
                        let mode = if self.param_count > 0 {
                            self.params[0]
                        } else {
                            0
                        };
                        if mode == 2 || mode == 3 {
                            callback(AnsiAction::ClearScreen);
                        }
                        self.state = State::Normal;
                    }
                    // Cursor Position
                    'H' | 'f' => {
                        self.push_current_num();
                        let row = if self.param_count > 0 && self.params[0] > 0 {
                            (self.params[0] - 1) as usize
                        } else {
                            0
                        };
                        let col = if self.param_count > 1 && self.params[1] > 0 {
                            (self.params[1] - 1) as usize
                        } else {
                            0
                        };
                        callback(AnsiAction::SetCursor { col, row });
                        self.state = State::Normal;
                    }
                    // Erase in Line
                    'K' => {
                        callback(AnsiAction::ClearLine);
                        self.state = State::Normal;
                    }
                    // Unhandled or end of sequence
                    _ => {
                        self.state = State::Normal;
                    }
                }
            }
        }
    }

    fn push_current_num(&mut self) {
        if self.has_num && self.param_count < self.params.len() {
            self.params[self.param_count] = self.current_num;
            self.param_count += 1;
            self.current_num = 0;
            self.has_num = false;
        }
    }

    fn apply_sgr<F>(&self, code: u16, callback: &mut F)
    where
        F: FnMut(AnsiAction),
    {
        match code {
            0 => callback(AnsiAction::ResetAttributes),
            1 => callback(AnsiAction::SetBold(true)),
            22 => callback(AnsiAction::SetBold(false)),
            // Standard foreground
            30 => callback(AnsiAction::SetForeground(Color::BLACK)),
            31 => callback(AnsiAction::SetForeground(Color::RED)),
            32 => callback(AnsiAction::SetForeground(Color::GREEN)),
            33 => callback(AnsiAction::SetForeground(Color::YELLOW)),
            34 => callback(AnsiAction::SetForeground(Color::BLUE)),
            35 => callback(AnsiAction::SetForeground(Color::MAGENTA)),
            36 => callback(AnsiAction::SetForeground(Color::CYAN)),
            37 => callback(AnsiAction::SetForeground(Color::WHITE)),
            39 => callback(AnsiAction::SetForeground(Color::LIGHT_GRAY)), // default FG
            // Standard background
            40 => callback(AnsiAction::SetBackground(Color::BLACK)),
            41 => callback(AnsiAction::SetBackground(Color::RED)),
            42 => callback(AnsiAction::SetBackground(Color::GREEN)),
            43 => callback(AnsiAction::SetBackground(Color::YELLOW)),
            44 => callback(AnsiAction::SetBackground(Color::BLUE)),
            45 => callback(AnsiAction::SetBackground(Color::MAGENTA)),
            46 => callback(AnsiAction::SetBackground(Color::CYAN)),
            47 => callback(AnsiAction::SetBackground(Color::WHITE)),
            49 => callback(AnsiAction::SetBackground(Color::BLACK)), // default BG
            // High-intensity foreground
            90 => callback(AnsiAction::SetForeground(Color::DARK_GRAY)),
            91 => callback(AnsiAction::SetForeground(Color::BRIGHT_RED)),
            92 => callback(AnsiAction::SetForeground(Color::BRIGHT_GREEN)),
            93 => callback(AnsiAction::SetForeground(Color::BRIGHT_YELLOW)),
            94 => callback(AnsiAction::SetForeground(Color::BRIGHT_BLUE)),
            95 => callback(AnsiAction::SetForeground(Color::BRIGHT_MAGENTA)),
            96 => callback(AnsiAction::SetForeground(Color::BRIGHT_CYAN)),
            97 => callback(AnsiAction::SetForeground(Color::BRIGHT_WHITE)),
            // High-intensity background
            100 => callback(AnsiAction::SetBackground(Color::DARK_GRAY)),
            101 => callback(AnsiAction::SetBackground(Color::BRIGHT_RED)),
            102 => callback(AnsiAction::SetBackground(Color::BRIGHT_GREEN)),
            103 => callback(AnsiAction::SetBackground(Color::BRIGHT_YELLOW)),
            104 => callback(AnsiAction::SetBackground(Color::BRIGHT_BLUE)),
            105 => callback(AnsiAction::SetBackground(Color::BRIGHT_MAGENTA)),
            106 => callback(AnsiAction::SetBackground(Color::BRIGHT_CYAN)),
            107 => callback(AnsiAction::SetBackground(Color::BRIGHT_WHITE)),
            _ => {}
        }
    }
}
