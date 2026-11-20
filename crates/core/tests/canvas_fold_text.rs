//! What a box takes of the texts in it (docs/design/fold.md, "Contents at
//! fold time"): a text's frame is the column it wraps in, as wide as a typed
//! text's 48 characters however short its rows; the box measures the rows it
//! draws (slinger-2c7eec).
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::canvas::file::Log;
use slinger_core::canvas::{Author, Item, ItemId, PageId, Time};

const AT: Time = Time(1_762_790_400_000);
const P1: PageId = PageId::new(1);

fn id(s: &str) -> ItemId {
    ItemId::new(s[1..].parse().unwrap())
}

/// Adds one item (a file item object, without its page); its id.
fn add(log: &mut Log, mut item: Value) -> String {
    item["page"] = "p1".into();
    log.apply(&Author::User, AT, &json!({"op": "add", "item": item})).unwrap().made.remove(0)
}

fn boxed(log: &mut Log, x: i64, y: i64, w: i64, h: i64) -> String {
    add(log, json!({"kind": "box", "x": x, "y": y, "w": w, "h": h}))
}

/// A text at size 24 as the app types it: its frame is its 48-character
/// column (582 units), one row a line, in tenths.
fn typed(log: &mut Log, x: i64, y: i64, text: &str) -> String {
    let rows = i64::try_from(text.split('\n').count()).unwrap();
    add(log, json!({"kind": "text", "x": x, "y": y, "w": 5820, "h": 240 * rows, "text": text}))
}

fn shown(log: &Log) -> Vec<ItemId> {
    log.canvas().drawn(P1).map(Item::id).collect()
}

#[test]
fn a_box_around_short_typed_texts_takes_them_though_their_columns_cross_its_edge() {
    let (mut log, _) = Log::create("c").unwrap();
    let b = boxed(&mut log, 0, 0, 3000, 1500);
    let one = typed(&mut log, 200, 200, "hi");
    let two = typed(&mut log, 200, 600, "a list\nof two");
    log.fold(&Author::User, AT, &b, None).unwrap();
    assert_eq!(log.canvas().contents(id(&b)), vec![id(&one), id(&two)]);
    assert_eq!(shown(&log), vec![id(&b)]);
}

#[test]
fn a_text_whose_rows_cross_the_edge_stays_out() {
    let (mut log, _) = Log::create("c").unwrap();
    let b = boxed(&mut log, 0, 0, 3000, 1500);
    // 30 characters at 12 units a cell: 360 units, past the box's 300.
    let wide = typed(&mut log, 200, 200, "a line that runs past the edge");
    let low = typed(&mut log, 200, 1400, "hi");
    log.fold(&Author::User, AT, &b, None).unwrap();
    assert_eq!(log.canvas().contents(id(&b)), Vec::<ItemId>::new());
    assert_eq!(shown(&log), vec![id(&b), id(&wide), id(&low)]);
}

#[test]
fn a_wrapped_paragraph_is_as_wide_as_its_longest_row() {
    let (mut log, _) = Log::create("c").unwrap();
    let b = boxed(&mut log, 0, 0, 3000, 1500);
    // A 10-character column (120 units): rows "one two" and "three", 84 units.
    let words = add(
        &mut log,
        json!({"kind": "text", "x": 100, "y": 100, "w": 1200, "h": 480, "text": "one two three"}),
    );
    let narrow = boxed(&mut log, 50, 50, 1000, 600);
    log.fold(&Author::User, AT, &narrow, None).unwrap();
    assert_eq!(log.canvas().contents(id(&narrow)), vec![id(&words)]);
    assert_eq!(shown(&log), vec![id(&b), id(&narrow)]);
}
