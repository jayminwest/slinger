//! The page under the ink (docs/design/pdf.md, "Sling"): a [`Backdrop`] is
//! the pixels of one `pdf` item over one canvas area, at [`SCALE`], and the
//! PDF's [`Word`]s there. Only the app can render a PDF; the core reads what
//! it rendered as raw RGB, with no decoder, and draws ink over it.

use serde::Deserialize;

use super::pixels;
use crate::canvas::{Canvas, ItemId, PageId, Point, Rect, Shape};
use crate::{Error, Result};

/// One word of a PDF, where it lies on the canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    /// A run without whitespace.
    pub text: String,
    /// Its box, in canvas units.
    pub frame: Rect,
}

/// Pixels of one `pdf` item over `area`, and its words there.
#[derive(Debug, Clone, PartialEq)]
pub struct Backdrop {
    /// The `pdf` item it shows.
    pub item: ItemId,
    /// The canvas area the pixels cover.
    pub area: Rect,
    /// The pixels across and down: [`pixels`] of the area's size.
    pub width: u32,
    pub height: u32,
    /// 8-bit R, G, B, rows top to bottom, `width * height * 3` bytes.
    pub rgb: Vec<u8>,
    /// The words that overlap the area, in the PDF's reading order.
    pub words: Vec<Word>,
}

impl Backdrop {
    /// A backdrop of `item` over `area`. Its size in pixels follows from the
    /// area, by the render's own rounding.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] when `rgb` is not `width * height * 3` bytes.
    pub fn new(item: ItemId, area: Rect, rgb: Vec<u8>, words: Vec<Word>) -> Result<Self> {
        let (width, height) = (pixels(area.w), pixels(area.h));
        let want = u64::from(width) * u64::from(height) * 3;
        if rgb.len() as u64 != want {
            return Err(Error::Invalid {
                message: format!(
                    "the backdrop of {item} is {} bytes, not {width} x {height} x 3 = {want}",
                    rgb.len()
                ),
                hint: Some("a backdrop is width × height × 3 bytes of raw RGB".into()),
            });
        }
        Ok(Self { item, area, width, height, rgb, words })
    }

    /// The pixel nearest the canvas point `p`, which lies in the area.
    pub(super) fn pixel(&self, p: Point) -> [u8; 3] {
        let at = |v: f64, start: f64, len: f64, n: u32| {
            let cell = ((v - start) / len * f64::from(n)).floor();
            #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped")]
            let cell = cell.clamp(0.0, f64::from(n - 1)) as usize;
            cell
        };
        let col = at(p.x, self.area.x, self.area.w, self.width);
        let row = at(p.y, self.area.y, self.area.h, self.height);
        let i = (row * self.width as usize + col) * 3;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }
}

impl Word {
    /// The words of a words file: one JSON array of
    /// `{"text", "x", "y", "w", "h"}`, boxes in canvas tenths.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] when it is not such an array.
    pub fn read(json: &[u8]) -> Result<Vec<Self>> {
        #[derive(Deserialize)]
        struct Raw {
            text: String,
            x: f64,
            y: f64,
            w: f64,
            h: f64,
        }
        let raw: Vec<Raw> = serde_json::from_slice(json).map_err(|e| Error::Invalid {
            message: format!("the backdrop's words do not parse: {e}"),
            hint: Some(r#"write one JSON array of {"text","x","y","w","h"} in tenths"#.into()),
        })?;
        let words = raw.into_iter().map(|r| Self {
            text: r.text,
            frame: Rect::new(r.x / 10.0, r.y / 10.0, r.w / 10.0, r.h / 10.0),
        });
        Ok(words.collect())
    }
}

/// The backdrops that draw on `page`, each with the part it may cover (its
/// item's frame ∩ its area), in its item's draw order. A backdrop whose item
/// is gone, is not a `pdf` item or is on another page is skipped (B4).
#[must_use]
pub fn usable<'a>(
    canvas: &Canvas,
    page: PageId,
    backdrops: &'a [Backdrop],
) -> Vec<(&'a Backdrop, Rect)> {
    let order: Vec<ItemId> = canvas.drawn(page).map(crate::canvas::Item::id).collect();
    let mut found: Vec<(usize, &Backdrop, Rect)> = backdrops
        .iter()
        .filter_map(|b| {
            let item = canvas.item(b.item).filter(|i| i.page() == page)?;
            let Shape::Pdf { frame, .. } = item.shape() else { return None };
            let at = order.iter().position(|id| *id == b.item)?;
            Some((at, b, meet(*frame, b.area)?))
        })
        .collect();
    found.sort_by_key(|(at, ..)| *at);
    found.into_iter().map(|(_, b, clip)| (b, clip)).collect()
}

/// The rectangle two share, when it has an area.
#[must_use]
pub fn meet(one: Rect, other: Rect) -> Option<Rect> {
    let (left, top) = (one.x.max(other.x), one.y.max(other.y));
    let right = (one.x + one.w).min(other.x + other.w);
    let bottom = (one.y + one.h).min(other.y + other.h);
    (right > left && bottom > top).then(|| Rect::new(left, top, right - left, bottom - top))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backdrop_of_another_length_is_invalid() {
        let area = Rect::new(0.0, 0.0, 100.0, 20.0);
        let ok = Backdrop::new(ItemId::new(1), area, vec![0; 35 * 7 * 3], vec![]);
        assert_eq!((ok.as_ref().unwrap().width, ok.unwrap().height), (35, 7));
        let short = Backdrop::new(ItemId::new(1), area, vec![0; 10], vec![]).unwrap_err();
        assert_eq!(short.code(), "invalid");
        assert!(short.hint().unwrap().contains("width × height × 3"), "{short}");
    }

    #[test]
    fn words_read_in_tenths_and_junk_is_invalid() {
        let words = Word::read(br#"[{"text":"Packet","x":2410,"y":1033,"w":402,"h":118}]"#);
        let frame = Rect::new(241.0, 103.3, 40.2, 11.8);
        assert_eq!(words.unwrap(), [Word { text: "Packet".into(), frame }]);
        assert_eq!(Word::read(b"{}").unwrap_err().code(), "invalid");
    }

    #[test]
    fn the_nearest_pixel_is_taken() {
        let area = Rect::new(10.0, 10.0, 20.0, 20.0);
        let mut rgb = vec![0; 7 * 7 * 3];
        rgb[(7 * 6 + 6) * 3] = 9;
        let b = Backdrop::new(ItemId::new(1), area, rgb, vec![]).unwrap();
        assert_eq!(b.pixel(Point::new(29.9, 29.9)), [9, 0, 0]);
        assert_eq!(b.pixel(Point::new(10.0, 10.0)), [0, 0, 0]);
    }
}
