//! The ids a change names, renamed (slinger-b10c99). An item a change not
//! yet written adds shows under an id of the ink's own ([`PENDING`] and up)
//! until the daemon says the id it made; the eraser, the lasso and the
//! select tool take it under that id, so a change made after it may name
//! it. Each such id is renamed to the server's once the daemon makes it,
//! and a change is sent only once every change before it is answered, so
//! one still naming an id of the ink's own when it goes names an item
//! never made (its add was refused): that id is dropped.

use super::{Change, LOCAL, PENDING};
use crate::scene::canvas::{Item, ItemId, Shape};

/// Whether `id` is one the ink shows an item a change adds under, until
/// the daemon makes it.
pub(super) fn pending(id: ItemId) -> bool {
    (PENDING..LOCAL).contains(&id.0)
}

/// Each id of the ink's own the daemon made an item of, and the id it made,
/// in the order of the ink's ids (nearly always the order made, so each
/// goes at the end).
#[derive(Debug, Default)]
pub(super) struct Made(Vec<(ItemId, ItemId)>);

impl Made {
    /// The daemon made the items that showed under the first id of each of
    /// `ids` under the second.
    pub(super) fn add(&mut self, ids: &[(ItemId, ItemId)]) {
        for &(was, now) in ids {
            let at = self.0.partition_point(|(id, _)| *id < was);
            self.0.insert(at, (was, now));
        }
    }

    fn get(&self, id: ItemId) -> Option<ItemId> {
        let at = self.0.binary_search_by_key(&id, |(was, _)| *was).ok()?;
        Some(self.0[at].1)
    }

    /// `id`, or the id the daemon made it under: an item it has not made
    /// yet keeps its id.
    pub(super) fn known(&self, id: ItemId) -> ItemId {
        self.get(id).unwrap_or(id)
    }

    /// `id` as the server names it; `None` for an id of the ink's own it
    /// never made.
    pub(super) fn sent(&self, id: ItemId) -> Option<ItemId> {
        if pending(id) { self.get(id) } else { Some(id) }
    }
}

/// The rename takes `to` as a `dyn`, and each part of it is a function of
/// its own: one copy of each for every caller, as the app's size budget is
/// tight.
type To<'a> = &'a dyn Fn(ItemId) -> Option<ItemId>;

impl Change {
    /// Renames every item it names by `to`, dropping each `to` has no name
    /// for. Returns false when nothing is left for it to do: a removal or a
    /// move of no item, or an edit of an item dropped.
    pub(super) fn rename(&mut self, to: To<'_>) -> bool {
        match self {
            Change::Remove { ids, .. } => all(ids, to),
            Change::Move { ids, follow, .. } => {
                items(follow, to);
                all(ids, to)
            }
            Change::Put { shape, .. } => {
                bound(shape, to);
                true
            }
            Change::Set { id, shape, .. } => {
                bound(shape, to);
                one(id, to)
            }
            Change::Reshape { items: list, .. } => items(list, to),
            Change::Carry { gone, .. } => {
                all(gone, to);
                true
            }
            Change::Add(_) | Change::Page { .. } | Change::Step { .. } => true,
        }
    }
}

/// `id` renamed by `to`; false when `to` drops it.
fn one(id: &mut ItemId, to: To<'_>) -> bool {
    to(*id).map(|named| *id = named).is_some()
}

/// Each of `ids` renamed, those dropped gone; false when none is left.
fn all(ids: &mut Vec<ItemId>, to: To<'_>) -> bool {
    ids.retain_mut(|id| one(id, to));
    !ids.is_empty()
}

/// Each of `list` renamed, its arrow's ends too, those dropped gone; false
/// when none is left.
fn items(list: &mut Vec<Item>, to: To<'_>) -> bool {
    list.retain_mut(|item| {
        bound(&mut item.shape, to);
        one(&mut item.id, to)
    });
    !list.is_empty()
}

/// An arrow's ends renamed by `to`; an end bound to an item dropped is
/// bound to nothing.
fn bound(shape: &mut Shape, to: To<'_>) {
    if let Shape::Arrow { bound, .. } = shape {
        for end in bound {
            *end = end.and_then(to);
        }
    }
}
