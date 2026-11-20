//! Folded boxes as the mirror sees them (docs/design/fold.md): a box whose
//! object has `fold` is folded; its frame is the miniature, and the items
//! its `fold.items` names (and theirs, at any depth) are hidden. The rule
//! for "hidden" is the core's (`Canvas::holders`), read again here from
//! the objects `canvas_show` and the feed give, so the app needs no new
//! reply: a listed id that is gone is skipped, an item two boxes list
//! belongs to the lower box id, and an entry that would make a box hold
//! itself is skipped.
//!
//! What a folded box holds draws inside it, scaled: its miniature
//! ([`Mirror::miniature`], made once per change of the page, not per
//! frame), or at full size in its old room for a peek
//! ([`Mirror::inside`]). A folded box it holds draws as its own
//! miniature. Pen lines are thinned to the points a miniature can show,
//! so a page of many folded boxes costs little more to draw than their
//! ink at full size.

use std::collections::HashMap;

use serde_json::{Map, Value};

use super::{LINE_WIDTH, Mirror};
use crate::layout::{Point, Rect};
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::glyphs;

/// How deep a miniature draws the folded boxes inside folded boxes.
const DEPTH: usize = 8;
/// The thinnest line a miniature draws, in canvas units.
const THINNEST: f64 = 0.5;
/// How close two points of a pen line may lie in a miniature, in canvas
/// units, before the second is left out.
const THIN: f64 = 0.5;

/// What a folded box keeps: its open size, in canvas units, and the ids of
/// the items it holds directly, as its object's `fold` says.
#[derive(Debug, Clone, PartialEq)]
pub struct Folded {
    pub open: (f64, f64),
    pub items: Vec<ItemId>,
}

/// The `fold` of a box's object; `None` for an open box, any other kind,
/// or a `fold` without an open size above zero.
pub(super) fn folded(o: &Map<String, Value>) -> Option<Folded> {
    if o.get("kind")?.as_str()? != "box" {
        return None;
    }
    let fold = o.get("fold")?.as_object()?;
    let units = |key: &str| fold.get(key).and_then(Value::as_f64).map(|v| v / 10.0);
    let open = (units("w")?, units("h")?);
    let ids = fold.get("items").and_then(Value::as_array).into_iter().flatten();
    let items = ids.filter_map(|id| super::item_id(id.as_str()?)).collect();
    (open.0 > 0.0 && open.1 > 0.0).then_some(Folded { open, items })
}

/// Each item a folded box holds, with the box that holds it directly, from
/// the page's items in id order and each one's fold, if folded.
pub(super) fn holders<'a>(
    items: impl Iterator<Item = (ItemId, Option<Folded>)> + Clone + 'a,
) -> HashMap<ItemId, ItemId> {
    let here: std::collections::HashSet<ItemId> = items.clone().map(|(id, _)| id).collect();
    let mut held: HashMap<ItemId, ItemId> = HashMap::new();
    for (b, fold) in items {
        let Some(fold) = fold else { continue };
        for &m in &fold.items {
            if m == b || !here.contains(&m) || held.contains_key(&m) {
                continue;
            }
            // `m` may not be `b`, or hold it at any depth.
            let mut up = Some(b);
            while let Some(u) = up.filter(|&u| u != m) {
                up = held.get(&u).copied();
            }
            if up.is_none() {
                held.insert(m, b);
            }
        }
    }
    held
}

/// Canvas points placed in a smaller (or the same) frame: `to + (p - from) * scale`.
#[derive(Debug, Clone, Copy)]
struct Fit {
    from: Point,
    to: Point,
    scale: f64,
}

impl Fit {
    /// The fit of `open` into `frame`, keeping its proportions, top lefts together.
    fn of(open: Rect, frame: Rect) -> Fit {
        let scale = (frame.width / open.width).min(frame.height / open.height);
        Fit {
            from: corner(open),
            to: corner(frame),
            scale: if scale.is_finite() { scale } else { 1.0 },
        }
    }

    fn point(self, p: Point) -> Point {
        Point {
            x: self.to.x + (p.x - self.from.x) * self.scale,
            y: self.to.y + (p.y - self.from.y) * self.scale,
        }
    }

    fn rect(self, r: Rect) -> Rect {
        let at = self.point(corner(r));
        Rect { x: at.x, y: at.y, width: r.width * self.scale, height: r.height * self.scale }
    }

    /// This fit, then `outer`.
    fn then(self, outer: Fit) -> Fit {
        Fit { from: self.from, to: outer.point(self.to), scale: self.scale * outer.scale }
    }
}

fn corner(r: Rect) -> Point {
    Point { x: r.x, y: r.y }
}

impl Mirror {
    /// Where item `id` is: whether a folded box hides it, and its index there.
    pub(super) fn at(&self, id: ItemId) -> Option<(bool, usize)> {
        let find = |items: &[Item]| items.binary_search_by_key(&id, |i| i.id).ok();
        find(&self.items).map(|at| (false, at)).or_else(|| Some((true, find(&self.held)?)))
    }

    /// The items shown, or those hidden, and their objects.
    pub(super) fn lists(&mut self, held: bool) -> (&mut Vec<Item>, &mut Vec<Map<String, Value>>) {
        if held {
            (&mut self.held, &mut self.held_objects)
        } else {
            (&mut self.items, &mut self.objects)
        }
    }

    /// Sorts the items again into those shown and those a folded box
    /// hides, and makes each folded box's miniature.
    pub(super) fn refold(&mut self) {
        let shown =
            std::mem::take(&mut self.items).into_iter().zip(std::mem::take(&mut self.objects));
        let hidden =
            std::mem::take(&mut self.held).into_iter().zip(std::mem::take(&mut self.held_objects));
        let mut all: Vec<(Item, Map<String, Value>)> = shown.chain(hidden).collect();
        all.sort_by_key(|(i, _)| i.id);
        self.holders = holders(all.iter().map(|(i, o)| (i.id, folded(o))));
        for (item, object) in all {
            let (items, objects) = self.lists(self.holders.contains_key(&item.id));
            items.push(item);
            objects.push(object);
        }
        self.minis = self.miniatures();
    }

    /// Whether item `id` is a box, shown or hidden, and if so whether it
    /// is folded; `None` for any other item.
    pub fn box_folded(&self, id: ItemId) -> Option<bool> {
        let (held, at) = self.at(id)?;
        let (items, objects) =
            if held { (&self.held, &self.held_objects) } else { (&self.items, &self.objects) };
        matches!(items[at].shape, Shape::Box { .. }).then(|| folded(&objects[at]).is_some())
    }

    /// The old room of folded box `id`: its top left, its open size.
    pub fn room(&self, id: ItemId) -> Option<Rect> {
        let (frame, fold) = self.fold_of(id)?;
        Some(Rect { x: frame.x, y: frame.y, width: fold.open.0, height: fold.open.1 })
    }

    /// What folded box `id` (shown, or hidden in another) holds, drawn in
    /// its miniature as the server has it; `None` for any other item.
    pub fn miniature(&self, id: ItemId) -> Option<&[Item]> {
        self.minis.get(&id).map(Vec::as_slice)
    }

    /// What folded box `id` holds, drawn into `frame`: its miniature
    /// wherever it is shown (dragged, resized), or its old room for a peek.
    /// Under the ids of the items it holds, which the page does not draw.
    pub fn inside(&self, id: ItemId, frame: Rect) -> Vec<Item> {
        let mut out = Vec::new();
        if let Some(open) = self.room(id) {
            self.draw_into(id, open, Fit::of(open, frame), &mut out, DEPTH);
        }
        out
    }

    /// Each folded box's miniature, for [`Mirror::miniature`].
    fn miniatures(&self) -> HashMap<ItemId, Vec<Item>> {
        let boxes = self.items.iter().chain(&self.held).filter_map(|i| match i.shape {
            Shape::Box { frame, .. } if self.fold_of(i.id).is_some() => Some((i.id, frame)),
            _ => None,
        });
        boxes.map(|(id, frame)| (id, self.inside(id, frame))).collect()
    }

    /// The frame and the fold of folded box `id`, shown or hidden.
    pub(super) fn fold_of(&self, id: ItemId) -> Option<(Rect, Folded)> {
        let (held, at) = self.at(id)?;
        let (items, objects) =
            if held { (&self.held, &self.held_objects) } else { (&self.items, &self.objects) };
        let Shape::Box { frame, .. } = items[at].shape else { return None };
        Some((frame, folded(&objects[at])?))
    }

    /// Draws what box `id` holds directly, whose room is `open`, by `fit`,
    /// into `out`; a folded box among them draws its own, `depth` deep. An
    /// item left wholly outside the room (an old build moved the box
    /// alone) is not drawn.
    fn draw_into(&self, id: ItemId, open: Rect, fit: Fit, out: &mut Vec<Item>, depth: usize) {
        let members = self.held.iter().filter(|i| self.holders.get(&i.id) == Some(&id));
        for m in members {
            if m.shape.bounds().and_then(|b| b.intersection(open)).is_none() {
                continue;
            }
            let Some(shape) = scaled(&m.shape, fit) else { continue };
            out.push(Item { id: m.id, colour: m.colour, shape });
            if let (Some(inner), Some((frame, _))) = (self.room(m.id), self.fold_of(m.id))
                && depth > 0
            {
                self.draw_into(m.id, inner, Fit::of(inner, frame).then(fit), out, depth - 1);
            }
        }
    }
}

/// `shape` placed by `fit`, as a miniature draws it: lines no thinner than
/// [`THINNEST`], pen lines thinned, an agent UI or a button as its outline,
/// arrows let go of their items. `None` for what has no place there.
fn scaled(shape: &Shape, fit: Fit) -> Option<Shape> {
    let s = fit.scale;
    let line = |w: f64| (w * s).max(THINNEST);
    Some(match shape {
        Shape::Stroke { points, width } => Shape::Stroke {
            points: thinned(points.iter().map(|p| fit.point(*p))),
            width: line(*width),
        },
        Shape::Text { frame, text, size, column } => Shape::Text {
            frame: fit.rect(*frame),
            text: text.clone(),
            size: size * s,
            column: column.map(|c| if c >= glyphs::UNWRAPPED { c } else { c * s }),
        },
        Shape::Box { frame, width } => Shape::Box { frame: fit.rect(*frame), width: line(*width) },
        Shape::Live { frame, .. } | Shape::Button { frame, .. } => {
            Shape::Box { frame: fit.rect(*frame), width: line(LINE_WIDTH) }
        }
        Shape::Pdf { frame, .. } => shape.reframed(fit.rect(*frame)),
        Shape::Arrow { from, to, width, .. } => Shape::Arrow {
            from: fit.point(*from),
            to: fit.point(*to),
            width: line(*width),
            bound: [None; 2],
        },
        Shape::Panel { .. } | Shape::Caret { .. } => return None,
    })
}

/// The points of a line, each at least [`THIN`] from the one kept before
/// it; the last always kept.
fn thinned(points: impl Iterator<Item = Point>) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::new();
    let mut last = None;
    for p in points {
        let near = out.last().is_some_and(|q| (p.x - q.x).hypot(p.y - q.y) < THIN);
        if near {
            last = Some(p);
        } else {
            out.push(p);
            last = None;
        }
    }
    out.extend(last);
    out
}

#[cfg(test)]
mod tests;
