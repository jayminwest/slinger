//! The `crop-pdf` tool (docs/design/pdf.md, "Snippets"). A drag draws a
//! box; the lift crops the topmost `pdf` item under the press to it (with
//! none there, the topmost one the box touches), as a new `pdf` item for
//! the clipboard ([`crate::mirror::clip::snippet`]): the same PDF, page and
//! name, `crop` the box mapped to page points, its frame the box's size.
//! The canvas does not change. A box that touches no `pdf` item, or a drag
//! shorter than [`MIN_DRAG`], does nothing. Works on any canvas, a PDF's in
//! reader mode too: select and lasso skip its pages, this tool does not.
//!
//! Pure: [`cropped`] is the arithmetic, from the item alone.

use std::borrow::Cow;

use crate::config::Theme;
use crate::layout::{Point, Rect};
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::shapes::{MIN_DRAG, Press};
use crate::scene::tools::Tool;

/// The id of the box drawn under the pen: apart from every other mark's.
const BOX: ItemId = ItemId(u64::MAX - 12);
/// The width of the box's dashed line, in screen points.
const LINE: f64 = 1.0;

/// A box under the pen, on `page` of `canvas`, from where it went down to
/// where it is, at `zoom`, and the `pdf` item under the press, if any.
#[derive(Debug, Clone, PartialEq)]
struct Drag {
    canvas: String,
    page: String,
    from: Point,
    to: Point,
    zoom: f64,
    under: Option<Shape>,
}

/// The box under the pen, and the last snippet made, until it is taken.
#[derive(Debug, Default)]
pub struct Crop {
    drag: Option<Drag>,
    snippet: Option<Shape>,
}

impl Crop {
    /// The pen went down at `press` on `page` of `canvas`, over `items`,
    /// with `tool`: the crop tool starts a box. False for any other tool.
    pub fn down(
        &mut self,
        tool: Tool,
        (canvas, page): (&str, &str),
        press: Press,
        items: &[Cow<'_, Item>],
    ) -> bool {
        if tool != Tool::CropPdf {
            return false;
        }
        let under = topmost(items, |frame| frame.contains(press.at)).cloned();
        let (canvas, page, from, zoom) =
            (canvas.to_string(), page.to_string(), press.at, press.zoom);
        self.drag = Some(Drag { canvas, page, from, to: from, zoom, under });
        true
    }

    /// The pen moved to `at` while down. False when no box is under it.
    pub fn drag(&mut self, at: Point) -> bool {
        let Some(drag) = &mut self.drag else { return false };
        drag.to = at;
        true
    }

    /// The pen lifted, at `at` if it said where, over `items`: a box long
    /// enough that meets a `pdf` item makes the snippet [`Crop::take`]
    /// gives. False when no box was under the pen.
    pub fn up(&mut self, at: Option<Point>, items: &[Cow<'_, Item>]) -> bool {
        if let Some(at) = at {
            self.drag(at);
        }
        let Some(drag) = self.drag.take() else { return false };
        if (drag.from.x - drag.to.x).hypot(drag.from.y - drag.to.y) * drag.zoom < MIN_DRAG {
            return true;
        }
        let area = boxed(drag.from, drag.to);
        let meets =
            |frame: Rect| frame.intersection(area).is_some_and(|r| r.width * r.height > 0.0);
        let source = drag.under.as_ref().or_else(|| topmost(items, meets));
        self.snippet = source.and_then(|s| cropped(s, area));
        true
    }

    /// The snippet the last lift made, once.
    pub fn take(&mut self) -> Option<Shape> {
        self.snippet.take()
    }

    /// `items`, the page shown of `canvas`, with the box under the pen laid
    /// over it, dashed in `theme`'s focus colour, sized for `zoom`.
    pub fn shown<'a>(
        &self,
        (canvas, page): (&str, &str),
        mut items: Vec<Cow<'a, Item>>,
        zoom: f64,
        theme: &Theme,
    ) -> Vec<Cow<'a, Item>> {
        let here = |d: &&Drag| d.canvas == canvas && d.page == page;
        if let Some(drag) = self.drag.as_ref().filter(here) {
            let frame = boxed(drag.from, drag.to);
            let shape = Shape::Panel { frame, fill: None, width: LINE / zoom, dashed: true };
            items.push(Cow::Owned(Item { id: BOX, colour: theme.focus, shape }));
        }
        items
    }
}

/// `source`, a `pdf` item, cropped to `area` (canvas units): its frame
/// cut to `area`, and the part of its page that part shows, mapped from
/// its shown region ([`Shape::shown`]), so a crop of a crop works. Both on
/// whole tenths, as the canvas file keeps them, the crop inside the page.
/// `None` for another kind, or when `area` leaves nothing of it.
pub fn cropped(source: &Shape, area: Rect) -> Option<Shape> {
    let Shape::Pdf { frame, hash, pdf_page, page_size, name, .. } = source else { return None };
    let shown = source.shown()?;
    let part = area.intersection(*frame)?;
    let (kx, ky) = (shown.width / frame.width, shown.height / frame.height);
    // Each edge mapped and put on a tenth: the right edge apart from the
    // left, so no rounding takes the crop past the page.
    let span = |from: f64, to: f64, page: f64| {
        let (a, b) = (tenth(from).max(0.0), tenth(to).min(tenth(page)));
        (b > a).then_some((a, b - a))
    };
    let left = shown.x + (part.x - frame.x) * kx;
    let top = shown.y + (part.y - frame.y) * ky;
    let (x, width) = span(left, left + part.width * kx, page_size.0)?;
    let (y, height) = span(top, top + part.height * ky, page_size.1)?;
    let to = Rect {
        x: tenth(part.x),
        y: tenth(part.y),
        width: tenth(part.width),
        height: tenth(part.height),
    };
    (to.width > 0.0 && to.height > 0.0).then(|| Shape::Pdf {
        frame: to,
        hash: hash.clone(),
        pdf_page: *pdf_page,
        page_size: *page_size,
        crop: Some(Rect { x, y, width, height }),
        name: name.clone(),
    })
}

/// The topmost `pdf` item of `items` whose frame `meets`: `pdf` items draw
/// under every other kind, in item order among themselves.
fn topmost<'b>(items: &'b [Cow<'_, Item>], meets: impl Fn(Rect) -> bool) -> Option<&'b Shape> {
    let mut shapes = items.iter().map(|i| &i.shape);
    shapes.rfind(|s| matches!(s, Shape::Pdf { frame, .. } if meets(*frame)))
}

/// The box with corners `a` and `b`, whichever way the drag went.
fn boxed(a: Point, b: Point) -> Rect {
    Rect { x: a.x.min(b.x), y: a.y.min(b.y), width: (a.x - b.x).abs(), height: (a.y - b.y).abs() }
}

/// `v` on a whole tenth, as the canvas file keeps lengths.
fn tenth(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests;
