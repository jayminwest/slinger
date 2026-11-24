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
/// How many characters a row of a new typed text holds ([`column`]): the
/// writing column (docs/design/writing-column.md), as wide as nvim's grid.
pub const NEW_COLUMN: f64 = 80.0;
/// The most rows a canvas text's frame is tall (the core's
/// `render::MOST_ROWS`): a longer text shows its first rows, the last
/// fading, and "N more lines" in the last ([`more`]); stepped into, all.
pub const MOST_ROWS: usize = 80;
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

/// The column a text of `size` wraps in when it is about `width` wide (a
/// typed text's, as the user drags it): as many whole cells as fit, at
/// least one, and half a cell more. The rows are those of the whole
/// cells, and the half keeps the stored width off every whole number of
/// cells, so the column never reads back as a text that does not wrap
/// (one as wide as its longest line, docs/design/canvas-file.md).
pub fn column(width: f64, size: f64) -> f64 {
    let cell = size * CELL;
    ((width / cell).floor().max(1.0) + 0.5) * cell
}

/// A text's column of about `width` at its new `size`, as [`column`] puts
/// it, so a resize never lands it on whole cells; a text that does not
/// wrap ([`UNWRAPPED`]) stays so.
pub fn rewrapped(width: f64, size: f64) -> f64 {
    if width < UNWRAPPED { column(width, size) } else { width }
}

/// One of the core's rows of a text: the byte range of its characters. The
/// line end or the space a row breaks at is in neither row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub start: usize,
    pub end: usize,
}

/// `text`'s rows as [`rows`] cuts them, as byte ranges: so the rows drawn
/// and the rows a caret moves through never differ. A line that does not
/// fit in the cells breaks as nvim breaks it (the core's `raster::wrap`):
/// at the last space in its first cells that a character other than a
/// space follows and that comes after the line's first other character,
/// so a word ending on the last cell with a space after it goes to the
/// next row; else it is cut at the cells.
#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "floored, >= 1")]
pub fn layout(text: &str, size: f64, column: f64) -> Vec<Row> {
    let cell = size * CELL;
    let cells = if column >= cell { (column / cell) as usize } else { usize::MAX };
    let mut rows = Vec::new();
    // Where the paragraph starts.
    let mut pos = 0;
    for paragraph in text.split('\n') {
        // The characters that take a cell, and where each starts.
        let chars: Vec<(usize, char)> =
            paragraph.char_indices().filter(|&(_, c)| !zero_width(c)).collect();
        let space = |i: usize| chars[i].1 == ' ';
        // No break in the spaces a line starts with.
        let lead = (0..chars.len()).take_while(|&i| space(i)).count();
        let (mut first, mut start) = (0, pos);
        while chars.len() - first > cells {
            let cut = first + cells;
            let after = (first.max(lead + 1)..cut).rev().find(|&i| space(i) && !space(i + 1));
            // The space a row breaks at is in neither row; a cut row's next starts at the cut.
            let (end, next, skip) = after.map_or((cut, cut, 0), |i| (i, i + 1, 1));
            rows.push(Row { start, end: pos + chars[end].0 });
            (first, start) = (next, pos + chars[end].0 + skip);
        }
        rows.push(Row { start, end: pos + paragraph.len() });
        pos += paragraph.len() + 1;
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
/// plus a point) and a size tall a row ([`rows`]), at most [`MOST_ROWS`]
/// rows, as the core boxes it.
pub fn rows_frame(at: Point, text: &str, size: f64, column: f64) -> Rect {
    let whole = whole_frame(at, text, size, column);
    Rect { height: whole.height.min(rows_height(MOST_ROWS, size)), ..whole }
}

/// [`rows_frame`] with every row: the frame a text stepped into, or typed
/// in slinger's editor, shows in.
pub fn whole_frame(at: Point, text: &str, size: f64, column: f64) -> Rect {
    let rows = rows(text, size, column);
    let width = measured(&rows, size * FONT).width.ceil() + 1.0;
    Rect { x: at.x, y: at.y, width, height: rows_height(rows.split('\n').count(), size) }
}

/// How tall `n` rows of a text of `size` are.
#[expect(clippy::cast_precision_loss, reason = "row counts are far below 2^52")]
fn rows_height(n: usize, size: f64) -> f64 {
    n as f64 * size
}

/// How many of a text's `rows` its last shown row stands for when it has
/// more than [`MOST_ROWS`]: those past the one before it, which "N more
/// lines" counts. None for a text that shows whole.
pub fn hidden(rows: usize) -> usize {
    if rows > MOST_ROWS { rows - (MOST_ROWS - 1) } else { 0 }
}

/// The row of a canvas text drawn in `frame` that says "N more lines", and
/// N: when it has more than [`MOST_ROWS`] rows in its `column` and its
/// frame stops short of them (a text stepped into is framed whole). Two
/// quick taps on that row step into the text.
#[expect(clippy::cast_precision_loss, reason = "row counts are far below 2^52")]
pub fn more(frame: Rect, text: &str, size: f64, column: f64) -> Option<(Rect, usize)> {
    let rows = layout(text, size, column).len();
    let hidden = hidden(rows);
    let short = frame.height < rows_height(rows, size) - size / 2.0;
    let y = frame.y + (MOST_ROWS - 1) as f64 * size;
    (hidden > 0 && short).then_some((Rect { y, height: size, ..frame }, hidden))
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
