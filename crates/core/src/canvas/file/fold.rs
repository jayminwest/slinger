//! Folding a box (docs/design/fold.md): a fold freezes what lies inside the
//! box into its `fold` list and shrinks it to its miniature; an unfold opens
//! it again when its old room is free. Both are a `set` of the box, so undo
//! is the `set` of its old shape. The contents stay items on the page, at
//! their places: a move or an erase of a folded box carries them
//! ([`Log::carried`]).
//!
//! Who may: the canvas's author rule, for every item a line names. An agent
//! folds only a box it made, and moves or erases a folded box only when it
//! may change every item carried.

use std::collections::HashSet;

use super::{Log, Placed, Written, json};
use crate::canvas::{Action, Author, Canvas, Fold, ItemId, Point, Rect, Shape, Time, miniature};
use crate::{Error, Result};

impl Log {
    /// Folds the box `id` names (`i4`, or a packet's `b4`), as `by` at `at`,
    /// as one undo step: it takes [`Canvas::contents`], shrinks to its
    /// [`miniature`] and keeps its open size. A box already folded writes
    /// nothing. A box folded inside a folded box takes its contents out of
    /// that box's list, in the same step. Under `once`, as
    /// [`Log::apply_once`]. The frame is the miniature's.
    ///
    /// # Errors
    ///
    /// `not_found` for an item that is not on the canvas, and `invalid` for
    /// an id that is not one, an item that is not a box, a box of no area,
    /// or a box `by` may not change.
    pub fn fold(&mut self, by: &Author, at: Time, id: &str, once: Option<&str>) -> Result<Placed> {
        let written = self.once(once, |log| {
            let (item, outer, shape) = log.a_box(by, id)?;
            let Shape::Box { frame, colour, fold: None } = shape else {
                return Ok(Written::default());
            };
            if !(frame.w > 0.0 && frame.h > 0.0) {
                return Err(invalid("the box has no area", "give it a size above 0, then fold it"));
            }
            let items = log.canvas().contents(item);
            let fold = Fold { open: (frame.w, frame.h), items: items.clone() };
            let shape = Shape::Box { frame: miniature(frame), colour, fold: Some(fold) };
            let mut actions = vec![Action::Edit { item, shape }];
            let taken: HashSet<ItemId> = items.into_iter().collect();
            actions.extend(outer.and_then(|o| {
                relisted(log.canvas(), o, |list| {
                    list.retain(|i| !taken.contains(i));
                })
            }));
            let actions = log.followed(by, actions);
            log.step(by, at, actions)
        })?;
        self.placed(id, written)
    }

    /// Opens the folded box `id` names in its old room, as `by` at `at`, as
    /// one undo step: its open size back, its `fold` cleared. A box that is
    /// open writes nothing. A box inside a folded box gives what it held
    /// to that box's list, in the same step. Under `once`, as
    /// [`Log::apply_once`]. The frame is the open one.
    ///
    /// # Errors
    ///
    /// `conflict` when an item is in the way ([`Canvas::in_the_way`]),
    /// naming them: nothing is written. Otherwise as [`Log::fold`]'s.
    pub fn unfold(
        &mut self,
        by: &Author,
        at: Time,
        id: &str,
        once: Option<&str>,
    ) -> Result<Placed> {
        let written = self.once(once, |log| {
            let (item, outer, shape) = log.a_box(by, id)?;
            let Shape::Box { frame, colour, fold: Some(fold) } = shape else {
                return Ok(Written::default());
            };
            let canvas = log.canvas();
            let way = canvas.in_the_way(item);
            if !way.is_empty() {
                let way: Vec<String> = way.into_iter().map(json::item_id).collect();
                return Err(Error::Conflict {
                    message: format!("the box's room is taken by {}", way.join(", ")),
                    hint: Some(
                        "move them first, or read the box with 'slinger canvas read --in'".into(),
                    ),
                });
            }
            let held = canvas.contents(item);
            let open = Rect::new(frame.x, frame.y, fold.open.0, fold.open.1);
            let mut actions =
                vec![Action::Edit { item, shape: Shape::Box { frame: open, colour, fold: None } }];
            actions.extend(outer.and_then(|o| relisted(canvas, o, |list| list.extend(held))));
            let actions = log.followed(by, actions);
            log.step(by, at, actions)
        })?;
        self.placed(id, written)
    }

    /// `actions` and, after them, what each move or remove of a folded box
    /// carries: a move or remove of every item it holds, at any depth
    /// ([`Canvas::held`]). An edit of a folded box that moves its top left
    /// and leaves it folded is a move too. Items `actions` name themselves
    /// are left to them. Undo and redo never come through here: they apply
    /// the lines as they were.
    pub(super) fn carried(&self, actions: Vec<Action>) -> Vec<Action> {
        let canvas = self.canvas();
        let mut named: HashSet<ItemId> = actions.iter().filter_map(named_item).collect();
        let mut out = Vec::with_capacity(actions.len());
        for action in actions {
            // The item that carries, and by how much (`None`: a remove).
            let carries = match &action {
                Action::Move { item, by } => Some((*item, Some(*by))),
                Action::Remove { item } => Some((*item, None)),
                Action::Edit { item, shape } => {
                    moved(canvas, *item, shape).map(|by| (*item, Some(by)))
                }
                _ => None,
            };
            out.push(action);
            let Some((item, by)) = carries else { continue };
            for m in canvas.held(item) {
                if named.insert(m) {
                    out.push(match by {
                        Some(by) => Action::Move { item: m, by },
                        None => Action::Remove { item: m },
                    });
                }
            }
        }
        out
    }

    /// Makes the item the last `add` of `out` makes join the folded box
    /// `inside` names (docs/design/fold.md, "Step in"): an edit of the box,
    /// after the add, that lists the item and grows the open size, right
    /// and down only, to hold it. The box as an earlier action of `out`
    /// left it, else as it is. An `in` an open box adds plainly: what lies
    /// inside an open box is its contents already.
    ///
    /// # Errors
    ///
    /// `invalid` for an `in` that is not an item id, an item that is not a
    /// box, or a box on another page than the item; `not_found` for a box
    /// that is not there.
    pub(super) fn join(&self, inside: &serde_json::Value, out: &mut Vec<Action>) -> Result<()> {
        let id = inside.as_str().ok_or_else(|| invalid("`in` is not an item id", JOIN_HINT))?;
        self.join_box(id, out)
    }

    /// [`Log::join`] with the box's id as text.
    pub(super) fn join_box(&self, id: &str, out: &mut Vec<Action>) -> Result<()> {
        let hint = JOIN_HINT;
        let (item, _, on) = self.named(id)?;
        let adds = out.iter().filter(|a| matches!(a, Action::Add { .. } | Action::PageAdd { .. }));
        let made = self.made(out, &(adds.count().saturating_sub(1)).to_string());
        let (Some(made), Some(Action::Add { page, shape: added })) = (made, out.last()) else {
            return Ok(());
        };
        let earlier = out.iter().rev().find_map(|a| match a {
            Action::Edit { item: e, shape } if *e == item => Some(shape.clone()),
            _ => None,
        });
        let current = earlier.or_else(|| self.canvas().item(item).map(|i| i.shape().clone()));
        let Some(shape) = current else { return Ok(()) };
        if !matches!(shape, Shape::Box { .. }) {
            return Err(invalid(&format!("{id} is not a box"), hint));
        }
        if *page != on {
            return Err(invalid(&format!("{id} is on another page than the item"), hint));
        }
        let Shape::Box { frame, colour, fold: Some(mut fold) } = shape else { return Ok(()) };
        let r = added.clone().fitted().bounds();
        fold.open.0 = fold.open.0.max(r.x + r.w - frame.x);
        fold.open.1 = fold.open.1.max(r.y + r.h - frame.y);
        fold.items.push(made);
        out.push(Action::Edit { item, shape: Shape::Box { frame, colour, fold: Some(fold) } });
        Ok(())
    }

    /// The box `id` names, if `by` may change it: its id, the folded box
    /// that holds it and its shape.
    fn a_box(&self, by: &Author, id: &str) -> Result<(ItemId, Option<ItemId>, Shape)> {
        let (item, _, _) = self.named(id)?;
        let canvas = self.canvas();
        let shape = canvas.may_change(item, by)?.shape().clone();
        if !matches!(shape, Shape::Box { .. }) {
            return Err(invalid(&format!("{id} is not a box"), "fold or unfold a box"));
        }
        Ok((item, canvas.holder(item), shape))
    }

    /// What a fold or unfold of `id` gives back: its frame now.
    fn placed(&self, id: &str, written: Written) -> Result<Placed> {
        let (item, frame, page) = self.named(id)?;
        let frame = [frame.x, frame.y, frame.w, frame.h].map(json::tenths);
        Ok(Placed { written, item: json::item_id(item), page: json::page_id(page), frame })
    }
}

/// The edit of the folded box `outer` with its list changed by `change`.
fn relisted(
    canvas: &Canvas,
    outer: ItemId,
    change: impl FnOnce(&mut Vec<ItemId>),
) -> Option<Action> {
    let mut shape = canvas.item(outer)?.shape().clone();
    let Shape::Box { fold: Some(fold), .. } = &mut shape else { return None };
    change(&mut fold.items);
    Some(Action::Edit { item: outer, shape })
}

/// The item a move, remove or edit names.
fn named_item(action: &Action) -> Option<ItemId> {
    match action {
        Action::Move { item, .. } | Action::Remove { item } | Action::Edit { item, .. } => {
            Some(*item)
        }
        _ => None,
    }
}

/// How far an edit moves a folded box that stays folded; `None` when it
/// does not move one.
fn moved(canvas: &Canvas, item: ItemId, shape: &Shape) -> Option<Point> {
    let old = canvas.item(item)?.shape();
    let (Shape::Box { frame: a, fold: Some(_), .. }, Shape::Box { frame: b, fold: Some(_), .. }) =
        (old, shape)
    else {
        return None;
    };
    let by = Point::new(b.x - a.x, b.y - a.y);
    (by != Point::default()).then_some(by)
}

const JOIN_HINT: &str = "pass `in` as the id of a folded box on the item's page (b4 or i4)";

fn invalid(message: &str, hint: &str) -> Error {
    Error::Invalid { message: message.into(), hint: Some(hint.into()) }
}
