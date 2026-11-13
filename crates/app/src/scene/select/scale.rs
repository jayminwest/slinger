//! Resizing the selection by a corner handle, with the lasso or the select
//! tool: a press on a handle grabs that corner, and the drag scales the
//! selection about the opposite corner, which stays put. The width and the
//! height scale apart, and neither goes below [`SMALLEST`] or turns over;
//! with shift held they scale together, by the larger of the two, so the
//! selection keeps its shape. The pointer on a handle is the resize cursor
//! along its diagonal ([`Select::handle_cursor`]), and stays it while the
//! handle is dragged.
//!
//! - A stroke's points, a box's corners and an arrow's ends scale; a
//!   stroke's width and an arrow's do not.
//! - A text's size scales evenly, by the geometric mean of the two sides'
//!   scales (their one scale when they scale together), kept within
//!   [`TEXT_SIZES`]; its top left goes where the scale puts it.
//! - A button and an agent UI keep their size (the agent's), and their top
//!   left goes where the scale puts it.
//! - A `pdf` item keeps its aspect (docs/design/pdf.md), whichever handle
//!   is dragged: it scales evenly about the opposite corner, by the
//!   text's one scale.
//! - An arrow not selected with an end bound to a selected item follows:
//!   that end goes where the scale puts it ([`shapes::follow`]).
//!
//! The selection shows scaled as the pen goes, in every pane on its page,
//! and the lift writes it in one [`Change::Reshape`], so one undo step.

use std::borrow::Cow;

use super::{Gesture, HANDLE, MARGIN, Select, bounds};
use crate::layout::{Point, Rect};
use crate::mirror::TEXT_SIZES;
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::cursor::{self, Cursor};
use crate::scene::ink::{Change, Ink};
use crate::scene::shapes;

/// The least a side scales to, as a part of what it was.
const SMALLEST: f64 = 0.05;

/// A drag of a corner handle: the corner of the selection's items' box it
/// grabbed, the corner opposite, which stays put, the press, how far the
/// pen has gone from it, and whether the sides scale together (shift).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Grip {
    corner: Point,
    anchor: Point,
    from: Point,
    pub(super) by: Point,
    even: bool,
}

impl Grip {
    /// How much the width and the height scale by.
    fn factors(&self) -> (f64, f64) {
        match self.sides() {
            (Some(sx), Some(sy)) if self.even => (sx.max(sy), sx.max(sy)),
            (sx, sy) if self.even => (sx.or(sy).unwrap_or(1.0), sx.or(sy).unwrap_or(1.0)),
            (sx, sy) => (sx.unwrap_or(1.0), sy.unwrap_or(1.0)),
        }
    }

    /// How much the width and the height scale by, each on its own;
    /// `None` for a side of no length: whatever it scales by, it stays.
    fn sides(&self) -> (Option<f64>, Option<f64>) {
        let scale = |corner: f64, anchor: f64, by: f64| {
            let side = corner - anchor;
            (side != 0.0).then(|| ((side + by) / side).max(SMALLEST))
        };
        let sx = scale(self.corner.x, self.anchor.x, self.by.x);
        (sx, scale(self.corner.y, self.anchor.y, self.by.y))
    }

    /// How much a text's size scales by: evenly, by the geometric mean of
    /// the sides' scales. A side of no length scales by the other's.
    fn text_factor(&self) -> f64 {
        let (sx, sy) = self.factors();
        match self.sides() {
            (None, Some(_)) => sy,
            (Some(_), None) => sx,
            _ => (sx * sy).sqrt(),
        }
    }

    /// Where the scale puts `p`.
    fn map(&self, p: Point) -> Point {
        let (sx, sy) = self.factors();
        Point {
            x: self.anchor.x + (p.x - self.anchor.x) * sx,
            y: self.anchor.y + (p.y - self.anchor.y) * sy,
        }
    }

    /// The items of `items` whose ids are `ids` as the scale makes them,
    /// then the arrows that follow them, each as it is after; with every
    /// point on whole tenths when `tenths`, as the canvas file stores them.
    fn scaled(&self, items: &[Cow<'_, Item>], ids: &[ItemId], tenths: bool) -> Vec<Item> {
        let tenth = |v: f64| if tenths { (v * 10.0).round() / 10.0 } else { v };
        let map = |p: Point| {
            let p = self.map(p);
            Point { x: tenth(p.x), y: tenth(p.y) }
        };
        let grow = |size: f64| {
            let size = (size * self.text_factor()).clamp(*TEXT_SIZES.start(), *TEXT_SIZES.end());
            tenth(size)
        };
        let even = |r: &Rect| {
            let (k, a) = (self.text_factor(), self.anchor);
            let (x, y) = (a.x + (r.x - a.x) * k, a.y + (r.y - a.y) * k);
            Rect {
                x: tenth(x),
                y: tenth(y),
                width: tenth(r.width * k),
                height: tenth(r.height * k),
            }
        };
        let chosen = items.iter().filter(|i| ids.contains(&i.id));
        let mut after: Vec<Item> = chosen
            .map(|i| Item { shape: reshaped(&i.shape, (map, grow, even)), ..(**i).clone() })
            .collect();
        after.extend(shapes::follow_by(items, ids, map));
        after
    }
}

/// `shape` with its points put where `map` says, a text's size made what
/// `grow` says and a `pdf` item's frame what `even` makes it, as the
/// module says.
fn reshaped(
    shape: &Shape,
    (map, grow, even): (impl Fn(Point) -> Point, impl Fn(f64) -> f64, impl Fn(&Rect) -> Rect),
) -> Shape {
    let corner = |r: &Rect| map(Point { x: r.x, y: r.y });
    let placed = |r: &Rect| {
        let at = corner(r);
        Rect { x: at.x, y: at.y, ..*r }
    };
    match shape {
        Shape::Stroke { points, width } => {
            Shape::Stroke { points: points.iter().map(|&p| map(p)).collect(), width: *width }
        }
        Shape::Box { frame, width } => {
            let (a, b) =
                (corner(frame), map(Point { x: frame.x + frame.width, y: frame.y + frame.height }));
            let frame = Rect { x: a.x, y: a.y, width: b.x - a.x, height: b.y - a.y };
            Shape::Box { frame, width: *width }
        }
        Shape::Arrow { from, to, width, bound } => {
            Shape::Arrow { from: map(*from), to: map(*to), width: *width, bound: *bound }
        }
        Shape::Text { frame, text, size } => {
            let (at, after) = (placed(frame), grow(*size));
            let by = after / size;
            let frame = Rect { width: frame.width * by, height: frame.height * by, ..at };
            Shape::Text { frame, text: text.clone(), size: after }
        }
        Shape::Button { frame, status } => Shape::Button { frame: placed(frame), status: *status },
        Shape::Pdf { frame, .. } => shape.reframed(even(frame)),
        Shape::Live { frame, name, open } => {
            Shape::Live { frame: placed(frame), name: name.clone(), open: open.clone() }
        }
        Shape::Panel { frame, fill, width, dashed } => {
            Shape::Panel { frame: placed(frame), fill: *fill, width: *width, dashed: *dashed }
        }
        Shape::Caret { frame, text, size, width } => {
            Shape::Caret { frame: placed(frame), text: text.clone(), size: *size, width: *width }
        }
    }
}

impl Select {
    /// The handle a press at `at` on `page` of `canvas` at `zoom` is on,
    /// over `items`, as a grip not moved yet; `None` off every handle or
    /// without a selection there.
    pub(super) fn handle(
        &self,
        (canvas, page): (&str, &str),
        at: Point,
        zoom: f64,
        items: &[Cow<'_, Item>],
    ) -> Option<Grip> {
        let selection = self.selection.as_ref()?;
        if selection.canvas != canvas || selection.page != page {
            return None;
        }
        let b = bounds(items, &selection.ids)?;
        let (margin, reach) = (MARGIN / zoom, HANDLE / 2.0 / zoom);
        let (left, top, right, bottom) = (b.x, b.y, b.x + b.width, b.y + b.height);
        let corners = [
            (left, top, -1.0, -1.0),
            (right, top, 1.0, -1.0),
            (right, bottom, 1.0, 1.0),
            (left, bottom, -1.0, 1.0),
        ];
        let (i, _) = corners.iter().enumerate().find(|(_, (x, y, sx, sy))| {
            let (hx, hy) = (x + sx * margin, y + sy * margin);
            (at.x - hx).abs() <= reach && (at.y - hy).abs() <= reach
        })?;
        let (x, y, _, _) = corners[i];
        let (ax, ay, _, _) = corners[(i + 2) % 4];
        let zero = Point { x: 0.0, y: 0.0 };
        let (corner, anchor) = (Point { x, y }, Point { x: ax, y: ay });
        Some(Grip { corner, anchor, from: at, by: zero, even: false })
    }

    /// The pointer at `at` on `page` of `canvas` at `zoom`, over `items`:
    /// on a handle of the selection, the resize cursor along that handle's
    /// diagonal; `None` anywhere else.
    pub fn handle_cursor(
        &self,
        (canvas, page): (&str, &str),
        at: Point,
        zoom: f64,
        items: &[Cow<'_, Item>],
    ) -> Option<Cursor> {
        let grip = self.handle((canvas, page), at, zoom, items)?;
        Some(cursor::over_handle(grip.corner, grip.anchor))
    }

    /// Whether shift is held as the pen moves or lifts: a resize keeps the
    /// selection's shape while it is.
    pub fn keep_ratio(&mut self, shift: bool) {
        if let Some((_, _, Gesture::Scale(grip))) = &mut self.gesture {
            grip.even = shift;
        }
    }

    /// A press at `at` on a handle starts a resize: returns false, and
    /// nothing starts, off every handle.
    pub(super) fn grip(
        &mut self,
        (canvas, page): (&str, &str),
        at: Point,
        zoom: f64,
        items: &[Cow<'_, Item>],
    ) -> bool {
        let Some(grip) = self.handle((canvas, page), at, zoom, items) else { return false };
        self.arriving = None;
        self.gesture = Some((canvas.to_string(), page.to_string(), Gesture::Scale(grip)));
        true
    }
}

impl Grip {
    /// The pen moved to `at`.
    pub(super) fn drag(&mut self, at: Point) {
        self.by = Point { x: at.x - self.from.x, y: at.y - self.from.y };
    }
}

/// The selected `ids` of `items` as `grip` shows them now, the arrows that
/// follow them too, laid over `items` in place.
pub(super) fn show(grip: &Grip, items: &mut [Cow<'_, Item>], ids: &[ItemId]) {
    let after = grip.scaled(items, ids, false);
    for item in items.iter_mut() {
        if let Some(scaled) = after.iter().find(|a| a.id == item.id) {
            *item = Cow::Owned(scaled.clone());
        }
    }
}

/// The lift of `grip` on `page` of `canvas`: the selected `ids` of `items`
/// scaled, as one change to `ink`; nothing when the pen did not move.
pub(super) fn lift(
    grip: &Grip,
    (canvas, page): (String, String),
    items: &[Cow<'_, Item>],
    ids: &[ItemId],
    ink: &mut Ink,
) {
    if grip.by.x == 0.0 && grip.by.y == 0.0 {
        return;
    }
    let after = grip.scaled(items, ids, true);
    let changed: Vec<Item> =
        after.into_iter().filter(|a| items.iter().all(|i| i.id != a.id || **i != *a)).collect();
    if !changed.is_empty() {
        ink.push(Change::Reshape { canvas, page, items: changed });
    }
}

#[cfg(test)]
mod tests;
