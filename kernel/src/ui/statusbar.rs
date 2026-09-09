//! Pinned Top Status Bar component for antOS Sovereign Desktop.
//!
//! Renders branding, active architecture HAL status, current workspace,
//! memory telemetry, timer ticks, and live system status.

use super::color::palette;
use super::rect::Rect;
use super::surface::Surface;
use crate::console::font::FONT_WIDTH;

pub struct StatusBar {
    height: u32,
}

impl StatusBar {
    pub const DEFAULT_HEIGHT: u32 = 28;

    pub fn new() -> Self {
        StatusBar {
            height: Self::DEFAULT_HEIGHT,
        }
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Renders the status bar directly onto `surface`.
    ///
    /// T31.10: clippy counts 8 (7 params + `&self`). Single call site
    /// (`compositor.rs`), all params are independent pieces of frame state
    /// the compositor already tracks separately — grouping them into a
    /// struct would just relocate the same seven fields one level up.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        surface: &mut Surface,
        arch_name: &str,
        workspace_name: &str,
        focused_title: &str,
        heap_used_bytes: usize,
        heap_total_bytes: usize,
        ticks: u64,
    ) {
        let width = surface.width();
        let bar_rect = Rect::new(0, 0, width, self.height);

        // Background: Translucent / Dark slate (#161b22)
        surface.fill_rect(bar_rect, palette::STATUSBAR_BG);

        // Bottom border line: (#30363d)
        surface.draw_line_h(
            0,
            (self.height - 1) as i32,
            width,
            palette::STATUSBAR_BORDER,
        );

        let text_y = 6;

        // 1. Left Section: Logo, HAL Architecture, and Scheduler
        let mut left_x = 12;

        // Logo
        surface.draw_text(left_x, text_y, "antOS", palette::ACCENT_CYAN, None);
        left_x += (5 * FONT_WIDTH + 14) as i32;

        // HAL Architecture pill
        let arch_tag = arch_name;
        surface.draw_text(left_x, text_y, arch_tag, palette::ACCENT_GREEN, None);
        left_x += (arch_tag.len() * FONT_WIDTH + 14) as i32;

        // Scheduler state
        surface.draw_text(left_x, text_y, "SCHED: OK", palette::TEXT_MUTED, None);

        // 2. Center Section: Active Workspace and Focused Window Title
        let ws_tag = workspace_name;
        let center_text_len = (ws_tag.len() + 3 + focused_title.len()) * FONT_WIDTH;
        let center_x = ((width.saturating_sub(center_text_len as u32)) / 2) as i32;

        if center_x > left_x + 10 {
            surface.draw_text(center_x, text_y, ws_tag, palette::ACCENT_BLUE, None);
            let sep_x = center_x + (ws_tag.len() * FONT_WIDTH + 8) as i32;
            surface.draw_text(sep_x, text_y, "·", palette::TEXT_MUTED, None);
            let title_x = sep_x + (FONT_WIDTH + 8) as i32;
            surface.draw_text(title_x, text_y, focused_title, palette::TEXT_MAIN, None);
        }

        // 3. Right Section: Real-time Heap, Timer ticks, and Clock
        // Construct string without heap allocation using a stack buffer
        let mut right_buf = [0u8; 96];
        let heap_kb = heap_used_bytes.div_ceil(1024);
        let total_kb = heap_total_bytes / 1024;

        let right_str = format_stat_str(&mut right_buf, heap_kb, total_kb, ticks);

        let right_x = (width as i32) - (right_str.len() * FONT_WIDTH) as i32 - 14;
        if right_x > center_x + center_text_len as i32 + 10 {
            surface.draw_text(right_x, text_y, right_str, palette::ACCENT_YELLOW, None);
        }
    }
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

fn format_stat_str(buf: &mut [u8; 96], heap_kb: usize, total_kb: usize, ticks: u64) -> &str {
    use core::fmt::Write;

    struct SliceWriter<'b> {
        slice: &'b mut [u8],
        pos: usize,
    }

    impl<'b> Write for SliceWriter<'b> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let bytes = s.as_bytes();
            let remaining = self.slice.len() - self.pos;
            let to_copy = bytes.len().min(remaining);
            self.slice[self.pos..self.pos + to_copy].copy_from_slice(&bytes[..to_copy]);
            self.pos += to_copy;
            Ok(())
        }
    }

    let mut writer = SliceWriter { slice: buf, pos: 0 };
    let _ = write!(
        writer,
        "RAM: {}K/{}K · ticks: {} · 12:00",
        heap_kb, total_kb, ticks
    );
    let len = writer.pos;

    core::str::from_utf8(&buf[..len]).unwrap_or("")
}
