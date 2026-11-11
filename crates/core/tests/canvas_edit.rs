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
    assert_eq!(edited.written.seq.len(), 1);
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
