//! What a script sees of the canvas round its figure through the real
//! socket (docs/design/figures.md, rule 5): `anchor`, the item it goes
//! beside or over, and `around`, the items near where it goes with
//! `--over` or `--at`, in figure coordinates; a redraw sees them again.
//! A failing script is `script` with its source line, a runaway one `limit`.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use slinger_control::{CanvasDraw, CanvasOps, CanvasRedraw, Client, Place};
use tempfile::TempDir;

use super::Daemon;

const LOOK: &str = "box(0, 0, 10, 10);\nwarn(JSON.stringify({anchor, around}));\n";

fn ask(name: &str, path: &Path, place: Place) -> CanvasDraw {
    CanvasDraw {
        canvas: "main".into(),
        by: "agent:scout".into(),
        name: name.into(),
        path: path.into(),
        data: None,
        place,
        page: None,
        dry_run: false,
        png: None,
        once: None,
    }
}

fn file(dir: &TempDir, name: &str, text: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, text).unwrap();
    path
}

/// The JSON the script warned with.
fn seen(warnings: &[String]) -> Value {
    let said = warnings.iter().find_map(|w| w.split_once(": ").map(|(_, m)| m)).unwrap();
    serde_json::from_str(said).unwrap()
}

fn ids(seen: &Value) -> Vec<&str> {
    seen["around"].as_array().unwrap().iter().map(|n| n["id"].as_str().unwrap()).collect()
}

/// A text i1 at (100, 100), a box b2 at (300, 300) 200x100 with a stroke
/// i3 in it, and a stroke i4 far away.
fn page(client: &Client) {
    let ops = json!([
        { "op": "add", "item": { "page": "p1", "kind": "text", "x": 1000, "y": 1000, "text": "hello" } },
        { "op": "add", "item": { "page": "p1", "kind": "box", "x": 3000, "y": 3000, "w": 2000, "h": 1000 } },
        { "op": "add", "item": { "page": "p1", "kind": "stroke", "width": 20, "pts": [3500, 3500, 4000, 3600] } },
        { "op": "add", "item": { "page": "p1", "kind": "stroke", "width": 20, "pts": [20000, 20000, 21000, 20000] } },
    ]);
    let ask = CanvasOps { canvas: "main".into(), by: "user".into(), ops, once: None };
    client.canvas_apply(ask).unwrap();
}

#[test]
fn a_script_sees_its_anchor_and_the_items_around_it_and_a_redraw_sees_them_again() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    page(&client);
    let dir = TempDir::new().unwrap();
    let look = file(&dir, "look.js", LOOK);

    let beside = client.canvas_draw(ask("note", &look, Place::Beside("i1".into()))).unwrap();
    let beside = seen(&beside.warnings);
    assert_eq!(
        (&beside["anchor"]["id"], &beside["anchor"]["kind"]),
        (&json!("i1"), &json!("text"))
    );
    assert_eq!(beside["anchor"]["text"], "hello");
    assert_eq!((beside["anchor"]["h"].as_f64(), beside["around"].clone()), (Some(24.0), json!([])));

    let over = client.canvas_draw(ask("marks", &look, Place::Over("b2".into()))).unwrap();
    let over = seen(&over.warnings);
    assert_eq!(
        over["anchor"],
        json!({"id": "b2", "kind": "box", "w": 200, "h": 100, "text": null})
    );
    // Nearest first: the stroke in the box, then the text and the note
    // figure beside it (i5); not the box itself, not the far stroke.
    let mut near = ids(&over);
    assert_eq!(near[0], "i3", "{over}");
    near.sort_unstable();
    assert_eq!(near, ["i1", "i3", "i5"]);
    assert_eq!(
        over["around"][0],
        json!({"id": "i3", "kind": "stroke", "x": 49, "y": 49, "w": 402, "h": 362}),
        "in the figure's own coordinates"
    );

    let at = client.canvas_draw(ask("plan", &look, Place::At([3000, 3000]))).unwrap();
    let at = seen(&at.warnings);
    assert_eq!(at["anchor"], Value::Null);
    // The box and the marks figure over it touch the point, then the stroke.
    assert_eq!(ids(&at)[..3], ["i2", "i6", "i3"], "{at}");
    assert_eq!(
        at["around"][0],
        json!({"id": "i2", "kind": "box", "x": 0, "y": 0, "w": 200, "h": 100})
    );

    let events = client
        .record_list(slinger_control::EventFilter {
            kind: Some("figure_drawn".into()),
            ..Default::default()
        })
        .unwrap();
    let anchors: Vec<&Value> = events.iter().map(|e| &e["payload"]["anchor"]).collect();
    assert_eq!(anchors, [&json!("i1"), &json!("b2"), &Value::Null]);

    let again = file(&dir, "again.js", &format!("box(0, 0, 20, 10);\n{}", &LOOK[19..]));
    let redraw = CanvasRedraw {
        canvas: "main".into(),
        by: "agent:scout".into(),
        name: "marks".into(),
        data: None,
        path: Some(again),
        once: None,
    };
    let drawn = client.canvas_redraw(redraw).unwrap();
    let redrawn = seen(&drawn.warnings);
    assert_eq!(redrawn["anchor"], over["anchor"], "the same anchor again");
    assert_eq!(ids(&redrawn)[0], "i3", "not its own box: {redrawn}");
    assert!(!ids(&redrawn).contains(&"i2") && !ids(&redrawn).contains(&"i6"), "{redrawn}");
    daemon.stop();
}

#[test]
fn a_failing_script_is_script_with_its_source_line_and_a_runaway_one_is_limit() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    let dir = TempDir::new().unwrap();
    let bad = file(&dir, "bad.js", "function row(y) {\n  box(0, y, 'wide', 10);\n}\nrow(5);\n");
    let e = client.canvas_draw(ask("f", &bad, Place::At([0, 0]))).unwrap_err();
    assert_eq!(e.code(), "script");
    let said = e.to_string();
    assert!(said.starts_with("script:2:"), "{said}");
    assert!(said.contains("\n  2 |   box(0, y, 'wide', 10);"), "{said}");
    assert!(
        said.contains("\n  at row (script:2:") && said.contains("at top level (script:4:1)"),
        "{said}"
    );
    assert!(e.hint().is_some_and(|h| h.contains("--dry-run")), "{e:?}");

    let spin = file(&dir, "spin.js", "while (true) {}\n");
    let e = client.canvas_draw(ask("f", &spin, Place::At([0, 0]))).unwrap_err();
    assert_eq!(e.code(), "limit");
    assert!(e.to_string().contains("ran longer than 1 s"), "{e}");
    assert!(e.hint().is_some_and(|h| h.contains("two figures")), "{e:?}");
    daemon.stop();
}
