//! `canvas_import` through the real socket: the real board in, a stored
//! canvas out that survives a restart, and the codes for bad files.

use std::path::PathBuf;

use serde_json::json;
use tempfile::TempDir;

use super::Daemon;

fn board() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures/idea.tldraw")
}

#[test]
fn the_real_board_becomes_a_canvas_that_survives_a_restart() {
    let data = TempDir::new().unwrap();
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    let imported = client.canvas_import(&board(), None).unwrap();
    assert_eq!((imported.canvas.as_str(), imported.pages), ("idea", 5));
    let items = json!({ "stroke": 7563, "text": 125, "box": 80, "arrow": 57 });
    assert_eq!(serde_json::to_value(&imported.items).unwrap(), items);
    let skipped = json!({ "geo:diamond": 1, "geo:ellipse": 2, "group": 8, "line": 4 });
    assert_eq!(serde_json::to_value(&imported.skipped).unwrap(), skipped);

    let shown = client.canvas_show("idea", Some("spec")).unwrap();
    let names: Vec<_> = shown.pages.iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["inital-planning", "claude", "slinger-diagrams", "fable-5.2", "spec"]);
    assert_eq!(client.canvas_list().unwrap(), ["idea"]);
    // The canvas takes changes like any other.
    let add = json!({ "op": "add", "item": { "page": "p1", "kind": "box", "x": 0, "y": 0, "w": 1, "h": 1 } });
    let change = client
        .canvas_apply(slinger_control::CanvasOps {
            canvas: "idea".into(),
            by: "user".into(),
            ops: add,
        })
        .unwrap();
    assert_eq!(change.seq, [shown.next]);
    let before = client.canvas_show("idea", None).unwrap();
    daemon.stop();

    let daemon = Daemon::start_in(data.path());
    assert_eq!(daemon.client().canvas_show("idea", None).unwrap(), before);
    daemon.stop();
}

#[test]
fn a_name_missing_or_wrong_files_and_a_taken_name_get_their_codes() {
    let daemon = Daemon::start();
    let client = daemon.client();
    let named = client.canvas_import(&board(), Some("board")).unwrap();
    assert_eq!(named.canvas, "board");
    assert_eq!(client.canvas_import(&board(), Some("board")).unwrap_err().code(), "conflict");
    assert_eq!(client.canvas_import(&board(), Some("Board")).unwrap_err().code(), "invalid");

    let dir = TempDir::new().unwrap();
    let missing = client.canvas_import(&dir.path().join("gone.tldraw"), None).unwrap_err();
    assert_eq!(missing.code(), "not_found");
    assert!(missing.hint().is_some());
    let wrong = dir.path().join("notes.tldraw");
    std::fs::write(&wrong, "just some text").unwrap();
    let refused = client.canvas_import(&wrong, None).unwrap_err();
    assert_eq!(refused.code(), "invalid");
    assert!(refused.hint().is_some());
    assert_eq!(client.canvas_list().unwrap(), ["board"]);
    daemon.stop();
}

#[test]
fn a_raw_request_imports() {
    let daemon = Daemon::start();
    let line = json!({ "v": 1, "op": "canvas_import", "path": board(), "canvas": "raw" });
    let reply: serde_json::Value =
        serde_json::from_str(&daemon.raw(format!("{line}\n").as_bytes())).unwrap();
    assert_eq!(
        (&reply["ok"], &reply["canvas"], &reply["pages"]),
        (&true.into(), &"raw".into(), &5.into())
    );
    let relative = json!({ "v": 1, "op": "canvas_import", "path": "idea.tldraw" });
    let reply: serde_json::Value =
        serde_json::from_str(&daemon.raw(format!("{relative}\n").as_bytes())).unwrap();
    assert_eq!(reply["code"], "invalid");
    daemon.stop();
}
