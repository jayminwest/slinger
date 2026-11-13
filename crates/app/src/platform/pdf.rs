//! PDFs (docs/design/pdf.md): every `CoreGraphics` PDF call in the project.
//! [`pages`] reads each page's size as displayed, [`draw`] draws the part of
//! a page a `pdf` item shows into its frame of the canvas view (the open
//! panel, `panel`, asks for the PDF to open). This is the only code that knows a
//! PDF's own space (origin at the bottom left, y up, the page unrotated);
//! everything else measures a page as displayed: its crop box, its
//! `/Rotate` applied, origin at the top left, y down.
//!
//! A document is opened once per file and kept while the app runs
//! ([`draw`], main thread); [`pages`] opens its own, on any thread.
//!
//! [`backdrop`] renders the part of a page a sling draws ink over into a
//! raw RGB file and writes the page's words there, with `PDFKit`; it runs on
//! the sling's thread, with that thread's own documents, never `AppKit`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use objc2::msg_send;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2_app_kit::NSGraphicsContext;
use objc2_core_foundation::{CFRetained, CFURL, CGAffineTransform, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImageAlphaInfo, CGPDFBox, CGPDFDocument,
    CGPDFPage,
};
use objc2_foundation::{NSString, NSURL};
use slinger_control::BackdropArea;

use crate::layout::Rect;

// PDFKit holds the words of a page; nothing else of it is used.
#[link(name = "PDFKit", kind = "framework")]
unsafe extern "C" {}

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

/// Renders `area` of a `pdf` item for a sling (docs/design/pdf.md): the
/// part of its page under the area, from the PDF at `pdf`, as raw RGB of
/// exactly `width × height` pixels on white into `image`, and the page's
/// words that overlap the area into `words`, boxes in canvas tenths (no
/// file when there are none). Any thread. False when the page cannot be
/// rendered or a file written.
pub fn backdrop(area: &BackdropArea, pdf: &Path, image: &Path, words: &Path) -> bool {
    #[expect(clippy::cast_precision_loss, reason = "tenths of canvas units are far below 2^52")]
    let tenths = |v: [i64; 4]| v.map(|n| n as f64 / 10.0);
    let ([fx, fy, fw, fh], [ax, ay, aw, ah]) = (tenths(area.frame), tenths(area.area));
    let whole = [0, 0, area.pagesize[0], area.pagesize[1]];
    let [sx, sy, sw, sh] = tenths(area.crop.unwrap_or(whole));
    let (across, down) = (f64::from(area.width), f64::from(area.height));
    if [fw, fh, aw, ah, sw, sh, across, down].iter().any(|v| *v <= 0.0) {
        return false;
    }
    let Some(page) = DOCUMENTS
        .with_borrow_mut(|open_ones| {
            open_ones.entry(pdf.to_path_buf()).or_insert_with(|| open(pdf)).clone()
        })
        .and_then(|d| CGPDFDocument::page(Some(&d), area.pdfpage as usize))
    else {
        return false;
    };
    let (map, _, _) = space(Some(&page));
    // A page point as displayed, in canvas units.
    let to_canvas = |(dx, dy): (f64, f64)| (fx + (dx - sx) * fw / sw, fy + (dy - sy) * fh / sh);
    // Canvas units to the bitmap's pixels, whose y goes up from the bottom.
    let (kx, ky) = (fw / sw * across / aw, fh / sh * down / ah);
    let (left, top) = to_canvas((0.0, 0.0));
    let place = CGAffineTransform {
        a: kx * map[0],
        c: kx * map[1],
        tx: kx * map[2] + (left - ax) * across / aw,
        b: -ky * map[3],
        d: -ky * map[4],
        ty: down - (ky * map[5] + (top - ay) * down / ah),
    };
    let Some(rgb) = render(&page, place, area.width as usize, area.height as usize) else {
        return false;
    };
    if std::fs::write(image, rgb).is_err() {
        return false;
    }
    let found: Vec<String> = page_words(pdf, area.pdfpage, map)
        .into_iter()
        .filter_map(|(text, [x0, y0, x1, y1])| {
            let ((left, top), (right, bottom)) = (to_canvas((x0, y0)), to_canvas((x1, y1)));
            let meets = left < ax + aw && ax < right && top < ay + ah && ay < bottom;
            #[expect(clippy::cast_possible_truncation, reason = "canvas tenths fit an i64")]
            let tenths = |v: f64| (v * 10.0).round() as i64;
            let text = serde_json::to_string(&text).ok()?;
            meets.then(|| {
                let (x, y) = (tenths(left), tenths(top));
                let (wide, high) = (tenths(right - left), tenths(bottom - top));
                format!(r#"{{"text":{text},"x":{x},"y":{y},"w":{wide},"h":{high}}}"#)
            })
        })
        .collect();
    found.is_empty() || std::fs::write(words, format!("[{}]", found.join(","))).is_ok()
}

/// `page` drawn by `place` into a bitmap `width` by `height` on white, as
/// raw RGB rows from the top. `None` when no bitmap can be made.
fn render(
    page: &CGPDFPage,
    place: CGAffineTransform,
    width: usize,
    height: usize,
) -> Option<Vec<u8>> {
    let mut rgbx = vec![255u8; width * height * 4];
    let space = CGColorSpace::new_device_rgb();
    // SAFETY: the buffer is `width * height * 4` bytes, in rows of
    // `width * 4`, and outlives the context, which this block drops.
    let cg = unsafe {
        CGBitmapContextCreate(
            rgbx.as_mut_ptr().cast(),
            width,
            height,
            8,
            width * 4,
            space.as_deref(),
            CGImageAlphaInfo::NoneSkipLast.0,
        )
    }?;
    let cg = Some(&*cg);
    #[expect(clippy::cast_precision_loss, reason = "a bitmap's size is far below 2^52")]
    let size = CGSize::new(width as f64, height as f64);
    CGContext::set_rgb_fill_color(cg, 1.0, 1.0, 1.0, 1.0);
    CGContext::fill_rect(cg, CGRect::new(CGPoint::new(0.0, 0.0), size));
    CGContext::concat_ctm(cg, place);
    CGContext::draw_pdf_page(cg, Some(page));
    // The bitmap's first row in memory is its top.
    Some(rgbx.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect())
}

/// The words of page `number` (from one) of the PDF at `path`, in the
/// page's reading order, each with its box `[x0, y0, x1, y1]` in page
/// points as displayed (`map`, from [`space`]). A word is a run without
/// whitespace. Empty for a page with no text, or when `PDFKit` cannot read
/// it.
fn page_words(path: &Path, number: u32, map: [f64; 6]) -> Vec<(String, [f64; 4])> {
    let Some(class) = AnyClass::get(c"PDFDocument") else { return Vec::new() };
    let Some(path) = path.to_str() else { return Vec::new() };
    let url = NSURL::fileURLWithPath(&NSString::from_str(path));
    let Some(index) = (number as usize).checked_sub(1) else { return Vec::new() };
    // SAFETY: `PDFDocument`'s `initWithURL:` takes an `NSURL` and gives a
    // document or nil; `pageAtIndex:` an `NSUInteger` below the count;
    // `string` and `characterBoundsAtIndex:` are `PDFPage`'s, with an index
    // into that string in UTF-16 units.
    unsafe {
        let made: Allocated<AnyObject> = msg_send![class, alloc];
        let document: Option<Retained<AnyObject>> = msg_send![made, initWithURL: &*url];
        let Some(document) = document else { return Vec::new() };
        let count: usize = msg_send![&*document, pageCount];
        if index >= count {
            return Vec::new();
        }
        let page: Option<Retained<AnyObject>> = msg_send![&*document, pageAtIndex: index];
        let Some(page) = page else { return Vec::new() };
        let text: Option<Retained<NSString>> = msg_send![&*page, string];
        let Some(text) = text else { return Vec::new() };
        let mut out: Vec<(String, [f64; 4])> = Vec::new();
        let (mut word, mut frame) = (String::new(), None::<[f64; 4]>);
        let mut at: isize = 0;
        for c in text.to_string().chars().chain([' ']) {
            if c.is_whitespace() {
                if let Some(frame) = frame.take().filter(|_| !word.is_empty()) {
                    out.push((std::mem::take(&mut word), frame));
                }
                word.clear();
            } else {
                let r: CGRect = msg_send![&*page, characterBoundsAtIndex: at];
                word.push(c);
                if r.size.width > 0.0 || r.size.height > 0.0 {
                    let b = shown_box(map, r);
                    frame = Some(frame.map_or(b, |f| {
                        [f[0].min(b[0]), f[1].min(b[1]), f[2].max(b[2]), f[3].max(b[3])]
                    }));
                }
            }
            at += c.len_utf16().cast_signed();
        }
        out
    }
}

/// A box of a page's own space, `[x0, y0, x1, y1]` as displayed by `map`.
fn shown_box(map: [f64; 6], r: CGRect) -> [f64; 4] {
    let (x0, y0) = (r.origin.x, r.origin.y);
    let (x1, y1) = (x0 + r.size.width, y0 + r.size.height);
    let at = |u: f64, v: f64| (map[0] * u + map[1] * v + map[2], map[3] * u + map[4] * v + map[5]);
    let corners = [at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1)];
    let xs = corners.map(|c| c.0);
    let ys = corners.map(|c| c.1);
    let lo = |v: [f64; 4]| v.into_iter().fold(f64::INFINITY, f64::min);
    let hi = |v: [f64; 4]| v.into_iter().fold(f64::NEG_INFINITY, f64::max);
    [lo(xs), lo(ys), hi(xs), hi(ys)]
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

    /// A page 200 by 100 points with "Hello world" in 20-point Helvetica,
    /// its baseline 40 points from the top, starting 20 points in.
    const HELLO: &str = "%PDF-1.4
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>
endobj
4 0 obj
<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>
endobj
5 0 obj
<< /Length 41 >>
stream
BT /F1 20 Tf 20 60 Td (Hello world) Tj ET
endstream
endobj
xref
0 6
0000000000 65535 f 
0000000009 00000 n 
0000000058 00000 n 
0000000115 00000 n 
0000000241 00000 n 
0000000311 00000 n 
trailer
<< /Size 6 /Root 1 0 R >>
startxref
402
%%EOF
";

    /// The backdrop of the area `[x, y, w, h]` (tenths) of `HELLO` shown
    /// whole, or its `crop`, in a frame `[0, 0, w, 1000]`: its pixels and
    /// the words file, if any.
    fn hello(crop: Option<[i64; 4]>, area: [i64; 4]) -> (Vec<u8>, Option<String>) {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("hello.pdf");
        std::fs::write(&file, HELLO).unwrap();
        let wide = crop.map_or(2000, |c| c[2]);
        let pixels = |tenths: i64| u32::try_from((tenths * 35 + 999) / 1000).unwrap();
        let entry = BackdropArea {
            item: "i1".into(),
            hash: String::new(),
            pdfpage: 1,
            pagesize: [2000, 1000],
            crop,
            frame: [0, 0, wide, 1000],
            area,
            width: pixels(area[2]),
            height: pixels(area[3]),
        };
        let (image, words) = (dir.path().join("1.rgb"), dir.path().join("1.json"));
        assert!(backdrop(&entry, &file, &image, &words));
        let rgb = std::fs::read(image).unwrap();
        assert_eq!(rgb.len(), (entry.width * entry.height * 3) as usize);
        (rgb, std::fs::read_to_string(words).ok())
    }

    #[test]
    fn a_backdrop_is_the_page_under_the_area_and_the_words_on_it() {
        let (rgb, words) = hello(None, [0, 0, 2000, 1000]);
        let dark = |x: usize, y: usize| rgb[(y * 70 + x) * 3] < 128;
        assert!(!dark(0, 0) && !dark(69, 34), "white paper");
        // The text runs from 20 points in, its body above the baseline at 40.
        assert!((10..30).any(|x| (8..14).any(|y| dark(x, y))), "the text is drawn where it is");
        assert!((0..70).all(|x| (20..35).all(|y| !dark(x, y))), "nothing below the line");
        let words: serde_json::Value = serde_json::from_str(&words.unwrap()).unwrap();
        let texts: Vec<&str> =
            words.as_array().unwrap().iter().map(|w| w["text"].as_str().unwrap()).collect();
        assert_eq!(texts, ["Hello", "world"]);
        let (x, y, h) =
            (&words[0]["x"], words[0]["y"].as_i64().unwrap(), words[0]["h"].as_i64().unwrap());
        assert!((200..=230).contains(&x.as_i64().unwrap()), "{words}");
        assert!(y > 150 && y + h < 480 && h > 100, "{words}");
    }

    #[test]
    fn a_crop_s_backdrop_holds_only_what_it_shows() {
        let (rgb, words) = hello(Some([1000, 0, 1000, 1000]), [0, 0, 1000, 500]);
        assert_eq!(rgb.len(), 35 * 18 * 3);
        let words: serde_json::Value = serde_json::from_str(&words.unwrap()).unwrap();
        assert_eq!(words.as_array().unwrap().len(), 1, "{words}");
        assert_eq!(words[0]["text"], "world");
        let (_, none) = hello(None, [0, 700, 2000, 300]);
        assert_eq!(none, None, "no words below the line: no file");
    }

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
