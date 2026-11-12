//! The 8 by 8 bitmap of each character a picture draws: `font8x8`'s sets,
//! then the typography people and agents type that it lacks (dashes, curly
//! quotes, an ellipsis, a bullet, arrows), then `?`.

use font8x8::UnicodeFonts as _;

/// Eight rows from the top, each bit a pixel, bit 0 the leftmost.
type Bitmap = [u8; 8];

/// The 8 by 8 bitmap of a character: each byte is a row from the top, bit 0
/// the leftmost pixel. A character no set has is drawn as `?`.
pub(super) fn glyph(c: char) -> Bitmap {
    let c = stand_in(c);
    let fonts = [
        font8x8::BASIC_FONTS.get(c),
        font8x8::LATIN_FONTS.get(c),
        font8x8::GREEK_FONTS.get(c),
        font8x8::BOX_FONTS.get(c),
        font8x8::BLOCK_FONTS.get(c),
        font8x8::MISC_FONTS.get(c),
        font8x8::HIRAGANA_FONTS.get(c),
        typography(c),
        font8x8::BASIC_FONTS.get('?'),
    ];
    fonts.into_iter().flatten().next().unwrap_or_default()
}

/// Whether `c` is drawn as nothing and takes no cell: a zero-width space,
/// joiner or no-break space, or a variation selector. `wrap` leaves these
/// out, so they never widen a line or a frame.
pub(super) fn zero_width(c: char) -> bool {
    matches!(c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FE0F}' | '\u{FEFF}')
}

/// The character `font8x8` draws for one that only looks different: a
/// curly quote is drawn straight, a hyphen or minus as `-`.
fn stand_in(c: char) -> char {
    match c {
        '\u{2010}' | '\u{2011}' | '\u{2212}' | '\u{2043}' => '-',
        '\u{2018}' | '\u{201B}' => '`',
        '\u{2019}' | '\u{2032}' => '\'',
        '\u{201A}' => ',',
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' => '"',
        '\u{2039}' => '<',
        '\u{203A}' => '>',
        '\u{2044}' | '\u{2215}' => '/',
        '\u{2217}' => '*',
        '\u{2007}' | '\u{2008}' | '\u{2009}' | '\u{200A}' | '\u{202F}' | '\u{2002}'
        | '\u{2003}' => ' ',
        c => c,
    }
}

/// The marks `font8x8` has no glyph or stand-in for, drawn here.
fn typography(c: char) -> Option<Bitmap> {
    Some(match c {
        // En and figure dash: a little longer than `-`, on its row.
        '\u{2012}' | '\u{2013}' => [0x00, 0x00, 0x00, 0x7F, 0x00, 0x00, 0x00, 0x00],
        // Em dash and horizontal bar: the whole cell, so dashes join up.
        '\u{2014}' | '\u{2015}' => [0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, 0x00],
        // Ellipsis: three dots on the row of `.`.
        '\u{2026}' => [0x00, 0x00, 0x00, 0x00, 0x00, 0xDB, 0xDB, 0x00],
        // Bullet: a round dot in the middle of the cell.
        '\u{2022}' => [0x00, 0x00, 0x1C, 0x3E, 0x3E, 0x1C, 0x00, 0x00],
        '\u{2190}' => [0x00, 0x04, 0x06, 0x3F, 0x06, 0x04, 0x00, 0x00],
        '\u{2192}' => [0x00, 0x08, 0x18, 0x3F, 0x18, 0x08, 0x00, 0x00],
        '\u{2191}' => [0x08, 0x1C, 0x3E, 0x08, 0x08, 0x08, 0x08, 0x00],
        '\u{2193}' => [0x08, 0x08, 0x08, 0x08, 0x3E, 0x1C, 0x08, 0x00],
        '\u{2194}' => [0x00, 0x12, 0x33, 0x7F, 0x33, 0x12, 0x00, 0x00],
        // Check mark.
        '\u{2713}' | '\u{2714}' => [0x00, 0x40, 0x20, 0x11, 0x0A, 0x04, 0x00, 0x00],
        // Zero-width characters draw nothing (and `wrap` leaves them out).
        c if zero_width(c) => [0; 8],
        _ => return None,
    })
}
