//! How an agent changes or takes back what it put (`Log::edit`,
//! `Log::remove`): only its own items, each change one undo step.
#![cfg(test)]

use slinger_core::canvas::file::{Edit, Log, Placed, Put};
use slinger_core::canvas::{Author, Time};

const AT: Time = Time(1_762_790_400_000);

fn scout() -> Author {
    Author::Agent("scout".into())
}

/// A log whose page holds the user's box `i1`, scout's text `i2` and
/// scout's arrow `i3` from one to the other.
fn log() -> Log {
    let (mut log, _) = Log::create("idea").unwrap();
    let bx = serde_json::json!({ "op": "add", "item": { "page": "p1", "kind": "box", "x": 0, "y": 0, "w": 1000, "h": 500 } });
    log.apply(&Author::User, AT, &bx).unwrap();
    log.put(&scout(), AT, "i1", &Put::Text("hi".into())).unwrap();
    let arrow = Put::Arrow { from: "i1".into(), to: "i2".into() };
    log.put(&scout(), AT, "i1", &arrow).unwrap();
    log
}

/// What a put of `words` beside the user's box makes, on a fresh log.
fn log_with(words: &str) -> Placed {
    let (mut log, _) = Log::create("other").unwrap();
    let bx = serde_json::json!({ "op": "add", "item": { "page": "p1", "kind": "box", "x": 0, "y": 0, "w": 1000, "h": 500 } });
    log.apply(&Author::User, AT, &bx).unwrap();
    log.put(&scout(), AT, "i1", &Put::Text(words.into())).unwrap()
}

#[test]
fn new_text_resizes_its_frame_unless_a_size_is_given() {
    let mut log = log();
    let words = "a longer answer than the first one, and it wraps";
    let edit = Edit { text: Some(words.into()), ..Edit::default() };
    let edited = log.edit(&scout(), AT, "i2", &edit).unwrap();
    // The frame is the one a put of the same words gets.
    let fresh = log_with(words);
    assert_eq!(edited.frame[2..], fresh.frame[2..]);
    let edit = Edit { text: Some("short".into()), size: Some([900, 300]), to: Some([-50, 70]) };
    let edited = log.edit(&scout(), AT, "i2", &edit).unwrap();
    assert_eq!(edited.frame, [-50, 70, 900, 300]);
    assert_eq!(edited.written.seq.len(), 2, "the text, and the arrow that follows it");
    // Undo takes back one edit at a time.
    log.undo(&scout(), AT);
    let item = log.show(None).unwrap().items[1].clone();
    assert_eq!(item["text"], words);
}

#[test]
fn an_arrow_moves_but_has_no_text_or_size() {
    let mut log = log();
    let moved = log.edit(&scout(), AT, "i3", &Edit { to: Some([0, 0]), ..Edit::default() });
    assert_eq!(moved.unwrap().frame[..2], [0, 0]);
    for edit in [
        Edit { text: Some("x".into()), ..Edit::default() },
        Edit { size: Some([1, 1]), ..Edit::default() },
    ] {
        let err = log.edit(&scout(), AT, "i3", &edit).unwrap_err();
        assert_eq!(err.code(), "invalid", "{edit:?}");
    }
}

#[test]
fn the_user_may_edit_anything_and_an_agent_only_its_own() {
    let mut log = log();
    let to = Edit { to: Some([0, 2000]), ..Edit::default() };
    assert!(log.edit(&Author::User, AT, "i2", &to).is_ok());
    let err = log.edit(&scout(), AT, "i1", &to).unwrap_err();
    assert!(err.hint().is_some_and(|h| h.contains("only items it made")), "{err:?}");
    assert_eq!(log.remove(&Author::Agent("ada".into()), AT, "i2").unwrap_err().code(), "invalid");
    assert_eq!(log.remove(&scout(), AT, "i2").unwrap().seq.len(), 1);
    assert_eq!(log.remove(&scout(), AT, "i2").unwrap().bytes, "");
    assert_eq!(log.remove(&scout(), AT, "x2").unwrap_err().code(), "invalid");
}

/// The field `key` of the item `id`, as `canvas show` gives it.
fn field(log: &Log, id: &str, key: &str) -> serde_json::Value {
    let shown = log.show(None).unwrap();
    shown.items.iter().find(|i| i["id"] == id).unwrap()[key].clone()
}

/// The `pts` of the arrow `id`: the start, then the step to the end.
fn pts(log: &Log, id: &str) -> Vec<i64> {
    let pts = field(log, id, "pts");
    pts.as_array().unwrap().iter().map(|n| n.as_i64().unwrap()).collect()
}

#[test]
fn a_move_takes_the_ends_of_the_agents_arrows_bound_to_the_item_along() {
    let mut log = log();
    // The user's arrow from scout's text to the user's box; scout's i3 runs
    // the other way.
    let theirs = serde_json::json!({ "op": "add", "item": { "page": "p1", "kind": "arrow",
        "pts": [1300, 100, -400, 0], "from": "i2", "to": "i1" } });
    log.apply(&Author::User, AT, &theirs).unwrap();
    let (ours, user) = (pts(&log, "i3"), pts(&log, "i4"));
    let [x, y] = ["x", "y"].map(|k| field(&log, "i2", k).as_i64().unwrap());
    let to = Edit { to: Some([x + 100, y + 2000]), ..Edit::default() };
    let moved = log.edit(&scout(), AT, "i2", &to).unwrap();
    assert_eq!(moved.written.seq.len(), 2, "the text and scout's arrow, in one step");
    let after = pts(&log, "i3");
    assert_eq!(after[..2], ours[..2], "the end on the user's box stays");
    let end = |p: &[i64]| [p[0] + p[2], p[1] + p[3]];
    assert_eq!(
        end(&after),
        [end(&ours)[0] + 100, end(&ours)[1] + 2000],
        "the end on the text follows"
    );
    assert_eq!(pts(&log, "i4"), user, "the user's arrow is not scout's to change");
    log.undo(&scout(), AT);
    assert_eq!(pts(&log, "i3"), ours, "one undo puts both back");
    // An edit that does not move the item leaves its arrows be.
    let words = Edit { text: Some("hello".into()), ..Edit::default() };
    assert_eq!(log.edit(&scout(), AT, "i2", &words).unwrap().written.seq.len(), 1);
}
