//! How much room a text's glyphs take on the canvas, as the pane draws them:
//! the frame every hit test, selection, lasso and caret of a text uses.
//!
//! A canvas file stores a text's frame in the core's render cells (a cell
//! of [`TEXT_SIZE`](crate::mirror::TEXT_SIZE) a character, so an agent's
//! image never wraps it), which is far wider than the system font's glyphs.
//! The app never trusts that width for a hit: it takes the box the glyphs
//! fill. So texts saved before this rule hit right too, with nothing
//! rewritten.
//!
//! But a canvas text keeps the core's rows (docs/design/canvas-file.md,
//! slinger-f66a97): the lines it breaks into in its stored column, one
//! `size` apart, are the ones the agent's picture draws and `canvas put`
//! boxes. The app draws them in the system font at [`FONT`] of the size,
//! whose own line is about that tall ([`rows`], [`rows_frame`]), so a note
//! beside a line lines up in both.
//!
//! Pure, with one door to `AppKit`: `platform` hands in the function that
//! lays text out as it draws it ([`set_measure`]), on the main thread. Off
//! it, and in tests, an estimate of the system font stands in.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::layout::{Point, Rect, Size};

/// Lays `text` out `size` points high, broken only at its line ends, and
/// gives the box its glyphs fill, in the same points.
pub type Measure = fn(text: &str, size: f64) -> Size;

/// The column of a text that does not wrap.
pub const UNWRAPPED: f64 = 1e7;
/// The estimate's width of a character, and height of a line, as a part of
/// the text's size: about the system font's.
const CHAR_WIDTH: f64 = 0.55;
const LINE_HEIGHT: f64 = 1.2;
/// A canvas text's font size, as a part of its size: the system font's
/// line at this size is just under a size tall, so its rows sit one size
/// apart, as the core's do.
pub const FONT: f64 = 0.83;
/// How many measures are kept before the cache starts again.
const KEPT: usize = 4096;

/// What a measure is kept under: the text and its size.
type Key = (String, u64);

thread_local! {
    static MEASURE: Cell<Option<Measure>> = const { Cell::new(None) };
    static CACHE: RefCell<HashMap<Key, Size>> = RefCell::new(HashMap::new());
}

/// Measures text with `measure` on this thread from now on.
pub fn set_measure(measure: Measure) {
    MEASURE.set(Some(measure));
    CACHE.with_borrow_mut(HashMap::clear);
}

/// The frame of `text` set `size` high from `at`, broken only at its line
/// ends: as wide as its widest line (rounded up, plus a point) and as tall
/// as its lines. Never shorter than one line.
pub fn frame(at: Point, text: &str, size: f64) -> Rect {
    let Size { width, height } = measured(text, size);
    let width = width.ceil() + 1.0;
    Rect { x: at.x, y: at.y, width, height: height.max(size * LINE_HEIGHT) }
}

/// `text` cut into the core's rows for a `column` that wide at `size`, one
/// a line: at each line end, and at spaces to at most as many characters
/// as whole cells of `size` fit (a word longer is cut). A column narrower
/// than a cell does not wrap. Zero-width characters take no cell.
#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "floored, >= 1")]
pub fn rows(text: &str, size: f64, column: f64) -> String {
    let cells = if column >= size { (column / size) as usize } else { usize::MAX };
    let mut rows = String::new();
    for paragraph in text.split('\n') {
        // The cells the row so far takes.
        let mut taken: usize = 0;
        for (i, word) in paragraph.split(' ').enumerate() {
            if i > 0 {
                // The space before a word that does not fit becomes the line break.
                let wide = word.chars().filter(|&c| !zero_width(c)).count();
                let broken = taken > 0 && taken.saturating_add(wide + 1) > cells;
                rows.push(if broken { '\n' } else { ' ' });
                taken = if broken { 0 } else { taken + 1 };
            }
            for c in word.chars() {
                if !zero_width(c) {
                    if taken >= cells {
                        rows.push('\n');
                        taken = 0;
                    }
                    taken += 1;
                }
                rows.push(c);
            }
        }
        rows.push('\n');
    }
    rows.pop();
    rows
}

/// The frame of canvas text `text` set `size` high from `at` in its
/// stored `column`: as wide as its widest row in the [`FONT`] (rounded up,
/// plus a point) and a size tall a row ([`rows`]).
#[expect(clippy::cast_precision_loss, reason = "row counts are far below 2^52")]
pub fn rows_frame(at: Point, text: &str, size: f64, column: f64) -> Rect {
    let rows = rows(text, size, column);
    let width = measured(&rows, size * FONT).width.ceil() + 1.0;
    let count = rows.split('\n').count();
    Rect { x: at.x, y: at.y, width, height: count as f64 * size }
}

/// A character that draws nothing and takes no cell in the core.
fn zero_width(c: char) -> bool {
    matches!(c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FE0F}' | '\u{FEFF}')
}

/// The box `text`'s glyphs fill, measured once per text and size.
fn measured(text: &str, size: f64) -> Size {
    let Some(measure) = MEASURE.get() else { return estimate(text, size) };
    let key = (text.to_string(), size.to_bits());
    if let Some(size) = CACHE.with_borrow(|cache| cache.get(&key).copied()) {
        return size;
    }
    let found = measure(text, size);
    CACHE.with_borrow_mut(|cache| {
        if cache.len() >= KEPT {
            cache.clear();
        }
        cache.insert(key, found);
    });
    found
}

/// About how much room `text` takes in the system font, wrapped only at
/// its line ends: [`CHAR_WIDTH`] a character, [`LINE_HEIGHT`] a line.
fn estimate(text: &str, size: f64) -> Size {
    let count = |n: usize| u32::try_from(n).map_or(f64::from(u32::MAX), f64::from);
    let longest = text.split('\n').map(|l| l.chars().count()).max().unwrap_or(0);
    let lines = count(text.split('\n').count());
    Size { width: count(longest) * size * CHAR_WIDTH, height: lines * size * LINE_HEIGHT }
}

#[cfg(test)]
mod tests;
