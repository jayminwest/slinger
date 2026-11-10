//! What an agent asks a canvas for beyond the packet (`Log::crop`,
//! `Log::strokes`): an id or a box in, images or points out.
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::canvas::file::{Log, MAX_CROP};
use slinger_core::canvas::{Author, Time};
use slinger_core::render::{MARGIN, SCALE};

const AT: Time = Time(1_762_790_400_000);

/// A stroke from (x, y) to (x + 100, y), in tenths, by `by`.
fn stroke(x: i64, y: i64) -> Value {
    json!({ "op": "add", "item": { "page": "p1", "kind": "stroke", "width": 20, "pts": [x, y, 100, 0] } })
}

/// A canvas holding `ops`, applied by the user unless `agent` is set.
fn canvas(ops: &Value, agent: Option<&str>) -> Log {
    let (mut log, _) = Log::create("main").unwrap();
    let by = agent.map_or(Author::User, |a| Author::Agent(a.into()));
    log.apply(&by, AT, ops).unwrap();
    log
}

fn code<T: std::fmt::Debug>(result: slinger_core::Result<T>) -> &'static str {
    result.unwrap_err().code()
}

#[test]
fn a_crop_of_an_item_is_framed_with_the_margin_at_the_packet_scale() {
    let bx = json!({ "op": "add", "item": { "page": "p1", "kind": "box", "x": 0, "y": 0, "w": 1000, "h": 500 } });
    let log = canvas(&bx, None);
    let images = log.crop("i1", "shot").unwrap();
    assert_eq!(images.len(), 1);
    let side = |units: f64| ((units + 2.0 * MARGIN) * SCALE).ceil();
    assert_eq!(f64::from(images[0].width), side(100.0));
    assert_eq!(f64::from(images[0].height), side(50.0));
    assert_eq!(images[0].name, "shot.png");
    assert!(images[0].png.starts_with(b"\x89PNG"));
    // The packet's letter for a box names the same item.
    assert_eq!(log.crop("b1", "shot").unwrap(), images);
}

#[test]
fn a_cluster_id_names_every_stroke_of_the_users_note() {
    // Two strokes side by side make one note; one far away is its own.
    let log = canvas(&json!([stroke(0, 0), stroke(120, 0), stroke(50_000, 0)]), None);
    let ids =
        |v: Vec<Value>| v.iter().map(|s| s["id"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
    assert_eq!(ids(log.strokes("c1").unwrap()), ["i1", "i2"]);
    assert_eq!(ids(log.strokes("c2").unwrap()), ["i1", "i2"]);
    assert_eq!(ids(log.strokes("i2").unwrap()), ["i2"]);
    assert_eq!(ids(log.strokes("c3").unwrap()), ["i3"]);
    let note = log.crop("c1", "c1").unwrap();
    let one = log.crop("i1", "i1").unwrap();
    assert!(note[0].width > one[0].width);
}

#[test]
fn strokes_give_absolute_points_in_tenths_and_the_item_fields() {
    let log = canvas(&json!([stroke(30, 40)]), None);
    let strokes = log.strokes("i1").unwrap();
    assert_eq!(strokes[0]["points"], json!([[30, 40], [130, 40]]));
    assert_eq!(
        (strokes[0]["kind"].as_str(), strokes[0]["width"].as_i64()),
        (Some("stroke"), Some(20))
    );
    assert!(strokes[0].get("pts").is_none());
}

#[test]
fn an_agents_stroke_is_never_clustered() {
    let log = canvas(&json!([stroke(0, 0), stroke(120, 0)]), Some("ada"));
    assert_eq!(log.strokes("c1").unwrap().len(), 1);
}

#[test]
fn bad_ids_missing_items_and_non_ink_fail_with_their_codes() {
    let text = json!({ "op": "add", "item": { "page": "p1", "kind": "text", "x": 0, "y": 0, "w": 100, "h": 50, "text": "hi" } });
    let log = canvas(&text, None);
    for bad in ["", "i", "x1", "i1x", "c-1", "1"] {
        assert_eq!(code(log.crop(bad, "x")), "invalid", "{bad:?}");
    }
    assert_eq!(code(log.crop("i9", "x")), "not_found");
    assert_eq!(code(log.strokes("c9")), "not_found");
    let err = log.strokes("t1").unwrap_err();
    assert_eq!(err.code(), "invalid");
    assert!(err.hint().is_some());
}

#[test]
fn an_empty_box_is_a_white_image_and_a_large_one_is_tiles() {
    let log = canvas(&json!([]), None);
    let empty = log.crop("0,0,0,0", "box").unwrap();
    assert_eq!((empty.len(), empty[0].width, empty[0].height), (1, 1, 1));
    let blank = log.crop("p1:-500, 0, 1000, 1000", "box").unwrap();
    assert_eq!((blank[0].width, blank[0].name.as_str()), (35, "box.png"));
    assert_eq!(log.crop("Page 1:0,0,1000,1000", "box").unwrap(), blank);
    let wide = log.crop("0,0,100000,1000", "box").unwrap();
    assert_eq!(wide.len(), 3);
    assert_eq!(wide[2].name, "box-3.png");
}

#[test]
fn a_crop_is_limited_in_size_and_needs_a_box_and_page_that_are_ones() {
    let log = canvas(&json!([]), None);
    assert!((MAX_CROP - 16_000.0).abs() < 0.5);
    let biggest = log.crop("0,0,160000,160000", "box").unwrap();
    assert!(biggest.len() <= 16);
    assert_eq!(code(log.crop("0,0,160001,10", "box")), "invalid");
    for bad in ["0,0,10,-1", "0,0,10", "0,0,10,10,10", "0,0,x,10", "0,0,1.5,10"] {
        let err = log.crop(bad, "box").unwrap_err();
        assert_eq!(err.code(), "invalid", "{bad}");
        assert!(err.hint().unwrap().contains("x,y,w,h"));
    }
    assert_eq!(code(log.crop("p9:0,0,10,10", "box")), "not_found");
    let huge = json!({ "op": "add", "item": { "page": "p1", "kind": "box", "x": 0, "y": 0, "w": 160_000, "h": 10 } });
    let log = canvas(&huge, None);
    let err = log.crop("i1", "x").unwrap_err();
    assert_eq!(err.code(), "invalid");
    assert!(err.hint().unwrap().contains("box"));
}
