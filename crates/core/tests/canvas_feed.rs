//! The stored lines after a point (`Log::after` and `lines`): what a change
//! feed serves, as the file holds it.
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::canvas::file::{Log, Span, lines};
use slinger_core::canvas::{Author, Time};

const AT: Time = Time(1_762_790_400_000);

fn bx(x: i64) -> Value {
    json!({ "op": "add", "item": { "page": "p1", "kind": "box", "x": x, "y": 0, "w": 100, "h": 50 } })
}

/// A canvas with three boxes, and its file.
fn three_boxes() -> (Log, String) {
    let (mut log, mut file) = Log::create("main").unwrap();
    for x in [0, 10, 20] {
        file.push_str(&log.apply(&Author::User, AT, &bx(x)).unwrap().bytes);
    }
    (log, file)
}

/// The lines a span covers, read back from `file`.
fn read(file: &str, span: &Span) -> Vec<Value> {
    let at = |n: u64| usize::try_from(n).unwrap();
    let (start, end) = (at(span.bytes.start), at(span.bytes.end));
    lines(&file.as_bytes()[start..end]).unwrap()
}

fn numbers(lines: &[Value]) -> Vec<u64> {
    lines.iter().map(|line| line["n"].as_u64().unwrap()).collect()
}

#[test]
fn the_lines_after_a_point_are_the_files_own() {
    let (log, file) = three_boxes();
    let all = log.after(0, u64::MAX).unwrap();
    assert_eq!(all.next, 4);
    let header = file.find('\n').unwrap() as u64 + 1;
    assert_eq!(all.bytes, header..file.len() as u64);
    let served = read(&file, &all);
    assert_eq!(numbers(&served), [1, 2, 3]);
    assert_eq!(served[2]["item"]["x"], 20);

    let later = log.after(2, u64::MAX).unwrap();
    assert_eq!((numbers(&read(&file, &later)), later.next), (vec![3], 4));

    // A reopened file finds the same lines.
    let reopened = Log::open(file.as_bytes()).unwrap().0;
    assert_eq!(reopened.after(0, u64::MAX).unwrap(), all);
    assert_eq!(reopened.after(2, u64::MAX).unwrap(), later);
}

#[test]
fn after_the_newest_is_empty_and_beyond_it_is_invalid() {
    let (log, file) = three_boxes();
    let none = log.after(3, u64::MAX).unwrap();
    assert_eq!(
        (none.bytes.start, none.bytes.end, none.next),
        (file.len() as u64, file.len() as u64, 4)
    );
    assert_eq!(read(&file, &none), Vec::<Value>::new());
    let err = log.after(4, u64::MAX).unwrap_err();
    assert_eq!(err.code(), "invalid");
    assert!(err.hint().unwrap().contains("below 4"), "{err:?}");

    let (empty, _) = Log::create("blank").unwrap();
    assert_eq!(empty.after(0, 0).unwrap().next, 1);
}

#[test]
fn a_limit_cuts_at_a_whole_line_and_always_gives_one() {
    let (log, file) = three_boxes();
    let one = log.after(0, 1).unwrap();
    assert_eq!((numbers(&read(&file, &one)), one.next), (vec![1], 2));
    let line = file.lines().nth(1).unwrap().len() as u64 + 1;
    let two = log.after(0, 2 * line + 1).unwrap();
    assert_eq!((numbers(&read(&file, &two)), two.next), (vec![1, 2], 3));
    let rest = log.after(two.next - 1, u64::MAX).unwrap();
    assert_eq!(numbers(&read(&file, &rest)), [3]);
}

#[test]
fn a_line_keeps_fields_this_reader_does_not_know() {
    let (_, mut file) = Log::create("main").unwrap();
    file.push_str(
        "{\"n\":1,\"t\":\"2025-11-10T16:00:00.000Z\",\"by\":\"user\",\"op\":\"page.add\",\"page\":\"p2\",\"name\":\"x\",\"tint\":\"blue\"}\n",
    );
    file.push_str(
        "{\"n\":2,\"t\":\"2025-11-10T16:00:00.000Z\",\"by\":\"user\",\"op\":\"page.fold\"}\n",
    );
    let (log, _) = Log::open(file.as_bytes()).unwrap();
    let served = read(&file, &log.after(0, u64::MAX).unwrap());
    assert_eq!(served[0]["tint"], "blue");
    assert_eq!(served[1]["op"], "page.fold");
}

#[test]
fn bytes_that_are_not_lines_are_corrupt() {
    assert_eq!(lines(b"").unwrap(), Vec::<Value>::new());
    let err = lines(b"{\"n\":1}\n[1]\n").unwrap_err();
    assert_eq!(err.code(), "canvas_corrupt");
}
