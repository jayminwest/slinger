//! How much room a text's glyphs take on the canvas, as the pane draws them:
//! the frame every hit test, selection, lasso and caret of a text uses.
//!
//! A canvas file stores a text's frame in the core's render cells (a cell
//! of [`TEXT_SIZE`](crate::mirror::TEXT_SIZE) a character, so an agent's
//! image never wraps it), which is far wider than the system font's glyphs.
//! The app never trusts that width: it lays the text out in the stored
//! column and takes the box the glyphs fill. So texts saved before this
//! rule hit right too, with nothing rewritten.
//!
//! Pure, with one door to `AppKit`: `platform` hands in the function that
//! lays text out as it draws it ([`set_measure`]), on the main thread. Off
//! it, and in tests, an estimate of the system font stands in.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::layout::{Point, Rect, Size};

/// Lays `text` out `size` points high in a column `column` points wide
/// and gives the box its glyphs fill, in the same points.
pub type Measure = fn(text: &str, size: f64, column: f64) -> Size;

/// The column of a text that does not wrap.
pub const UNWRAPPED: f64 = 1e7;
/// The estimate's width of a character, and height of a line, as a part of
/// the text's size: about the system font's.
const CHAR_WIDTH: f64 = 0.55;
const LINE_HEIGHT: f64 = 1.2;
/// How many measures are kept before the cache starts again.
const KEPT: usize = 4096;

/// What a measure is kept under: the text, its size and its column.
type Key = (String, u64, u64);

thread_local! {
    static MEASURE: Cell<Option<Measure>> = const { Cell::new(None) };
    static CACHE: RefCell<HashMap<Key, Size>> = RefCell::new(HashMap::new());
}

/// Measures text with `measure` on this thread from now on.
pub fn set_measure(measure: Measure) {
    MEASURE.set(Some(measure));
    CACHE.with_borrow_mut(HashMap::clear);
}

/// The frame of `text` set `size` high from `at`, wrapped to `column`: as
/// wide as its widest line (rounded up, plus a point, but no wider than the
/// column, so laying it out in that width again breaks its lines in the
/// same places) and as tall as its lines. Never narrower than a point,
/// never shorter than one line.
pub fn frame(at: Point, text: &str, size: f64, column: f64) -> Rect {
    let Size { width, height } = measured(text, size, column);
    let width = (width.ceil() + 1.0).min(column).max(1.0);
    Rect { x: at.x, y: at.y, width, height: height.max(size * LINE_HEIGHT) }
}

/// The box `text`'s glyphs fill, measured once per text, size and column.
fn measured(text: &str, size: f64, column: f64) -> Size {
    let Some(measure) = MEASURE.get() else { return estimate(text, size, column) };
    let key = (text.to_string(), size.to_bits(), column.to_bits());
    if let Some(size) = CACHE.with_borrow(|cache| cache.get(&key).copied()) {
        return size;
    }
    let found = measure(text, size, column);
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
fn estimate(text: &str, size: f64, column: f64) -> Size {
    let count = |n: usize| u32::try_from(n).map_or(f64::from(u32::MAX), f64::from);
    let longest = text.split('\n').map(|l| l.chars().count()).max().unwrap_or(0);
    let lines = count(text.split('\n').count());
    Size {
        width: (count(longest) * size * CHAR_WIDTH).min(column),
        height: lines * size * LINE_HEIGHT,
    }
}

#[cfg(test)]
mod tests;
