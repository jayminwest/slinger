//! What is new after the last sling, through the public interface only: an
//! operation log and a marker in, the changes out.
#![cfg(test)]

use slinger_core::canvas::{
    Action, Author, Canvas, Changes, Colour, ItemId, Marker, Op, PageId, Point, Rect, Shape, Time,
    changes_since,
};

/// The page a new canvas starts with.
const PAGE: PageId = PageId::new(1);

fn op(by: &Author, action: Action) -> Op {
    Op { by: by.clone(), at: Time(1_700_000_000_000), action }
}

fn user(action: Action) -> Op {
    op(&Author::User, action)
}

fn stroke(x: f64) -> Shape {
    Shape::Stroke {
        points: vec![Point::new(x, 0.0), Point::new(x + 10.0, 5.0)],
        colour: Colour::BLACK,
        width: 2.0,
    }
}

fn add(x: f64) -> Op {
    user(Action::Add { page: PAGE, shape: stroke(x) })
}

/// Item ids are handed out from 1 in the order items are added.
fn id(n: u64) -> ItemId {
    ItemId::new(n)
}

fn ids(ns: &[u64]) -> Vec<ItemId> {
    ns.iter().copied().map(id).collect()
}

/// Two strokes, then the last sling.
fn slung() -> (Vec<Op>, Marker) {
    (vec![add(0.0), add(50.0)], Marker(2))
}

#[test]
fn items_added_after_the_marker_are_new() {
    let (mut log, marker) = slung();
    log.extend([add(100.0), add(150.0)]);
    let c = changes_since(&log, marker);
    assert_eq!(c.new, ids(&[3, 4]));
    assert!(c.changed.is_empty() && c.removed.is_empty());
    assert_eq!((c.from, c.to), (Marker(2), Marker(4)));
}

#[test]
fn an_edited_or_moved_item_is_changed_not_new() {
    let (mut log, marker) = slung();
    log.push(user(Action::Edit { item: id(1), shape: stroke(300.0) }));
    log.push(user(Action::Move { item: id(2), by: Point::new(5.0, 5.0) }));
    let c = changes_since(&log, marker);
    assert_eq!(c.changed, ids(&[1, 2]));
    assert!(c.new.is_empty() && c.removed.is_empty());
}

#[test]
fn an_item_moved_and_moved_back_is_not_changed() {
    let (mut log, marker) = slung();
    log.push(user(Action::Move { item: id(1), by: Point::new(8.0, -4.0) }));
    log.push(user(Action::Move { item: id(1), by: Point::new(-8.0, 4.0) }));
    assert!(changes_since(&log, marker).is_empty());
}

#[test]
fn an_item_removed_after_the_marker_is_removed() {
    let (mut log, marker) = slung();
    log.push(user(Action::Remove { item: id(1) }));
    let c = changes_since(&log, marker);
    assert_eq!(c.removed, ids(&[1]));
    assert!(c.new.is_empty() && c.changed.is_empty());
}

#[test]
fn an_item_added_then_removed_after_the_marker_is_not_new() {
    let (mut log, marker) = slung();
    log.push(add(100.0));
    log.push(user(Action::Remove { item: id(3) }));
    let c = changes_since(&log, marker);
    assert!(c.is_empty(), "{c:?}");
}

#[test]
fn an_item_added_then_edited_after_the_marker_is_new() {
    let (mut log, marker) = slung();
    log.push(add(100.0));
    log.push(user(Action::Edit { item: id(3), shape: stroke(400.0) }));
    let c = changes_since(&log, marker);
    assert_eq!(c.new, ids(&[3]));
    assert_eq!(c.changed, ids(&[]));
}

#[test]
fn nothing_after_the_marker_is_nothing_new() {
    // Spike C: the third sling reselected old ink and sent 0 new strokes.
    let (log, marker) = slung();
    let c = changes_since(&log, marker);
    assert!(c.is_empty());
    assert_eq!((c.from, c.to), (Marker(2), Marker(2)));
}

#[test]
fn the_end_of_one_answer_is_the_marker_for_the_next() {
    let (mut log, marker) = slung();
    log.push(add(100.0));
    let first = changes_since(&log, marker);
    assert_eq!(first.new, ids(&[3]));
    log.push(add(200.0));
    assert_eq!(changes_since(&log, first.to).new, ids(&[4]));
}

#[test]
fn a_marker_past_the_end_is_at_the_end() {
    let (log, _) = slung();
    let c = changes_since(&log, Marker(99));
    assert_eq!(c, Changes { from: Marker(2), to: Marker(2), ..Changes::default() });
}

#[test]
fn the_first_sling_sees_everything_and_an_empty_log_sees_nothing() {
    let (log, _) = slung();
    assert_eq!(changes_since(&log, Marker(0)).new, ids(&[1, 2]));
    assert!(changes_since(&[], Marker(0)).is_empty());
}

#[test]
fn removing_a_page_removes_its_old_items_and_hides_its_new_ones() {
    let (mut log, marker) = slung();
    log.push(user(Action::PageAdd { name: "spec".into() }));
    let spec = PageId::new(2);
    log.push(user(Action::Add { page: spec, shape: stroke(0.0) }));
    let c = changes_since(&log, Marker(4));
    assert!(c.is_empty());
    assert_eq!(changes_since(&log, marker).new, ids(&[3]));

    log.push(user(Action::PageRemove { page: spec }));
    assert_eq!(changes_since(&log, Marker(4)).removed, ids(&[3]));
    assert!(changes_since(&log, marker).is_empty());
}

#[test]
fn a_refused_op_in_the_log_changes_nothing() {
    let (mut log, marker) = slung();
    // An agent may not change the user's ink, and item 9 does not exist.
    log.push(op(&Author::Agent("scout".into()), Action::Remove { item: id(1) }));
    log.push(user(Action::Move { item: id(9), by: Point::new(1.0, 1.0) }));
    log.push(user(Action::Add {
        page: PAGE,
        shape: Shape::Text {
            text: "t".into(),
            frame: Rect::new(0.0, 0.0, 9.0, 9.0),
            colour: Colour::BLACK,
        },
    }));
    let c = changes_since(&log, marker);
    assert_eq!(c.new, ids(&[3]));
    assert!(c.changed.is_empty() && c.removed.is_empty());
}

#[test]
fn an_item_removed_then_restored_is_not_changed() {
    let (mut log, marker) = slung();
    let mut canvas = Canvas::new();
    for op in &log {
        canvas.apply(op.clone()).unwrap();
    }
    let item = canvas.item(id(1)).cloned().unwrap();
    log.push(user(Action::Remove { item: id(1) }));
    log.push(user(Action::Restore { item }));
    assert!(changes_since(&log, marker).is_empty());
}
