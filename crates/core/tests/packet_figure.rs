//! Figures in the packet (docs/design/figures.md, "In the packet"): a
//! figure is one line, ink over a part says which, an arrow bound to a
//! figure names its address, and `read --in` takes a figure or a part.
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::canvas::file::Log;
use slinger_core::canvas::{Author, Time};
use slinger_core::packet::{Entry, Packet};

const AT: Time = Time(1_762_790_400_000);

fn claude() -> Author {
    Author::Agent("claude".into())
}

fn add(log: &mut Log, by: &Author, item: Value) -> String {
    let mut item = item;
    item["page"] = "p1".into();
    log.apply(by, AT, &json!({"op": "add", "item": item})).unwrap().made.remove(0)
}

/// A short stroke of the user's from (x, y), in tenths.
fn ink(log: &mut Log, x: i64, y: i64) -> String {
    add(log, &Author::User, json!({"kind": "stroke", "width": 20, "pts": [x, y, 100, 100]}))
}

/// The worksheet by claude, file lines 1 to 5: a title text (i1), a
/// question box (i2, part q1), the answer area of 1a (box i3 and a line
/// i4), all locked, and a box of no part (i5).
fn worksheet(log: &mut Log) {
    let item = |v: Value| {
        let mut v = v;
        v["fig"] = "worksheet".into();
        v
    };
    let c = claude();
    add(
        log,
        &c,
        item(json!({"kind": "text", "x": 0, "y": 0, "text": "Growth\nper year",
        "locked": true, "part": "title"})),
    );
    add(
        log,
        &c,
        item(json!({"kind": "box", "x": 0, "y": 1000, "w": 2000, "h": 500,
        "locked": true, "part": "q1"})),
    );
    add(
        log,
        &c,
        item(json!({"kind": "box", "x": 0, "y": 2000, "w": 2000, "h": 1000,
        "dash": "dashed", "locked": true, "part": "1a-answer"})),
    );
    add(
        log,
        &c,
        item(json!({"kind": "arrow", "pts": [0, 3200, 2000, 0], "heads": "none",
        "locked": true, "part": "1a-answer"})),
    );
    add(log, &c, item(json!({"kind": "box", "x": 3000, "y": 0, "w": 400, "h": 400})));
}

/// The worksheet, then the user's ink in the answer area (i6) and far off
/// (i7), and a text of the user's (i8) with an arrow (i9) to the question.
fn written() -> Log {
    let (mut log, _) = Log::create("sheet").unwrap();
    worksheet(&mut log);
    ink(&mut log, 500, 2500);
    ink(&mut log, 9000, 9000);
    add(&mut log, &Author::User, json!({"kind": "text", "x": 6000, "y": 1000, "text": "hm"}));
    let arrow = json!({"kind": "arrow", "pts": [6000, 1000, -4000, 200], "from": "i8", "to": "i2"});
    add(&mut log, &Author::User, arrow);
    log
}

fn packet(log: &Log, inside: Option<&str>, after: u64) -> Packet {
    log.packet("sheet", None, inside, after, &[], &[]).unwrap()
}

fn ids(entries: &[Entry]) -> Vec<&str> {
    entries.iter().map(|e| e.id.as_str()).collect()
}

fn line<'a>(packet: &'a Packet, id: &str) -> &'a Entry {
    let all = packet.new.iter().chain(&packet.made).chain(&packet.page);
    all.clone().find(|e| e.id == id).unwrap_or_else(|| panic!("no {id} in {}", packet.text()))
}

#[test]
fn a_figure_is_one_line_named_by_the_figure_and_its_items_make_none() {
    let log = written();
    let p = packet(&log, None, 9);
    assert_eq!(ids(&p.page), ["worksheet", "c6", "a9", "t8", "c7"], "{}", p.text());
    let text = p.text();
    let want = "worksheet figure 0,0 340x320 by=claude parts=title,q1,1a-answer locked=yes \
                \"Growth\"\n";
    assert!(text.contains(want), "{text}");
    // The arrow's end bound to an item of the figure names its address.
    assert_eq!(line(&p, "a9").field("to"), Some("worksheet/q1"));
    assert_eq!(Packet::read(&text).unwrap().page, p.page, "a reader keeps the new kind");
}

#[test]
fn a_figure_an_agent_drew_since_the_last_sling_is_in_made() {
    let (mut log, _) = Log::create("sheet").unwrap();
    worksheet(&mut log);
    assert_eq!(ids(&packet(&log, None, 0).made), ["worksheet"]);
    assert_eq!(ids(&packet(&log, None, 5).page), ["worksheet"]);
    // A redraw of one item puts the whole figure in made again.
    let set = json!({"op": "set", "id": "i5", "fields": {"w": 500}});
    log.apply(&claude(), AT, &set).unwrap();
    assert_eq!(ids(&packet(&log, None, 5).made), ["worksheet"]);
}

#[test]
fn the_quote_is_the_title_or_else_the_first_text() {
    let (mut log, _) = Log::create("plot").unwrap();
    let c = claude();
    add(&mut log, &c, json!({"kind": "text", "x": 0, "y": 500, "text": "low", "fig": "plot"}));
    add(&mut log, &c, json!({"kind": "text", "x": 0, "y": 0, "text": "high", "fig": "plot"}));
    let p = log.packet("plot", None, None, 0, &[], &[]).unwrap();
    assert_eq!(line(&p, "plot").text.as_deref(), Some("high"), "the first in reading order");
    assert_eq!(line(&p, "plot").field("parts"), None);
    assert_eq!(line(&p, "plot").field("locked"), None);
    let title = json!({"kind": "text", "x": 0, "y": 900, "text": "Title", "fig": "plot",
        "part": "title"});
    add(&mut log, &c, title);
    let p = log.packet("plot", None, None, 0, &[], &[]).unwrap();
    assert_eq!(line(&p, "plot").text.as_deref(), Some("Title"));
}

#[test]
fn ink_over_a_part_names_it_and_ink_elsewhere_names_none() {
    let log = written();
    let p = packet(&log, None, 5);
    assert_eq!(ids(&p.new), ["c6", "c7"]);
    assert_eq!(line(&p, "c6").field("over"), Some("worksheet/1a-answer"));
    // A box of a figure makes no line, so ink never lies `in` it.
    assert_eq!(line(&p, "c6").field("in"), None);
    assert_eq!(line(&p, "c7").field("over"), None);
}

#[test]
fn ink_over_nested_parts_names_the_smallest_first() {
    let (mut log, _) = Log::create("nest").unwrap();
    let c = claude();
    let part = |part: &str, x: i64, w: i64| json!({"kind": "box", "x": x, "y": x, "w": w, "h": w, "fig": "board", "part": part});
    add(&mut log, &c, part("outer", 0, 4000));
    add(&mut log, &c, part("inner", 1000, 2000));
    ink(&mut log, 1900, 1900);
    let p = log.packet("nest", None, None, 2, &[], &[]).unwrap();
    assert_eq!(line(&p, "c3").field("over"), Some("board/inner,board/outer"));
}

#[test]
fn a_read_in_a_part_gives_its_items_and_the_ink_over_it() {
    let log = written();
    let p = packet(&log, Some("worksheet/1a-answer"), 5);
    assert_eq!(p.header.inside.as_deref(), Some("worksheet/1a-answer"));
    assert!(p.text().starts_with("slinger-packet 1 canvas=sheet page="), "{}", p.text());
    assert!(p.text().lines().next().unwrap().contains(" in=worksheet/1a-answer "));
    assert_eq!(ids(&p.new), ["c6"]);
    assert_eq!(ids(&p.page), ["b3", "a4"], "{}", p.text());
    assert_eq!(line(&p, "b3").field("part"), Some("1a-answer"));
    assert_eq!(line(&p, "b3").field("by"), Some("claude"));
    assert_eq!(line(&p, "c6").field("over"), Some("worksheet/1a-answer"));
    // Inside the read, the open figure's box holds the ink.
    assert_eq!(line(&p, "c6").field("in"), Some("b3"));
    assert_eq!(Packet::read(&p.text()).unwrap(), Packet { images: vec![], ..p.clone() });
}

#[test]
fn a_read_in_a_figure_gives_each_item_with_its_part() {
    let log = written();
    let p = packet(&log, Some("worksheet"), 9);
    assert_eq!(ids(&p.page), ["t1", "b5", "b2", "b3", "c6", "a4"], "{}", p.text());
    assert_eq!(line(&p, "t1").field("part"), Some("title"));
    assert_eq!(line(&p, "b5").field("part"), None, "an item of no part has none");
    // The user's text and arrow lie outside the figure's frame.
    assert!(p.page.iter().all(|e| e.id != "t8" && e.id != "a9"));
}

#[test]
fn a_read_in_a_figure_that_is_not_there_fails_with_a_code() {
    let log = written();
    let read = |inside: &str, page: Option<&str>| {
        log.packet("sheet", page, Some(inside), 0, &[], &[]).unwrap_err().code()
    };
    assert_eq!(read("worksheet/2b-answer", None), "not_found");
    assert_eq!(read("sheet", None), "not_found");
    assert_eq!(read("worksheet/Bad Part", None), "invalid");
    let (mut two, _) = Log::create("two").unwrap();
    worksheet(&mut two);
    two.apply(&Author::User, AT, &json!({"op": "page.add", "name": "two"})).unwrap();
    let err = two.packet("two", Some("two"), Some("worksheet"), 0, &[], &[]).unwrap_err();
    assert_eq!(err.code(), "not_found");
    assert!(err.hint().is_some());
    assert_eq!(two.backdrops(None, Some("worksheet/q1"), 0).unwrap(), []);
}

#[test]
fn a_selected_item_of_a_figure_selects_the_figure() {
    let log = written();
    let select = ["i3".to_owned(), "t8".to_owned()];
    let p = log.packet("sheet", None, None, 9, &select, &[]).unwrap();
    assert_eq!(p.selected, ["worksheet", "t8"]);
    assert_eq!(ids(&p.page), ["worksheet", "t8"]);
}

#[test]
fn a_folded_box_holding_a_figure_counts_it_as_one_line() {
    let (mut log, _) = Log::create("held").unwrap();
    add(
        &mut log,
        &Author::User,
        json!({"kind": "box", "x": -100, "y": -100, "w": 4000, "h": 4000}),
    );
    worksheet(&mut log);
    ink(&mut log, 500, 2500);
    log.fold(&Author::User, AT, "i1", None).unwrap();
    let p = log.packet("held", None, None, 99, &[], &[]).unwrap();
    assert_eq!(ids(&p.page), ["b1"], "{}", p.text());
    // The figure and the ink over it: two lines; the quote is the title's.
    assert_eq!(line(&p, "b1").field("folded"), Some("2"));
    assert_eq!(line(&p, "b1").text.as_deref(), Some("Growth"));
    // Read in the box, the figure is one line again, and the ink is over its part.
    let inside = log.packet("held", None, Some("b1"), 99, &[], &[]).unwrap();
    assert_eq!(ids(&inside.page), ["worksheet", "c7"]);
    assert_eq!(
        line(&inside, "c7").field("over"),
        None,
        "a hidden part is under no ink on the page"
    );
}
