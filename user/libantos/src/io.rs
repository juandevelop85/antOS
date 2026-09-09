//! Standard I/O macros and line-input helper for antOS userspace (T26.5).
//!
//! `SYS_WRITE`/`SYS_READ` only move raw bytes; this module is the thin,
//! ergonomic layer every other language's libc puts on top of them:
//! `print!`/`println!` formatted output, and a blocking `read_line` that
//! turns a stream of decoded keystrokes (see `SYS_READ` in the kernel,
//! backed by `crate::input::drain_ascii`) into a line the shell can parse.

use crate::syscall;
use core::fmt::Write as _;

/// Adapts [`syscall::write`] to [`core::fmt::Write`] so `format_args!` can
/// target it directly — the same trick the kernel's own console uses.
struct SyscallWriter;

impl core::fmt::Write for SyscallWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let _ = syscall::write(s);
        Ok(())
    }
}

#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    let _ = SyscallWriter.write_fmt(args);
}

/// Formats and writes to standard output (`SYS_WRITE`), like `std::print!`.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::io::_print(core::format_args!($($arg)*)));
}

/// Same as [`print!`], with a trailing newline.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", core::format_args!($($arg)*)));
}

/// Blocks until a full line is typed, echoing each character back and
/// handling backspace. Reads through [`syscall::read`] (`fd 0`), which is
/// non-blocking, so between empty polls it calls [`syscall::yield_now`] to
/// give the rest of the system its time slice instead of busy-spinning.
///
/// Returns the number of bytes written into `buf` (excluding the newline).
/// The line is truncated — further typed characters are ignored — if it
/// would not fit in `buf`.
pub fn read_line(buf: &mut [u8]) -> usize {
    let mut len = 0usize;
    let mut chunk = [0u8; 16];

    loop {
        match syscall::read(&mut chunk) {
            Ok(0) | Err(_) => syscall::yield_now(),
            Ok(n) => {
                for &byte in &chunk[..n] {
                    match byte {
                        b'\n' | b'\r' => {
                            let _ = syscall::write("\n");
                            return len;
                        }
                        0x08 | 0x7f => {
                            // Backspace / DEL: drop the last byte and erase it visually.
                            if len > 0 {
                                len -= 1;
                                let _ = syscall::write("\x08 \x08");
                            }
                        }
                        printable if len < buf.len() => {
                            buf[len] = printable;
                            len += 1;
                            let _ =
                                syscall::write(core::str::from_utf8(&[printable]).unwrap_or(""));
                        }
                        _ => {} // Buffer full: drop extra input silently.
                    }
                }
            }
        }
    }
}
