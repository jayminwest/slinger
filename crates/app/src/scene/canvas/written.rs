//! A canvas text as it is drawn when no one edits it
//! (docs/design/writing-column.md, 4A): its rows ([`glyphs::layout`]),
//! each a list of runs of cells of one look ([`markdown`]), in Menlo
//! cells, a cell [`glyphs::CELL`] of the size wide and a size tall, which
//! `platform::cells` paints. So the words are in the cells Nvim drew them
//! in. Pure: [`written`] keeps what it worked out per text, as
//! [`glyphs`] keeps its measures, so a scene rebuilt for each pen sample
//! lays out each text once (docs/perf/canvas.md).

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use super::markdown::{self, Look};
use crate::config::{Rgb, Theme};
use crate::scene::glyphs::{self, Row};
use crate::scene::style::mix;

/// How far toward the paper a mark is faded, and code's tint, of 255.
const DIM: u16 = 140;
const TINT: u16 = 225;
/// How many texts' runs are kept before the cache starts again.
const KEPT: usize = 1024;

/// What a text's runs are kept under: a hash of everything they are made
/// from, and the text itself, to tell two texts with one hash apart.
type Kept = HashMap<u64, (String, Arc<Written>)>;

thread_local! {
    static CACHE: RefCell<Kept> = RefCell::new(HashMap::new());
}

/// The cells of one look in a row, from cell `col` (from 0), `cells`
/// long: a run of plain characters, or one other character alone (with
/// the zero-width ones after it), so each keeps to its cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub row: usize,
    pub col: usize,
    pub cells: usize,
    pub text: String,
    pub look: Look,
}

/// A fenced code block's tint: from row `row`, `rows` tall, as many
/// cells wide as the block's widest row, so the block sits on one
/// rectangle (its blank lines too), not on a ragged edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub row: usize,
    pub rows: usize,
    pub cells: usize,
}

/// A text's runs and fenced blocks, and the colours its marks and its
/// code are drawn in beside its ink: `dim`, the ink toward the paper, and
/// `tint`, what code sits on. `Eq`, so two scenes holding the same kept
/// one compare by pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub runs: Vec<Run>,
    pub blocks: Vec<Block>,
    pub dim: Rgb,
    pub tint: Rgb,
}

/// `text`, `size` high in its `column`, in the rows from `from` to `to`
/// (counted from `from`), written in `ink` on `theme`'s paper: kept,
/// so the same text asked again is not laid out again.
pub fn written(
    text: &str,
    (size, column): (f64, f64),
    rows: (usize, usize),
    ink: Rgb,
    theme: &Theme,
) -> Arc<Written> {
    let paper = theme.background;
    let mut hasher = DefaultHasher::new();
    (text, size.to_bits(), column.to_bits(), rows).hash(&mut hasher);
    [ink, paper].map(|c| [c.r, c.g, c.b]).hash(&mut hasher);
    let key = hasher.finish();
    let kept = CACHE.with_borrow(|cache| {
        cache.get(&key).filter(|(kept, _)| kept == text).map(|(_, w)| Arc::clone(w))
    });
    if let Some(written) = kept {
        return written;
    }
    let laid = glyphs::layout(text, size, column);
    let written = Arc::new(Written {
        runs: runs(text, &laid, rows),
        blocks: blocks(text, &laid, rows),
        dim: mix(ink, paper, DIM),
        tint: mix(ink, paper, TINT),
    });
    CACHE.with_borrow_mut(|cache| {
        if cache.len() >= KEPT {
            cache.clear();
        }
        cache.insert(key, (text.to_string(), Arc::clone(&written)));
    });
    written
}

/// A character drawn in a run with others: one that keeps to Menlo's cell.
fn plain(c: char) -> bool {
    c.is_ascii_graphic() || c == ' '
}

/// The runs of `text` laid in `rows`, in the rows from `from` to `to`
/// (counted from `from`).
fn runs(text: &str, rows: &[Row], (from, to): (usize, usize)) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    let (mut pos, mut fenced, mut row) = (0, false, 0);
    for line in text.split('\n') {
        let cells: Vec<(usize, char)> =
            line.char_indices().filter(|&(_, c)| !glyphs::zero_width(c)).collect();
        let chars: Vec<char> = cells.iter().map(|&(_, c)| c).collect();
        let (looks, open) = markdown::line(&chars, fenced);
        fenced = open;
        let end = pos + line.len();
        // The rows of this line, each from its first cell.
        let mut k = 0;
        while row < rows.len() && rows[row].start <= end && row < to {
            let Row { start, end: stop } = rows[row];
            if row >= from {
                while k < cells.len() && pos + cells[k].0 < start {
                    k += 1;
                }
                let first = k;
                while k < cells.len() && pos + cells[k].0 < stop {
                    let (look, c) = (looks[k], cells[k].1);
                    let next = cells.get(k + 1).map_or(line.len(), |&(b, _)| b);
                    let piece = &line[cells[k].0..next.min(stop - pos)];
                    let joins = out.last().is_some_and(|r: &Run| {
                        r.row == row - from
                            && r.look == look
                            && r.col + r.cells == k - first
                            && plain(c)
                            && r.text.chars().all(plain)
                    });
                    match out.last_mut().filter(|_| joins) {
                        Some(run) => {
                            run.text.push_str(piece);
                            run.cells += 1;
                        }
                        None => out.push(Run {
                            row: row - from,
                            col: k - first,
                            cells: 1,
                            text: piece.to_string(),
                            look,
                        }),
                    }
                    k += 1;
                }
            }
            row += 1;
            if stop >= end {
                break;
            }
        }
        pos = end + 1;
        if row >= to {
            break;
        }
    }
    out
}

/// The fenced code blocks of `text` laid in `rows`, each as wide as
/// its widest row, in the rows from `from` to `to` (counted from
/// `from`): a block cut by them keeps the width of the whole block.
fn blocks(text: &str, rows: &[Row], (from, to): (usize, usize)) -> Vec<Block> {
    let mut out = Vec::new();
    let mut shut = |(first, last, cells): (usize, usize, usize)| {
        let (first, last) = (first.max(from), last.min(to));
        if first < last {
            out.push(Block { row: first - from, rows: last - first, cells });
        }
    };
    // The block open now: its first row, the row after it so far, its widest.
    let mut open: Option<(usize, usize, usize)> = None;
    let (mut pos, mut fenced, mut row) = (0, false, 0);
    for line in text.split('\n') {
        let chars: Vec<char> = line.chars().filter(|&c| !glyphs::zero_width(c)).collect();
        let fence = markdown::fence(&chars);
        let end = pos + line.len();
        let first = row;
        let mut widest = 0;
        while row < rows.len() && rows[row].start <= end {
            let Row { start, end: stop } = rows[row];
            widest =
                widest.max(text[start..stop].chars().filter(|&c| !glyphs::zero_width(c)).count());
            row += 1;
            if stop >= end {
                break;
            }
        }
        if fenced || fence {
            let block = open.get_or_insert((first, row, 0));
            *block = (block.0, row, block.2.max(widest));
        }
        if fence {
            fenced = !fenced;
        }
        if !fenced {
            open.take().map(&mut shut);
        }
        pos = end + 1;
    }
    open.map(shut);
    out
}

#[cfg(test)]
mod tests;
