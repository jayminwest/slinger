//! What a pane stepped into boxes shows of the page (docs/design/fold.md,
//! "Step in"): inside a folded box, what it holds directly; inside an open
//! box, what lies wholly inside its frame now, of what the level above
//! shows. A trail is the boxes stepped into, outermost first, each with
//! whether it was folded then: a box that is gone, has left the level
//! above, or was folded or unfolded since, ends the trail there
//! ([`Mirror::step`]). Pure.

use super::Mirror;
use crate::layout::Rect;
use crate::scene::canvas::{Item, ItemId, Shape};

/// How many characters of a box's first text its name in a trail keeps.
const NAME: usize = 24;

/// A pane's place inside boxes, as [`Mirror::step`] finds it on the page.
#[derive(Debug, Clone, PartialEq)]
pub struct Inside {
    /// The innermost folded box of the trail: the pane shows what it holds
    /// directly, from the hidden items. `None`: from the page's.
    folded: Option<ItemId>,
    /// Each open box after it, and its frame: an item shows only when it
    /// lies wholly inside each.
    open: Vec<(ItemId, Rect)>,
    /// The innermost box's room: its open frame.
    pub room: Rect,
}

impl Inside {
    /// Whether the pane inside shows `item`, as the ink shows it (moved,
    /// say): any item the server has not made yet (new ink) does.
    pub fn shows(&self, mirror: &Mirror, item: &Item) -> bool {
        if mirror.item(item.id).is_none() {
            return true;
        }
        let holder = mirror.holders.get(&item.id);
        let level = self.folded.map_or(holder.is_none(), |b| holder == Some(&b));
        let bounds = item.shape.bounds();
        let inside =
            |&(b, frame): &(ItemId, Rect)| b != item.id && bounds.is_some_and(|i| within(i, frame));
        level && self.open.iter().all(inside)
    }

    /// The folded box what is written inside joins: the innermost of the
    /// trail, if any.
    pub fn joins(&self) -> Option<ItemId> {
        self.folded
    }

    /// The innermost box of the trail, open or folded: what a sling from
    /// the pane is of.
    pub fn innermost(&self) -> Option<ItemId> {
        self.open.last().map(|(id, _)| *id).or(self.folded)
    }

    /// Whether the pane shows the hidden items: it is inside a folded box.
    pub fn held(&self) -> bool {
        self.folded.is_some()
    }
}

impl Mirror {
    /// Item `id`, shown on the page or hidden in a folded box.
    pub fn item(&self, id: ItemId) -> Option<&Item> {
        let (held, at) = self.at(id)?;
        Some(if held { &self.held[at] } else { &self.items[at] })
    }

    /// The items folded boxes hide, in id order.
    pub fn held(&self) -> &[Item] {
        &self.held
    }

    /// Where a pane stepped into `trail` is, and how many of its steps
    /// still hold: all of them, or the trail ends before the first box that
    /// is gone, no longer at the level above, or no longer folded (or open)
    /// as it was. `None` for an empty trail, or one whose first step fails.
    pub fn step(&self, trail: &[(ItemId, bool)]) -> (Option<Inside>, usize) {
        let mut inside: Option<Inside> = None;
        for (n, &(id, was)) in trail.iter().enumerate() {
            let Some(item) = self.item(id) else { return (inside, n) };
            let Shape::Box { frame, .. } = &item.shape else { return (inside, n) };
            let here = match &inside {
                None => !self.holders.contains_key(&id),
                Some(up) => up.shows(self, item),
            };
            let folded = self.fold_of(id).is_some();
            if !here || folded != was {
                return (inside, n);
            }
            inside = Some(match (inside, self.room(id)) {
                (_, Some(room)) => Inside { folded: Some(id), open: Vec::new(), room },
                (None, None) => Inside { folded: None, open: vec![(id, *frame)], room: *frame },
                (Some(mut up), None) => {
                    up.open.push((id, *frame));
                    up.room = *frame;
                    up
                }
            });
        }
        (inside, trail.len())
    }

    /// Box `id` as the trail names it: the first line of the first text it
    /// shows inside, in reading order, cut to [`NAME`] characters; else "Box".
    pub fn named(&self, inside: &Inside) -> String {
        let source = if inside.held() { &self.held } else { &self.items };
        let mut texts: Vec<(Rect, &str)> = (source.iter())
            .filter(|i| inside.shows(self, i))
            .filter_map(|i| match &i.shape {
                Shape::Text { frame, text, .. } => Some((*frame, text.as_str())),
                _ => None,
            })
            .collect();
        texts.sort_by(|(a, _), (b, _)| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
        let first = texts.first().and_then(|(_, t)| t.lines().find(|l| !l.trim().is_empty()));
        match first {
            Some(line) if line.chars().count() > NAME => {
                format!("{}…", line.chars().take(NAME).collect::<String>().trim_end())
            }
            Some(line) => line.trim().to_string(),
            None => "Box".to_string(),
        }
    }
}

/// Whether `inner` lies wholly inside `outer`, edges included.
fn within(inner: Rect, outer: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

#[cfg(test)]
mod tests;
