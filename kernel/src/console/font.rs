//! Monospace 8x16 bitmap font for the antOS framebuffer console.
//!
//! Provides glyph lookup for ASCII (0..127), box-drawing characters,
//! symbols, and international characters (CP437 mapping).

pub const FONT_WIDTH: usize = 8;
pub const FONT_HEIGHT: usize = 16;

/// Embedded 8x16 font binary containing 256 glyphs in CP437 order (4096 bytes).
const FONT_DATA: &[u8; 4096] = include_bytes!("font.bin");

/// Maps a Unicode character to its corresponding CP437 glyph index.
pub fn unicode_to_cp437(c: char) -> u8 {
    match c {
        // Standard ASCII range (0x00..=0x7F)
        '\0'..='\x7F' => c as u8,

        // Spanish accents and symbols
        'á' => 160,
        'é' => 130,
        'í' => 161,
        'ó' => 162,
        'ú' => 163,
        'ñ' => 164,
        'Ñ' => 165,
        '¿' => 168,
        '¡' => 173,
        'ü' => 129,
        'Ü' => 154,

        // Single box-drawing characters
        '─' => 196,
        '│' => 179,
        '┌' => 218,
        '┐' => 191,
        '└' => 192,
        '┘' => 217,
        '├' => 195,
        '┤' => 180,
        '┬' => 194,
        '┴' => 193,
        '┼' => 197,

        // Double box-drawing characters
        '═' => 205,
        '║' => 186,
        '╔' => 201,
        '╗' => 187,
        '╚' => 200,
        '╝' => 188,
        '╠' => 204,
        '╣' => 185,
        '╦' => 203,
        '╩' => 202,
        '╬' => 206,

        // Block elements & symbols
        '█' => 219,
        '▄' => 220,
        '▌' => 221,
        '▐' => 222,
        '▀' => 223,
        '■' => 254,
        '·' => 250,
        '•' => 7,
        '«' => 174,
        '»' => 175,
        '°' => 248,
        '→' => 26,
        '←' => 27,
        '▲' => 30,
        '▼' => 31,
        '√' => 251,
        '≈' => 247,

        // Fallback for unmapped characters
        _ => b'?',
    }
}

/// Retrieves the 16-row bitmap glyph for the specified character.
///
/// Each byte represents one row of 8 horizontal pixels, where the most
/// significant bit (bit 7) is the leftmost pixel.
pub fn get_glyph(c: char) -> &'static [u8; 16] {
    let index = unicode_to_cp437(c);
    let offset = (index as usize) * FONT_HEIGHT;
    let slice = &FONT_DATA[offset..offset + FONT_HEIGHT];
    // SAFETY: FONT_DATA is 4096 bytes; for any u8 index (0..255),
    // offset + 16 <= 4096 and the pointer is properly aligned for [u8; 16].
    unsafe { &*(slice.as_ptr() as *const [u8; 16]) }
}
