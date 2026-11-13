//! How an agent answers on a canvas: free space beside an item
//! (`Canvas::free_beside`) and the put that uses it (`Log::put`).
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::canvas::file::{How, Log, Put, TEXT_COLUMNS};
use slinger_core::canvas::{
    Action, Author, Canvas, Colour, GAP, ItemId, Op, PageId, Rect, Shape, Time,
};
use slinger_core::render;

const AT: Time = Time(1_762_790_400_000);

fn scout() -> Author {
    Author::Agent("scout".into())
}

/// A canvas whose first page holds boxes at these frames, in order.
fn boxes(frames: &[Rect]) -> (Canvas, PageId) {
    let mut c = Canvas::new();
    let page = c.pages().next().unwrap().id();
    for &frame in frames {
        let shape = Shape::Box { frame, colour: Colour::BLACK };
        c.apply(Op { by: Author::User, at: AT, action: Action::Add { page, shape } }).unwrap();
    }
    (c, page)
}

/// Whether the canvas has an item that comes within half a gap of `r`.
fn covers_something(c: &Canvas, page: PageId, r: Rect) -> bool {
    let g = GAP / 2.0;
    !c.items_in_box(page, Rect::new(r.x - g, r.y - g, r.w + 2.0 * g, r.h + 2.0 * g)).is_empty()
}

#[test]
fn with_room_on_the_right_the_new_item_goes_there_top_aligned() {
    let near = Rect::new(0.0, 0.0, 100.0, 50.0);
    let (c, page) = boxes(&[near]);
    let put = c.free_beside(page, near, 80.0, 30.0);
    assert_eq!(put, Rect::new(100.0 + GAP, 0.0, 80.0, 30.0));
    assert!(!covers_something(&c, page, put));
}

#[test]
fn a_crowded_right_side_sends_it_below_then_left_then_above() {
    let near = Rect::new(0.0, 0.0, 100.0, 50.0);
    let right = Rect::new(130.0, 0.0, 50.0, 50.0);
    let below = Rect::new(0.0, 80.0, 100.0, 50.0);
    let left = Rect::new(-150.0, 0.0, 100.0, 50.0);
    let (c, page) = boxes(&[near, right]);
    assert_eq!(c.free_beside(page, near, 80.0, 30.0), Rect::new(0.0, 50.0 + GAP, 80.0, 30.0));
    let (c, page) = boxes(&[near, right, below]);
    assert_eq!(c.free_beside(page, near, 80.0, 30.0), Rect::new(-GAP - 80.0, 0.0, 80.0, 30.0));
    let (c, page) = boxes(&[near, right, below, left]);
    assert_eq!(c.free_beside(page, near, 80.0, 30.0), Rect::new(0.0, -GAP - 30.0, 80.0, 30.0));
}

#[test]
fn boxed_in_on_every_side_it_goes_right_past_whatever_is_in_the_way() {
    let near = Rect::new(0.0, 0.0, 100.0, 50.0);
    let around = [
        near,
        Rect::new(130.0, 0.0, 50.0, 50.0),
        Rect::new(200.0, 10.0, 300.0, 20.0),
        Rect::new(0.0, 80.0, 100.0, 50.0),
        Rect::new(-150.0, 0.0, 100.0, 50.0),
        Rect::new(0.0, -80.0, 100.0, 50.0),
    ];
    let (c, page) = boxes(&around);
    let put = c.free_beside(page, near, 80.0, 30.0);
    assert_eq!(put, Rect::new(500.0 + GAP, 0.0, 80.0, 30.0));
    assert!(!covers_something(&c, page, put));
}

#[test]
fn an_item_at_the_edge_of_the_page_gets_the_open_side_and_items_elsewhere_do_not_matter() {
    // The rightmost item of a full row, far out on the endless canvas.
    let row: Vec<Rect> =
        (0..20).map(|i| Rect::new(f64::from(i) * 150.0, 1e6, 100.0, 50.0)).collect();
    let mut frames = row.clone();
    frames.push(Rect::new(1e6, 0.0, 10.0, 10.0));
    let (c, page) = boxes(&frames);
    let edge = row[19];
    let put = c.free_beside(page, edge, 200.0, 200.0);
    assert_eq!(put, Rect::new(edge.x + edge.w + GAP, 1e6, 200.0, 200.0));
    // The leftmost of the row: its right is taken, so it goes below.
    let put = c.free_beside(page, row[0], 200.0, 30.0);
    assert_eq!(put, Rect::new(0.0, 1e6 + 50.0 + GAP, 200.0, 30.0));
    // Another page is empty, so it goes right.
    assert_eq!(c.free_beside(PageId::new(9), row[0], 1.0, 1.0).x, 100.0 + GAP);
}

#[test]
fn a_size_that_is_not_one_counts_as_zero() {
    let near = Rect::new(0.0, 0.0, 10.0, 10.0);
    let (c, page) = boxes(&[near]);
    let size = |w, h| {
        let put = c.free_beside(page, near, w, h);
        (put.w, put.h)
    };
    assert_eq!(size(-5.0, 3.0), (0.0, 3.0));
    assert_eq!(size(f64::NAN, f64::INFINITY), (0.0, 0.0));
}

/// A log whose first page holds `ops`, applied by the user.
fn log_of(ops: &Value) -> Log {
    let (mut log, _) = Log::create("main").unwrap();
    log.apply(&Author::User, AT, ops).unwrap();
    log
}

fn bx(x: i64, y: i64, w: i64, h: i64) -> Value {
    json!({ "op": "add", "item": { "page": "p1", "kind": "box", "x": x, "y": y, "w": w, "h": h } })
}

#[test]
fn put_text_lands_beside_the_item_as_the_agents_at_its_time() {
    let mut log = log_of(&bx(0, 0, 1000, 500));
    let placed = log.put(&scout(), AT, "b1", &Put::Text("looks right".into())).unwrap();
    assert_eq!(render::text_size("looks right", TEXT_COLUMNS, 24.0), (264.0, 24.0));
    assert_eq!(placed.item, "i2");
    assert_eq!(placed.page, "p1");
    // Lengths in tenths: right of the box by a gap, top-aligned.
    assert_eq!(placed.frame, [1000 + 240, 0, 2640, 240]);
    assert_eq!(placed.written.seq, [2]);
    let item = log.canvas().item(ItemId::new(2)).unwrap();
    assert_eq!((item.author(), item.time()), (&scout(), AT));
    assert!(placed.written.bytes.contains(r#""by":"agent:scout""#), "{}", placed.written.bytes);
    // The agent may take it back: the put is its own undo step.
    log.undo(&scout(), AT);
    assert!(log.canvas().item(ItemId::new(2)).is_none());
}

#[test]
fn long_text_wraps_and_its_frame_holds_every_line() {
    // Eight words fit a line of 40; thirty make four lines.
    let words = "word ".repeat(30);
    assert_eq!(render::text_size(&words, TEXT_COLUMNS, 24.0), (39.0 * 24.0, 4.0 * 24.0));
    let mut log = log_of(&bx(0, 0, 10, 10));
    let placed = log.put(&scout(), AT, "i1", &Put::Text(words)).unwrap();
    assert_eq!(placed.frame[2..], [9360, 960]);
}

#[test]
fn put_box_and_arrow_beside_a_cluster() {
    let stroke = |x: i64| json!({ "op": "add", "item": { "page": "p1", "kind": "stroke", "width": 20, "pts": [x, 0, 100, 0] } });
    // Two strokes make one note (c1); the box far below is its own item.
    let mut log = log_of(&json!([stroke(0), stroke(120), bx(0, 5000, 100, 100)]));
    let placed = log.put(&scout(), AT, "c1", &Put::Box { w: 300, h: 200 }).unwrap();
    // Right of the whole cluster, which ends at x = 22 + 1 (half the width).
    assert_eq!(placed.frame, [230 + 240, -10, 300, 200]);
    let arrow = Put::Arrow { from: "i1".into(), to: "b3".into() };
    let placed = log.put(&scout(), AT, "c1", &arrow).unwrap();
    let shown = log.show(None).unwrap();
    let item = shown.items.iter().find(|i| i["id"] == placed.item.as_str()).unwrap();
    assert_eq!(
        (&item["from"], &item["to"], &item["by"]),
        (&json!("i1"), &json!("i3"), &json!("agent:scout"))
    );
    // From the bottom edge of the stroke's box to the top edge of the box.
    let pts = item["pts"].as_array().unwrap();
    assert_eq!(pts[1], json!(10));
    assert_eq!(pts[3].as_i64().unwrap() + pts[1].as_i64().unwrap(), 5000);
}

#[test]
fn a_put_beside_nothing_or_of_nothing_fails_and_changes_nothing() {
    let mut log = log_of(&json!([bx(0, 0, 10, 10), { "op": "page.add", "name": "two" }]));
    let other = json!({ "op": "add", "item": { "page": "p2", "kind": "box", "x": 0, "y": 0, "w": 1, "h": 1 } });
    log.apply(&Author::User, AT, &other).unwrap();
    let next = log.next();
    let text = Put::Text("hi".into());
    let cases: [(&str, Put, &str); 6] = [
        ("i9", text.clone(), "not_found"),
        ("x1", text.clone(), "invalid"),
        ("i1", Put::Text(" \n".into()), "invalid"),
        ("i1", Put::Box { w: -1, h: 5 }, "invalid"),
        ("i1", Put::Arrow { from: "i1".into(), to: "i2".into() }, "invalid"),
        ("i1", Put::Arrow { from: "i1".into(), to: "i7".into() }, "not_found"),
    ];
    for (beside, what, code) in cases {
        let err = log.put(&scout(), AT, beside, &what).unwrap_err();
        assert_eq!(err.code(), code, "{beside} {what:?}");
        assert!(err.hint().is_some() || code == "not_found", "{beside} {what:?}");
    }
    let blank = log.put(&Author::Agent(" ".into()), AT, "i1", &text).unwrap_err();
    assert_eq!(blank.code(), "invalid");
    assert_eq!(log.next(), next);
}

#[test]
fn a_put_takes_a_colour_and_a_place_and_ink_goes_where_any_put_goes() {
    let mut log = log_of(&bx(0, 0, 1000, 500));
    let yellow = How { colour: Some("#ffd400".into()), at: Some([-300, 900]), text_size: None };
    let placed = log.put_how(&scout(), AT, "i1", &Put::Text("yes".into()), &yellow).unwrap();
    assert_eq!(placed.frame[..2], [-300, 900], "the corner where the agent said");
    let item = log.canvas().item(ItemId::new(2)).unwrap();
    assert!(
        matches!(item.shape(), Shape::Text { colour, .. } if *colour == Colour::rgb(255, 212, 0))
    );
    // A bracket: down, right, up. Its start does not matter, only its shape.
    let ink = Put::Ink(vec![9000, 9000, 0, 300, 200, 0, 0, -300]);
    let placed = log.put(&scout(), AT, "i1", &ink).unwrap();
    assert_eq!(placed.frame, [1000 + 240, 0, 220, 320], "beside, its box half a width out");
    let at = How { at: Some([0, 0]), ..How::default() };
    let arrow = Put::Arrow { from: "i1".into(), to: "i2".into() };
    let next = log.next();
    for (what, how) in [
        (&arrow, &at),
        (&ink, &How { colour: Some("yellow".into()), ..How::default() }),
        (&Put::Ink(vec![1, 2, 3]), &How::default()),
    ] {
        let err = log.put_how(&scout(), AT, "i1", what, how).unwrap_err();
        assert_eq!(err.code(), "invalid", "{what:?} {how:?}");
        assert!(err.hint().is_some(), "{what:?}");
    }
    assert_eq!(log.next(), next, "a refused put writes nothing");
}

fn text(x: i64, y: i64, words: &str) -> Value {
    json!({ "op": "add", "item": { "page": "p1", "kind": "text", "x": x, "y": y, "text": words } })
}

/// An item's frame as `canvas show` gives it, in tenths.
fn frame_of(log: &Log, id: &str) -> [i64; 4] {
    let shown = log.show(None).unwrap();
    let item = shown.items.iter().find(|i| i["id"] == id).unwrap();
    ["x", "y", "w", "h"].map(|k| item[k].as_i64().unwrap())
}

#[test]
fn text_applied_without_a_size_gets_one_so_a_put_beside_it_does_not_cover_it() {
    // slinger-981b5c: the user's text had no box, and the put landed on it.
    let (mut log, mut file) = Log::create("main").unwrap();
    file += &log.apply(&Author::User, AT, &text(0, 0, "plan")).unwrap().bytes;
    assert_eq!(frame_of(&log, "i1"), [0, 0, 960, 240]);
    let placed = log.put(&scout(), AT, "i1", &Put::Text("agent reply".into())).unwrap();
    assert_eq!(placed.frame, [960 + 240, 0, 2640, 240]);
    let page = log.canvas().pages().next().unwrap().id();
    let plan = log.canvas().item(ItemId::new(1)).unwrap().bounds();
    let reply = log.canvas().item(ItemId::new(2)).unwrap().bounds();
    assert!(plan.x + plan.w < reply.x, "{plan:?} {reply:?}");
    assert_eq!(log.canvas().items_in_box(page, reply), [ItemId::new(2)]);
    // The line holds the box, so the file reads back to the same canvas.
    assert!(file.contains(r#""w":960,"h":240"#), "{file}");
    let (reopened, _) = Log::open(file.as_bytes()).unwrap();
    assert_eq!(frame_of(&reopened, "i1"), [0, 0, 960, 240]);
}

#[test]
fn a_text_with_a_size_keeps_it_and_one_with_no_height_gets_its_lines() {
    let sized = json!({ "op": "add", "item": { "page": "p1", "kind": "text", "x": 0, "y": 0, "w": 480, "h": 312, "text": "hi" } });
    let no_height = json!({ "op": "add", "item": { "page": "p1", "kind": "text", "x": 0, "y": 0, "w": 1200, "text": "one two three" } });
    let log = log_of(&json!([sized, no_height, text(0, 0, "a\nlonger line")]));
    assert_eq!(frame_of(&log, "i1"), [0, 0, 480, 312]);
    // Five characters a line: "one", "two" and "three", a line each.
    assert_eq!(frame_of(&log, "i2"), [0, 0, 1200, 3 * 240]);
    assert_eq!(frame_of(&log, "i3"), [0, 0, 11 * 240, 2 * 240]);
}

#[test]
fn old_text_with_no_box_reads_back_with_one() {
    let file = concat!(
        r#"{"slinger":"canvas","v":1,"id":"main","first":1}"#,
        "\n",
        r##"{"n":1,"t":"2026-11-10T16:42:10.400Z","by":"user","op":"add","item":{"id":"i1","page":"p1","kind":"text","color":"#000000","x":0,"y":0,"w":0,"h":0,"text":"plan"}}"##,
        "\n",
    );
    let (log, _) = Log::open(file.as_bytes()).unwrap();
    assert_eq!(frame_of(&log, "i1"), [0, 0, 960, 240]);
}

#[test]
fn a_set_of_new_text_without_a_size_refits_the_box() {
    let mut log = log_of(&text(0, 0, "plan"));
    let set = |fields: Value| json!({ "op": "set", "id": "i1", "fields": fields });
    log.apply(&Author::User, AT, &set(json!({ "text": "a longer plan" }))).unwrap();
    assert_eq!(frame_of(&log, "i1"), [0, 0, 13 * 240, 240]);
    log.apply(&Author::User, AT, &set(json!({ "text": "ab", "w": 5000, "h": 900 }))).unwrap();
    assert_eq!(frame_of(&log, "i1"), [0, 0, 5000, 900]);
    log.apply(&Author::User, AT, &set(json!({ "x": 100 }))).unwrap();
    assert_eq!(frame_of(&log, "i1"), [100, 0, 5000, 900]);
}

#[test]
fn a_put_text_takes_a_size_in_tenths_and_its_frame_fits_it() {
    let mut log = log_of(&bx(0, 0, 1000, 500));
    let big = How { text_size: Some(480), at: Some([0, 1000]), ..How::default() };
    let placed = log.put_how(&scout(), AT, "i1", &Put::Text("hi".into()), &big).unwrap();
    assert_eq!(placed.frame, [0, 1000, 2 * 480, 480]);
    assert_eq!(log.show(None).unwrap().items[1]["size"], 480);
    let next = log.next();
    for (what, tenths) in [
        (Put::Text("hi".into()), 39),
        (Put::Text("hi".into()), 4001),
        (Put::Box { w: 10, h: 10 }, 240),
    ] {
        let how = How { text_size: Some(tenths), ..How::default() };
        let err = log.put_how(&scout(), AT, "i1", &what, &how).unwrap_err();
        assert_eq!(err.code(), "invalid", "{what:?} {tenths}");
        assert!(err.hint().is_some(), "{what:?} {tenths}");
    }
    assert_eq!(log.next(), next, "a refused put writes nothing");
}

/// A stroke of the user's in red, from `x,y` down `h` tenths, 2 units wide.
fn letter(x: i64, y: i64, h: i64) -> Value {
    json!({ "op": "add", "item": { "page": "p1", "kind": "stroke", "color": "#cc0000",
        "width": 20, "pts": [x, y, 0, h] } })
}

/// Two lines of red writing, letters 62 units tall with the pen's width
/// (some half that), and a bracket two lines tall that touches the first
/// letter, so all of it is one cluster, `c1`.
fn note() -> Vec<Value> {
    let mut ops = Vec::new();
    for y in [0, 900] {
        ops.extend([letter(0, y, 600), letter(150, y + 300, 300), letter(300, y, 600)]);
    }
    ops.push(letter(0, 0, 1500));
    ops
}

/// The colour and size, in tenths, of the item `id` as `canvas show` gives
/// them (`size` is absent at the default).
fn look_of(log: &Log, id: &str) -> (String, i64) {
    let shown = log.show(None).unwrap();
    let item = shown.items.iter().find(|i| i["id"] == id).unwrap();
    (item["color"].as_str().unwrap().to_owned(), item["size"].as_i64().unwrap_or(240))
}

#[test]
fn a_reply_with_no_colour_or_size_matches_the_writing_it_goes_beside() {
    // slinger-1da937: replies came out 240 and black, whatever the ink.
    let mut log = log_of(&json!(note()));
    let placed = log.put(&scout(), AT, "c1", &Put::Text("noted".into())).unwrap();
    assert_eq!(look_of(&log, &placed.item), ("#cc0000".into(), 620), "a line's height, red");
    assert_eq!(placed.frame[2..], [5 * 620, 620]);
    // A single stroke counts with its whole note; a box takes the colour.
    let placed = log.put(&scout(), AT, "i2", &Put::Text("again".into())).unwrap();
    assert_eq!(look_of(&log, &placed.item), ("#cc0000".into(), 620));
    let placed = log.put(&scout(), AT, "c1", &Put::Box { w: 100, h: 100 }).unwrap();
    assert_eq!(look_of(&log, &placed.item).0, "#cc0000");
}

#[test]
fn what_the_agent_gives_wins_and_beside_no_writing_the_page_decides() {
    let mut ops = note();
    let blue = json!({ "op": "add", "item": { "page": "p1", "kind": "text", "x": 0, "y": 5000,
        "text": "a", "size": 480, "color": "#0000ff" } });
    ops.extend([blue.clone(), blue, bx(9000, 0, 100, 100)]);
    let mut log = log_of(&json!(ops));
    let how = How { colour: Some("#00ff00".into()), text_size: Some(300), ..How::default() };
    let placed = log.put_how(&scout(), AT, "c1", &Put::Text("mine".into()), &how).unwrap();
    assert_eq!(look_of(&log, &placed.item), ("#00ff00".into(), 300));
    let size_only = How { text_size: Some(300), ..How::default() };
    let placed = log.put_how(&scout(), AT, "c1", &Put::Text("half".into()), &size_only).unwrap();
    assert_eq!(look_of(&log, &placed.item), ("#cc0000".into(), 300));
    // Beside the user's box: the page's marks, seven red strokes against
    // two blue texts, and sizes 620 (the note), 480 and 480.
    let placed = log.put(&scout(), AT, "b10", &Put::Text("page".into())).unwrap();
    assert_eq!(look_of(&log, &placed.item), ("#cc0000".into(), 480));
    // The agent's own replies do not count, and a page with no writing
    // gives the old default.
    let mut log = log_of(&bx(0, 0, 100, 100));
    log.put_how(&scout(), AT, "i1", &Put::Text("x".into()), &how).unwrap();
    let placed = log.put(&scout(), AT, "i2", &Put::Text("plain".into())).unwrap();
    assert_eq!(look_of(&log, &placed.item), ("#000000".into(), 240));
}

#[test]
fn a_lone_drawing_apart_from_the_writing_does_not_size_the_page() {
    // slinger-a6c4ec: a frame drawn alone counted as a line of its own
    // height, and the page's size came out between it and the writing.
    let mut ops = note();
    ops.extend([letter(20000, 0, 3000), bx(9000, 0, 100, 100)]);
    let mut log = log_of(&json!(ops));
    let placed = log.put(&scout(), AT, "b9", &Put::Text("page".into())).unwrap();
    assert_eq!(look_of(&log, &placed.item), ("#cc0000".into(), 620), "the note's lines alone");
    // Beside the frame itself it is the only mark, and gives its height.
    let placed = log.put(&scout(), AT, "i8", &Put::Text("frame".into())).unwrap();
    assert_eq!(look_of(&log, &placed.item).1, 3020);
}
