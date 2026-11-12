//! Moving the selection between panes with the pen (SPEC section 11). A
//! drag of the selection goes where the canvas pane under the pen puts it,
//! read through that pane's own view, at every pen event and at the lift:
//!
//! - over a pane on the same page (the pane it started in, or another on
//!   that page with its own pan and zoom): the items follow the pen there,
//!   and every pane on the page shows them move; the lift moves them, one
//!   `move`, by the canvas distance from where they were grabbed to where
//!   they were dropped, as [`Select::up`];
//! - over a pane on another page: they show there under the pen, and are
//!   gone from their own page; the lift carries them, kept where they were
//!   under the pen. On the same canvas that is one `add` and `remove`, so
//!   one undo step; onto another canvas, an `add` there and then a
//!   `remove` here, one step on each ([`Ink::carry`]). Once the carried
//!   items are on their new page they are the selection there;
//! - outside every canvas pane (the sidebar, a footer, an agent UI, a
//!   divider): they show where they were, and the lift moves nothing.
//!
//! Pure: `canvases` says which pane is under the pen and hands in the items
//! as they would be carried (the canvas file's objects are the mirror's).

use std::borrow::Cow;
use std::collections::HashSet;

use super::{Gesture, Select, Selection};
use crate::layout::Point;
use crate::scene::canvas::{Item, ItemId};
use crate::scene::ink::{Carried, Change, Ink};

/// The ids the items shown coming to another page draw under: apart from
/// the server's, the ink's and the marks'.
const COMING: u64 = 1 << 63;

/// The canvas pane under the pen: the page it shows, and the point there,
/// in that canvas's units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Landing<'a> {
    pub canvas: &'a str,
    pub page: &'a str,
    pub at: Point,
}

/// Where a drag of the selection would put it if the pen lifted now.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Target {
    /// Its own page, the gesture's `by` from where it was.
    Here,
    /// Another page: the items as they would land there.
    Away { canvas: String, page: String, items: Vec<Item> },
    /// No canvas pane: where it was.
    Nowhere,
}

/// A carry on its way: the key of its `add`, the page it goes to, and the
/// ids the server gave the items, once it said.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Arriving {
    key: u64,
    canvas: String,
    page: String,
    ids: Option<Vec<ItemId>>,
}

impl Select {
    /// The pen moved to `at`, in the units of the canvas the gesture is on,
    /// over `landing`, the canvas pane under it (`None` outside every one),
    /// with `items` the gesture's page as shown. A drag of the selection
    /// goes as the module says; any other gesture goes on at `at`
    /// ([`Select::drag`]). Returns false when no gesture is under the pen.
    pub fn hover(
        &mut self,
        at: Point,
        landing: Option<Landing<'_>>,
        items: &[Cow<'_, Item>],
    ) -> bool {
        let ids = self.selection.as_ref().map(|s| s.ids.clone()).unwrap_or_default();
        let Some((canvas, page, Gesture::Move { from, by, to, .. })) = &mut self.gesture else {
            return self.drag(at, items);
        };
        let Some(landing) = landing else {
            (*by, *to) = (Point { x: 0.0, y: 0.0 }, Target::Nowhere);
            return true;
        };
        *by = Point { x: landing.at.x - from.x, y: landing.at.y - from.y };
        *to = if landing.canvas == canvas && landing.page == page {
            Target::Here
        } else {
            let chosen = items.iter().filter(|i| ids.contains(&i.id));
            let moved = chosen.map(|i| Item { shape: i.shape.moved(*by), ..(**i).clone() });
            let (canvas, page) = (landing.canvas.to_string(), landing.page.to_string());
            Target::Away { canvas, page, items: moved.collect() }
        };
        true
    }

    /// The pen lifted at `at` over `landing`, as [`Select::hover`]: a drag
    /// of the selection moves or carries it as the module says; `carry`
    /// gives the selected items, as `ink` shows them, moved by a distance
    /// and set on the landing page. Any other gesture ends as [`Select::up`] at `at`. Returns
    /// false when no gesture was under the pen.
    pub fn lift(
        &mut self,
        at: Point,
        landing: Option<Landing<'_>>,
        items: &[Cow<'_, Item>],
        ink: &mut Ink,
        carry: impl FnOnce(&Ink, &[ItemId], Point) -> Vec<Carried>,
    ) -> bool {
        if !matches!(self.gesture, Some((_, _, Gesture::Move { .. }))) {
            return self.up(Some(at), items, ink);
        }
        self.hover(at, landing, items);
        match &self.gesture {
            Some((_, _, Gesture::Move { to: Target::Here, .. })) => {
                return self.up(None, items, ink);
            }
            Some((_, _, Gesture::Move { to: Target::Away { .. }, .. })) => {}
            _ => {
                self.gesture = None;
                return true;
            }
        }
        let Some((
            canvas,
            page,
            Gesture::Move { by, to: Target::Away { canvas: c, page: p, .. }, .. },
        )) = self.gesture.take()
        else {
            return true;
        };
        let by = Point { x: tenth(by.x), y: tenth(by.y) };
        let Some(Selection { ids, .. }) = self.selection.take() else { return true };
        let carried = carry(ink, &ids, by);
        if carried.is_empty() {
            return true;
        }
        let gone: Vec<ItemId> = carried.iter().map(|c| c.item.id).collect();
        let key = if c == canvas {
            ink.push_keyed(Change::Carry { canvas, page: p.clone(), items: carried, gone })
        } else {
            let to =
                Change::Carry { canvas: c.clone(), page: p.clone(), items: carried, gone: vec![] };
            ink.carry(to, Change::Remove { canvas, page, ids: gone })
        };
        self.arriving = Some(Arriving { key, canvas: c, page: p, ids: None });
        true
    }

    /// Items pasted onto `page` of `canvas`, in the change sent under
    /// `key`: the selection there once they are on it, as a carry's.
    pub fn pasted(&mut self, key: u64, canvas: &str, page: &str) {
        self.selection = None;
        let (canvas, page) = (canvas.to_string(), page.to_string());
        self.arriving = Some(Arriving { key, canvas, page, ids: None });
    }

    /// The daemon wrote the change sent under `key`, making the items
    /// `ids`: a carry's items, selected once they are on their page.
    pub fn made(&mut self, key: u64, ids: &[ItemId]) {
        if let Some(arriving) = self.arriving.as_mut().filter(|a| a.key == key) {
            arriving.ids = Some(ids.to_vec());
        }
    }

    /// Selects the carried items once all of them are in `items`, the
    /// page shown of `canvas`.
    pub(super) fn arrive(&mut self, (canvas, page): (&str, &str), items: &[Cow<'_, Item>]) {
        let Some(Arriving { canvas: c, page: p, ids: Some(ids), .. }) = &self.arriving else {
            return;
        };
        if c != canvas || p != page {
            return;
        }
        let present: HashSet<ItemId> = items.iter().map(|i| i.id).collect();
        if ids.iter().all(|id| present.contains(id)) {
            let (canvas, page, ids) = (c.clone(), p.clone(), ids.clone());
            self.selection = Some(Selection { canvas, page, ids });
            self.arriving = None;
        }
    }

    /// The selection as it would land on `page` of `canvas`, while a drag
    /// of it is over a pane there and it is not its own page; ids apart.
    pub(super) fn coming(&self, (canvas, page): (&str, &str)) -> Option<Vec<Cow<'static, Item>>> {
        let Some((_, _, Gesture::Move { to: Target::Away { canvas: c, page: p, items }, .. })) =
            &self.gesture
        else {
            return None;
        };
        let apart = |i: &Item| Cow::Owned(Item { id: ItemId(COMING | i.id.0), ..i.clone() });
        (c == canvas && p == page).then(|| items.iter().map(apart).collect())
    }
}

/// `v` in whole tenths, as the canvas file stores lengths.
fn tenth(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests;
