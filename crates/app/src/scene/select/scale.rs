//! Resizing the selection by a corner handle, with the lasso or the select
//! tool: a press on a handle grabs that corner, and the drag scales the
//! selection about the opposite corner, which stays put. The width and the
//! height scale apart, and neither goes below [`SMALLEST`] or turns over.
//!
//! - A stroke's points, a box's corners and an arrow's ends scale; a
//!   stroke's width and an arrow's do not.
//! - A text, a button and an agent UI keep their size (their size is their
//!   words', or the agent's), and their top left goes where the scale puts it.
//! - An arrow not selected with an end bound to a selected item follows:
//!   that end goes where the scale puts it ([`shapes::follow`]).
//!
//! The selection shows scaled as the pen goes, in every pane on its page,
//! and the lift writes it in one [`Change::Reshape`], so one undo step.

use std::borrow::Cow;

use super::{Gesture, HANDLE, MARGIN, Select, bounds};
use crate::layout::{Point, Rect};
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::ink::{Change, Ink};
use crate::scene::shapes;

/// The least a side scales to, as a part of what it was.
const SMALLEST: f64 = 0.05;

/// A drag of a corner handle: the corner of the selection's items' box it
/// grabbed, the corner opposite, which stays put, the press, and how far
/// the pen has gone from it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Grip {
    corner: Point,
    anchor: Point,
    from: Point,
    pub(super) by: Point,
}

impl Grip {
    /// Where the scale puts `p`.
    fn map(&self, p: Point) -> Point {
        let scale = |corner: f64, anchor: f64, by: f64| {
            let side = corner - anchor;
            if side == 0.0 { 1.0 } else { ((side + by) / side).max(SMALLEST) }
        };
        let sx = scale(self.corner.x, self.anchor.x, self.by.x);
        let sy = scale(self.corner.y, self.anchor.y, self.by.y);
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
        let chosen = items.iter().filter(|i| ids.contains(&i.id));
        let mut after: Vec<Item> =
            chosen.map(|i| Item { shape: reshaped(&i.shape, map), ..(**i).clone() }).collect();
        after.extend(shapes::follow_by(items, ids, map));
        after
    }
}

/// `shape` with its points put where `map` says, as the module says.
fn reshaped(shape: &Shape, map: impl Fn(Point) -> Point) -> Shape {
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
            Shape::Text { frame: placed(frame), text: text.clone(), size: *size }
        }
        Shape::Button { frame, status } => Shape::Button { frame: placed(frame), status: *status },
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
        Some(Grip { corner: Point { x, y }, anchor: Point { x: ax, y: ay }, from: at, by: zero })
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
