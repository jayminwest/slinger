//! Images of canvas items: a set of items, or a box of a page, in; PNG
//! images out. The rules are the image rules of `docs/design/packet.md`.
//!
//! The scale is fixed ([`SCALE`]) and does not depend on any viewport, so the
//! same ink is always the same number of pixels and handwriting stays
//! readable (SPEC R3). Ink is drawn in its own colour on white. An image whose
//! long edge is over [`MAX_EDGE`] pixels is cut into tiles that overlap by
//! [`OVERLAP`] pixels, so a word cut at one tile's edge is whole in the next
//! (Spike B finding 3).
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
/// canvas are skipped; when none is left there are no images.
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
    draw(&found, frame, id)
}

/// Draws what is on `page` inside `area`, cut at its edges: every item that
/// touches the area, in drawing order, on white. Images are named after `id`.
/// A missing page draws as an empty, white area.
#[must_use]
pub fn crop(canvas: &Canvas, page: PageId, area: Rect, id: &str) -> Vec<Image> {
    let found: Vec<&Item> =
        canvas.items_in_box(page, area).into_iter().filter_map(|i| canvas.item(i)).collect();
    draw(&found, area, id)
}

/// The frame size, in canvas units, that holds `text` as it is drawn when
/// wrapped to at most `columns` characters a line (at least one): one
/// character cell per character, a line per row.
#[must_use]
pub fn text_size(text: &str, columns: usize) -> (f64, f64) {
    let rows = raster::wrap(text, columns.max(1));
    let longest = rows.iter().map(Vec::len).max().unwrap_or(0);
    let cells = |n: usize| u32::try_from(n).map_or(f64::from(u32::MAX), f64::from);
    (cells(longest) * raster::TEXT_SIZE, cells(rows.len()) * raster::TEXT_SIZE)
}

/// Draws `items` into the picture of `frame`, as one image or as tiles.
fn draw(items: &[&Item], frame: Rect, id: &str) -> Vec<Image> {
    let (width, height) = (pixels(frame.w), pixels(frame.h));
    let tiles: Vec<(u32, u32, u32, u32)> = spans(height)
        .into_iter()
        .flat_map(|(y, h)| spans(width).into_iter().map(move |(x, w)| (x, y, w, h)))
        .collect();
    let single = tiles.len() == 1;
    tiles
        .into_iter()
        .enumerate()
        .map(|(n, (x, y, w, h))| {
            let mut raster = Raster::new(frame, x, y, w, h);
            for item in items {
                raster.item(item);
            }
            let name = if single { format!("{id}.png") } else { format!("{id}-{}.png", n + 1) };
            Image { name, x, y, width: w, height: h, png: raster.png() }
        })
        .collect()
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
