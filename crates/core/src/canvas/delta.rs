//! What is new after the last sling (SPEC R1): a canvas's operation log and a
//! marker in, the items that differ from the canvas at the marker out.
//!
//! The answer compares two canvases, the one at the marker and the one at the
//! end of the log, so it reports what is different, not what was done:
//! - An item made after the marker is new, even if it was edited or moved
//!   since. One made and then removed after the marker is not there at all.
//! - An item from before the marker whose shape differs now is changed. One
//!   moved and moved back is not.
//! - An item from before the marker that is gone now is removed.
//!
//! There are no errors (SPEC B4). Nothing new is an empty answer. A marker past
//! the end of the log is at the end. An operation in the log that a canvas
//! refuses changes nothing, exactly as when it was first applied.

use super::{Canvas, Item, ItemId, Op};

/// A point in a canvas's operation log: the number of operations applied
/// before it. Operations are numbered from 1, so the marker after operation
/// `n` is `Marker(n)` and the first operation after it is `n + 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Marker(pub u64);

/// What differs between the canvas at a marker and the canvas now. Each list
/// is in id order, which is drawing order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Changes {
    /// The marker these changes start from (clamped to the end of the log).
    pub from: Marker,
    /// The end of the log: the marker to pass next time.
    pub to: Marker,
    /// Items made after the marker that are still on the canvas.
    pub new: Vec<ItemId>,
    /// Items made before the marker whose shape is different now.
    pub changed: Vec<ItemId>,
    /// Items made before the marker that are gone now.
    pub removed: Vec<ItemId>,
}

impl Changes {
    /// Whether nothing differs: no item is new, changed or removed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.new.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }
}

/// The changes to the canvas that `log` builds, after `marker`.
#[must_use]
pub fn changes_since(log: &[Op], marker: Marker) -> Changes {
    let split = usize::try_from(marker.0).map_or(log.len(), |m| m.min(log.len()));
    let (before, after) = log.split_at(split);
    let then = replay(Canvas::new(), before);
    let now = replay(then.clone(), after);

    let mut changes =
        Changes { from: Marker(split as u64), to: Marker(log.len() as u64), ..Changes::default() };
    for old in then.all_items() {
        match now.item(old.id()) {
            None => changes.removed.push(old.id()),
            Some(item) if item.shape() != old.shape() => changes.changed.push(old.id()),
            Some(_) => {}
        }
    }
    changes.new = now.all_items().map(Item::id).filter(|id| then.item(*id).is_none()).collect();
    changes
}

/// `canvas` with each op applied in order. An op the canvas refuses changes
/// nothing, so it is skipped.
fn replay(mut canvas: Canvas, ops: &[Op]) -> Canvas {
    for op in ops {
        // A refused op changed nothing when it was first applied either.
        let _ = canvas.apply(op.clone());
    }
    canvas
}

impl Canvas {
    /// Every item on every page, in id order.
    fn all_items(&self) -> impl Iterator<Item = &Item> {
        self.items.values()
    }
}
