//! Images of canvas items: a set of items, or a box of a page, in; PNG
//! images out. The rules are the image rules of `docs/design/packet.md`.
//!
//! The scale is fixed ([`SCALE`]) and does not depend on any viewport, so the
//! same ink is always the same number of pixels and handwriting stays
//! readable (SPEC R3). Ink is drawn in its own colour on white. An image whose
//! long edge is over [`MAX_EDGE`] pixels is cut into tiles that overlap by
//! [`OVERLAP`] pixels, so a word cut at one tile's edge is whole in the next
//! (Spike B finding 3). An image of items is cut down to its ink plus
//! [`MARGIN`], tile by tile, and a tile with no ink of its own (white, or
//! ink only in its overlap with a tile kept) is left out.
//!
//! Pure and deterministic: no IO, no clock, and the same input gives the same
//! bytes. There are no errors: ids not on the canvas are skipped, and nothing
//! to draw gives no images.

mod png;
mod raster;

use crate::canvas::{Canvas, Item, ItemId, PageId, Rect};
use raster::Raster;

/// Pixels per canvas unit. Spike B read handwriting at this scale.
pub const SCALE: f64 = 0.35;

/// The longest edge, in pixels, of one image. A larger image becomes tiles.
pub const MAX_EDGE: u32 = 1568;

/// How many pixels two neighbouring tiles share.
pub const OVERLAP: u32 = 64;

/// The white space, in canvas units, around a set of items.
pub const MARGIN: f64 = 16.0;

/// One PNG image, or one tile of a larger one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// `<id>.png`, or `<id>-<n>.png` for the `n`th tile in reading order.
    pub name: String,
    /// Where the image's top-left pixel sits in the whole picture. Zero
    /// unless the image is a tile.
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// The encoded PNG: 8-bit RGB, no alpha.
    pub png: Vec<u8>,
}

/// Draws `items` in the order given, framed by the box around them plus
/// [`MARGIN`] on every side. Images are named after `id`. Ids not on the
/// canvas are skipped; when none is left there are no images. A picture cut
/// into tiles keeps only the tiles with ink of their own, each cut down to
/// its ink plus the margin; `x` and `y` place each in the whole picture.
#[must_use]
pub fn items(canvas: &Canvas, items: &[ItemId], id: &str) -> Vec<Image> {
    let found: Vec<&Item> = items.iter().filter_map(|i| canvas.item(*i)).collect();
    let Some(frame) = found.iter().map(|i| i.bounds()).reduce(union) else {
        return Vec::new();
    };
    let frame = Rect::new(
        frame.x - MARGIN,
        frame.y - MARGIN,
        frame.w + 2.0 * MARGIN,
        frame.h + 2.0 * MARGIN,
    );
    draw(&found, frame, id, true)
}

/// Draws what is on `page` inside `area`, cut at its edges: every item that
/// touches the area, in drawing order, on white. Images are named after `id`.
/// A missing page draws as an empty, white area.
#[must_use]
pub fn crop(canvas: &Canvas, page: PageId, area: Rect, id: &str) -> Vec<Image> {
    let found: Vec<&Item> =
        canvas.items_in_box(page, area).into_iter().filter_map(|i| canvas.item(i)).collect();
    draw(&found, area, id, false)
}

/// The frame size, in canvas units, that holds `text` as it is drawn at
/// `size` when wrapped to at most `columns` characters a line (at least
/// one): one character cell of `size` per character, a line per row.
#[must_use]
pub fn text_size(text: &str, columns: usize, size: f64) -> (f64, f64) {
    let rows = raster::wrap(text, columns.max(1));
    let longest = rows.iter().map(Vec::len).max().unwrap_or(0);
    let cells = |n: usize| u32::try_from(n).map_or(f64::from(u32::MAX), f64::from);
    (cells(longest) * size, cells(rows.len()) * size)
}

/// The frame size, in canvas units, that text drawn at `size` in a frame
/// `w` by `h` has. A frame at least a character wide and with a height is kept as it
/// is. One with no height keeps its width and gets the height of the text
/// wrapped to it. One narrower than a character is drawn unwrapped, so it
/// gets the unwrapped text's size, at least one character wide.
#[must_use]
pub fn text_frame(text: &str, w: f64, h: f64, size: f64) -> (f64, f64) {
    match (w >= size, h > 0.0) {
        (true, true) => (w, h),
        (true, false) => (w, text_size(text, raster::columns(w, size), size).1),
        (false, _) => {
            let (w, h) = text_size(text, usize::MAX, size);
            (w.max(size), h)
        }
    }
}

/// Draws `items` into the picture of `frame`, as one image or as tiles.
/// With `trim`, tiles are cut down to their ink ([`trimmed`]); without, each
/// tile is whole, white or not.
fn draw(items: &[&Item], frame: Rect, id: &str, trim: bool) -> Vec<Image> {
    let (width, height) = (pixels(frame.w), pixels(frame.h));
    let drawn: Vec<([u32; 4], Raster)> = spans(height)
        .into_iter()
        .flat_map(|(y, h)| spans(width).into_iter().map(move |(x, w)| [x, y, w, h]))
        .map(|[x, y, w, h]| {
            let mut raster = Raster::new(frame, x, y, w, h);
            for item in items {
                raster.item(item);
            }
            ([x, y, w, h], raster)
        })
        .collect();
    let whole = |tile: [u32; 4]| [0, 0, tile[2], tile[3]];
    let parts: Vec<(usize, [u32; 4])> = if trim {
        trimmed(&drawn)
    } else {
        drawn.iter().enumerate().map(|(n, (tile, _))| (n, whole(*tile))).collect()
    };
    let single = parts.len() == 1;
    let name = |n: usize| if single { format!("{id}.png") } else { format!("{id}-{n}.png") };
    (1..)
        .zip(parts)
        .map(|(n, (k, part @ [x, y, w, h]))| {
            let ([left, top, ..], raster) = &drawn[k];
            let (x, y) = (left + x, top + y);
            Image { name: name(n), x, y, width: w, height: h, png: raster.png(part) }
        })
        .collect()
}

/// The tiles worth an image, in reading order, each as its index and the
/// part of it to keep: its ink plus [`MARGIN`], within the tile. A tile with
/// no ink is left out, and so is one whose ink the tiles kept already hold
/// (it lies in their overlaps); tiles are weighed largest ink first, so the
/// tile that holds the most stays. When no tile has ink (white ink), the
/// first is kept whole, so ink always has an image.
fn trimmed(drawn: &[([u32; 4], Raster)]) -> Vec<(usize, [u32; 4])> {
    let pad = pixels(MARGIN);
    let mut by_ink: Vec<(usize, [u32; 4])> = drawn
        .iter()
        .enumerate()
        .filter_map(|(i, ([left, top, ..], raster))| {
            raster.inked().map(|[x, y, w, h]| (i, [left + x, top + y, w, h]))
        })
        .collect();
    by_ink.sort_by_key(|&(i, [.., w, h])| (std::cmp::Reverse(u64::from(w) * u64::from(h)), i));
    let mut kept: Vec<(usize, [u32; 4])> = Vec::new();
    for (i, [x, y, w, h]) in by_ink {
        let (tile, raster) = &drawn[i];
        let held: Vec<[u32; 4]> = kept.iter().map(|&(k, _)| drawn[k].0).collect();
        if raster.inked_within(&held) {
            continue;
        }
        let [left, top, tw, th] = *tile;
        let (x0, y0) = (x.saturating_sub(pad).max(left), y.saturating_sub(pad).max(top));
        let (x1, y1) = ((x + w + pad).min(left + tw), (y + h + pad).min(top + th));
        kept.push((i, [x0 - left, y0 - top, x1 - x0, y1 - y0]));
    }
    if kept.is_empty() {
        let first = drawn.first().map(|([.., w, h], _)| (0, [0, 0, *w, *h]));
        return first.into_iter().collect();
    }
    kept.sort_unstable_by_key(|&(i, _)| i);
    kept
}

/// A length in canvas units as pixels: rounded up, and at least one.
fn pixels(units: f64) -> u32 {
    // The cast saturates; a picture that large is cut into tiles anyway.
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped above")]
    let px = (units.max(0.0) * SCALE).ceil().min(f64::from(u32::MAX)) as u32;
    px.max(1)
}

/// Cuts `len` pixels into `(start, len)` spans of at most [`MAX_EDGE`] that
/// overlap their neighbours by [`OVERLAP`]. A length that fits is one span.
fn spans(len: u32) -> Vec<(u32, u32)> {
    let stride = MAX_EDGE - OVERLAP;
    let mut out = vec![(0, len.min(MAX_EDGE))];
    while let Some(&(start, n)) = out.last()
        && start + n < len
    {
        let next = start + stride;
        out.push((next, (len - next).min(MAX_EDGE)));
    }
    out
}

fn union(a: Rect, b: Rect) -> Rect {
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    Rect::new(x, y, (a.x + a.w).max(b.x + b.w) - x, (a.y + a.h).max(b.y + b.h) - y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_length_that_fits_is_one_span() {
        assert_eq!(spans(1), vec![(0, 1)]);
        assert_eq!(spans(MAX_EDGE), vec![(0, MAX_EDGE)]);
    }

    #[test]
    fn spans_overlap_by_the_constant_and_cover_the_length() {
        for len in [MAX_EDGE + 1, 3000, 3 * MAX_EDGE, 10_000] {
            let s = spans(len);
            assert!(s.len() > 1);
            assert!(s.iter().all(|&(_, n)| n <= MAX_EDGE && n > OVERLAP));
            for pair in s.windows(2) {
                assert_eq!(pair[0].0 + pair[0].1 - pair[1].0, OVERLAP, "{len}: {s:?}");
            }
            let (start, n) = s[s.len() - 1];
            assert_eq!(start + n, len);
        }
    }

    #[test]
    fn pixels_round_up_and_are_never_zero() {
        assert_eq!(pixels(0.0), 1);
        assert_eq!(pixels(-5.0), 1);
        assert_eq!(pixels(100.0), 35);
        assert_eq!(pixels(100.1), 36);
    }
}
