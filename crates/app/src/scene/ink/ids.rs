//! The ids a change names, renamed (slinger-b10c99). An item a change not
//! yet written adds shows under an id of the ink's own ([`PENDING`] and up)
//! until the daemon says the id it made; the eraser, the lasso and the
//! select tool take it under that id, so a change made after it may name
//! it. Each such id is renamed to the server's once the daemon makes it,
//! and a change is sent only once every change before it is answered, so
//! one still naming an id of the ink's own when it goes names an item
//! never made (its add was refused): that id is dropped.

use std::collections::HashMap;

use super::{Change, LOCAL, PENDING};
use crate::scene::canvas::{Item, ItemId, Shape};

/// Whether `id` is one the ink shows an item a change adds under, until
/// the daemon makes it.
pub(super) fn pending(id: ItemId) -> bool {
    (PENDING..LOCAL).contains(&id.0)
}

/// `id`, or the id the daemon made it under, by `made`: an item it has
/// not made yet keeps its id.
pub(super) fn known(made: &HashMap<ItemId, ItemId>, id: ItemId) -> ItemId {
    made.get(&id).copied().unwrap_or(id)
}

/// `id` as the server names it, by `made` (the ink's own ids the daemon
/// made, and the ids it made them under); `None` for an id of the ink's
/// own it never made.
pub(super) fn sent(made: &HashMap<ItemId, ItemId>, id: ItemId) -> Option<ItemId> {
    if pending(id) { made.get(&id).copied() } else { Some(id) }
}

impl Change {
    /// Renames every item it names by `to`, dropping each `to` has no name
    /// for. Returns false when nothing is left for it to do: a removal or a
    /// move of no item, or an edit of an item dropped.
    pub(super) fn rename(&mut self, to: impl Fn(ItemId) -> Option<ItemId>) -> bool {
        let ids = |ids: &mut Vec<ItemId>| *ids = ids.iter().filter_map(|id| to(*id)).collect();
        // An item kept by its new id, its arrow's ends renamed too.
        let item = |item: &mut Item| {
            bound(&mut item.shape, &to);
            to(item.id).map(|id| item.id = id).is_some()
        };
        match self {
            Change::Remove { ids: named, .. } => {
                ids(named);
                !named.is_empty()
            }
            Change::Move { ids: named, follow, .. } => {
                ids(named);
                follow.retain_mut(item);
                !named.is_empty()
            }
            Change::Put { shape, .. } => {
                bound(shape, &to);
                true
            }
            Change::Set { id, shape, .. } => {
                bound(shape, &to);
                to(*id).map(|named| *id = named).is_some()
            }
            Change::Reshape { items, .. } => {
                items.retain_mut(item);
                !items.is_empty()
            }
            Change::Carry { gone, .. } => {
                ids(gone);
                true
            }
            Change::Add(_) | Change::Page { .. } | Change::Step { .. } => true,
        }
    }
}

/// An arrow's ends renamed by `to`; an end bound to an item dropped is
/// bound to nothing.
fn bound(shape: &mut Shape, to: &impl Fn(ItemId) -> Option<ItemId>) {
    if let Shape::Arrow { bound, .. } = shape {
        for end in bound {
            *end = end.and_then(to);
        }
    }
}
