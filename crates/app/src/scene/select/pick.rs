//! The select tool (`tool-select`): the lasso's work without the loop. It
//! makes and moves the same selection as the lasso, so delete, sling and
//! escape act on it alike.
//!
//! - A press on an item selects it alone and drags it: the topmost item the
//!   press picks ([`hit::picks`]; a box by its outline). A press inside the selection's box drags the
//!   selection, as the lasso's does; if it was on an item and the pen lifts
//!   without moving, that item alone is selected.
//! - A press on empty canvas clears the selection and draws a box; the lift
//!   selects every item the box touches ([`hit::touches`]). A press and lift
//!   without a drag selects what the point touches: nothing, or the boxes
//!   it is inside.
//! - With shift, a press on an item adds it to the selection or takes it
//!   out, and a box adds what it touches.

use std::borrow::Cow;

use super::{Gesture, Select, Selection, hit};
use crate::layout::{Point, Rect};
use crate::scene::canvas::{Item, ItemId};
use crate::scene::ink::on_server;

/// How far from an item a press still picks it, in screen points.
pub const REACH: f64 = 6.0;

impl Select {
    /// The select tool went down at `at` on `page` of `canvas`, at `zoom`,
    /// over `items`, with shift held or not: it selects, toggles, starts a
    /// box or drags the selection, as the module says.
    pub fn pick(
        &mut self,
        (canvas, page): (&str, &str),
        at: Point,
        zoom: f64,
        shift: bool,
        items: &[Cow<'_, Item>],
    ) {
        let hit = topmost(items, at, REACH / zoom);
        self.arriving = None;
        let gesture = match hit {
            Some(id) if shift => {
                self.toggle((canvas, page), id);
                return;
            }
            None if shift => Gesture::Marquee { from: at, to: at, adding: true },
            click if self.grabs((canvas, page), at, zoom, items) => Gesture::moving(at, click),
            Some(id) => {
                let ids = vec![id];
                self.selection = Some(Selection { canvas: canvas.into(), page: page.into(), ids });
                Gesture::moving(at, None)
            }
            None => {
                self.selection = None;
                Gesture::Marquee { from: at, to: at, adding: false }
            }
        };
        self.gesture = Some((canvas.to_string(), page.to_string(), gesture));
    }

    /// Adds `id` on `page` of `canvas` to the selection, or takes it out;
    /// a selection elsewhere gives way to it.
    fn toggle(&mut self, (canvas, page): (&str, &str), id: ItemId) {
        let here = |s: &Selection| s.canvas == canvas && s.page == page;
        match self.selection.as_mut().filter(|s| here(s)) {
            Some(selection) if selection.ids.contains(&id) => {
                selection.ids.retain(|&i| i != id);
                if selection.ids.is_empty() {
                    self.selection = None;
                }
            }
            Some(selection) => selection.ids.push(id),
            None => {
                let ids = vec![id];
                self.selection = Some(Selection { canvas: canvas.into(), page: page.into(), ids });
            }
        }
    }

    /// Selects the server's items of `items` that touch `area` on `page` of
    /// `canvas`, added to the selection there when `adding`; nothing
    /// selected when none does and it is not adding.
    pub(super) fn boxed(
        &mut self,
        (canvas, page): (String, String),
        area: Rect,
        adding: bool,
        items: &[Cow<'_, Item>],
    ) {
        let touched = items.iter().filter(|i| on_server(i.id) && hit::touches(&i.shape, &area));
        let mut ids: Vec<ItemId> = touched.map(|i| i.id).collect();
        let here = |s: &Selection| s.canvas == canvas && s.page == page;
        if let Some(kept) = self.selection.take().filter(|s| adding && here(s)) {
            let new: Vec<ItemId> = ids.into_iter().filter(|id| !kept.ids.contains(id)).collect();
            ids = kept.ids.into_iter().chain(new).collect();
        }
        self.selection = (!ids.is_empty()).then_some(Selection { canvas, page, ids });
    }
}

/// The topmost of the server's items of `items` a press at `at` with a
/// reach of `radius` picks.
fn topmost(items: &[Cow<'_, Item>], at: Point, radius: f64) -> Option<ItemId> {
    let mut ours = items.iter().rev().filter(|i| on_server(i.id));
    ours.find(|i| hit::picks(&i.shape, at, radius)).map(|i| i.id)
}

/// The rectangle with corners `a` and `b`.
pub(super) fn spanned(a: Point, b: Point) -> Rect {
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    Rect { x, y, width: (a.x - b.x).abs(), height: (a.y - b.y).abs() }
}

#[cfg(test)]
mod tests;
