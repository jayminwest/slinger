//! Which parts of PDF pages a sling needs (docs/design/pdf.md,
//! `sling_backdrops`): the area of each `pdf` item under each new cluster's
//! image. The core decides; the app renders exactly these and passes them
//! back as [`crate::render::Backdrop`]s.

use super::build::{Ask, Read};
use crate::Result;
use crate::canvas::{Item, ItemId, Op, Rect, Shape};
use crate::render::{self, MARGIN, meet, pixels};

/// One area of one `pdf` item to render for a sling.
#[derive(Debug, Clone, PartialEq)]
pub struct Area {
    /// The `pdf` item.
    pub item: ItemId,
    /// Its blob id, page number, page size and crop, as stored.
    pub hash: String,
    pub pdf_page: u32,
    pub page_size: (f64, f64),
    pub crop: Option<Rect>,
    /// The item's frame, in canvas units.
    pub frame: Rect,
    /// The part of the frame to render: a new cluster's image frame ∩ the
    /// item's frame.
    pub area: Rect,
    /// The backdrop's size in pixels, by the render's own rounding.
    pub width: u32,
    pub height: u32,
}

/// The areas the packet of `ask` would draw over: one per pair of a cluster
/// in `new` and a `pdf` item whose frame meets the cluster's image frame
/// (its ink's box plus [`MARGIN`]), by item in draw order, then by cluster
/// in reading order. Areas of one item are not merged. Renders nothing.
/// Nothing new, or no new ink over a PDF, is an empty list (B4).
///
/// # Errors
///
/// [`crate::Error::NotFound`] when the page is not on the canvas.
pub fn backdrops(log: &[Op], ask: &Ask<'_>) -> Result<Vec<Area>> {
    let Read { canvas, index, .. } = Read::new(log, ask)?;
    let frames: Vec<Rect> = index
        .render()
        .iter()
        .filter_map(|r| {
            let ink = r.strokes.iter().filter_map(|s| canvas.item(*s)).map(Item::bounds);
            let b = ink.reduce(render::union)?;
            Some(Rect::new(b.x - MARGIN, b.y - MARGIN, b.w + 2.0 * MARGIN, b.h + 2.0 * MARGIN))
        })
        .collect();
    let mut out = Vec::new();
    for item in canvas.drawn(ask.page) {
        let Shape::Pdf { frame, hash, pdf_page, page_size, crop, .. } = item.shape() else {
            continue;
        };
        for area in frames.iter().filter_map(|f| meet(*f, *frame)) {
            out.push(Area {
                item: item.id(),
                hash: hash.clone(),
                pdf_page: *pdf_page,
                page_size: *page_size,
                crop: *crop,
                frame: *frame,
                area,
                width: pixels(area.w),
                height: pixels(area.h),
            });
        }
    }
    Ok(out)
}
