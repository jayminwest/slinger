//! How a footer's buttons and words are laid along it: each a cell, in
//! groups ([`Unit`]) that stay or go together, on the left, in the middle
//! or on the right. A footer too narrow for all of them drops whole groups,
//! the lowest rank first, until the rest fit. Pure.

use crate::commands::Command;
use crate::config::Rgb;
use crate::layout::Rect;
use crate::scene::status::Run;
use crate::scene::style;
use crate::scene::taps::Tap;

/// The space between two groups of a part.
pub const GAP: f64 = 8.0;
/// The least space kept between two parts.
pub const SPACE: f64 = 16.0;
/// Groups ranked this high stay however narrow the footer, cut off at
/// its edge: the pen's tool and state, and the sidebar's button.
pub const KEEP: u8 = 85;
/// The room either side of a word's text in its cell.
const INSET: f64 = 6.0;
/// About how wide a character of footer text is: enough to tell whether a
/// part fits. Wide enough that a part said to fit does.
const CHAR: f64 = 7.0;

/// What a cell shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Face {
    /// Its runs, one after the other, centred in it.
    Text(Vec<Run>),
    /// The pen's swatch: its colour, and how high (its width).
    Swatch(Rgb, f64),
    /// The width button's bars.
    Widths,
}

/// One button, or a word: how wide it is, what it shows, and what a tap
/// on it does (`None`: a word that says a state).
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    width: f64,
    face: Face,
    tap: Option<Tap>,
}

impl Cell {
    /// A one-glyph button: the least width a pen taps.
    pub fn glyph(runs: Vec<Run>, tap: Tap) -> Cell {
        Cell { width: style::TAP, face: Face::Text(runs), tap: Some(tap) }
    }

    /// A mark that runs `command`: the least width a pen taps.
    pub fn mark(face: Face, command: Command) -> Cell {
        Cell { width: style::TAP, face, tap: Some(Tap::Run(command)) }
    }

    /// Words, a button when `tap` says what a tap does: as wide as the
    /// text and its room, a button at least as wide as a pen taps.
    pub fn words(runs: Vec<Run>, tap: Option<Tap>) -> Cell {
        let text: f64 = runs.iter().map(|r| width(&r.text)).sum();
        let least = if tap.is_some() { style::TAP } else { 0.0 };
        Cell { width: (text + 2.0 * INSET).max(least), face: Face::Text(runs), tap }
    }
}

/// Where a part sits in the footer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Middle,
    Right,
}

/// Cells that stay or go together, side by side, on one `side`; a footer
/// too narrow drops the lowest `rank` first.
#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    side: Side,
    rank: u8,
    cells: Vec<Cell>,
}

impl Unit {
    pub fn new(side: Side, rank: u8, cells: Vec<Cell>) -> Unit {
        Unit { side, rank, cells }
    }
}

/// Lays `units` (each part's in order, left to right) along `line`, each
/// cell as tall as `bar`: the left part from the line's left end, the right
/// part to its right end, the middle centred on the line when that leaves
/// [`SPACE`] to either side, else centred between them. Groups that do not
/// fit go first, the lowest rank first, down to those ranked [`KEEP`].
/// Each cell's frame, face and tap.
pub fn lay(mut units: Vec<Unit>, line: Rect, bar: Rect) -> Vec<(Rect, Face, Option<Tap>)> {
    while !fits(&units, line.width) {
        let low = units.iter().enumerate().filter(|(_, u)| u.rank < KEEP);
        let low = low.min_by_key(|(_, u)| u.rank).map(|(i, _)| i);
        let Some(low) = low else { break };
        units.remove(low);
    }
    let (left, middle) = (span(&units, Side::Left), span(&units, Side::Middle));
    let (start, end) = (line.x + left, line.x + line.width - span(&units, Side::Right));
    let centred = line.x + (line.width - middle) / 2.0;
    let room = |x: f64| (left == 0.0 || x >= start + SPACE) && x + middle <= end - SPACE;
    let centre = if room(centred) { centred } else { start + (end - start - middle) / 2.0 };
    let mut cells = Vec::new();
    for (side, from) in [(Side::Left, line.x), (Side::Middle, centre), (Side::Right, end)] {
        let mut x = from;
        for unit in units.iter().filter(|u| u.side == side) {
            for cell in &unit.cells {
                let frame = Rect { x, y: bar.y, width: cell.width, height: bar.height };
                cells.push((frame, cell.face.clone(), cell.tap));
                x += cell.width;
            }
            x += GAP;
        }
    }
    cells
}

/// How wide the groups on `side` are, with the gaps between them.
fn span(units: &[Unit], side: Side) -> f64 {
    let on: Vec<f64> = units
        .iter()
        .filter(|u| u.side == side)
        .map(|u| u.cells.iter().map(|c| c.width).sum())
        .collect();
    let gaps = GAP * count(on.len().saturating_sub(1));
    if on.is_empty() { 0.0 } else { on.iter().sum::<f64>() + gaps }
}

/// Whether `units` fit in `room` points, [`SPACE`] between the parts.
fn fits(units: &[Unit], room: f64) -> bool {
    let spans = [Side::Left, Side::Middle, Side::Right].map(|side| span(units, side));
    let parts = spans.iter().filter(|&&s| s > 0.0).count();
    spans.iter().sum::<f64>() + SPACE * count(parts.saturating_sub(1)) <= room
}

/// About how wide `text` is in footer text.
fn width(text: &str) -> f64 {
    count(text.chars().count()) * CHAR
}

/// `n` as a number of points' worth.
fn count(n: usize) -> f64 {
    u32::try_from(n).map_or(f64::from(u32::MAX), f64::from)
}
