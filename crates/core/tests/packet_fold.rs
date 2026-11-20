//! Folded boxes in the packet and in crops (docs/design/fold.md, "The packet
//! line", "Read", "Crop"): one line for a folded box, nothing for what it
//! hides, `read --in` of a box, and a crop that draws a folded box open.
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::canvas::file::Log;
use slinger_core::canvas::{Author, Time};
use slinger_core::packet::{Entry, Packet};

const AT: Time = Time(1_762_790_400_000);

fn add(log: &mut Log, item: Value) -> String {
    let mut item = item;
    item["page"] = "p1".into();
    log.apply(&Author::User, AT, &json!({"op": "add", "item": item})).unwrap().made.remove(0)
}

/// A short stroke from (x, y), in tenths.
fn ink(log: &mut Log, x: i64, y: i64) -> String {
    add(log, json!({"kind": "stroke", "width": 20, "pts": [x, y, 100, 100]}))
}

/// The board, by file line:
/// 1. box i1, 400 by 400 units at the origin;
/// 2. text i2 in it, two lines;
/// 3. stroke i3 in it;
/// 4. box i4 in it, 100 by 100;
/// 5. stroke i5 in i4;
/// 6. text i6 far right, below;
/// 7. arrow i7 from i5 to i6;
/// 8. i4 folded; 9. i1 folded.
fn board() -> Log {
    let (mut log, _) = Log::create("folds").unwrap();
    add(&mut log, json!({"kind": "box", "x": 0, "y": 0, "w": 4000, "h": 4000}));
    add(&mut log, json!({"kind": "text", "x": 500, "y": 500, "text": "Monday: what ships\nmore"}));
    ink(&mut log, 2000, 2000);
    add(&mut log, json!({"kind": "box", "x": 500, "y": 2500, "w": 1000, "h": 1000}));
    ink(&mut log, 800, 2800);
    add(&mut log, json!({"kind": "text", "x": 9000, "y": 3000, "text": "far"}));
    let arrow = json!({"kind": "arrow", "pts": [810, 2810, 8190, 190], "from": "i5", "to": "i6"});
    add(&mut log, arrow);
    log.fold(&Author::User, AT, "i4", None).unwrap();
    log.fold(&Author::User, AT, "i1", None).unwrap();
    log
}

fn packet(log: &Log, inside: Option<&str>, after: u64) -> Packet {
    log.packet("folds", None, inside, after, &[], &[]).unwrap()
}

fn ids(entries: &[Entry]) -> Vec<&str> {
    entries.iter().map(|e| e.id.as_str()).collect()
}

fn line<'a>(packet: &'a Packet, id: &str) -> &'a Entry {
    let all = packet.new.iter().chain(&packet.made).chain(&packet.page);
    all.clone().find(|e| e.id == id).unwrap_or_else(|| panic!("no {id} in {}", packet.text()))
}

#[test]
fn a_folded_box_is_one_line_and_what_it_hides_is_none() {
    let log = board();
    let p = packet(&log, None, 9);
    assert_eq!([ids(&p.new), ids(&p.made)], [Vec::<&str>::new(), vec![]]);
    assert_eq!(ids(&p.page), ["b1", "a7", "t6"]);
    let text = p.text();
    assert!(text.contains("b1 box 0,0 100x100 folded=3 \"Monday: what ships\"\n"), "{text}");
    // The arrow's end bound to the hidden stroke names the outermost folded box.
    assert_eq!(line(&p, "a7").field("from"), Some("b1"));
    assert_eq!(line(&p, "a7").field("to"), Some("t6"));
    assert_eq!(Packet::read(&text).unwrap().page, p.page, "a reader keeps the new field");
}

#[test]
fn new_ink_in_a_folded_box_puts_its_line_in_new_with_no_image() {
    let log = board();
    // Line 5, the stroke in the inner box, is new.
    let p = packet(&log, None, 4);
    assert_eq!(ids(&p.new), ["b1"]);
    assert_eq!(line(&p, "b1").field("new"), Some("1"));
    assert_eq!(p.images.len(), 0);
    assert_eq!(p.new_strokes(), 1);
    // Everything new: both strokes it holds, at any depth.
    assert_eq!(line(&packet(&log, None, 0), "b1").field("new"), Some("2"));
}

#[test]
fn a_read_in_a_box_is_its_direct_contents_with_a_folded_box_as_one_line() {
    let log = board();
    let p = packet(&log, Some("b1"), 9);
    assert_eq!(ids(&p.page), ["t2", "c3", "b4"]);
    assert_eq!(line(&p, "b4").field("folded"), Some("1"));
    assert_eq!(line(&p, "t2").text.as_deref(), Some("Monday: what ships\nmore"));
    let inner = packet(&log, Some("i4"), 9);
    assert_eq!(ids(&inner.page), ["c5"]);
    // The header names the box, as the packet does, however the read named it.
    assert_eq!(
        (p.header.inside.as_deref(), inner.header.inside.as_deref()),
        (Some("b1"), Some("b4"))
    );
    assert!(inner.text().starts_with("slinger-packet 1 canvas=folds page="), "{}", inner.text());
    assert!(inner.text().lines().next().unwrap().contains(" in=b4 from="), "{}", inner.text());
    assert_eq!(Packet::read(&inner.text()).unwrap().header, inner.header);
    assert_eq!(packet(&log, None, 9).header.inside, None);
    // An open box reads too: what lies inside it.
    let (mut open, _) = Log::create("open").unwrap();
    add(&mut open, json!({"kind": "box", "x": 0, "y": 0, "w": 1000, "h": 1000}));
    ink(&mut open, 100, 100);
    ink(&mut open, 5000, 5000);
    assert_eq!(ids(&open.packet("open", None, Some("b1"), 0, &[], &[]).unwrap().new), ["c2"]);
}

#[test]
fn a_read_in_something_else_fails_with_a_code() {
    let log = board();
    let read = |inside: &str, page: Option<&str>| {
        log.packet("folds", page, Some(inside), 0, &[], &[]).unwrap_err().code()
    };
    assert_eq!(read("t2", None), "invalid");
    assert_eq!(read("b99", None), "not_found");
    assert_eq!(read("zz", None), "invalid");
    assert!(log.packet("folds", Some("p1"), Some("b1"), 0, &[], &[]).is_ok());
    let page = json!({"op": "page.add", "name": "two"});
    let (mut two, _) = Log::create("two").unwrap();
    add(&mut two, json!({"kind": "box", "x": 0, "y": 0, "w": 1000, "h": 1000}));
    two.apply(&Author::User, AT, &page).unwrap();
    let err = two.packet("two", Some("two"), Some("b1"), 0, &[], &[]).unwrap_err();
    assert_eq!(err.code(), "not_found");
    assert!(err.hint().is_some());
}

#[test]
fn a_crop_of_a_folded_box_draws_it_open_and_a_crop_of_the_page_draws_what_it_shows() {
    let log = board();
    let open = log.crop("b1", "open").unwrap();
    assert_eq!(open.len(), 1);
    // The open frame, 400 units plus the margins, at the packet's scale.
    let side = |units: f64| (units * slinger_core::render::SCALE).round();
    assert!(f64::from(open[0].width) >= side(400.0), "{} wide", open[0].width);
    // A crop of the page's area under the open room draws only the miniature.
    let area = log.crop("0,0,4000,2500", "area").unwrap();
    let (mut bare, _) = Log::create("bare").unwrap();
    add(&mut bare, json!({"kind": "box", "x": 0, "y": 0, "w": 1000, "h": 1000}));
    let same = bare.crop("0,0,4000,2500", "area").unwrap();
    assert_eq!(area[0].png, same[0].png, "the hidden items are not drawn");
}

#[test]
fn a_cluster_id_on_a_hidden_stroke_names_its_cluster_in_its_box() {
    let log = board();
    let strokes = log.strokes("c5").unwrap();
    assert_eq!(strokes.len(), 1);
    assert_eq!(strokes[0]["id"], "i5");
}

#[test]
fn find_sees_into_folded_boxes_and_names_the_outermost_one() {
    let (mut log, _) = Log::create("find").unwrap();
    // Far down the page, a box holding a box holding the text; above, a plain text.
    add(&mut log, json!({"kind": "box", "x": 0, "y": 5000, "w": 4000, "h": 4000}));
    add(&mut log, json!({"kind": "box", "x": 500, "y": 5500, "w": 2000, "h": 2000}));
    add(&mut log, json!({"kind": "text", "x": 600, "y": 7000, "text": "the plan, deep"}));
    add(&mut log, json!({"kind": "text", "x": 600, "y": 6000, "text": "the plan, near"}));
    add(&mut log, json!({"kind": "text", "x": 600, "y": 0, "text": "the plan, open"}));
    log.fold(&Author::User, AT, "i2", None).unwrap();
    log.fold(&Author::User, AT, "i1", None).unwrap();
    let canvas = log.canvas();
    let found: Vec<String> =
        canvas.find("THE plan").into_iter().map(|i| format!("i{}", i.get())).collect();
    assert_eq!(found, ["i5", "i4", "i3"], "the open one first, then by their own place");
    assert_eq!(
        canvas
            .outermost(slinger_core::canvas::ItemId::new(3))
            .map(slinger_core::canvas::ItemId::get),
        Some(1)
    );
    let hits = log.find("the plan");
    let folded: Vec<&Value> = hits.iter().map(|h| &h["folded"]).collect();
    assert_eq!(folded, [&Value::Null, &json!("i1"), &json!("i1")]);
}
