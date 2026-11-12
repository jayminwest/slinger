//! How an agent changes or takes back what it put (SPEC section 7): new
//! text, a new size or a new place for one item, or the item gone.
//!
//! Ids are as `put` takes them (`i12`, or a packet's `c12`, `t12`). Who may
//! change what is the canvas's author rule ([`Canvas::apply`]): an agent
//! changes only items it made, so the user's ink is never an agent's to
//! change. That rule is checked before anything else about the edit, so an
//! agent naming the user's item always hears that it is not its own.
//! Removing an item already gone succeeds and writes nothing (B4), as does
//! an edit that leaves the item as it was. A move takes along the ends of
//! the arrows bound to the item, in the same step, as a move in the app
//! does; only the arrows the agent may change.
//!
//! [`Canvas::apply`]: crate::canvas::Canvas::apply

use super::put::TEXT_COLUMNS;
use super::{Log, Placed, Written, json};
use crate::canvas::{Action, Author, End, Item, ItemId, Point, Rect, Shape, Time};
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
        let mut by_moved = Point::new(0.0, 0.0);
        if let Some([x, y]) = edit.to {
            let Rect { x: left, y: top, .. } = new.bounds();
            by_moved = Point::new(json::units(x) - left, json::units(y) - top);
            new = new.shifted(by_moved);
        }
        let written = if new == shape {
            Written::default()
        } else {
            let mut actions = vec![Action::Edit { item, shape: new }];
            actions.extend(self.followers(by, item, by_moved));
            self.step(by, at, actions)?
        };
        let frame = self.canvas().item(item).map(Item::bounds).unwrap_or_default();
        let frame = [frame.x, frame.y, frame.w, frame.h].map(json::tenths);
        Ok(Placed { written, item: json::item_id(item), page: json::page_id(page), frame })
    }

    /// The arrows that follow a move of `item` by `by_moved`, as edits: each
    /// arrow `by` may change with an end bound to `item`, that end moved. An
    /// arrow `by` may not change (an agent's move of its note, the user's
    /// arrow to it) stays as it is, as the author rule says.
    fn followers(&self, by: &Author, item: ItemId, by_moved: Point) -> Vec<Action> {
        if by_moved == Point::new(0.0, 0.0) {
            return Vec::new();
        }
        let Some(page) = self.canvas().item(item).map(Item::page) else { return Vec::new() };
        let mut follow = Vec::new();
        for arrow in self.canvas().items(page) {
            let Shape::Arrow { from, to, colour } = arrow.shape() else { continue };
            let on = |end: &End| end.item == Some(item);
            if arrow.id() == item || !(on(from) || on(to)) || !by.may_change(arrow.author()) {
                continue;
            }
            let shift = |end: &End| {
                if on(end) {
                    End { at: Point::new(end.at.x + by_moved.x, end.at.y + by_moved.y), ..*end }
                } else {
                    *end
                }
            };
            let shape = Shape::Arrow { from: shift(from), to: shift(to), colour: *colour };
            follow.push(Action::Edit { item: arrow.id(), shape });
        }
        follow
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

fn invalid(message: &str, hint: &str) -> Error {
    Error::Invalid { message: message.into(), hint: Some(hint.into()) }
}
