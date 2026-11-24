//! A canvas text with more rows than its frame shows
//! (docs/design/writing-column.md, 3A): its first [`MOST_ROWS`] - 1 rows,
//! the last [`FADING`] of them fading to the paper, and in the last row,
//! dimmed, "N more lines". Stepped into, its frame holds every row and it
//! draws whole ([`glyphs::more`] says which). Pure.

use super::{Draw, Item, ItemId, Mark, Shape, glyphs, legible};
use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect, Viewport};
use crate::scene::style::mix;

use glyphs::MOST_ROWS;

/// How many rows above "N more lines" fade to the paper.
pub(super) const FADING: usize = 3;
/// The draw ids of a capped text's fading rows and its note, under
/// `TAIL` + 4 × its id: below the fold marks' and the room's
/// (`canvases::fold`, `canvases::step`), above every item's.
const TAIL: u64 = 0xE << 56;

/// The draws of `item` seen through `view` on `theme`'s paper when it
/// is a canvas text with more rows than its frame shows; `None` for any
/// other item, which draws as one mark.
pub(super) fn draws(item: &Item, view: Viewport, theme: &Theme) -> Option<Vec<Draw>> {
    let Shape::Text { frame, text, size, column: Some(column) } = &item.shape else { return None };
    let (note, hidden) = glyphs::more(*frame, text, *size, *column)?;
    let rows = glyphs::layout(text, *size, *column);
    let ink = legible(item.colour, theme);
    let row = |n: usize| Rect { y: frame.y + rows_tall(n, *size), height: *size, ..*frame };
    let piece = |id: ItemId, from: usize, to: usize, colour: Rgb| {
        let lines: Vec<&str> = rows[from..to].iter().map(|r| &text[r.start..r.end]).collect();
        written(id, (row(from), to - from), (lines.join("\n"), *size), colour, view)
    };
    let tail = |k: usize| ItemId(TAIL + (item.id.0 & ((1 << 50) - 1)) * 4 + k as u64);
    let body = MOST_ROWS - 1 - FADING;
    let mut out = vec![piece(item.id, 0, body, ink)];
    for k in 0..FADING {
        let by = u16::try_from(255 * (k + 1) / (FADING + 1)).unwrap_or(u16::MAX);
        out.push(piece(tail(k), body + k, body + k + 1, mix(ink, theme.background, by)));
    }
    let said = format!("{hidden} more lines");
    out.push(written(tail(FADING), (note, 1), (said, *size), theme.muted, view));
    Some(out)
}

/// One mark of `text`, `n` rows of a canvas text of `size` from the top
/// left of `frame`, in `colour`, under `id`.
fn written(
    id: ItemId,
    (frame, n): (Rect, usize),
    (text, size): (String, f64),
    colour: Rgb,
    view: Viewport,
) -> Draw {
    let bounds = Rect { height: rows_tall(n, size), ..frame };
    let at = view.to_screen(Point { x: frame.x, y: frame.y });
    let (width, line, zoom) = (glyphs::UNWRAPPED, size, view.zoom());
    let mark = Mark::Text { at, width, text, size: size * glyphs::FONT, line, zoom };
    Draw { id, bounds: view.rect_to_screen(bounds), colour, mark }
}

/// How tall `n` rows of `size` are.
#[expect(clippy::cast_precision_loss, reason = "row counts are far below 2^52")]
fn rows_tall(n: usize, size: f64) -> f64 {
    n as f64 * size
}

#[cfg(test)]
mod tests;
