//! The 8 by 8 bitmap of each character a picture draws: `font8x8`'s sets,
//! then the typography and symbols people and agents type that it lacks
//! (dashes, curly quotes, an ellipsis, a bullet, arrows, stars, boxes),
//! the Russian alphabet, then a letter without its accent, then an empty box
//! (tofu).
//!
//! Every character takes one cell whatever it draws, so nothing here changes
//! how wide a text is (`render::text_size`): only what its cells show.

use font8x8::UnicodeFonts as _;

/// Eight rows from the top, each bit a pixel, bit 0 the leftmost.
type Bitmap = [u8; 8];

/// What a character with no glyph draws: an empty box as tall as a capital,
/// so the text around it keeps its place and nobody reads it as a `?` that
/// was typed. Chinese, Korean and most emoji draw this.
pub(super) const TOFU: Bitmap = [0x7E, 0x42, 0x42, 0x42, 0x42, 0x42, 0x7E, 0x00];

/// The 8 by 8 bitmap of a character: each byte is a row from the top, bit 0
/// the leftmost pixel. A character no set has is drawn as [`TOFU`].
pub(super) fn glyph(c: char) -> Bitmap {
    let c = stand_in(c);
    drawn(c).or_else(|| unaccented(c).and_then(drawn)).unwrap_or(TOFU)
}

/// The bitmap a set has for `c` itself.
fn drawn(c: char) -> Option<Bitmap> {
    let fonts = [
        font8x8::BASIC_FONTS.get(c),
        font8x8::LATIN_FONTS.get(c),
        font8x8::GREEK_FONTS.get(c),
        font8x8::BOX_FONTS.get(c),
        font8x8::BLOCK_FONTS.get(c),
        font8x8::MISC_FONTS.get(c),
        font8x8::HIRAGANA_FONTS.get(c),
        typography(c),
        cyrillic(c),
    ];
    fonts.into_iter().flatten().next()
}

/// Whether `c` is drawn as nothing and takes no cell: a zero-width space,
/// joiner or no-break space, or a variation selector. `wrap` leaves these
/// out, so they never widen a line or a frame.
pub(super) fn zero_width(c: char) -> bool {
    matches!(c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FE0F}' | '\u{FEFF}')
}

/// The character drawn for one that only looks different or says the same:
/// a curly quote is drawn straight, a hyphen or minus as `-`, a full-width
/// letter as itself, katakana as the hiragana of the same sound, a Cyrillic
/// letter shaped like a Latin or Greek one as that letter, an emoji arrow or
/// mark as the plain one.
fn stand_in(c: char) -> char {
    let shifted = |down: u32| char::from_u32(u32::from(c) - down).unwrap_or(c);
    match c {
        '\u{2010}' | '\u{2011}' | '\u{2212}' | '\u{2043}' => '-',
        '\u{2018}' | '\u{201B}' => '`',
        '\u{2019}' | '\u{2032}' => '\'',
        '\u{201A}' | '\u{3001}' => ',',
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{2033}' => '"',
        '\u{2039}' | '\u{3008}' => '<',
        '\u{203A}' | '\u{3009}' => '>',
        '\u{300A}' => '«',
        '\u{300B}' => '»',
        '\u{2044}' | '\u{2215}' => '/',
        '\u{2217}' => '*',
        '\u{2007}' | '\u{2008}' | '\u{2009}' | '\u{200A}' | '\u{202F}' | '\u{2002}'
        | '\u{2003}' | '\u{3000}' => ' ',
        // Full-width ASCII, as Chinese and Japanese text types it.
        '\u{FF01}'..='\u{FF5E}' => shifted(0xFEE0),
        // Katakana: the hiragana of the same sound, 0x60 below.
        '\u{30A1}'..='\u{30F6}' | '\u{30FD}' | '\u{30FE}' => shifted(0x60),
        '\u{30FB}' => '·',
        '\u{30FC}' => '\u{2014}',
        '\u{3002}' => '.',
        '\u{300C}' | '\u{300E}' | '\u{3010}' | '\u{3014}' => '[',
        '\u{300D}' | '\u{300F}' | '\u{3011}' | '\u{3015}' => ']',
        '\u{301C}' => '~',
        '\u{21D0}' | '\u{27F5}' | '\u{2B05}' => '\u{2190}',
        '\u{21D2}' | '\u{27F6}' | '\u{279C}' | '\u{27A1}' => '\u{2192}',
        '\u{21D4}' | '\u{27F7}' => '\u{2194}',
        // Cyrillic letters that look like a Latin or Greek one.
        'А' => 'A',
        'В' => 'B',
        'Е' => 'E',
        'К' => 'K',
        'М' => 'M',
        'Н' => 'H',
        'О' => 'O',
        'Р' => 'P',
        'С' => 'C',
        'Т' => 'T',
        'Х' => 'X',
        'Ё' => 'Ë',
        'З' => '3',
        'Г' => 'Γ',
        'П' => 'Π',
        'Ф' => 'Φ',
        'а' => 'a',
        'е' => 'e',
        'о' => 'o',
        'р' => 'p',
        'с' => 'c',
        'у' => 'y',
        'х' => 'x',
        'ё' => 'ë',
        '\u{2B06}' => '\u{2191}',
        '\u{2B07}' => '\u{2193}',
        '\u{2705}' | '\u{1F5F9}' => '\u{2611}',
        '\u{274C}' | '\u{274E}' | '\u{2715}' | '\u{2716}' | '\u{2718}' => '\u{2717}',
        '\u{2757}' | '\u{2755}' => '!',
        '\u{2753}' | '\u{2754}' => '?',
        '\u{2B50}' | '\u{1F31F}' => '\u{2605}',
        '\u{26AB}' | '\u{1F534}' | '\u{1F535}' | '\u{1F7E0}'..='\u{1F7E4}' => '\u{25CF}',
        '\u{26AA}' => '\u{25CB}',
        '\u{25FC}' | '\u{25FE}' | '\u{2B1B}' | '\u{1F7E5}'..='\u{1F7EB}' => '\u{25A0}',
        '\u{25FB}' | '\u{25FD}' | '\u{2B1C}' | '\u{2610}' => '\u{25A1}',
        '\u{25BA}' => '\u{25B6}',
        '\u{25C4}' => '\u{25C0}',
        '\u{2764}' => '\u{2665}',
        '\u{2666}' => '\u{25C6}',
        c => c,
    }
}

/// Latin Extended-A (U+0100 to U+017F), in order, each without its accent.
const EXTENDED_A: &[u8; 128] = b"AaAaAaCcCcCcCcDdDdEeEeEeEeEeGgGgGgGgHhHhIiIiIiIiIiJjJjKkk\
LlLlLlLlLlNnNnNnnNnOoOoOoOoRrRrRrSsSsSsSsTtTtTtUuUuUuUuUuUuWwYyYZzZzZzs";

/// The letter `c` is with its accent taken off, for the Latin letters no
/// set draws: Central European (U+0100 to U+017F) and Vietnamese (U+1EA0 to
/// U+1EF9). `Łódź` reads `Lódz`, a word and not `?ód?`.
fn unaccented(c: char) -> Option<char> {
    let n = u32::from(c);
    let letter = match n {
        0x0100..=0x017F => EXTENDED_A[(n - 0x0100) as usize],
        // Vietnamese: runs of one letter, capital then small.
        0x1EA0..=0x1EF9 => {
            let upper = match n {
                0x1EA0..=0x1EB7 => b'A',
                0x1EB8..=0x1EC7 => b'E',
                0x1EC8..=0x1ECB => b'I',
                0x1ECC..=0x1EE3 => b'O',
                0x1EE4..=0x1EF1 => b'U',
                _ => b'Y',
            };
            if n % 2 == 0 { upper } else { upper.to_ascii_lowercase() }
        }
        _ => return None,
    };
    Some(char::from(letter))
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
        // Bullet and black circle: a round dot in the middle of the cell.
        '\u{2022}' | '\u{25CF}' => [0x00, 0x00, 0x1C, 0x3E, 0x3E, 0x1C, 0x00, 0x00],
        '\u{25CB}' => [0x00, 0x1C, 0x22, 0x22, 0x22, 0x1C, 0x00, 0x00],
        '\u{2190}' => [0x00, 0x04, 0x06, 0x3F, 0x06, 0x04, 0x00, 0x00],
        '\u{2192}' => [0x00, 0x08, 0x18, 0x3F, 0x18, 0x08, 0x00, 0x00],
        '\u{2191}' => [0x08, 0x1C, 0x3E, 0x08, 0x08, 0x08, 0x08, 0x00],
        '\u{2193}' => [0x08, 0x08, 0x08, 0x08, 0x3E, 0x1C, 0x08, 0x00],
        '\u{2194}' => [0x00, 0x12, 0x33, 0x7F, 0x33, 0x12, 0x00, 0x00],
        // Check mark, and a cross.
        '\u{2713}' | '\u{2714}' => [0x00, 0x40, 0x20, 0x11, 0x0A, 0x04, 0x00, 0x00],
        '\u{2717}' => [0x00, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x00, 0x00],
        // Squares: filled, empty, ticked and crossed (a ballot box).
        '\u{25A0}' => [0x00, 0x3E, 0x3E, 0x3E, 0x3E, 0x3E, 0x00, 0x00],
        '\u{25A1}' => [0x00, 0x3E, 0x22, 0x22, 0x22, 0x3E, 0x00, 0x00],
        '\u{2611}' => [0x80, 0x5E, 0x32, 0x2A, 0x26, 0x3E, 0x00, 0x00],
        '\u{2612}' => [0x00, 0x3E, 0x36, 0x2A, 0x36, 0x3E, 0x00, 0x00],
        // Triangles pointing up, right, down and left.
        '\u{25B2}' => [0x00, 0x08, 0x1C, 0x3E, 0x7F, 0x00, 0x00, 0x00],
        '\u{25B6}' => [0x02, 0x06, 0x0E, 0x1E, 0x0E, 0x06, 0x02, 0x00],
        '\u{25BC}' => [0x00, 0x7F, 0x3E, 0x1C, 0x08, 0x00, 0x00, 0x00],
        '\u{25C0}' => [0x10, 0x18, 0x1C, 0x1E, 0x1C, 0x18, 0x10, 0x00],
        '\u{25C6}' => [0x08, 0x1C, 0x3E, 0x7F, 0x3E, 0x1C, 0x08, 0x00],
        '\u{2605}' => [0x08, 0x08, 0x7F, 0x3E, 0x1C, 0x36, 0x22, 0x00],
        '\u{2606}' => [0x08, 0x14, 0x63, 0x22, 0x2A, 0x36, 0x22, 0x00],
        '\u{2665}' => [0x00, 0x36, 0x7F, 0x7F, 0x3E, 0x1C, 0x08, 0x00],
        // Warning sign: a triangle with a `!` in it.
        '\u{26A0}' => [0x08, 0x14, 0x14, 0x2A, 0x2A, 0x41, 0x7F, 0x00],
        '\u{2260}' => [0x00, 0x20, 0x7E, 0x10, 0x7E, 0x04, 0x00, 0x00],
        '\u{2248}' => [0x00, 0x26, 0x19, 0x00, 0x26, 0x19, 0x00, 0x00],
        '\u{221E}' => [0x00, 0x00, 0x36, 0x49, 0x36, 0x00, 0x00, 0x00],
        // Zero-width characters draw nothing (and `wrap` leaves them out).
        c if zero_width(c) => [0; 8],
        _ => return None,
    })
}

/// The Russian letters no set or stand-in draws, capitals on rows 0 to 6 and
/// small letters on rows 2 to 6 as `font8x8`'s are, `Д` `Ц` `Щ` and their
/// small letters reaching down into row 7.
fn cyrillic(c: char) -> Option<Bitmap> {
    Some(match c {
        'Б' => [0x7F, 0x46, 0x06, 0x3E, 0x66, 0x66, 0x3F, 0x00],
        'Д' => [0x3C, 0x36, 0x36, 0x36, 0x36, 0x36, 0x7F, 0x63],
        'Ж' => [0x6B, 0x6B, 0x3E, 0x1C, 0x3E, 0x6B, 0x6B, 0x00],
        'И' => [0x63, 0x73, 0x7B, 0x6F, 0x67, 0x63, 0x63, 0x00],
        'Й' => [0x1C, 0x63, 0x73, 0x7B, 0x6F, 0x67, 0x63, 0x00],
        'Л' => [0x7C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x63, 0x00],
        'У' => [0x33, 0x33, 0x33, 0x3E, 0x30, 0x33, 0x1E, 0x00],
        'Ц' => [0x33, 0x33, 0x33, 0x33, 0x33, 0x33, 0x7F, 0x40],
        'Ч' => [0x33, 0x33, 0x33, 0x3E, 0x30, 0x30, 0x30, 0x00],
        'Ш' => [0x6B, 0x6B, 0x6B, 0x6B, 0x6B, 0x6B, 0x7F, 0x00],
        'Щ' => [0x6B, 0x6B, 0x6B, 0x6B, 0x6B, 0x6B, 0xFF, 0x80],
        'Ъ' => [0x07, 0x06, 0x06, 0x3E, 0x66, 0x66, 0x3E, 0x00],
        'Ы' => [0x63, 0x63, 0x63, 0x6F, 0x6B, 0x6B, 0x6F, 0x00],
        'Ь' => [0x03, 0x03, 0x03, 0x1F, 0x33, 0x33, 0x1F, 0x00],
        'Э' => [0x1E, 0x33, 0x30, 0x3C, 0x30, 0x33, 0x1E, 0x00],
        'Ю' => [0x33, 0x4B, 0x4B, 0x4F, 0x4B, 0x4B, 0x33, 0x00],
        'Я' => [0x7E, 0x33, 0x33, 0x3E, 0x36, 0x33, 0x73, 0x00],
        'б' => [0x3C, 0x06, 0x03, 0x1F, 0x33, 0x33, 0x1E, 0x00],
        'в' => [0x00, 0x00, 0x1F, 0x33, 0x1F, 0x33, 0x1F, 0x00],
        'г' => [0x00, 0x00, 0x3F, 0x03, 0x03, 0x03, 0x03, 0x00],
        'д' => [0x00, 0x00, 0x3C, 0x36, 0x36, 0x36, 0x7F, 0x63],
        'ж' => [0x00, 0x00, 0x6B, 0x3E, 0x1C, 0x3E, 0x6B, 0x00],
        'з' => [0x00, 0x00, 0x1E, 0x30, 0x1C, 0x30, 0x1E, 0x00],
        'и' => [0x00, 0x00, 0x33, 0x3B, 0x3F, 0x37, 0x33, 0x00],
        'й' => [0x00, 0x12, 0x0C, 0x33, 0x3B, 0x3F, 0x37, 0x33],
        'к' => [0x00, 0x00, 0x33, 0x1B, 0x0F, 0x1B, 0x33, 0x00],
        'л' => [0x00, 0x00, 0x3C, 0x36, 0x36, 0x36, 0x33, 0x00],
        'м' => [0x00, 0x00, 0x63, 0x77, 0x6B, 0x63, 0x63, 0x00],
        'н' => [0x00, 0x00, 0x33, 0x33, 0x3F, 0x33, 0x33, 0x00],
        'п' => [0x00, 0x00, 0x3F, 0x33, 0x33, 0x33, 0x33, 0x00],
        'т' => [0x00, 0x00, 0x3F, 0x0C, 0x0C, 0x0C, 0x0C, 0x00],
        'ф' => [0x08, 0x08, 0x3E, 0x49, 0x49, 0x3E, 0x08, 0x08],
        'ц' => [0x00, 0x00, 0x33, 0x33, 0x33, 0x33, 0x7F, 0x40],
        'ч' => [0x00, 0x00, 0x33, 0x33, 0x3E, 0x30, 0x30, 0x00],
        'ш' => [0x00, 0x00, 0x6B, 0x6B, 0x6B, 0x6B, 0x7F, 0x00],
        'щ' => [0x00, 0x00, 0x6B, 0x6B, 0x6B, 0x6B, 0xFF, 0x80],
        'ъ' => [0x00, 0x00, 0x07, 0x06, 0x1E, 0x36, 0x1E, 0x00],
        'ы' => [0x00, 0x00, 0x63, 0x63, 0x6F, 0x6B, 0x6F, 0x00],
        'ь' => [0x00, 0x00, 0x03, 0x03, 0x1F, 0x33, 0x1F, 0x00],
        'э' => [0x00, 0x00, 0x1E, 0x30, 0x3C, 0x30, 0x1E, 0x00],
        'ю' => [0x00, 0x00, 0x33, 0x4B, 0x4F, 0x4B, 0x33, 0x00],
        'я' => [0x00, 0x00, 0x3E, 0x33, 0x3E, 0x36, 0x33, 0x00],
        _ => return None,
    })
}
