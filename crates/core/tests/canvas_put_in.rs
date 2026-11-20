//! An answer to a sling of a box (docs/design/fold.md, "Sling a box"): a put
//! with `in` lands in the box's room among its contents, joins a folded
//! box (the user's too) in one undo step, and fails loudly when an open box
//! has no room.
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::canvas::file::{How, Log, Put};
use slinger_core::canvas::{Author, GAP, ItemId, PageId, Rect, Time};

const AT: Time = Time(1_762_790_400_000);
const P1: PageId = PageId::new(1);

fn scout() -> Author {
    Author::Agent("scout".into())
}

fn id(s: &str) -> ItemId {
    ItemId::new(s[1..].parse().unwrap())
}

/// Adds one item (a file item object, without its page) as the user; its id.
fn add(log: &mut Log, mut item: Value) -> String {
    item["page"] = "p1".into();
    log.apply(&Author::User, AT, &json!({"op": "add", "item": item})).unwrap().made.remove(0)
}

fn frame(log: &Log, item: &str) -> Rect {
    log.canvas().item(id(item)).unwrap().bounds()
}

fn inside(r: Rect, room: Rect) -> bool {
    r.x >= room.x && r.y >= room.y && r.x + r.w <= room.x + room.w && r.y + r.h <= room.y + room.h
}

fn put_in(log: &mut Log, beside: &str, boxed: &str, what: &Put) -> slinger_core::Result<String> {
    let how = How { inside: Some(boxed.into()), ..How::default() };
    Ok(log.put_how(&scout(), AT, beside, what, &how)?.item)
}

/// A user's box at (100,100) 400x200 holding a text, with a text on the
/// page right beside the box, where a plain put would go.
fn page() -> (Log, String, String) {
    let (mut log, _) = Log::create("c").unwrap();
    let b = add(&mut log, json!({"kind": "box", "x": 1000, "y": 1000, "w": 4000, "h": 2000}));
    let t = add(&mut log, json!({"kind": "text", "x": 1240, "y": 1240, "text": "Monday"}));
    add(&mut log, json!({"kind": "text", "x": 5300, "y": 1000, "text": "elsewhere"}));
    (log, b, t)
}

#[test]
fn a_put_in_an_open_box_lands_beside_the_item_inside_the_box() {
    let (mut log, b, t) = page();
    let made = put_in(&mut log, &t, &b, &Put::Text("ships".into())).unwrap();
    let (room, near, put) = (frame(&log, &b), frame(&log, &t), frame(&log, &made));
    assert!(inside(put, room), "{put:?} in {room:?}");
    assert_eq!((put.x, put.y), (near.x + near.w + GAP, near.y), "right of the item");
    assert!(log.canvas().contents(id(&b)).contains(&id(&made)));
    assert_eq!(log.canvas().item(id(&b)).unwrap().bounds(), room, "the user's box is as it was");
}

#[test]
fn a_put_beside_the_box_itself_goes_in_from_its_top_left() {
    let (mut log, b, _) = page();
    let made = put_in(&mut log, &b, &b, &Put::Box { w: 400, h: 400 }).unwrap();
    let put = frame(&log, &made);
    assert!(inside(put, frame(&log, &b)));
    assert_eq!(put.y, 100.0 + GAP, "the first row in the room");
}

#[test]
fn an_open_box_with_no_room_is_a_conflict_and_a_place_outside_it_invalid() {
    let (mut log, b, t) = page();
    let big = Put::Box { w: 5000, h: 100 };
    assert_eq!(put_in(&mut log, &t, &b, &big).unwrap_err().code(), "conflict");
    let how = How { inside: Some(b.clone()), at: Some([9000, 9000]), ..How::default() };
    let out = log.put_how(&scout(), AT, &t, &Put::Text("x".into()), &how);
    assert_eq!(out.unwrap_err().code(), "invalid");
    let not_a_box = put_in(&mut log, &t, &t, &Put::Text("x".into()));
    assert_eq!(not_a_box.unwrap_err().code(), "invalid");
    assert_eq!(
        put_in(&mut log, &t, "b99", &Put::Text("x".into())).unwrap_err().code(),
        "not_found"
    );
    assert_eq!(log.canvas().items(P1).count(), 3, "nothing was put");
}

#[test]
fn an_agent_answers_in_the_users_folded_box_and_one_undo_takes_it_out() {
    let (mut log, b, t) = page();
    log.fold(&Author::User, AT, &b, None).unwrap();
    let mini = frame(&log, &b);
    let made = put_in(&mut log, &t, &b, &Put::Text("ships".into())).unwrap();
    let canvas = log.canvas();
    let fold = canvas.item(id(&b)).unwrap().shape().fold().unwrap().clone();
    assert_eq!(fold.items, vec![id(&t), id(&made)], "it joins the box");
    assert!(canvas.hidden(id(&made)), "the page shows the miniature, not the answer");
    assert_eq!(frame(&log, &b), mini);
    let near = frame(&log, &t);
    assert_eq!((frame(&log, &made).x, frame(&log, &made).y), (near.x + near.w + GAP, near.y));
    // The agent's undo takes the answer out of the user's box again.
    log.undo(&scout(), AT);
    let canvas = log.canvas();
    assert!(canvas.item(id(&made)).is_none());
    assert_eq!(canvas.item(id(&b)).unwrap().shape().fold().unwrap().items, vec![id(&t)]);
}

#[test]
fn a_full_folded_box_takes_the_answer_below_all_it_holds_and_grows() {
    let (mut log, b, t) = page();
    log.fold(&Author::User, AT, &b, None).unwrap();
    let made = put_in(&mut log, &t, &b, &Put::Box { w: 5000, h: 100 }).unwrap();
    let put = frame(&log, &made);
    assert_eq!((put.x, put.y), (100.0 + GAP, frame(&log, &t).y + frame(&log, &t).h + GAP));
    let open = log.canvas().item(id(&b)).unwrap().shape().fold().unwrap().open;
    assert_eq!(open.0, put.x + put.w - 100.0, "the open size grows to hold it");
}

#[test]
fn an_agent_still_may_not_take_the_users_items_into_or_out_of_a_box() {
    let (mut log, b, t) = page();
    let loose = add(&mut log, json!({"kind": "text", "x": 9000, "y": 9000, "text": "loose"}));
    log.fold(&Author::User, AT, &b, None).unwrap();
    let set = |items: Vec<&str>| json!({"op": "set", "id": b, "fields": {"fold": {"w": 4000, "h": 2000, "items": items}}});
    for items in [vec![t.as_str(), loose.as_str()], vec![]] {
        assert_eq!(log.apply(&scout(), AT, &set(items)).unwrap_err().code(), "invalid");
    }
}
