//! A PDF opened as a canvas (docs/design/pdf.md), as arithmetic: the
//! canvas id made from its file name ([`canvas_id`]), and its pages stacked
//! top to bottom on the canvas's first page ([`stack`]), each a `pdf` item
//! ([`pages`]). And the part of its page a `pdf` item shows
//! ([`Shape::shown`]). Reader mode: which canvases are a PDF's
//! ([`reader`]) and the column of pages a pane fits to its width
//! ([`column`], [`crate::layout::Viewport::reading`]). Pure: only
//! `platform` reads a PDF.

use std::path::Path;

use super::canvas::{Item, Shape};
use crate::layout::Rect;

/// The width of the widest page of an opened PDF, in canvas units.
pub const PAGE_WIDTH: f64 = 1600.0;
/// The space between two pages of an opened PDF, in canvas units.
pub const PAGE_GAP: f64 = 40.0;
/// The longest part of a canvas id made from a file's stem.
const STEM: usize = 56;
/// The longest file name a `pdf` item keeps, in bytes.
const NAME_BYTES: usize = 255;

/// The canvas id for the PDF `file`, the `try_n`th id tried (from 1): its
/// stem lowercased, each character outside `a-z 0-9 - _` made `-`, cut to
/// 56 characters (`pdf` when nothing is left), then `-<n>` from the second
/// try on, then `.pdf`. `Paper (v2).pdf` is `paper--v2-.pdf`.
pub fn canvas_id(file: &Path, try_n: u32) -> String {
    let stem = file.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    let keep = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_';
    let mut id: String = stem.chars().map(|c| if keep(c) { c } else { '-' }).take(STEM).collect();
    if id.is_empty() {
        id.push_str("pdf");
    }
    if try_n > 1 {
        id.push('-');
        id.push_str(&try_n.to_string());
    }
    id.push_str(".pdf");
    id
}

/// The frame of each page of sizes `pages` (page points, as displayed):
/// one scale for the whole document, the widest page [`PAGE_WIDTH`]
/// wide, each page centred on that column, the first at the top and each
/// next [`PAGE_GAP`] under the one before.
pub fn stack(pages: &[(f64, f64)]) -> Vec<Rect> {
    let widest = pages.iter().map(|p| p.0).fold(0.0_f64, f64::max);
    let k = if widest > 0.0 { PAGE_WIDTH / widest } else { 1.0 };
    let mut y = 0.0;
    pages
        .iter()
        .map(|&(w, h)| {
            let (width, height) = (w * k, h * k);
            let frame = Rect { x: (PAGE_WIDTH - width) / 2.0, y, width, height };
            y += height + PAGE_GAP;
            frame
        })
        .collect()
}

/// One `pdf` item per page of sizes `pages`, in page order, stacked
/// ([`stack`]), each showing its whole page of the PDF that is blob
/// `hash`, named after `file` when its name is one the canvas keeps.
pub fn pages(pages: &[(f64, f64)], hash: &str, file: &Path) -> Vec<Shape> {
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).filter(|n| {
        !n.is_empty()
            && n.len() <= NAME_BYTES
            && !n.contains('/')
            && !n.chars().any(char::is_control)
    });
    stack(pages)
        .into_iter()
        .zip(pages)
        .zip(1_u32..)
        .map(|((frame, &page_size), pdf_page)| Shape::Pdf {
            frame,
            hash: hash.to_string(),
            pdf_page,
            page_size,
            crop: None,
            name: name.clone(),
        })
        .collect()
}

/// Whether `canvas` is a PDF's canvas, which a pane shows in reader
/// mode: its id ends in `.pdf`, as [`canvas_id`] makes it. Nothing else
/// decides it (docs/design/pdf.md).
#[expect(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "a canvas id, not a file: the design keys on `.pdf` exactly"
)]
pub fn reader(canvas: &str) -> bool {
    canvas.ends_with(".pdf")
}

/// The column of `pdf` items in `items`, a page as shown: the box around
/// their frames. `None` when the page has none.
pub fn column<'a>(items: impl IntoIterator<Item = &'a Item>) -> Option<Rect> {
    let frames = items.into_iter().filter_map(|i| match i.shape {
        Shape::Pdf { frame, .. } => Some(frame),
        _ => None,
    });
    frames.reduce(Rect::union)
}

impl Shape {
    /// The part of its page a `pdf` item shows, in page points: its crop,
    /// or the whole page. `None` for any other kind.
    pub fn shown(&self) -> Option<Rect> {
        match self {
            Self::Pdf { crop: Some(crop), .. } => Some(*crop),
            Self::Pdf { page_size: (width, height), .. } => {
                Some(Rect { x: 0.0, y: 0.0, width: *width, height: *height })
            }
            _ => None,
        }
    }

    /// A `pdf` item in `frame` instead; any other shape as it is.
    pub fn reframed(&self, frame: Rect) -> Shape {
        match self {
            Self::Pdf { hash, pdf_page, page_size, crop, name, .. } => Self::Pdf {
                frame,
                hash: hash.clone(),
                pdf_page: *pdf_page,
                page_size: *page_size,
                crop: *crop,
                name: name.clone(),
            },
            shape => shape.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
