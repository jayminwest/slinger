//! A stored canvas's packet (`Log::packet`), through the public interface: a
//! canvas file held open, a line number and a selection in; the packet out,
//! its op range in the file's line numbers.
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::canvas::Time;
use slinger_core::canvas::file::{Log, parse_author};
use slinger_core::packet::Packet;

const AT: Time = Time(1_762_790_400_000);

/// A stroke at `x`, as a client sends it (tenths of a unit).
fn stroke(x: i64, color: &str) -> Value {
    let item = json!({ "page": "p1", "kind": "stroke", "color": color, "width": 20,
        "pts": [x, 0, 100, 50, 100, -50] });
    json!({ "op": "add", "item": item })
}

fn text(x: i64) -> Value {
    json!({ "op": "add", "item": { "page": "p1", "kind": "text", "x": x, "y": 0, "text": "hi" } })
}

/// A canvas with a black stroke and an agent's text (lines 1 and 2), and the file.
fn canvas() -> (Log, String) {
    let (mut log, mut file) = Log::create("idea").unwrap();
    for (by, op) in [("user", stroke(0, "black")), ("agent:ada", text(5000))] {
        file.push_str(&log.apply(&parse_author(by).unwrap(), AT, &op).unwrap().bytes);
    }
    (log, file)
}

fn ids(packet: &Packet) -> [Vec<String>; 3] {
    let ids = |e: &[slinger_core::packet::Entry]| e.iter().map(|e| e.id.clone()).collect();
    [ids(&packet.new), ids(&packet.made), ids(&packet.page)]
}

#[test]
fn everything_after_line_zero_is_new() {
    let (log, _) = canvas();
    let p = log.packet("idea", None, 0, &[], &[]).unwrap();
    assert_eq!((p.header.from, p.header.to), (1, 2));
    assert_eq!(ids(&p), [vec!["c1".to_string()], vec!["t2".into()], vec![]]);
    assert_eq!(p.images.len(), 1);
    assert_eq!(p.header.at, None);
}

#[test]
fn only_the_lines_after_the_marker_are_new() {
    let (mut log, _) = canvas();
    let user = parse_author("user").unwrap();
    let written = log.apply(&user, AT, &stroke(20_000, "red")).unwrap();
    assert_eq!(written.seq, [3]);
    let p = log.packet("idea", Some("Page 1"), 2, &[], &[]).unwrap();
    assert_eq!((p.header.from, p.header.to), (3, 3));
    assert_eq!(ids(&p), [vec!["c3".to_string()], vec![], vec!["c1".into(), "t2".into()]]);
}

#[test]
fn a_marker_at_or_past_the_end_is_nothing_new() {
    let (log, _) = canvas();
    for after in [2, 99] {
        let p = log.packet("idea", Some("p1"), after, &[], &[]).unwrap();
        assert!(p.header.from > p.header.to, "after {after}");
        assert_eq!((p.header.from, p.header.to), (3, 2));
        assert!(p.new.is_empty() && p.made.is_empty() && p.images.is_empty());
        assert_eq!(p.page.len(), 2);
    }
}

#[test]
fn a_reopened_file_gives_the_same_packet() {
    let (log, file) = canvas();
    let (reopened, _) = Log::open(file.as_bytes()).unwrap();
    for after in [0, 1, 2] {
        let (a, b) = (
            log.packet("idea", None, after, &[], &[]),
            reopened.packet("idea", None, after, &[], &[]),
        );
        assert_eq!(a.unwrap(), b.unwrap(), "after {after}");
    }
}

#[test]
fn an_undo_and_a_redo_are_lines_too() {
    let (mut log, file) = canvas();
    let user = parse_author("user").unwrap();
    let mut file = file;
    for step in [log.undo(&user, AT), log.redo(&user, AT)] {
        file.push_str(&step.bytes);
    }
    assert_eq!(log.next(), 5);
    let p = log.packet("idea", None, 3, &[], &[]).unwrap();
    assert_eq!(ids(&p)[0], ["c1"]);
    let (reopened, _) = Log::open(file.as_bytes()).unwrap();
    assert_eq!(reopened.packet("idea", None, 3, &[], &[]).unwrap(), p);
}

#[test]
fn a_selection_takes_file_or_packet_ids() {
    let (log, _) = canvas();
    for id in ["i1", "c1"] {
        let p = log.packet("idea", None, 2, &[id.to_string()], &[]).unwrap();
        assert_eq!(ids(&p)[2], ["c1"], "{id}");
    }
    let p = log.packet("idea", None, 2, &["t2".to_string()], &[]).unwrap();
    assert_eq!(ids(&p)[2], ["t2"]);
}

#[test]
fn a_bad_page_or_id_is_refused() {
    let (log, _) = canvas();
    assert_eq!(log.packet("idea", Some("p9"), 0, &[], &[]).unwrap_err().code(), "not_found");
    for bad in ["", "c", "x1", "c1x", "i-1"] {
        let err = log.packet("idea", None, 0, &[bad.to_string()], &[]).unwrap_err();
        assert_eq!(err.code(), "invalid", "{bad:?}");
        assert!(err.hint().is_some());
    }
}
