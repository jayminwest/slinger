//! Folded boxes (docs/design/fold.md): which items a folded box holds, and
//! so which items a page hides. One rule decides "hidden" ([`Canvas::holders`]),
//! and every query of what a page shows ([`Canvas::visible`], and through it
//! [`Canvas::drawn`] and the geometry queries) goes through it.
//!
//! The rule never fails: a listed id that is gone or on another page is
//! skipped; an item two boxes list belongs to the lower box id; an entry
//! that would make a box hold itself, at any depth, is skipped.

use std::collections::HashMap;

use super::geometry::{GAP, grown};
use super::item::invalid;
use super::{Author, Canvas, Item, ItemId, PageId, Rect, Shape};
use crate::Result;

/// What a folded box keeps: its open size, in canvas units, and the ids of
/// the items it holds directly, in the order it took them.
#[derive(Debug, Clone, PartialEq)]
pub struct Fold {
    /// The open frame's `(w, h)`; its top left is the box's.
    pub open: (f64, f64),
    /// The items it holds directly. Ids that are gone are skipped.
    pub items: Vec<ItemId>,
}

impl Fold {
    /// Rejects an open size that is not above zero.
    pub(super) fn check(&self) -> Result<()> {
        let (w, h) = self.open;
        if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 {
            return Ok(());
        }
        Err(invalid("a folded box's open size is not above zero", "give an open w and h above 0"))
    }
}

impl Shape {
    /// A folded box's fold; `None` for an open box or any other kind.
    #[must_use]
    pub fn fold(&self) -> Option<&Fold> {
        match self {
            Shape::Box { fold, .. } => fold.as_ref(),
            _ => None,
        }
    }
}

impl Canvas {
    /// The items on `page` that are not hidden in a folded box, in item order.
    pub fn visible(&self, page: PageId) -> impl Iterator<Item = &Item> {
        let holders = self.holders(page);
        self.items(page).filter(move |i| !holders.contains_key(&i.id))
    }

    /// The folded box that holds `item` directly, if one does.
    #[must_use]
    pub fn holder(&self, item: ItemId) -> Option<ItemId> {
        let page = self.item(item)?.page;
        self.holders(page).get(&item).copied()
    }

    /// The outermost folded box that holds `item`, at any depth: the one
    /// the page shows in its place. `None` for an item that is not hidden.
    #[must_use]
    pub fn outermost(&self, item: ItemId) -> Option<ItemId> {
        let page = self.item(item)?.page;
        let holders = self.holders(page);
        let mut up = *holders.get(&item)?;
        while let Some(&next) = holders.get(&up) {
            up = next;
        }
        Some(up)
    }

    /// Whether a folded box holds `item`, at any depth.
    #[must_use]
    pub fn hidden(&self, item: ItemId) -> bool {
        self.holder(item).is_some()
    }

    /// A box's direct contents, in item order. Open: the items in the same
    /// place as the box (visible with it, or held by the same folded box),
    /// other than itself, whose bounds lie wholly inside its frame, edges
    /// included. Folded: the items it holds directly. Anything else: none.
    #[must_use]
    pub fn contents(&self, of: ItemId) -> Vec<ItemId> {
        let Some(Item { page, shape: Shape::Box { frame, fold, .. }, .. }) = self.item(of) else {
            return Vec::new();
        };
        let holders = self.holders(*page);
        let place = holders.get(&of);
        let taken = |i: &&Item| match fold {
            Some(_) => holders.get(&i.id) == Some(&of),
            None => i.id != of && holders.get(&i.id) == place && inside(covered(i), *frame),
        };
        self.items(*page).filter(taken).map(Item::id).collect()
    }

    /// Every item a folded box holds, at any depth, in item order: what a
    /// move or an erase of the box carries. None for an open box.
    #[must_use]
    pub fn held(&self, of: ItemId) -> Vec<ItemId> {
        let Some(page) = self.item(of).map(Item::page) else { return Vec::new() };
        held_in(&self.holders(page), of)
    }

    /// A folded box as it stands open: its open frame and no fold. `None`
    /// for an open box or any other item. What a crop of the box draws.
    #[must_use]
    pub fn opened(&self, of: ItemId) -> Option<Item> {
        let item = self.item(of)?;
        let Shape::Box { frame, colour, fold: Some(fold) } = &item.shape else { return None };
        let frame = Rect::new(frame.x, frame.y, fold.open.0, fold.open.1);
        Some(Item { shape: Shape::Box { frame, colour: *colour, fold: None }, ..item.clone() })
    }

    /// The items in the way of a folded box's unfold, in item order: those
    /// in the same place as the box whose bounds touch its open frame. None
    /// for an open box.
    #[must_use]
    pub fn in_the_way(&self, of: ItemId) -> Vec<ItemId> {
        let Some(item) = self.item(of) else { return Vec::new() };
        let Some(fold) = item.shape.fold() else { return Vec::new() };
        let frame = item.bounds();
        let room = Rect::new(frame.x, frame.y, fold.open.0, fold.open.1);
        let holders = self.holders(item.page);
        let place = holders.get(&of);
        let blocks =
            |i: &&Item| i.id != of && holders.get(&i.id) == place && touch(i.bounds(), room);
        self.items(item.page).filter(blocks).map(Item::id).collect()
    }

    /// Where an item `w` by `h` goes in the box `of` (an answer to a sling
    /// of the box, docs/design/fold.md "Sling a box"): a place in its room
    /// (the open frame, open or folded) that comes within half a [`GAP`]
    /// of none of its [`Canvas::contents`]. Beside `near` first (right,
    /// below, left, above, as [`Canvas::free_beside`]), else the first such
    /// place in reading order, [`GAP`] in from the room's top left or past
    /// a content item. When none fits, a folded box takes it below all it
    /// holds (its open size grows to hold it on the add); an open box has
    /// no place: `None`. `None` too for an item that is not a box.
    #[must_use]
    pub fn free_in(&self, of: ItemId, near: Option<Rect>, w: f64, h: f64) -> Option<Rect> {
        let Shape::Box { frame, fold, .. } = &self.item(of)?.shape else { return None };
        let room = match fold {
            Some(f) => Rect::new(frame.x, frame.y, f.open.0, f.open.1),
            None => *frame,
        };
        let size = |v: f64| if v.is_finite() { v.max(0.0) } else { 0.0 };
        let (w, h) = (size(w), size(h));
        let taken: Vec<Rect> =
            self.contents(of).into_iter().filter_map(|i| self.item(i)).map(Item::bounds).collect();
        let free = |r: &Rect| {
            let near = grown(*r, GAP / 2.0);
            inside(*r, room) && !taken.iter().any(|t| touch(near, *t))
        };
        let sides = near.into_iter().flat_map(|n| {
            [
                Rect::new(n.x + n.w + GAP, n.y, w, h),
                Rect::new(n.x, n.y + n.h + GAP, w, h),
                Rect::new(n.x - GAP - w, n.y, w, h),
                Rect::new(n.x, n.y - GAP - h, w, h),
            ]
        });
        let xs = std::iter::once(room.x + GAP).chain(taken.iter().map(|t| t.x + t.w + GAP));
        let ys: Vec<f64> =
            std::iter::once(room.y + GAP).chain(taken.iter().map(|t| t.y + t.h + GAP)).collect();
        let mut scan: Vec<Rect> =
            xs.flat_map(|x| ys.iter().map(move |&y| Rect::new(x, y, w, h))).collect();
        scan.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
        if let Some(place) = sides.chain(scan).find(free) {
            return Some(place);
        }
        let foot = taken.iter().map(|t| t.y + t.h).fold(room.y, f64::max);
        fold.is_some().then(|| Rect::new(room.x + GAP, foot + GAP, w, h))
    }

    /// Whether `by` may set the box `item` to `shape` as a join (an add in
    /// a folded box, or the undo of one) though it may not change the box:
    /// the box is folded before and after, with the same frame and colour,
    /// and every item its list gains or loses is gone or one `by` may
    /// change. Its open size may change. An agent answers in the user's
    /// box this way (docs/design/fold.md, "Sling a box").
    pub(super) fn joins(&self, item: ItemId, shape: &Shape, by: &Author) -> bool {
        let Some(Shape::Box { frame, colour, fold: Some(old) }) = self.item(item).map(Item::shape)
        else {
            return false;
        };
        let Shape::Box { frame: f, colour: c, fold: Some(new) } = shape else { return false };
        let mine = |id: &ItemId| self.item(*id).is_none_or(|i| by.may_change(i.author()));
        let gained = new.items.iter().filter(|i| !old.items.contains(i));
        let lost = old.items.iter().filter(|i| !new.items.contains(i));
        f == frame && c == colour && gained.chain(lost).all(mine)
    }

    /// Each item on `page` a folded box holds, with the box that holds it
    /// directly. The one rule for "hidden": see the module doc.
    pub(crate) fn holders(&self, page: PageId) -> HashMap<ItemId, ItemId> {
        let mut held: HashMap<ItemId, ItemId> = HashMap::new();
        for b in self.items(page) {
            let Some(fold) = b.shape.fold() else { continue };
            for &m in &fold.items {
                let here = self.items.get(&m).is_some_and(|i| i.page == page);
                if m == b.id || !here || held.contains_key(&m) {
                    continue;
                }
                // `m` may not be `b`, or hold it at any depth.
                let mut up = Some(b.id);
                while let Some(u) = up.filter(|&u| u != m) {
                    up = held.get(&u).copied();
                }
                if up.is_none() {
                    held.insert(m, b.id);
                }
            }
        }
        held
    }
}

/// [`Canvas::held`] from `holders`, one page's [`Canvas::holders`]: it looks
/// only at the held items, not at the page, so a caller that carries several
/// boxes works the page out once.
pub(crate) fn held_in(holders: &HashMap<ItemId, ItemId>, of: ItemId) -> Vec<ItemId> {
    let under = |mut id: ItemId| {
        while let Some(&up) = holders.get(&id) {
            if up == of {
                return true;
            }
            id = up;
        }
        false
    };
    let mut held: Vec<ItemId> = holders.keys().copied().filter(|&id| under(id)).collect();
    // Ids rise, so id order is item order.
    held.sort_unstable();
    held
}

/// What `item` covers for a box's contents: its bounds, but a text only as
/// wide as the rows it draws. A text's frame is the column it wraps in, as
/// wide as a typed text's 48 characters however short its rows, so a box
/// drawn around a short text would leave it out (slinger-2c7eec).
fn covered(item: &Item) -> Rect {
    match &item.shape {
        Shape::Text { text, frame, size, .. } => {
            Rect { w: crate::render::text_drawn_width(text, frame.w, *size), ..*frame }
        }
        shape => shape.bounds(),
    }
}

/// Whether `r` lies wholly inside `frame`, edges included.
fn inside(r: Rect, frame: Rect) -> bool {
    r.x >= frame.x
        && r.y >= frame.y
        && r.x + r.w <= frame.x + frame.w
        && r.y + r.h <= frame.y + frame.h
}

/// Whether two rectangles share any point, edges included.
fn touch(a: Rect, b: Rect) -> bool {
    a.x <= b.x + b.w && b.x <= a.x + a.w && a.y <= b.y + b.h && b.y <= a.y + a.h
}
