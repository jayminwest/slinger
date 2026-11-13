//! The "send to agent" button (SPEC section 8): a built-in item on a
//! canvas. It needs no blob and no `ui put`: a `canvas apply` of one `add`
//! places it. The model's rules, the canvas file's bytes, the packet's text
//! line and a crop.
#![cfg(test)]

use serde_json::json;
use slinger_core::canvas::file::{Log, Put, read};
use slinger_core::canvas::{Action, Author, Canvas, Op, Rect, Shape, Time};

const AT: Time = Time(1_762_790_400_000);

/// A log whose page holds the user's button `i1`, and its file so far.
fn log() -> (Log, String) {
    let (mut log, mut file) = Log::create("idea").unwrap();
    let button = json!({ "op": "add", "item": { "page": "p1", "kind": "button", "x": 0, "y": 0, "w": 1600, "h": 400 } });
    file += &log.apply(&Author::User, AT, &button).unwrap().bytes;
    (log, file)
}

#[test]
fn a_button_needs_an_area_and_nothing_else() {
    let mut c = Canvas::new();
    let page = c.pages().next().unwrap().id();
    let mut add = |frame| {
        let shape = Shape::Button { frame };
        c.apply(Op { by: Author::User, at: AT, action: Action::Add { page, shape } })
    };
    assert!(add(Rect::new(1.0, 2.0, 160.0, 40.0)).is_ok());
    for frame in [Rect::new(0.0, 0.0, 0.0, 40.0), Rect::new(0.0, 0.0, 10.0, -1.0)] {
        let err = add(frame).unwrap_err();
        assert_eq!(err.code(), "invalid", "{frame:?}");
        assert!(err.hint().is_some());
    }
}

#[test]
fn a_button_is_one_add_line_with_its_box_and_reads_back() {
    let (mut log, bytes) = log();
    let line = bytes.lines().nth(1).unwrap();
    assert_eq!(
        line,
        r#"{"n":1,"t":"2025-11-10T16:00:00.000Z","by":"user","op":"add","item":{"id":"i1","page":"p1","kind":"button","x":0,"y":0,"w":1600,"h":400},"step":1}"#
    );
    let file = read(bytes.as_bytes()).unwrap();
    assert_eq!(file.canvas().unwrap(), *log.canvas(), "the file replays to the same canvas");
    let moved = json!({"op": "move", "ids": ["i1"], "dx": 100, "dy": -50});
    log.apply(&Author::User, AT, &moved).unwrap();
    let item = log.canvas().item(slinger_core::canvas::ItemId::new(1)).unwrap();
    assert_eq!(item.shape(), &Shape::Button { frame: Rect::new(10.0, -5.0, 160.0, 40.0) });
}

#[test]
fn the_packet_says_a_button_as_text_and_an_agent_puts_beside_it() {
    let (mut log, _) = log();
    let packet = log.packet("idea", None, 0, &["u1".into()], &[]).unwrap();
    let text = packet.text();
    assert!(text.contains("u1 button 0,0 160x40\n"), "{text}");
    assert!(packet.images.is_empty(), "a button is never an image");
    assert_eq!(packet.selected, ["u1"]);
    assert_eq!(log.crop("u1", "u1").unwrap().len(), 1, "a crop draws its box and label");
    let scout = Author::Agent("scout".into());
    let placed = log.put(&scout, AT, "u1", &Put::Text("on it".into())).unwrap();
    assert!(placed.frame[0] >= 1600, "beside it, not on it: {:?}", placed.frame);
    let removed = log.remove(&scout, AT, "u1");
    assert_eq!(removed.unwrap_err().code(), "invalid", "an agent cannot take the user's button");
}
