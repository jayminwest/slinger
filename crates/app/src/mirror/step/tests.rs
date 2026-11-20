//! A pane stepped into a box shows only what the box holds.

use serde_json::{Value, json};
use slinger_control::CanvasPage;

use super::*;

fn page(items: Vec<Value>) -> Mirror {
    let pages = vec![json!({"id": "p1"})];
    Mirror::new(&CanvasPage {
        canvas: "main".into(),
        next: 1,
        page: "p1".into(),
        pages,
        items,
        path: None,
    })
}

/// A box at (`x`, `y`) tenths, `w` by `h`; folded to hold `items`, with
/// an open size of `open`, when given.
fn boxed(id: &str, frame: (i64, i64, i64, i64), fold: Option<((i64, i64), &[&str])>) -> Value {
    let mut b = json!({"id": id, "page": "p1", "kind": "box", "color": "#000000",
        "x": frame.0, "y": frame.1, "w": frame.2, "h": frame.3});
    if let Some((open, items)) = fold {
        b["fold"] = json!({"w": open.0, "h": open.1, "items": items});
    }
    b
}

/// A stroke from (`x`, `y`) tenths to 10 units right and down.
fn stroke(id: &str, x: i64, y: i64) -> Value {
    json!({"id": id, "page": "p1", "kind": "stroke", "color": "#ff0000", "width": 20,
        "pts": [x, y, 100, 100]})
}

fn text(id: &str, x: i64, y: i64, words: &str) -> Value {
    json!({"id": id, "page": "p1", "kind": "text", "color": "#000000",
        "x": x, "y": y, "w": 2000, "h": 300, "text": words})
}

/// What a pane inside `trail` shows of `m`, by id.
fn shown(m: &Mirror, trail: &[(ItemId, bool)]) -> Vec<u64> {
    let (inside, _) = m.step(trail);
    let inside = inside.expect("inside");
    let source = if inside.held() { m.held() } else { m.items() };
    source.iter().filter(|i| inside.shows(m, i)).map(|i| i.id.0).collect()
}

#[test]
fn inside_a_folded_box_a_pane_shows_what_it_holds_from_its_old_room() {
    let m = page(vec![
        boxed("i1", (1000, 1000, 1000, 500), Some(((4000, 2000), &["i2", "i3"]))),
        stroke("i2", 2000, 2000),
        text("i3", 1500, 1200, "\nPlans for the week, all of them here\nmore"),
        stroke("i4", 2000, 2000),
    ]);
    let (inside, held) = m.step(&[(ItemId(1), true)]);
    let inside = inside.unwrap();
    assert_eq!(held, 1);
    assert_eq!(inside.room, Rect { x: 100.0, y: 100.0, width: 400.0, height: 200.0 });
    assert_eq!(inside.joins(), Some(ItemId(1)), "what is written there joins it");
    assert_eq!(shown(&m, &[(ItemId(1), true)]), [2, 3], "not i4, on the page in its old room");
    assert_eq!(m.named(&inside), "Plans for the week, all…", "its first text's first line, cut");
}

#[test]
fn inside_an_open_box_a_pane_shows_what_lies_wholly_inside_it_now() {
    let m = page(vec![
        boxed("i1", (1000, 1000, 4000, 2000), None),
        stroke("i2", 2000, 2000),
        stroke("i3", 4950, 2000),
        stroke("i4", 9000, 9000),
    ]);
    let (inside, _) = m.step(&[(ItemId(1), false)]);
    let inside = inside.unwrap();
    assert_eq!(inside.joins(), None, "what is written there is on the page");
    assert_eq!(shown(&m, &[(ItemId(1), false)]), [2], "not the box, nor i3 across its edge");
    assert_eq!(m.named(&inside), "Box");
}

#[test]
fn a_pane_steps_into_a_box_inside_a_folded_box() {
    let m = page(vec![
        boxed("i1", (1000, 1000, 1000, 500), Some(((8000, 6000), &["i2", "i3", "i4"]))),
        boxed("i2", (2000, 2000, 3000, 3000), None),
        stroke("i3", 3000, 3000),
        stroke("i4", 7000, 1500),
    ]);
    let trail = [(ItemId(1), true), (ItemId(2), false)];
    assert_eq!(m.step(&trail).1, 2);
    assert_eq!(shown(&m, &trail), [3]);
    assert_eq!(m.step(&trail).0.unwrap().joins(), Some(ItemId(1)), "the folded box takes it");
}

#[test]
fn a_trail_ends_at_a_box_gone_or_folded_or_unfolded_since() {
    let m = page(vec![boxed("i1", (1000, 1000, 4000, 2000), None), stroke("i2", 2000, 2000)]);
    assert_eq!(m.step(&[(ItemId(1), true)]).1, 0, "unfolded since");
    assert_eq!(m.step(&[(ItemId(2), false)]).1, 0, "not a box");
    assert_eq!(m.step(&[(ItemId(1), false), (ItemId(9), false)]).1, 1, "gone");
    assert!(m.step(&[]).0.is_none());
    let folded = page(vec![boxed("i1", (1000, 1000, 1000, 500), Some(((4000, 2000), &[])))]);
    assert_eq!(folded.step(&[(ItemId(1), false)]).1, 0, "folded since");
}

#[test]
fn each_add_sent_from_inside_a_folded_box_says_it_goes_in() {
    let ops = json!([{"op": "add", "item": {}}, {"op": "move", "ids": ["i2"]}]);
    let joined = crate::mirror::joined(ops, ItemId(4));
    assert_eq!(joined[0]["in"], "i4");
    assert_eq!(joined[1].get("in"), None, "a move stays as it is");
    assert_eq!(crate::mirror::joined(json!({"op": "add"}), ItemId(4))["in"], "i4");
}
