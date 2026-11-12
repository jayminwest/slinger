//! What the ink shows over the server's copy of a page ([`Ink::shown`]),
//! and the items it adds there ([`Ink::adding`]).

use std::borrow::Cow;
use std::collections::HashSet;

use super::{Change, Ink, PENDING, Pending, Stroke};
use crate::scene::canvas::{Item, ItemId, Shape};

impl Ink {
    /// `items`, the server's copy of `page` of `canvas`, as the user should
    /// see it: each change not yet heard laid over it in the order made
    /// (removed items gone, moved ones moved, edited ones edited, new
    /// strokes, texts, boxes and arrows on top, each under the server's id
    /// once made, and not at all once `items` holds it), then the
    /// stroke under the pen. Items no change touches are borrowed, not
    /// copied, so a big page costs little.
    pub fn shown<'a>(&self, canvas: &str, page: &str, items: &'a [Item]) -> Vec<Cow<'a, Item>> {
        let mut shown: Vec<Cow<'a, Item>> = items.iter().map(Cow::Borrowed).collect();
        let made = self.pending.iter().any(|p| !p.made.is_empty());
        let held: HashSet<ItemId> = items.iter().filter(|_| made).map(|i| i.id).collect();
        let new = |id: &ItemId| !held.contains(id);
        let here = |c: &str, p: &str| c == canvas && p == page;
        // A carry within the canvas takes its items from whatever page they were on.
        let mine = |p: &&Pending| {
            let carry = matches!(&p.change, Change::Carry { gone, .. } if !gone.is_empty());
            let on = p.change.page().is_some_and(|page| here(p.change.canvas(), page));
            on || (carry && p.change.canvas() == canvas)
        };
        for pending in self.pending.iter().filter(mine) {
            match &pending.change {
                Change::Add(stroke) => {
                    let id = pending.id(0);
                    shown.extend(new(&id).then(|| Cow::Owned(local(id, stroke))));
                }
                Change::Carry { page: to, items, gone, .. } => {
                    let gone: HashSet<&ItemId> = gone.iter().collect();
                    shown.retain(|i| !gone.contains(&i.id));
                    let ids = (0..).map(|n| pending.id(n));
                    let landed = items.iter().zip(ids).filter(|(_, id)| to == page && new(id));
                    shown.extend(landed.map(|(c, id)| Cow::Owned(Item { id, ..c.item.clone() })));
                }
                Change::Remove { ids, .. } => {
                    let ids: HashSet<&ItemId> = ids.iter().collect();
                    shown.retain(|i| !ids.contains(&i.id));
                }
                Change::Move { ids, by, follow, .. } => {
                    let ids: HashSet<&ItemId> = ids.iter().collect();
                    for item in &mut shown {
                        if let Some(after) = follow.iter().find(|f| f.id == item.id) {
                            *item = Cow::Owned(after.clone());
                        } else if ids.contains(&item.id) {
                            let (id, colour) = (item.id, item.colour);
                            *item = Cow::Owned(Item { id, colour, shape: item.shape.moved(*by) });
                        }
                    }
                }
                Change::Put { colour, shape, .. } => {
                    let id = pending.id(0);
                    let item = Item { id, colour: *colour, shape: shape.clone() };
                    shown.extend(new(&id).then_some(Cow::Owned(item)));
                }
                Change::Set { id, shape, .. } => {
                    for item in shown.iter_mut().filter(|i| i.id == *id) {
                        let colour = item.colour;
                        *item = Cow::Owned(Item { id: *id, colour, shape: shape.clone() });
                    }
                }
                Change::Reshape { items: after, .. } => {
                    for item in &mut shown {
                        if let Some(after) = after.iter().find(|a| a.id == item.id) {
                            *item = Cow::Owned(after.clone());
                        }
                    }
                }
                Change::Page { .. } | Change::Step { .. } => {}
            }
        }
        let drawing = self.drawing.iter().filter(|(_, s)| here(&s.canvas, &s.page));
        shown.extend(drawing.map(|(key, stroke)| Cow::Owned(local(ItemId(PENDING + key), stroke))));
        shown
    }

    /// Each change not yet heard that adds items to `page` of `canvas` (a
    /// stroke, a put or a carry there), in the order made, with the id each
    /// of its adds shows under ([`Ink::shown`]): what a copy or a carry of
    /// those items writes them from.
    pub fn adding<'a>(
        &'a self,
        canvas: &'a str,
        page: &'a str,
    ) -> impl Iterator<Item = (&'a Change, Vec<ItemId>)> + 'a {
        self.pending.iter().filter_map(move |p| {
            let adds = match &p.change {
                Change::Add(_) | Change::Put { .. } => 1,
                Change::Carry { items, .. } => u64::try_from(items.len()).unwrap_or(0),
                _ => return None,
            };
            let here = p.change.canvas() == canvas && p.change.page() == Some(page);
            here.then(|| (&p.change, (0..adds).map(|n| p.id(n)).collect()))
        })
    }
}

/// A stroke of local ink as an item, under `id`.
fn local(id: ItemId, stroke: &Stroke) -> Item {
    Item {
        id,
        colour: stroke.colour,
        shape: Shape::Stroke { points: stroke.points.clone(), width: stroke.width },
    }
}
