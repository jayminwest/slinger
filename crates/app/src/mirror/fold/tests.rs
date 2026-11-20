//! A folded box hides what it holds and draws it as its miniature.

use serde_json::{Value, json};
use slinger_control::{CanvasFeed, CanvasPage};

use super::*;
use crate::mirror::Applied;

fn shown(items: Vec<Value>) -> CanvasPage {
    CanvasPage {
        canvas: "main".into(),
        next: 5,
        page: "p1".into(),
        pages: vec![json!({"id": "p1", "name": "one"})],
        items,
        path: None,
    }
}

/// A box at (`x`, `y`) tenths, `w` by `h`, folded to hold `items` with
/// an open size of `open` when given.
fn boxed(id: &str, frame: (i64, i64, i64, i64), fold: Option<((i64, i64), &[&str])>) -> Value {
    let mut shown = json!({"id": id, "page": "p1", "kind": "box", "color": "#000000",
        "x": frame.0, "y": frame.1, "w": frame.2, "h": frame.3});
    if let Some((open, items)) = fold {
        shown["fold"] = json!({"w": open.0, "h": open.1, "items": items});
    }
    shown
}

/// A stroke from (`x`, `y`) tenths to 10 units right and down.
fn stroke(id: &str, x: i64, y: i64) -> Value {
    json!({"id": id, "page": "p1", "kind": "stroke", "color": "#ff0000", "width": 20,
        "pts": [x, y, 100, 100]})
}

fn ids(items: &[Item]) -> Vec<u64> {
    items.iter().map(|i| i.id.0).collect()
}

fn points(item: &Item) -> Vec<(f64, f64)> {
    let Shape::Stroke { points, .. } = &item.shape else { panic!("{item:?}") };
    points.iter().map(|p| (p.x, p.y)).collect()
}

/// A box opened at (100, 100), 400 by 200, folded to (100, 100), 100 by 50.
fn folded_box(items: &[&str]) -> Value {
    boxed("i1", (1000, 1000, 1000, 500), Some(((4000, 2000), items)))
}

#[test]
fn a_folded_box_hides_what_it_lists_and_draws_it_scaled_in_its_miniature() {
    let m = Mirror::new(&shown(vec![
        folded_box(&["i2", "i9"]),
        stroke("i2", 3000, 2000),
        stroke("i3", 3000, 2000),
    ]));
    assert_eq!(ids(m.items()), [1, 3], "i2 is hidden; i9 is gone and skipped");
    assert_eq!((m.box_folded(ItemId(1)), m.box_folded(ItemId(3))), (Some(true), None));
    let room = Rect { x: 100.0, y: 100.0, width: 400.0, height: 200.0 };
    assert_eq!(m.room(ItemId(1)), Some(room));
    let mini = m.miniature(ItemId(1)).unwrap();
    assert_eq!(ids(mini), [2]);
    // (300, 200) is (200, 100) into the room: a quarter of that into the miniature.
    assert_eq!(points(&mini[0]), [(150.0, 125.0), (152.5, 127.5)]);
    assert!(matches!(mini[0].shape, Shape::Stroke { width: 0.5, .. }), "no thinner than 0.5");
    let peek = m.inside(ItemId(1), room);
    assert_eq!(points(&peek[0]), [(300.0, 200.0), (310.0, 210.0)], "a peek is at full size");
}

#[test]
fn a_set_of_fold_hides_and_a_set_of_null_shows_again() {
    let mut m = Mirror::new(&shown(vec![
        boxed("i1", (1000, 1000, 4000, 2000), None),
        stroke("i2", 3000, 2000),
    ]));
    assert_eq!((ids(m.items()), m.box_folded(ItemId(1))), (vec![1, 2], Some(false)));
    let fold = json!({"w": 4000, "h": 2000, "items": ["i2"]});
    let set = |fields: Value| json!({"op": "set", "id": "i1", "fields": fields});
    let folding = set(json!({"w": 1000, "h": 500, "fold": fold}));
    assert_eq!(m.apply(&CanvasFeed { next: 6, ops: vec![folding] }), Applied::Changed);
    assert_eq!(ids(m.items()), [1]);
    assert_eq!(ids(m.miniature(ItemId(1)).unwrap()), [2]);

    // A move of the box and what it holds moves the miniature's contents too.
    let carried = json!({"op": "move", "ids": ["i1", "i2"], "dx": 100, "dy": 0});
    m.apply(&CanvasFeed { next: 7, ops: vec![carried] });
    assert_eq!(points(&m.miniature(ItemId(1)).unwrap()[0])[0], (160.0, 125.0));

    let opening = set(json!({"w": 4000, "h": 2000, "fold": null}));
    assert_eq!(m.apply(&CanvasFeed { next: 8, ops: vec![opening] }), Applied::Changed);
    assert_eq!(ids(m.items()), [1, 2], "shown again");
    assert_eq!((m.miniature(ItemId(1)), m.box_folded(ItemId(1))), (None, Some(false)));
}

#[test]
fn a_folded_box_inside_draws_its_own_miniature_and_one_item_has_one_holder() {
    let m = Mirror::new(&shown(vec![
        folded_box(&["i2", "i4"]),
        // Opened at (300, 100), 160 by 80; folded to 40 by 20.
        boxed("i2", (3000, 1000, 400, 200), Some(((1600, 800), &["i3", "i1"]))),
        stroke("i3", 3800, 1400),
        // Listed by both: the lower box id holds it.
        stroke("i4", 1000, 1000),
        boxed("i5", (6000, 6000, 400, 200), Some(((800, 400), &["i4", "i5"]))),
    ]));
    assert_eq!(ids(m.items()), [1, 5], "i2 is in i1, i3 in i2; i1 is never in i2");
    let mini = m.miniature(ItemId(1)).unwrap();
    assert_eq!(ids(mini), [2, 3, 4]);
    // i3, (80, 40) into i2's room, is (20, 10) into i2's miniature at
    // (300, 100): (220, 10) into i1's room, a quarter of that into its miniature.
    assert_eq!(points(&mini[1])[0], (155.0, 102.5));
    assert!(m.miniature(ItemId(5)).unwrap().is_empty(), "i4 is i1's; i5 never holds itself");
}

#[test]
fn a_miniature_thins_a_line_to_the_points_it_can_show_and_keeps_its_ends() {
    let p = |x: f64| Point { x, y: 0.0 };
    let line = thinned([0.0, 0.1, 0.2, 0.6, 0.7, 2.0, 2.1].into_iter().map(p));
    assert_eq!(line, [p(0.0), p(0.6), p(2.0), p(2.1)]);
    assert_eq!(thinned([p(1.0)].into_iter()), [p(1.0)]);
}

#[test]
fn a_fold_without_an_open_size_is_an_open_box() {
    let bad = json!({"id": "i1", "page": "p1", "kind": "box", "x": 0, "y": 0, "w": 10, "h": 10,
        "fold": {"w": 0, "h": 10, "items": ["i2"]}});
    let m = Mirror::new(&shown(vec![bad, stroke("i2", 0, 0)]));
    assert_eq!((ids(m.items()), m.box_folded(ItemId(1))), (vec![1, 2], Some(false)));
}
