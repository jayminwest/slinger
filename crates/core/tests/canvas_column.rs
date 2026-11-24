//! A text grows to at most `MOST_ROWS` rows (docs/design/writing-column.md,
//! 3A): past that its frame stops, so its hit, the room a put keeps clear of
//! and the crop stop too, while the item keeps every row.
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::canvas::file::{Log, Put};
use slinger_core::canvas::{Author, ItemId, PageId, Rect, Time};
use slinger_core::render::{self, MARGIN, MOST_ROWS, SCALE};

const AT: Time = Time(1_762_790_400_000);

/// A row's height, in tenths, at the usual size (24 units).
const ROW: i64 = 240;

/// `n` rows of text, one a line.
fn rows(n: usize) -> String {
    (1..=n).map(|i| format!("row {i}")).collect::<Vec<_>>().join("\n")
}

/// A log whose page holds the user's text of `n` rows at the origin, with
/// `frame` (`w`, `h` in tenths, or none).
fn long_text(n: usize, frame: Option<[i64; 2]>) -> Log {
    let (mut log, _) = Log::create("notes").unwrap();
    let mut item = json!({"page": "p1", "kind": "text", "x": 0, "y": 0, "text": rows(n)});
    if let Some([w, h]) = frame {
        (item["w"], item["h"]) = (w.into(), h.into());
    }
    log.apply(&Author::User, AT, &json!({"op": "add", "item": item})).unwrap();
    log
}

fn shown(log: &Log, id: &str) -> Value {
    let items = log.show(None).unwrap().items;
    items.into_iter().find(|i| i["id"] == id).unwrap()
}

fn height(log: &Log, id: &str) -> i64 {
    shown(log, id)["h"].as_i64().unwrap()
}

#[test]
fn a_text_is_at_most_most_rows_tall_however_its_frame_was_written() {
    assert_eq!(MOST_ROWS, 80);
    let most = 80 * ROW;
    assert_eq!(render::text_size(&rows(100), 80, 24.0).1, 80.0 * 24.0);
    assert_eq!(render::text_size(&rows(79), 80, 24.0).1, 79.0 * 24.0, "a shorter one is its rows");
    // No frame: unwrapped. A column with no height. A height written past the cap.
    assert_eq!(height(&long_text(100, None), "i1"), most);
    assert_eq!(height(&long_text(100, Some([80 * 120 + 60, 0])), "i1"), most);
    assert_eq!(height(&long_text(100, Some([80 * 120 + 60, 100 * ROW])), "i1"), most);
    // A frame below the cap is kept as written.
    assert_eq!(height(&long_text(100, Some([80 * 120 + 60, 3 * ROW])), "i1"), 3 * ROW);
}

#[test]
fn the_whole_text_is_kept_in_the_item_and_the_packet_boxes_its_most_rows() {
    let log = long_text(100, None);
    assert_eq!(shown(&log, "i1")["text"], rows(100));
    let packet = log.packet("notes", None, None, 0, &[], &[]).unwrap().text();
    assert!(packet.contains("t1 text 0,0 84x1920 "), "the line's box is 80 rows tall: {packet}");
}

#[test]
fn a_put_beside_a_long_text_lands_beside_its_rows_and_the_room_below_is_free() {
    let mut log = long_text(100, None);
    let below = Rect::new(0.0, 80.0 * 24.0 + 1.0, 100.0, 19.0 * 24.0);
    let page = PageId::new(1);
    assert!(log.canvas().items_in_box(page, below).is_empty(), "no hidden row is hit");
    let scout = Author::Agent("scout".into());
    let placed = log.put(&scout, AT, "i1", &Put::Text("noted".into())).unwrap();
    let text = shown(&log, "i1");
    let right = text["w"].as_i64().unwrap();
    assert_eq!(placed.frame[..2], [right + ROW, 0], "right of it by a gap, top-aligned");
}

#[test]
fn the_crop_of_a_long_text_draws_its_most_rows_only() {
    let log = long_text(100, None);
    let [image] = render::items(log.canvas(), &[ItemId::new(1)], "t1").try_into().unwrap();
    let most = (80.0 * 24.0 + 2.0 * MARGIN) * SCALE;
    assert!(f64::from(image.height) <= most.ceil(), "{} pixels tall", image.height);
    assert!(f64::from(image.height) > (79.0 * 24.0) * SCALE, "{} pixels tall", image.height);
}
