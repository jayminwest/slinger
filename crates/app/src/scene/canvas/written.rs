//! A canvas text as it is drawn when no one edits it
//! (docs/design/writing-column.md, 4A): its rows ([`glyphs::layout`]),
//! each a list of runs of cells of one look ([`markdown`]), in Menlo
//! cells, a cell [`glyphs::CELL`] of the size wide and a size tall, which
//! `platform::cells` paints. So the words are in the cells Nvim drew them
//! in. Pure.

use super::markdown::{self, Look};
use crate::config::{Rgb, Theme};
use crate::scene::glyphs;
use crate::scene::style::mix;

/// How far toward the paper a mark is faded, and code's tint, of 255.
const DIM: u16 = 140;
const TINT: u16 = 225;

/// The cells of one look in a row, from cell `col` (from 0), `cells`
/// long: a run of plain characters, or one other character alone (with
/// the zero-width ones after it), so each keeps to its cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub row: usize,
    pub col: usize,
    pub cells: usize,
    pub text: String,
    pub look: Look,
}

/// A text's runs, and the colours its marks and its code are drawn in
/// beside its ink: `dim`, the ink toward the paper, and `tint`, what
/// code sits on.
#[derive(Debug, Clone, PartialEq)]
pub struct Written {
    pub runs: Vec<Run>,
    pub dim: Rgb,
    pub tint: Rgb,
}

impl Written {
    /// `runs` of a text in `ink` on `theme`'s paper.
    pub fn new(runs: Vec<Run>, ink: Rgb, theme: &Theme) -> Self {
        let paper = theme.background;
        Written { runs, dim: mix(ink, paper, DIM), tint: mix(ink, paper, TINT) }
    }
}

/// A character drawn in a run with others: one that keeps to Menlo's cell.
fn plain(c: char) -> bool {
    c.is_ascii_graphic() || c == ' '
}

/// The runs of `text`, `size` high in its `column`, in the rows
/// from `from` to `to` (counted from `from`).
pub fn runs(text: &str, size: f64, column: f64, (from, to): (usize, usize)) -> Vec<Run> {
    let rows = glyphs::layout(text, size, column);
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
            let glyphs::Row { start, end: stop } = rows[row];
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

#[cfg(test)]
mod tests;
