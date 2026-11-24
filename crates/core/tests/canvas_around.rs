//! What a drawing script sees round its figure (docs/design/figures.md,
//! rule 5): the anchor beside or over, the items around with `over` or
//! `at`, nothing in a box; a redraw reads the anchor again.
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::canvas::Author;
use slinger_core::canvas::Time;
use slinger_core::canvas::file::{AROUND, Log, Placement, REACH, Script, Surround};

const AT: Time = Time(1_762_790_400_000);

/// A text i1 "hi" at (0, 0), a box i2 at (100, 0) 100x100 and a stroke i3
/// in it, a stroke i4 far away; and the figure `fig` (i5) over the box.
fn log() -> Log {
    let (mut log, _) = Log::create("around").unwrap();
    let add = |item| json!({"op": "add", "item": item});
    let ops = json!([
        add(json!({"page": "p1", "kind": "text", "x": 0, "y": 0, "text": "hi"})),
        add(json!({"page": "p1", "kind": "box", "x": 1000, "y": 0, "w": 1000, "h": 1000})),
        add(json!({"page": "p1", "kind": "stroke", "width": 20, "pts": [1200, 200, 300, 0]})),
        add(json!({"page": "p1", "kind": "stroke", "width": 20, "pts": [90000, 0, 100, 0]})),
    ]);
    log.apply(&Author::User, AT, &ops).unwrap();
    let shapes = [json!({"kind": "box", "x": 0, "y": 0, "w": 100, "h": 100})];
    let script = Script {
        text: "fig.box(0, 0, 10, 10)".into(),
        data: Value::Null,
        target: Some([1000, 0, 1000, 1000]),
        anchor: Some("i2".into()),
    };
    let over = Placement::Over("i2".into());
    log.place_figure(&Author::User, AT, ("fig", &shapes), &over, None, Some(script)).unwrap();
    log
}

#[test]
fn beside_gives_the_anchor_and_in_gives_nothing() {
    let log = log();
    let seen = log.surround("new", &Placement::Beside("i1".into())).unwrap();
    let anchor = seen.anchor.unwrap();
    assert_eq!(
        (anchor.id.as_str(), anchor.kind, anchor.text.as_deref()),
        ("i1", "text", Some("hi"))
    );
    assert_eq!(seen.around, []);
    assert_eq!(log.surround("new", &Placement::In("i2".into())).unwrap(), Surround::default());
    let figure = log.surround("new", &Placement::Beside("fig".into())).unwrap();
    assert_eq!((figure.anchor.unwrap().kind), "figure");
    assert!(log.surround("new", &Placement::Beside("i99".into())).is_err());
}

#[test]
fn over_and_at_give_the_items_near_in_figure_coordinates_nearest_first() {
    let log = log();
    let over = log.surround("new", &Placement::Over("i2".into())).unwrap();
    let ids: Vec<&str> = over.around.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["i3", "i5", "i1"], "not the box, not the far stroke");
    assert_eq!((over.around[0].frame.x, over.around[0].frame.y), (19.0, 19.0));
    assert_eq!(over.around[2].frame.x, -100.0, "left of the box");
    let own = log.surround("fig", &Placement::Over("i2".into())).unwrap();
    assert!(own.around.iter().all(|n| n.id != "i5"), "a figure never sees its own items");
    let at = Placement::At { page: None, x: 1000, y: 0 };
    let at = log.surround("new", &at).unwrap();
    assert!(at.anchor.is_none());
    assert_eq!(at.around[0].id, "i2");
    assert!(at.around.len() <= AROUND && REACH > 0.0);
}

#[test]
fn a_redraw_reads_the_anchor_again_and_finds_the_items_where_the_figure_is() {
    let log = log();
    let again = log.again(&Author::User, "fig").unwrap();
    let over = log.surround_again("fig", &again, Some("i2"), Some([1000, 0, 1000, 1000]));
    assert_eq!(over.anchor.unwrap().id, "i2");
    assert_eq!(over.around.first().map(|n| n.id.as_str()), Some("i3"));
    let gone = log.surround_again("fig", &again, Some("i77"), Some([1000, 0, 1000, 1000]));
    assert!(gone.anchor.is_none(), "an anchor that is gone is none");
    let at = log.surround_again("fig", &again, None, None);
    assert_eq!(at.around.first().map(|n| n.id.as_str()), Some("i2"));
    let beside = log.surround_again("fig", &again, Some("i1"), None);
    assert!(beside.anchor.is_some() && beside.around.is_empty());
}
