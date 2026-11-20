//! Folding a box (docs/design/fold.md): what a fold takes, what it hides,
//! unfold into a free or a taken room, carrying on move and erase, undo,
//! and what a reader from before folds keeps of a file with folds.
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::canvas::file::Log;
use slinger_core::canvas::{Author, Item, ItemId, PageId, Rect, Shape, Time, miniature};

const AT: Time = Time(1_762_790_400_000);
const P1: PageId = PageId::new(1);

fn scout() -> Author {
    Author::Agent("scout".into())
}

fn id(s: &str) -> ItemId {
    ItemId::new(s[1..].parse().unwrap())
}

fn ids(list: &[&str]) -> Vec<ItemId> {
    list.iter().map(|s| id(s)).collect()
}

/// Adds one item (a file item object, without its page) as `by`; its id.
fn add(log: &mut Log, by: &Author, mut item: Value) -> String {
    item["page"] = "p1".into();
    log.apply(by, AT, &json!({"op": "add", "item": item})).unwrap().made.remove(0)
}

fn boxed(log: &mut Log, x: i64, y: i64, w: i64, h: i64) -> String {
    add(log, &Author::User, json!({"kind": "box", "x": x, "y": y, "w": w, "h": h}))
}

/// A short stroke of width 2 units from (x, y), in tenths.
fn ink(log: &mut Log, x: i64, y: i64) -> String {
    add(log, &Author::User, json!({"kind": "stroke", "width": 20, "pts": [x, y, 100, 100]}))
}

fn shown(log: &Log) -> Vec<ItemId> {
    log.canvas().drawn(P1).map(Item::id).collect()
}

fn frame(log: &Log, item: &str) -> Rect {
    log.canvas().item(id(item)).unwrap().bounds()
}

/// The bytes a log wrote, header first, after each change appended.
struct Book {
    log: Log,
    bytes: String,
}

impl Book {
    fn new() -> Self {
        let (log, bytes) = Log::create("folds").unwrap();
        Self { log, bytes }
    }

    fn keep(&mut self, written: &slinger_core::canvas::file::Written) {
        self.bytes.push_str(&written.bytes);
    }

    fn apply(&mut self, ops: &Value) -> Vec<String> {
        let w = self.log.apply(&Author::User, AT, ops).unwrap();
        self.keep(&w);
        w.made
    }

    fn fold(&mut self, item: &str) {
        let placed = self.log.fold(&Author::User, AT, item, None).unwrap();
        self.keep(&placed.written);
    }
}

#[test]
fn a_fold_takes_what_lies_wholly_inside_and_hides_it_in_one_undo_step() {
    let (mut log, _) = Log::create("c").unwrap();
    let b = boxed(&mut log, 0, 0, 4000, 2000);
    let inside = ink(&mut log, 500, 500);
    let across = ink(&mut log, 3950, 500);
    let outside = ink(&mut log, 9000, 9000);
    let placed = log.fold(&Author::User, AT, &b, None).unwrap();
    assert_eq!(placed.frame, [0, 0, 1000, 500], "a quarter of the longer side, proportions kept");
    let shape = log.canvas().item(id(&b)).unwrap().shape().clone();
    let fold = shape.fold().unwrap();
    assert_eq!((fold.open, fold.items.clone()), ((400.0, 200.0), ids(&[&inside])));
    assert_eq!(shown(&log), ids(&[&b, &across, &outside]));
    assert!(log.canvas().hidden(id(&inside)));
    assert_eq!(log.canvas().holder(id(&inside)), Some(id(&b)));
    assert_eq!(log.canvas().item_at(P1, rect_centre(frame(&log, &inside)), 0.0), None);
    assert!(placed.written.bytes.contains(r#""fold":{"h":2000,"items":["i2"],"w":4000}"#));

    let again = log.fold(&Author::User, AT, &b, None).unwrap();
    assert_eq!(again.written.bytes, "", "folding a folded box writes nothing");
    log.undo(&Author::User, AT);
    assert_eq!(frame(&log, &b), Rect::new(0.0, 0.0, 400.0, 200.0));
    assert_eq!(log.canvas().item(id(&b)).unwrap().shape().fold(), None);
    assert_eq!(shown(&log).len(), 4);
}

fn rect_centre(r: Rect) -> slinger_core::canvas::Point {
    slinger_core::canvas::Point::new(r.x + r.w / 2.0, r.y + r.h / 2.0)
}

#[test]
fn a_fold_survives_a_reopen_of_the_file() {
    let mut book = Book::new();
    let made = book.apply(&json!([
        {"op": "add", "item": {"page": "p1", "kind": "box", "x": 0, "y": 0, "w": 2000, "h": 2000}},
        {"op": "add", "item": {"page": "p1", "kind": "stroke", "width": 20, "pts": [500, 500, 10, 10]}},
    ]));
    book.fold(&made[0]);
    let (opened, _) = Log::open(book.bytes.as_bytes()).unwrap();
    assert_eq!(opened.canvas(), book.log.canvas());
    assert_eq!(shown(&opened), ids(&[&made[0]]));
}

#[test]
fn an_open_box_inside_is_taken_with_its_contents_and_a_folded_one_keeps_its_own() {
    let (mut log, _) = Log::create("c").unwrap();
    let outer = boxed(&mut log, 0, 0, 8000, 8000);
    let inner = boxed(&mut log, 1000, 1000, 2000, 2000);
    let deep = ink(&mut log, 1500, 1500);
    let loose = ink(&mut log, 5000, 5000);
    assert_eq!(log.canvas().contents(id(&outer)), ids(&[&inner, &deep, &loose]));

    log.fold(&Author::User, AT, &inner, None).unwrap();
    log.fold(&Author::User, AT, &outer, None).unwrap();
    let c = log.canvas();
    assert_eq!(c.contents(id(&outer)), ids(&[&inner, &loose]));
    assert_eq!(c.contents(id(&inner)), ids(&[&deep]));
    assert_eq!(c.held(id(&outer)), ids(&[&inner, &deep, &loose]));
    assert_eq!(shown(&log), ids(&[&outer]));
}

#[test]
fn a_box_folded_inside_a_folded_box_takes_its_contents_out_of_that_list() {
    let (mut log, _) = Log::create("c").unwrap();
    let outer = boxed(&mut log, 0, 0, 8000, 8000);
    let inner = boxed(&mut log, 1000, 1000, 2000, 2000);
    let deep = ink(&mut log, 1500, 1500);
    log.fold(&Author::User, AT, &outer, None).unwrap();
    let placed = log.fold(&Author::User, AT, &inner, None).unwrap();
    assert_eq!(placed.written.seq.len(), 2, "the inner fold and the outer list, one step");
    let c = log.canvas();
    assert_eq!(c.contents(id(&outer)), ids(&[&inner]));
    assert_eq!(c.contents(id(&inner)), ids(&[&deep]));
    log.unfold(&Author::User, AT, &inner, None).unwrap();
    assert_eq!(log.canvas().contents(id(&outer)), ids(&[&inner, &deep]));
    log.undo(&Author::User, AT);
    log.undo(&Author::User, AT);
    assert_eq!(log.canvas().contents(id(&outer)), ids(&[&inner, &deep]));
    assert_eq!(log.canvas().item(id(&inner)).unwrap().shape().fold(), None);
    assert_eq!(log.canvas().contents(id(&inner)), ids(&[&deep]), "open: by geometry");
}

#[test]
fn notes_written_in_the_old_room_never_join_and_block_the_unfold() {
    let (mut log, _) = Log::create("c").unwrap();
    let b = boxed(&mut log, 0, 0, 4000, 4000);
    let held = ink(&mut log, 500, 500);
    log.fold(&Author::User, AT, &b, None).unwrap();
    let later = ink(&mut log, 3000, 3000);
    assert_eq!(log.canvas().contents(id(&b)), ids(&[&held]));
    assert_eq!(log.canvas().in_the_way(id(&b)), ids(&[&later]));

    let next = log.next();
    let err = log.unfold(&Author::User, AT, &b, None).unwrap_err();
    assert_eq!(err.code(), "conflict");
    assert!(err.to_string().contains("i3"), "{err}");
    assert_eq!(log.next(), next, "a refused unfold writes nothing");

    log.apply(&Author::User, AT, &json!({"op": "move", "ids": [later], "dx": 9000, "dy": 0}))
        .unwrap();
    let placed = log.unfold(&Author::User, AT, &b, None).unwrap();
    assert_eq!(placed.frame, [0, 0, 4000, 4000]);
    assert_eq!(log.canvas().item(id(&b)).unwrap().shape().fold(), None);
    assert!(placed.written.bytes.contains(r#""fold":null"#));
    assert_eq!(shown(&log).len(), 3);
    assert_eq!(log.unfold(&Author::User, AT, &b, None).unwrap().written.bytes, "");
}

#[test]
fn a_move_of_a_folded_box_carries_what_it_holds_at_any_depth_and_its_arrows() {
    let (mut log, _) = Log::create("c").unwrap();
    let outer = boxed(&mut log, 0, 0, 8000, 8000);
    let inner = boxed(&mut log, 1000, 1000, 2000, 2000);
    let deep = ink(&mut log, 1500, 1500);
    let far = boxed(&mut log, 20_000, 0, 1000, 1000);
    let arrow = log
        .apply(
            &Author::User,
            AT,
            &json!({"op": "add", "item": {"page": "p1", "kind": "arrow",
                "pts": [1600, 1600, 18_400, -1600], "from": deep, "to": far}}),
        )
        .unwrap()
        .made
        .remove(0);
    log.fold(&Author::User, AT, &inner, None).unwrap();
    log.fold(&Author::User, AT, &outer, None).unwrap();
    let before = frame(&log, &deep);

    let w = log
        .apply(&Author::User, AT, &json!({"op": "move", "ids": [outer], "dx": 1000, "dy": 500}))
        .unwrap();
    assert_eq!(w.seq.len(), 4, "the box, what it holds, and the arrow");
    let after = frame(&log, &deep);
    assert_eq!((after.x - before.x, after.y - before.y), (100.0, 50.0));
    let Shape::Arrow { from, .. } = log.canvas().item(id(&arrow)).unwrap().shape() else {
        panic!("not an arrow")
    };
    assert_eq!((from.at.x, from.at.y), (260.0, 210.0));

    log.edit(
        &Author::User,
        AT,
        &outer,
        &slinger_core::canvas::file::Edit { to: Some([0, 0]), ..Default::default() },
    )
    .unwrap();
    assert_eq!(frame(&log, &deep), before, "an edit's new place carries too");
    log.undo(&Author::User, AT);
    log.undo(&Author::User, AT);
    assert_eq!(frame(&log, &deep), before);
    assert_eq!(frame(&log, &outer).x, 0.0);
}

#[test]
fn an_erase_of_a_folded_box_takes_what_it_holds_and_undo_puts_it_all_back() {
    let mut book = Book::new();
    let made = book.apply(&json!([
        {"op": "add", "item": {"page": "p1", "kind": "box", "x": 0, "y": 0, "w": 4000, "h": 4000}},
        {"op": "add", "item": {"page": "p1", "kind": "stroke", "width": 20, "pts": [500, 500, 10, 10]}},
    ]));
    book.fold(&made[0]);
    let w = book.apply(&json!({"op": "remove", "ids": [made[0]]}));
    assert_eq!(w, Vec::<String>::new());
    assert_eq!(book.log.canvas().items(P1).count(), 0);
    let undone = book.log.undo(&Author::User, AT);
    book.keep(&undone);
    assert!(undone.bytes.contains(r#""fold":{"w":4000,"h":4000,"items":["i2"]}"#));
    assert_eq!(shown(&book.log), ids(&[&made[0]]));
    assert_eq!(book.log.canvas().contents(id(&made[0])), ids(&[&made[1]]));
    let (opened, _) = Log::open(book.bytes.as_bytes()).unwrap();
    assert_eq!(opened.canvas(), book.log.canvas());
}

#[test]
fn an_agent_folds_only_its_own_box_and_carries_only_what_it_may_change() {
    let (mut log, _) = Log::create("c").unwrap();
    let users = boxed(&mut log, 0, 0, 4000, 4000);
    let err = log.fold(&scout(), AT, &users, None).unwrap_err();
    assert_eq!(err.code(), "invalid");

    let mine =
        add(&mut log, &scout(), json!({"kind": "box", "x": 10_000, "y": 0, "w": 4000, "h": 4000}));
    let theirs = ink(&mut log, 10_500, 500);
    log.fold(&scout(), AT, &mine, None).unwrap();
    assert_eq!(log.canvas().contents(id(&mine)), ids(&[&theirs]));
    let next = log.next();
    let err = log.remove(&scout(), AT, &mine).unwrap_err();
    assert_eq!(err.code(), "invalid");
    let err = log.apply(&scout(), AT, &json!({"op": "move", "ids": [mine], "dx": 10, "dy": 0}));
    assert_eq!(err.unwrap_err().code(), "invalid");
    assert_eq!(log.next(), next);
    assert_eq!(log.fold(&Author::User, AT, &theirs, None).unwrap_err().code(), "invalid");
}

#[test]
fn a_once_key_folds_once() {
    let (mut log, _) = Log::create("c").unwrap();
    let b = boxed(&mut log, 0, 0, 4000, 4000);
    let first = log.fold(&Author::User, AT, &b, Some("k1")).unwrap();
    log.undo(&Author::User, AT);
    let again = log.fold(&Author::User, AT, &b, Some("k1")).unwrap();
    assert_eq!(again.written.bytes, "");
    assert_eq!(again.written.seq, first.written.seq);
}

#[test]
fn a_list_never_fails_gone_ids_are_skipped_two_holders_go_to_the_lower_and_cycles_are_cut() {
    let (mut log, _) = Log::create("c").unwrap();
    let a = boxed(&mut log, 0, 0, 1000, 1000);
    let b = boxed(&mut log, 5000, 0, 1000, 1000);
    let x = ink(&mut log, 20_000, 0);
    let set = |item: &str, list: Value| json!({"op": "set", "id": item, "fields": {"fold": {"w": 1000, "h": 1000, "items": list}}});
    log.apply(&Author::User, AT, &json!([set(&a, json!([b, x, "i99"])), set(&b, json!([a, x]))]))
        .unwrap();
    let c = log.canvas();
    assert_eq!(c.holder(id(&x)), Some(id(&a)));
    assert_eq!(c.holder(id(&b)), Some(id(&a)));
    assert_eq!(c.holder(id(&a)), None, "a box never holds itself");
    assert_eq!(shown(&log), ids(&[&a]));

    let mut bad = set(&a, json!([]));
    bad["fields"]["fold"]["w"] = 0.into();
    assert_eq!(log.apply(&Author::User, AT, &bad).unwrap_err().code(), "invalid");
}

#[test]
fn a_request_add_may_list_what_an_add_before_it_makes() {
    let (mut log, _) = Log::create("c").unwrap();
    let made = log
        .apply(
            &Author::User,
            AT,
            &json!([
                {"op": "add", "item": {"page": "p1", "kind": "stroke", "width": 20, "pts": [0, 0, 10, 10]}},
                {"op": "add", "item": {"page": "p1", "kind": "box", "x": 0, "y": 0, "w": 800, "h": 800,
                    "fold": {"w": 4000, "h": 4000, "items": ["$0"]}}},
            ]),
        )
        .unwrap()
        .made;
    assert_eq!(log.canvas().contents(id(&made[1])), ids(&[&made[0]]));
}

#[test]
fn the_miniature_is_a_quarter_of_the_longer_side_within_its_range() {
    assert_eq!(miniature(Rect::new(5.0, 6.0, 800.0, 400.0)), Rect::new(5.0, 6.0, 200.0, 100.0));
    assert_eq!(miniature(Rect::new(0.0, 0.0, 200.0, 100.0)), Rect::new(0.0, 0.0, 80.0, 40.0));
    assert_eq!(miniature(Rect::new(0.0, 0.0, 4000.0, 2000.0)), Rect::new(0.0, 0.0, 320.0, 160.0));
    assert_eq!(miniature(Rect::new(0.0, 0.0, 50.0, 25.0)), Rect::new(0.0, 0.0, 50.0, 25.0));
    assert_eq!(miniature(Rect::new(0.0, 0.0, 0.0, 25.0)), Rect::new(0.0, 0.0, 0.0, 25.0));
}

/// Every `fold` key of a file taken out: what a reader from before folds
/// reads, since it ignores fields it does not know.
fn without_folds(bytes: &str) -> String {
    fn strip(v: &mut Value) {
        match v {
            Value::Object(m) => {
                m.remove("fold");
                m.values_mut().for_each(strip);
            }
            Value::Array(list) => list.iter_mut().for_each(strip),
            _ => {}
        }
    }
    let mut out = String::new();
    for line in bytes.lines() {
        let mut v: Value = serde_json::from_str(line).unwrap();
        strip(&mut v);
        out.push_str(&v.to_string());
        out.push('\n');
    }
    out
}

/// Acceptance 5 of plan pl-f130a2: a file with folds, nested and carried,
/// read as a build from before folds reads it, keeps every item at the
/// same place; and that build's erase and undo of a folded box (a `restore`
/// without `fold`) leaves the box open with every item it held visible.
#[test]
fn a_reader_from_before_folds_loses_no_item() {
    let mut book = Book::new();
    let made = book.apply(&json!([
        {"op": "add", "item": {"page": "p1", "kind": "box", "x": 0, "y": 0, "w": 8000, "h": 8000}},
        {"op": "add", "item": {"page": "p1", "kind": "box", "x": 1000, "y": 1000, "w": 2000, "h": 2000}},
        {"op": "add", "item": {"page": "p1", "kind": "stroke", "width": 20, "pts": [1500, 1500, 10, 10]}},
        {"op": "add", "item": {"page": "p1", "kind": "text", "x": 4000, "y": 4000, "text": "Monday"}},
        {"op": "add", "item": {"page": "p1", "kind": "stroke", "width": 20, "pts": [20000, 0, 10, 10]}},
    ]));
    book.fold(&made[1]);
    book.fold(&made[0]);
    book.apply(&json!({"op": "move", "ids": [made[0]], "dx": 3000, "dy": 1000}));

    let full = Log::open(book.bytes.as_bytes()).unwrap().0;
    let old_bytes = without_folds(&book.bytes);
    let (mut old, _) = Log::open(old_bytes.as_bytes()).unwrap();
    let places = |log: &Log| -> Vec<(ItemId, Rect)> {
        log.canvas().items(P1).map(|i| (i.id(), i.bounds())).collect()
    };
    assert_eq!(places(&old), places(&full));
    assert_eq!(places(&full).len(), 5);
    assert_eq!(old.canvas().drawn(P1).count(), 5, "the old reader sees every item");

    // The old build erases the folded box and takes it back: its `restore`
    // is whole as it knows the box, with no `fold`.
    let removed = old.remove(&Author::User, AT, &made[0]).unwrap();
    let restored = old.undo(&Author::User, AT);
    assert!(!restored.bytes.contains("fold"));
    let mut bytes = book.bytes.clone();
    bytes.push_str(&removed.bytes);
    bytes.push_str(&restored.bytes);
    let (new, _) = Log::open(bytes.as_bytes()).unwrap();
    let c = new.canvas();
    assert_eq!(c.item(id(&made[0])).unwrap().shape().fold(), None, "the fold is lost");
    let visible: Vec<ItemId> = c.drawn(P1).map(Item::id).collect();
    assert_eq!(visible, ids(&[&made[0], &made[1], &made[3], &made[4]]));
    assert!(c.hidden(id(&made[2])), "the inner box keeps its own fold");
    assert_eq!(c.items(P1).count(), 5, "no item lost");
}
