//! An arrow's ends bound to items: which item an end lands on, and how the
//! bound ends follow a move or a resize of those items.

use std::borrow::Cow;
use std::collections::HashSet;

use crate::layout::{Point, Rect};
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
    ends(items, |id| moved.contains(&id), |_, p| map(p))
}

/// The arrows of `items` that follow items resized in place, each of
/// `framed` an item's id with its box before and after: a bound end keeps
/// its place on its item's box, as a part of the box's width and height.
pub fn follow_frames(items: &[Cow<'_, Item>], framed: &[(ItemId, Rect, Rect)]) -> Vec<Item> {
    let find = |id: ItemId| framed.iter().find(|(i, ..)| *i == id);
    let stretch = |p: Point, (before, after): (Rect, Rect)| {
        let part = |a: f64, b: f64| if b > 0.0 { a / b } else { 1.0 };
        Point {
            x: after.x + (p.x - before.x) * part(after.width, before.width),
            y: after.y + (p.y - before.y) * part(after.height, before.height),
        }
    };
    let map = |id, p| find(id).map_or(p, |&(_, before, after)| stretch(p, (before, after)));
    ends(items, |id| find(id).is_some(), map)
}

/// The arrows of `items`, not `moved` themselves, with an end bound to an
/// item that is, as they are after: that end put where `map` says for its item.
fn ends(
    items: &[Cow<'_, Item>],
    moved: impl Fn(ItemId) -> bool,
    map: impl Fn(ItemId, Point) -> Point,
) -> Vec<Item> {
    let mut follows = Vec::new();
    for item in items.iter().filter(|i| !moved(i.id)) {
        let Shape::Arrow { from, to, width, bound } = &item.shape else { continue };
        if !bound.iter().flatten().any(|&id| moved(id)) {
            continue;
        }
        let shift = |p: Point, end: Option<ItemId>| match end {
            Some(id) if moved(id) => map(id, p),
            _ => p,
        };
        let (from, to) = (shift(*from, bound[0]), shift(*to, bound[1]));
        let shape = Shape::Arrow { from, to, width: *width, bound: *bound };
        follows.push(Item { id: item.id, colour: item.colour, shape });
    }
    follows
}
