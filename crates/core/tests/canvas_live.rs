//! Live items (docs/design/live-items.md): an agent UI as an item on a
//! canvas. The model's rules, the canvas file's bytes, what an old reader
//! does with one, the put beside an item and the packet's text line.
#![cfg(test)]

use serde_json::json;
use slinger_core::canvas::file::{Line, Log, Put, read};
use slinger_core::canvas::{Action, Author, Canvas, Op, Rect, Shape, Time};

const AT: Time = Time(1_762_790_400_000);

fn hash() -> String {
    format!("sha256:{}", "9f".repeat(32))
}

fn live(frame: Rect) -> Shape {
    Shape::Live { frame, ui: "pick-slot".into(), hash: hash(), open: "pick-slot#12".into() }
}

fn scout() -> Author {
    Author::Agent("scout".into())
}

/// A log whose page holds the user's box `i1`, and its file so far.
fn log() -> (Log, String) {
    let (mut log, mut file) = Log::create("idea").unwrap();
    let bx = json!({ "op": "add", "item": { "page": "p1", "kind": "box", "x": 0, "y": 0, "w": 1000, "h": 500 } });
    file += &log.apply(&Author::User, AT, &bx).unwrap().bytes;
    (log, file)
}

/// Puts scout's live item beside `i1`, appends its line to `file` and
/// gives its id.
fn put_live(log: &mut Log, file: &mut String) -> String {
    let open = "pick-slot#12".to_string();
    let what = Put::Live { ui: "pick-slot".into(), hash: hash(), open, w: 3200, h: 2400 };
    let placed = log.put(&scout(), AT, "i1", &what).unwrap();
    *file += &placed.written.bytes;
    placed.item
}

#[test]
fn a_live_item_needs_a_name_a_blob_id_a_showing_and_a_box() {
    let mut c = Canvas::new();
    let page = c.pages().next().unwrap().id();
    let mut add = |shape| c.apply(Op { by: scout(), at: AT, action: Action::Add { page, shape } });
    assert!(add(live(Rect::new(1.0, 2.0, 320.0, 240.0))).is_ok());
    let frame = Rect::new(0.0, 0.0, 10.0, 10.0);
    let bad = [
        Shape::Live { frame, ui: "Pick".into(), hash: hash(), open: "pick-slot#1".into() },
        Shape::Live { frame, ui: "pick".into(), hash: "sha256:00".into(), open: "pick#1".into() },
        Shape::Live { frame, ui: "pick".into(), hash: hash(), open: "pick".into() },
        live(Rect::new(0.0, 0.0, -1.0, 10.0)),
    ];
    for shape in bad {
        let err = add(shape.clone()).unwrap_err();
        assert_eq!(err.code(), "invalid", "{shape:?}");
        assert!(err.hint().is_some());
    }
}

#[test]
fn a_live_item_is_one_add_line_with_its_box_name_hash_and_showing() {
    let (mut log, mut bytes) = log();
    let item = put_live(&mut log, &mut bytes);
    assert_eq!(item, "i2");
    let line = bytes.lines().nth(2).unwrap();
    assert_eq!(
        line,
        format!(
            r#"{{"n":2,"t":"2025-11-10T16:00:00.000Z","by":"agent:scout","op":"add","item":{{"id":"i2","page":"p1","kind":"live","x":1240,"y":0,"w":3200,"h":2400,"ui":"pick-slot","hash":"{}","open":"pick-slot#12"}}}}"#,
            hash()
        )
    );
    let file = read(bytes.as_bytes()).unwrap();
    assert_eq!(file.canvas().unwrap(), *log.canvas(), "the file replays to the same canvas");
    let moved = json!({"op": "move", "ids": ["i2"], "dx": 100, "dy": -50});
    log.apply(&Author::User, AT, &moved).unwrap();
    let item = log.canvas().item(slinger_core::canvas::ItemId::new(2)).unwrap();
    assert_eq!(item.bounds(), Rect::new(134.0, -5.0, 320.0, 240.0));
    assert!(matches!(item.shape(), Shape::Live { open, .. } if open == "pick-slot#12"));
}

#[test]
fn an_old_reader_skips_a_live_item_and_what_follows_still_applies() {
    let (mut log, mut text) = log();
    put_live(&mut log, &mut text);
    let text = text.replace(r#""kind":"live""#, r#""kind":"hologram""#);
    let more = r#"{"n":3,"t":"2025-11-10T16:00:00.000Z","by":"user","op":"move","ids":["i2"],"dx":10,"dy":0}"#;
    let bytes = format!("{text}{more}\n");
    let file = read(bytes.as_bytes()).unwrap();
    assert!(matches!(&file.lines[1], Line::Unknown { n: 2, .. }), "kept whole, not applied");
    assert_eq!(file.lines[1].encode(), format!("{}\n", text.lines().nth(2).unwrap()));
    let canvas = file.canvas().unwrap();
    let page = canvas.pages().next().unwrap().id();
    assert_eq!(canvas.items(page).count(), 1, "only the box: the move of i2 is a no-op");
}

#[test]
fn a_put_goes_beside_the_item_and_needs_one_there_first() {
    let (mut log, mut file) = log();
    let live = |w| Put::Live { ui: "pick".into(), hash: hash(), open: String::new(), w, h: 10 };
    let fits = live(10);
    assert_eq!(log.check_put("i9", &fits).unwrap_err().code(), "not_found");
    assert!(log.check_put("b1", &fits).is_ok(), "the showing is not known yet, nor checked");
    let flat = live(0);
    assert_eq!(log.check_put("i1", &flat).unwrap_err().code(), "invalid");
    assert_eq!(log.put(&scout(), AT, "i1", &flat).unwrap_err().code(), "invalid");
    assert_eq!(log.put(&scout(), AT, "i1", &fits).unwrap_err().code(), "invalid", "no showing");
    put_live(&mut log, &mut file);
    let undone = log.undo(&scout(), AT);
    assert_eq!(undone.seq.len(), 1, "the put is one undo step");
}

#[test]
fn an_agent_resizes_its_live_item_and_the_packet_says_it_as_text() {
    let (mut log, mut file) = log();
    put_live(&mut log, &mut file);
    let size = slinger_core::canvas::file::Edit { size: Some([4000, 3000]), ..Default::default() };
    let placed = log.edit(&scout(), AT, "l2", &size).unwrap();
    assert_eq!(placed.frame, [1240, 0, 4000, 3000]);
    let packet = log.packet("idea", None, 0, &["l2".into()], &[]).unwrap();
    let text = packet.text();
    assert!(
        text.contains("l2 live 124,0 400x300 by=scout ui=pick-slot open=pick-slot#12\n"),
        "{text}"
    );
    assert!(packet.images.is_empty(), "a live item is never an image");
    assert_eq!(log.crop("l2", "l2").unwrap().len(), 1, "a crop draws its box and name");
    assert_eq!(packet.selected, ["l2"]);
    let read = slinger_core::packet::Packet::read(&text).unwrap();
    let line = read.made.iter().find(|e| e.id == "l2").unwrap();
    assert_eq!((line.kind.as_str(), line.field("open")), ("live", Some("pick-slot#12")));
}
