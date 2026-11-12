//! How an agent changes or takes back what it put (SPEC section 7): new
//! text, a new size or a new place for one item, or the item gone.
//!
//! Ids are as `put` takes them (`i12`, or a packet's `c12`, `t12`). Who may
//! change what is the canvas's author rule ([`Canvas::apply`]): an agent
//! changes only items it made, so the user's ink is never an agent's to
//! change. That rule is checked before anything else about the edit, so an
//! agent naming the user's item always hears that it is not its own.
//! Removing an item already gone succeeds and writes nothing (B4), as does
//! an edit that leaves the item as it was. A move or a resize takes along
//! the ends of the arrows bound to the item, in the same step, as one in the
//! app does ([`Log::followed`]); only the arrows the agent may change.
//!
//! [`Canvas::apply`]: crate::canvas::Canvas::apply

use super::put::TEXT_COLUMNS;
use std::collections::{HashMap, HashSet};

use super::{Log, Placed, Written, json};
use crate::canvas::{Action, Author, End, Item, ItemId, PageId, Point, Rect, Shape, Time};
use crate::render;
use crate::{Error, Result};

/// What an edit changes; what it leaves out stays. Lengths are in tenths
/// of a unit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Edit {
    /// New words for a text item. Its frame grows or shrinks to hold them,
    /// unless `size` says otherwise.
    pub text: Option<String>,
    /// A new `[w, h]` for a text, a box or a live item, its top-left corner kept.
    pub size: Option<[i64; 2]>,
    /// Where the top-left corner of the item's box goes, `[x, y]`.
    pub to: Option<[i64; 2]>,
}

impl Log {
    /// Changes the item `id` names, as `by` at `at`, as one undo step.
    ///
    /// # Errors
    ///
    /// `not_found` for an item that is not on the canvas, and `invalid` for
    /// an id that is not one, an item `by` did not make, blank text, text
    /// for an item that is not text, a size for an item that has none, or a
    /// size below zero.
    pub fn edit(&mut self, by: &Author, at: Time, id: &str, edit: &Edit) -> Result<Placed> {
        let (item, _, page) = self.named(id)?;
        let shape = self.canvas().may_change(item, by)?.shape().clone();
        let mut new = shape.clone();
        if let Some(text) = &edit.text {
            let Shape::Text { text: words, frame, .. } = &mut new else {
                return Err(invalid("only a text item has text", "edit a text item's text"));
            };
            if text.trim().is_empty() {
                let hint = "pass the new words, or remove the item";
                return Err(invalid("the text is blank", hint));
            }
            let (w, h) = render::text_size(text, TEXT_COLUMNS);
            (*words, frame.w, frame.h) = (text.clone(), w, h);
        }
        if let Some([w, h]) = edit.size {
            let (Shape::Text { frame, .. }
            | Shape::Box { frame, .. }
            | Shape::Live { frame, .. }
            | Shape::Button { frame }) = &mut new
            else {
                let hint = "move it instead";
                return Err(invalid(
                    "only a text, a box, a live item or a button has a size",
                    hint,
                ));
            };
            (frame.w, frame.h) = (json::units(w), json::units(h));
        }
        if let Some([x, y]) = edit.to {
            let Rect { x: left, y: top, .. } = new.bounds();
            new = new.shifted(Point::new(json::units(x) - left, json::units(y) - top));
        }
        let written = if new == shape {
            Written::default()
        } else {
            let actions = self.followed(by, vec![Action::Edit { item, shape: new }]);
            self.step(by, at, actions)?
        };
        let frame = self.canvas().item(item).map(Item::bounds).unwrap_or_default();
        let frame = [frame.x, frame.y, frame.w, frame.h].map(json::tenths);
        Ok(Placed { written, item: json::item_id(item), page: json::page_id(page), frame })
    }

    /// `actions` and, after them, the arrows that follow the items they move
    /// or resize, as edits: each arrow `by` may change, that `actions` do
    /// not touch themselves, with an end bound to such an item, that end
    /// refitted to the item's new box ([`refit`]). An arrow `by` may not
    /// change (an agent's move of its note, the user's arrow to it) stays as
    /// it is, as the author rule says. `canvas edit` and `canvas apply` both
    /// come through here; the app names its followers itself, so they are
    /// touched and left to it.
    pub(super) fn followed(&self, by: &Author, mut actions: Vec<Action>) -> Vec<Action> {
        let canvas = self.canvas();
        // Each touched item's shape before and after `actions`, in order.
        let mut shapes: HashMap<ItemId, (Rect, Shape)> = HashMap::new();
        let mut touched = HashSet::new();
        for action in &actions {
            let (item, after) = match action {
                Action::Edit { item, shape } => (*item, Some(shape.clone().fitted())),
                Action::Move { item, by } => {
                    let now = shapes.get(item).map(|(_, s)| s.clone());
                    let now = now.or_else(|| canvas.item(*item).map(|i| i.shape().clone()));
                    (*item, now.map(|s| s.shifted(*by)))
                }
                Action::Remove { item } => (*item, None),
                _ => continue,
            };
            touched.insert(item);
            let (Some(after), Some(before)) = (after, canvas.item(item)) else { continue };
            let before = before.bounds();
            shapes
                .entry(item)
                .and_modify(|(_, now)| *now = after.clone())
                .or_insert((before, after));
        }
        let boxes: HashMap<ItemId, (Rect, Rect)> = shapes
            .into_iter()
            .map(|(item, (before, after))| (item, (before, after.bounds())))
            .filter(|(_, (before, after))| before != after)
            .collect();
        let mut pages: Vec<PageId> =
            boxes.keys().filter_map(|&i| canvas.item(i).map(Item::page)).collect();
        pages.sort_unstable();
        pages.dedup();
        for arrow in pages.into_iter().flat_map(|page| canvas.items(page)) {
            let Shape::Arrow { from, to, colour } = arrow.shape() else { continue };
            let fit = |end: &End| {
                let to_box = end.item.and_then(|i| boxes.get(&i));
                to_box.map_or(*end, |&(before, after)| End {
                    at: refit(before, after, end.at),
                    ..*end
                })
            };
            let (new_from, new_to) = (fit(from), fit(to));
            if touched.contains(&arrow.id())
                || (new_from == *from && new_to == *to)
                || !by.may_change(arrow.author())
            {
                continue;
            }
            let shape = Shape::Arrow { from: new_from, to: new_to, colour: *colour };
            actions.push(Action::Edit { item: arrow.id(), shape });
        }
        actions
    }

    /// Takes the item `id` names off the canvas, as `by` at `at`, as one
    /// undo step. An item already gone is fine: nothing is written.
    ///
    /// # Errors
    ///
    /// `invalid` for an id that is not one or an item `by` did not make.
    pub fn remove(&mut self, by: &Author, at: Time, id: &str) -> Result<Written> {
        let items = match self.resolve(id) {
            Err(Error::NotFound { .. }) => return Ok(Written::default()),
            found => found?,
        };
        self.step(by, at, items.into_iter().map(|item| Action::Remove { item }).collect())
    }
}

/// Where a point at `at` goes when the box `before` becomes `after`: the
/// same place relative to the box, scaled with it (only shifted along a side
/// of no length). A move is a shift; a resize keeps an arrow's end on the
/// same spot of the item's edge.
fn refit(before: Rect, after: Rect, at: Point) -> Point {
    let axis = |p: f64, lo: f64, len: f64, new_lo: f64, new_len: f64| {
        if len > 0.0 { new_lo + (p - lo) * new_len / len } else { new_lo + p - lo }
    };
    Point::new(
        axis(at.x, before.x, before.w, after.x, after.w),
        axis(at.y, before.y, before.h, after.y, after.h),
    )
}

fn invalid(message: &str, hint: &str) -> Error {
    Error::Invalid { message: message.into(), hint: Some(hint.into()) }
}
