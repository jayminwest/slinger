//! Moving the selection between panes with the pen (SPEC section 11). A
//! drag of the selection ends where the pen lifts:
//!
//! - over a pane on the same page (the pane it started in, or another on
//!   that canvas): the items move there, one `move`, as [`Select::up`];
//! - over a pane on another page: the items go to that page, an `add`
//!   there and a `remove` here, one undo step on each canvas, kept where
//!   they were under the pen ([`Ink::carry`]);
//! - outside every canvas pane: nothing moves.
//!
//! Pure: `canvases` says which pane is under the pen and hands in the items
//! as they would be carried (the canvas file's objects are the mirror's).

use std::borrow::Cow;

use super::{Gesture, Select, Selection};
use crate::layout::Point;
use crate::scene::canvas::{Item, ItemId};
use crate::scene::ink::{Carried, Change, Ink};

/// The canvas pane under the pen as it lifts: the page it shows, and the
/// point there, in that canvas's units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Landing<'a> {
    pub canvas: &'a str,
    pub page: &'a str,
    pub at: Point,
}

impl Select {
    /// The pen lifted at `at`, in the units of the canvas the gesture is
    /// on, over `landing`, the canvas pane under it (`None` outside every
    /// one), with `items` the gesture's page as shown. A drag of the
    /// selection moves or carries it as the module says; `carry` gives the
    /// selected items moved by a distance and set on the landing page. Any
    /// other gesture ends as [`Select::up`] at `at`. Returns false when no
    /// gesture was under the pen.
    pub fn lift(
        &mut self,
        at: Point,
        landing: Option<Landing<'_>>,
        items: &[Cow<'_, Item>],
        ink: &mut Ink,
        carry: impl FnOnce(&[ItemId], Point) -> Vec<Carried>,
    ) -> bool {
        let Some((canvas, page, Gesture::Move { from, .. })) = &self.gesture else {
            return self.up(Some(at), items, ink);
        };
        let Some(landing) = landing else {
            self.gesture = None;
            return true;
        };
        if landing.canvas == canvas && landing.page == page {
            return self.up(Some(landing.at), items, ink);
        }
        let by = Point { x: tenth(landing.at.x - from.x), y: tenth(landing.at.y - from.y) };
        let (canvas, page) = (canvas.clone(), page.clone());
        self.gesture = None;
        let Some(Selection { ids, .. }) = self.selection.take() else { return true };
        let carried = carry(&ids, by);
        if carried.is_empty() {
            return true;
        }
        let ids = carried.iter().map(|c| c.item.id).collect();
        let to = Change::Carry {
            canvas: landing.canvas.to_string(),
            page: landing.page.to_string(),
            items: carried,
        };
        ink.carry(to, Change::Remove { canvas, page, ids });
        true
    }
}

/// `v` in whole tenths, as the canvas file stores lengths.
fn tenth(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests;
