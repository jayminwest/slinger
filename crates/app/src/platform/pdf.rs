//! PDFs (docs/design/pdf.md): every `CoreGraphics` PDF call in the project.
//! [`pages`] reads each page's size as displayed, [`draw`] draws the part of
//! a page a `pdf` item shows into its frame of the canvas view, and
//! [`choose`] asks for the PDF to open. This is the only code that knows a
//! PDF's own space (origin at the bottom left, y up, the page unrotated);
//! everything else measures a page as displayed: its crop box, its
//! `/Rotate` applied, origin at the top left, y down.
//!
//! A document is opened once per file and kept while the app runs
//! ([`draw`], main thread); [`pages`] opens its own, on any thread.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use objc2::MainThreadMarker;
use objc2_app_kit::{NSGraphicsContext, NSModalResponseOK, NSOpenPanel};
use objc2_core_foundation::{CFRetained, CFURL, CGAffineTransform, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGContext, CGPDFBox, CGPDFDocument, CGPDFPage};
use objc2_foundation::{NSArray, NSString};

use crate::layout::Rect;

thread_local! {
    /// Each PDF file drawn, opened once: `None` for one that cannot be read.
    static DOCUMENTS: RefCell<HashMap<PathBuf, Option<CFRetained<CGPDFDocument>>>> =
        RefCell::default();
}

/// Each page's size of the PDF at `path`, in points as displayed: its crop
/// box, turned by its `/Rotate`. Runs on any thread.
///
/// # Errors
///
/// For the user: a file that is not a PDF this Mac can read, or one locked
/// with a password.
pub fn pages(path: &Path) -> Result<Vec<(f64, f64)>, String> {
    let name =
        path.file_name().map_or_else(|| path.display().to_string(), |n| n.display().to_string());
    let Some(document) = open(path) else {
        return Err(format!("{name} is not a PDF this Mac can read"));
    };
    if CGPDFDocument::is_encrypted(Some(&document)) && !CGPDFDocument::is_unlocked(Some(&document))
    {
        return Err(format!("{name} is locked with a password: open it without one"));
    }
    let count = CGPDFDocument::number_of_pages(Some(&document));
    let sizes = (1..=count).map(|n| {
        let page = CGPDFDocument::page(Some(&document), n);
        let (_, size, _) = space(page.as_deref());
        (size.width, size.height)
    });
    Ok(sizes.collect())
}

/// Draws the part `shown` (page points, as displayed) of page `page`
/// (counted from one) of the PDF at `file` into `frame` of the canvas view drawing now,
/// stretched to fill it, clipped to it, on white. False, with nothing
/// drawn, when the file is not a PDF that has that page.
pub(super) fn draw(file: &Path, page: u32, shown: Rect, frame: Rect) -> bool {
    let Some(document) = DOCUMENTS.with_borrow_mut(|open_ones| {
        open_ones.entry(file.to_path_buf()).or_insert_with(|| open(file)).clone()
    }) else {
        return false;
    };
    let Ok(number) = usize::try_from(page) else { return false };
    let Some(page) = CGPDFDocument::page(Some(&document), number) else { return false };
    if shown.width <= 0.0 || shown.height <= 0.0 {
        return false;
    }
    let Some(context) = NSGraphicsContext::currentContext() else { return false };
    let cg = context.CGContext();
    let cg = Some(&*cg);
    let (to_shown, _, _) = space(Some(&page));
    let across = frame.width / shown.width;
    let down = frame.height / shown.height;
    // A point of the page as displayed goes to the frame.
    let place = CGAffineTransform {
        a: across * to_shown[0],
        c: across * to_shown[1],
        tx: frame.x + across * (to_shown[2] - shown.x),
        b: down * to_shown[3],
        d: down * to_shown[4],
        ty: frame.y + down * (to_shown[5] - shown.y),
    };
    let area = CGRect::new(CGPoint::new(frame.x, frame.y), CGSize::new(frame.width, frame.height));
    CGContext::save_g_state(cg);
    CGContext::clip_to_rect(cg, area);
    CGContext::set_rgb_fill_color(cg, 1.0, 1.0, 1.0, 1.0);
    CGContext::fill_rect(cg, area);
    CGContext::concat_ctm(cg, place);
    CGContext::draw_pdf_page(cg, Some(&page));
    CGContext::restore_g_state(cg);
    true
}

/// Asks for one PDF to open, in the open panel; `None` when none was
/// chosen, or off the main thread.
pub fn choose() -> Option<PathBuf> {
    let mtm = MainThreadMarker::new()?;
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(false);
    let pdf = NSString::from_str("pdf");
    allow(&panel, &NSArray::from_slice(&[&*pdf]));
    if panel.runModal() != NSModalResponseOK {
        return None;
    }
    panel.URLs().firstObject()?.to_file_path()
}

#[expect(
    deprecated,
    reason = "allowedContentTypes needs UniformTypeIdentifiers, a crate more for one type"
)]
fn allow(panel: &NSOpenPanel, types: &NSArray<NSString>) {
    panel.setAllowedFileTypes(Some(types));
}

/// The PDF at `path`, opened; `None` when it cannot be.
fn open(path: &Path) -> Option<CFRetained<CGPDFDocument>> {
    let url = CFURL::from_file_path(path)?;
    CGPDFDocument::with_url(Some(&url))
}

/// How a page lies: the map from its own space to its space as displayed,
/// as `[p, q, m, r, s, n]` with `x = p·u + q·v + m` and `y = r·u + s·v + n`;
/// its size as displayed; and its crop box. A missing page is empty.
fn space(page: Option<&CGPDFPage>) -> ([f64; 6], CGSize, CGRect) {
    let crop = CGPDFPage::box_rect(page, CGPDFBox::CropBox);
    let turn = CGPDFPage::rotation_angle(page);
    let map = displayed(crop, turn);
    let (w, h) = (crop.size.width, crop.size.height);
    let size = if turn.rem_euclid(180) == 90 { CGSize::new(h, w) } else { CGSize::new(w, h) };
    (map, size, crop)
}

/// The map from a page's own space to its space as displayed (origin at
/// the top left, y down), for its crop box `crop` turned `turn` degrees
/// clockwise (`/Rotate`, a multiple of 90).
fn displayed(crop: CGRect, turn: i32) -> [f64; 6] {
    let (x0, y0) = (crop.origin.x, crop.origin.y);
    let (x1, y1) = (x0 + crop.size.width, y0 + crop.size.height);
    match turn.rem_euclid(360) {
        90 => [0.0, 1.0, -y0, 1.0, 0.0, -x0],
        180 => [-1.0, 0.0, x1, 0.0, 1.0, -y0],
        270 => [0.0, -1.0, y1, -1.0, 0.0, x1],
        _ => [1.0, 0.0, -x0, 0.0, -1.0, y1],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where `map` puts the page point `(u, v)`.
    fn at(map: [f64; 6], point: (f64, f64)) -> (f64, f64) {
        let across = map[0] * point.0 + map[1] * point.1 + map[2];
        (across, map[3] * point.0 + map[4] * point.1 + map[5])
    }

    #[test]
    fn a_page_as_displayed_has_its_top_left_at_the_origin_whichever_way_it_turns() {
        let crop = CGRect::new(CGPoint::new(10.0, 20.0), CGSize::new(600.0, 800.0));
        // The page's own top left, top right and bottom left.
        let corners = [(10.0, 820.0), (610.0, 820.0), (10.0, 20.0)];
        let expect = |turn: i32| corners.map(|c| at(displayed(crop, turn), c));
        assert_eq!(expect(0), [(0.0, 0.0), (600.0, 0.0), (0.0, 800.0)]);
        // A quarter turn clockwise: the top left goes to the top right.
        assert_eq!(expect(90), [(800.0, 0.0), (800.0, 600.0), (0.0, 0.0)]);
        assert_eq!(expect(180), [(600.0, 800.0), (0.0, 800.0), (600.0, 0.0)]);
        assert_eq!(expect(-90), [(0.0, 600.0), (0.0, 0.0), (800.0, 600.0)]);
    }
}
