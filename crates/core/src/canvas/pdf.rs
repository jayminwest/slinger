//! The rules of a `pdf` item (docs/design/pdf.md): what a valid one holds,
//! and the part of its page it shows. Numbers only: the core never reads a
//! PDF.

use super::item::invalid;
use super::{Rect, Shape};
use crate::Result;

/// The longest file name a `pdf` item keeps, in bytes.
const NAME_BYTES: usize = 255;

impl Shape {
    /// The part of the page a `pdf` item shows, in page points: its crop,
    /// or the whole page when it has none. `None` for any other kind.
    #[must_use]
    pub fn shown(&self) -> Option<Rect> {
        match self {
            Shape::Pdf { crop: Some(crop), .. } => Some(*crop),
            Shape::Pdf { page_size: (w, h), .. } => Some(Rect::new(0.0, 0.0, *w, *h)),
            _ => None,
        }
    }
}

/// Rejects a `pdf` item that cannot be drawn. Other kinds pass.
pub(super) fn check(shape: &Shape) -> Result<()> {
    let Shape::Pdf { frame, hash, pdf_page, page_size: (pw, ph), crop, name } = shape else {
        return Ok(());
    };
    frame.check("the pdf item")?;
    if frame.w <= 0.0 || frame.h <= 0.0 {
        return Err(invalid("the pdf item has no area", "give its frame a size above 0"));
    }
    hash.parse::<crate::record::BlobId>()?;
    if *pdf_page == 0 {
        return Err(invalid("the pdf page number is 0", "number a PDF's pages from 1"));
    }
    if !(pw.is_finite() && ph.is_finite() && *pw > 0.0 && *ph > 0.0) {
        return Err(invalid(
            "the pdf page size is not above zero",
            "give the page's width and height in tenths of a point, both above 0",
        ));
    }
    if let Some(c) = crop {
        let finite = [c.x, c.y, c.w, c.h].iter().all(|v| v.is_finite());
        let inside = c.x >= 0.0 && c.y >= 0.0 && c.x + c.w <= *pw && c.y + c.h <= *ph;
        if !finite || c.w <= 0.0 || c.h <= 0.0 || !inside {
            return Err(invalid(
                "the pdf crop is not a part of the page",
                "give a crop with a size above 0 inside [0, 0, pagesize], or none",
            ));
        }
    }
    if let Some(name) = name
        && (name.is_empty()
            || name.len() > NAME_BYTES
            || name.contains('/')
            || name.chars().any(char::is_control))
    {
        return Err(invalid(
            format!("{name:?} is not a file name"),
            "give the PDF's file name without its directory: 1 to 255 bytes, no `/`",
        ));
    }
    Ok(())
}
