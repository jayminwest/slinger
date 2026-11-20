//! How much room a text's glyphs take on the canvas, as the pane draws them:
//! the frame every hit test, selection, lasso and caret of a text uses.
//!
//! A canvas file stores a text's frame in the core's render cells (a cell
//! [`CELL`] of its size wide a character, so an agent's image never wraps
//! it), which is about, not exactly, what the system font's glyphs take.
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

use unicode_segmentation::UnicodeSegmentation;

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
/// How wide the core's character cell is, as a part of the text's size
/// (the core's `render::CELL`): about a character of the [`FONT`], so a
/// full row fills its column (slinger-ce52ee).
pub const CELL: f64 = 0.5;
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
/// as whole cells ([`CELL`] of `size` wide) fit (a word longer is cut). A column narrower
/// than a cell does not wrap. Zero-width characters take no cell.
pub fn rows(text: &str, size: f64, column: f64) -> String {
    let rows: Vec<&str> =
        layout(text, size, column).iter().map(|r| &text[r.start..r.end]).collect();
    rows.join("\n")
}

/// One of the core's rows of a text: the byte range of its characters. The
/// line end or the space a row breaks at is in neither row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub start: usize,
    pub end: usize,
}

/// `text`'s rows as [`rows`] cuts them, as byte ranges: so the rows drawn
/// and the rows a caret moves through never differ.
#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "floored, >= 1")]
pub fn layout(text: &str, size: f64, column: f64) -> Vec<Row> {
    let cell = size * CELL;
    let cells = if column >= cell { (column / cell) as usize } else { usize::MAX };
    let mut rows = Vec::new();
    // Where the paragraph, then the row, then the word, starts.
    let mut pos = 0;
    for paragraph in text.split('\n') {
        // The cells the row so far takes.
        let mut taken: usize = 0;
        let mut start = pos;
        for (i, word) in paragraph.split(' ').enumerate() {
            if i > 0 {
                // The space before a word that does not fit becomes the line break.
                let wide = word.chars().filter(|&c| !zero_width(c)).count();
                if taken > 0 && taken.saturating_add(wide + 1) > cells {
                    rows.push(Row { start, end: pos });
                    start = pos + 1;
                    taken = 0;
                } else {
                    taken += 1;
                }
                pos += 1;
            }
            for (at, _) in word.char_indices().filter(|&(_, c)| !zero_width(c)) {
                if taken >= cells {
                    rows.push(Row { start, end: pos + at });
                    start = pos + at;
                    taken = 0;
                }
                taken += 1;
            }
            pos += word.len();
        }
        rows.push(Row { start, end: pos });
        pos += 1;
    }
    rows
}

/// A canvas text set from its top left `at`, `size` high in its `column`,
/// as its rows ([`layout`]) and their glyphs in the [`FONT`]: where a caret
/// at a byte index stands, which index a point is nearest, and the boxes a
/// selection fills. Indices are on grapheme cluster boundaries.
pub struct Laid<'a> {
    text: &'a str,
    at: Point,
    size: f64,
    rows: Vec<Row>,
}

impl<'a> Laid<'a> {
    pub fn new(text: &'a str, at: Point, size: f64, column: f64) -> Self {
        Laid { text, at, size, rows: layout(text, size, column) }
    }

    /// Its rows; never none.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The row a caret at `index` is in: the last that starts at or before it.
    pub fn row_of(&self, index: usize) -> usize {
        self.rows.iter().rposition(|r| r.start <= index).unwrap_or(0)
    }

    /// How far right of the text's left edge a caret at `index` stands: the
    /// measured width of its row up to it.
    pub fn x(&self, index: usize) -> f64 {
        self.x_in(self.rows[self.row_of(index)], index)
    }

    /// The measured width of `row` up to `index`, which is in it or past it.
    fn x_in(&self, row: Row, index: usize) -> f64 {
        measured(&self.text[row.start..index.min(row.end)], self.size * FONT).width
    }

    /// The index in row `row` (clamped) nearest `x` from the text's left edge.
    pub fn index_in(&self, row: usize, x: f64) -> usize {
        let Row { start, end } = self.rows[row.min(self.rows.len() - 1)];
        let line = &self.text[start..end];
        let mut stops: Vec<usize> = line.grapheme_indices(true).map(|(i, _)| start + i).collect();
        stops.push(end);
        let width = |i: usize| self.x_in(Row { start, end }, i);
        // The first stop at or right of `x`: widths grow along the row.
        let after = stops.partition_point(|&i| width(i) < x);
        match after {
            0 => start,
            n if n == stops.len() => end,
            n if x - width(stops[n - 1]) < width(stops[n]) - x => stops[n - 1],
            n => stops[n],
        }
    }

    /// The index nearest the canvas point `p`: in the row it is level
    /// with (the first or last when above or below the text).
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped >= 0")]
    pub fn index_at(&self, p: Point) -> usize {
        let row = ((p.y - self.at.y) / self.size).floor().max(0.0) as usize;
        self.index_in(row, p.x - self.at.x)
    }

    /// The caret at `index`, `width` wide: a size tall, at its row's top.
    #[expect(clippy::cast_precision_loss, reason = "row counts are far below 2^52")]
    pub fn caret(&self, index: usize, width: f64) -> Rect {
        let y = self.at.y + self.row_of(index) as f64 * self.size;
        Rect { x: self.at.x + self.x(index), y, width, height: self.size }
    }

    /// The boxes the characters from `from` to `to` fill, one a row they
    /// touch: a row whose line end is selected but none of its glyphs
    /// gets `least` wide, so a selected empty line shows.
    #[expect(clippy::cast_precision_loss, reason = "row counts are far below 2^52")]
    pub fn selection(&self, from: usize, to: usize, least: f64) -> Vec<Rect> {
        let mut boxes = Vec::new();
        for (n, row) in self.rows.iter().enumerate() {
            let (a, b) = (from.max(row.start), to.min(row.end));
            let past = to > row.end && from <= row.end;
            if a > b || a == b && !past {
                continue;
            }
            let (left, right) = (self.x_in(*row, a), self.x_in(*row, b));
            let width = if past { (right - left).max(least) } else { right - left };
            let y = self.at.y + n as f64 * self.size;
            boxes.push(Rect { x: self.at.x + left, y, width, height: self.size });
        }
        boxes
    }
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
