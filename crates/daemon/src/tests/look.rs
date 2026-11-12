//! Crops and strokes through the real socket: an item, a box and an id that
//! is not there, with the images written as files.

use serde_json::{Value, json};
use slinger_control::CanvasOps;
use tempfile::TempDir;

use super::Daemon;

/// A canvas `main` holding a box and two strokes of one note.
fn drawn(daemon: &Daemon) {
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    let stroke = |x: i64| json!({ "op": "add", "item": { "page": "p1", "kind": "stroke", "width": 20, "pts": [x, 0, 100, 0] } });
    let bx = json!({ "op": "add", "item": { "page": "p1", "kind": "box", "x": 0, "y": 500, "w": 1000, "h": 500 } });
    let ops = json!([bx, stroke(0), stroke(120)]);
    let request = CanvasOps { canvas: "main".into(), by: "user".into(), ops, once: None };
    client.canvas_apply(request).unwrap();
}

#[test]
fn a_crop_of_an_item_writes_its_image_where_asked() {
    let (daemon, out) = (Daemon::start(), TempDir::new().unwrap());
    drawn(&daemon);
    let file = out.path().join("shot.png");
    let images = daemon.client().canvas_crop("main", "b1", &file).unwrap();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].path, file);
    // 100 by 50 units and a 16 unit margin, at 0.35 pixels a unit.
    assert_eq!((images[0].x, images[0].width, images[0].height), (0, 47, 29));
    assert!(std::fs::read(&file).unwrap().starts_with(b"\x89PNG"));
    daemon.stop();
}

#[test]
fn a_crop_of_a_box_writes_tiles_beside_the_file_and_an_empty_box_is_white() {
    let (daemon, out) = (Daemon::start(), TempDir::new().unwrap());
    drawn(&daemon);
    let file = out.path().join("deeper/box.png");
    let tiles = daemon.client().canvas_crop("main", "p1:0,0,100000,1000", &file).unwrap();
    let names: Vec<_> = tiles.iter().map(|t| t.path.file_name().unwrap().to_owned()).collect();
    assert_eq!(names, ["box-1.png", "box-2.png", "box-3.png"]);
    assert!(tiles.iter().all(|t| t.path.exists()));
    assert_eq!(tiles[1].x, 1504);
    let empty = daemon.client().canvas_crop("main", "-9000,-9000,0,0", &file).unwrap();
    assert_eq!((empty[0].width, empty[0].height, &empty[0].path), (1, 1, &file));
    daemon.stop();
}

#[test]
fn strokes_of_a_cluster_come_back_with_their_points() {
    let daemon = Daemon::start();
    drawn(&daemon);
    let strokes = daemon.client().canvas_strokes("main", "c2").unwrap();
    let ids: Vec<&str> = strokes.iter().filter_map(|s| s["id"].as_str()).collect();
    assert_eq!(ids, ["i2", "i3"]);
    assert_eq!(strokes[1]["points"], json!([[120, 0], [220, 0]]));
    assert_eq!(strokes[1]["by"], Value::from("user"));
    daemon.stop();
}

#[test]
fn an_unknown_item_or_canvas_is_not_found_and_a_bad_request_invalid() {
    let (daemon, out) = (Daemon::start(), TempDir::new().unwrap());
    drawn(&daemon);
    let client = daemon.client();
    let file = out.path().join("x.png");
    for of in ["i99", "c99", "p9:0,0,10,10"] {
        let err = client.canvas_crop("main", of, &file).unwrap_err();
        assert_eq!(err.code(), "not_found", "{of}");
        assert!(err.hint().is_some());
    }
    assert_eq!(client.canvas_strokes("main", "i99").unwrap_err().code(), "not_found");
    assert_eq!(client.canvas_strokes("nope", "i1").unwrap_err().code(), "not_found");
    assert_eq!(client.canvas_crop("nope", "i1", &file).unwrap_err().code(), "not_found");
    assert_eq!(client.canvas_strokes("main", "b1").unwrap_err().code(), "invalid");
    assert_eq!(client.canvas_crop("main", "0,0,999999,10", &file).unwrap_err().code(), "invalid");
    assert_eq!(
        client.canvas_crop("main", "i1", std::path::Path::new("/")).unwrap_err().code(),
        "invalid"
    );
    assert!(!file.exists());
    daemon.stop();
}
