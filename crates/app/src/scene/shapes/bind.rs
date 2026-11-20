//! An arrow's ends bound to items: which item an end lands on, and how the
//! bound ends follow a move or a resize of those items.

use std::borrow::Cow;
use std::collections::HashSet;

use crate::layout::Point;
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::ink::on_server;

/// The topmost server item of `items` an arrow's end at `at` binds to: one
/// whose box, `reach` canvas units bigger, holds the end. Arrows are not
/// bound to.
pub fn target(items: &[Cow<'_, Item>], at: Point, reach: f64) -> Option<ItemId> {
    let bindable = items.iter().filter(|i| on_server(i.id));
    let bindable = bindable.filter(|i| !matches!(i.shape, Shape::Arrow { .. }));
    let mut found =
        bindable.filter(|i| i.shape.bounds().is_some_and(|b| b.outset(reach).contains(at)));
    found.next_back().map(|i| i.id)
}

/// The arrows of `items` that follow a move of the items `ids` by `by`, as
/// they are after it: each arrow not moved itself with an end bound to a
/// moved item, that end moved by `by`.
pub fn follow(items: &[Cow<'_, Item>], ids: &[ItemId], by: Point) -> Vec<Item> {
    follow_by(items, ids, |p| Point { x: p.x + by.x, y: p.y + by.y })
}

/// As [`follow`], with each bound end put where `map` says: a move, or a
/// resize of the selection.
pub fn follow_by(
    items: &[Cow<'_, Item>],
    ids: &[ItemId],
    map: impl Fn(Point) -> Point,
) -> Vec<Item> {
    let moved: HashSet<&ItemId> = ids.iter().collect();
    let on = |end: Option<ItemId>| end.is_some_and(|id| moved.contains(&id));
    let mut follows = Vec::new();
    for item in items.iter().filter(|i| !moved.contains(&i.id)) {
        let Shape::Arrow { from, to, width, bound } = &item.shape else { continue };
        if !on(bound[0]) && !on(bound[1]) {
            continue;
        }
        let shift = |p: Point, end| if on(end) { map(p) } else { p };
        let (from, to) = (shift(*from, bound[0]), shift(*to, bound[1]));
        let shape = Shape::Arrow { from, to, width: *width, bound: *bound };
        follows.push(Item { id: item.id, colour: item.colour, shape });
    }
    follows
}
