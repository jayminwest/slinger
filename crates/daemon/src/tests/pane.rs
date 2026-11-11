//! The pane verbs through the real socket: open, close, resize and list, the
//! `pane_moved` each change appends, a close of a pane already gone, and
//! the layout after a restart.

use serde_json::json;
use slinger_control::{Client, EventFilter, PaneKind, PaneLayout};
use tempfile::TempDir;

use super::Daemon;

/// The panes as `number kind:target` words.
fn words(layout: &PaneLayout) -> Vec<String> {
    let panes = layout.layout.panes();
    panes.iter().map(|p| format!("{} {:?}:{}", p.pane, p.kind, p.target)).collect()
}

fn moved(client: &Client) -> Vec<serde_json::Value> {
    let filter = EventFilter { kind: Some("pane_moved".into()), ..EventFilter::default() };
    client.record_list(filter).unwrap()
}

#[test]
fn panes_open_close_and_list_and_survive_a_restart() {
    let data = TempDir::new().unwrap();
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    client.workspace_open(None, Some("Notes")).unwrap();
    let first = client.pane_list("Notes").unwrap();
    assert_eq!((first.workspace.as_str(), first.pane, first.event), ("Notes", None, None));
    assert_eq!(words(&first), ["1 Canvas:notes"]);

    open_two(&client);
    let closed = close_twice(&client);
    check_moved(&client);

    let dir = client.workspace_show("Notes").unwrap().dir;
    let file = std::fs::read_to_string(dir.join("slinger-workspace.toml")).unwrap();
    assert!(file.contains("panes = \"row 0.5 canvas:notes canvas:board\""), "{file}");
    daemon.stop();

    let daemon = Daemon::start_in(data.path());
    assert_eq!(words(&daemon.client().pane_list("Notes").unwrap()), words(&closed));
    daemon.stop();
}

/// Closes pane 2 of three, then pane 3, already gone.
fn close_twice(client: &Client) -> PaneLayout {
    let closed = client.pane_close("Notes", 2).unwrap();
    assert!(closed.event.is_some());
    assert_eq!(words(&closed), ["1 Canvas:notes", "2 Canvas:board"]);
    let again = client.pane_close("Notes", 3).unwrap();
    assert_eq!((again.event, words(&again)), (None, words(&closed)), "already gone");
    closed
}

/// Opens a canvas to the right of pane 1, then one below it.
fn open_two(client: &Client) {
    client.canvas_create("board").unwrap();
    let opened = client.pane_open("Notes", PaneKind::Canvas, "board", Some(1), false).unwrap();
    assert_eq!(opened.pane, Some(2));
    assert_eq!(words(&opened), ["1 Canvas:notes", "2 Canvas:board"]);
    let below = client.pane_open("Notes", PaneKind::Canvas, "notes", Some(1), true).unwrap();
    assert_eq!(below.pane, Some(2));
    assert_eq!(words(&below), ["1 Canvas:notes", "2 Canvas:notes", "3 Canvas:board"]);
    let shape = serde_json::to_value(&below.layout).unwrap();
    assert_eq!(shape["split"], "row");
    assert_eq!(shape["first"]["split"], "column");
    assert_eq!(shape["second"], json!({ "pane": 3, "kind": "canvas", "target": "board" }));
}

/// The three changes' `pane_moved`, tagged with the workspace.
fn check_moved(client: &Client) {
    let events = moved(client);
    assert_eq!(events.len(), 3);
    assert_eq!(events[0]["tags"]["workspace"], "Notes");
    assert_eq!(
        events[0]["payload"],
        json!({ "pane": "canvas:board", "x": 0.5, "y": 0.0, "width": 0.5, "height": 1.0 })
    );
    assert_eq!(events[2]["payload"]["pane"], "canvas:notes");
    assert_eq!(events[2]["payload"]["width"], 0.0);
}

#[test]
fn a_pane_shows_only_what_is_there() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.workspace_open(None, Some("w")).unwrap();
    let code =
        |r: Result<PaneLayout, slinger_control::RemoteError>| r.unwrap_err().code().to_string();
    assert_eq!(code(client.pane_open("w", PaneKind::Canvas, "nope", None, false)), "not_found");
    assert_eq!(code(client.pane_open("w", PaneKind::Ui, "board", None, false)), "not_found");
    assert_eq!(code(client.pane_open("w", PaneKind::Canvas, "w", Some(2), false)), "not_found");
    assert_eq!(code(client.pane_list("other")), "not_found");
    assert_eq!(code(client.pane_close("other", 1)), "not_found");

    let html = TempDir::new().unwrap();
    let file = html.path().join("board.html");
    std::fs::write(&file, "<p>hi</p>").unwrap();
    client.ui_put("board", &file, "scout").unwrap();
    let opened = client.pane_open("w", PaneKind::Ui, "board", None, false).unwrap();
    assert_eq!(words(&opened), ["1 Canvas:w", "2 Ui:board"]);
    let last = client.pane_close("w", 1).unwrap();
    assert_eq!(words(&last), ["1 Ui:board"]);
    let kept = client.pane_close("w", 1).unwrap();
    assert_eq!((kept.event, words(&kept)), (None, words(&last)), "the last pane stays");
    assert_eq!(moved(&client).len(), 2);
    daemon.stop();
}

#[test]
fn a_resize_moves_a_border_records_it_and_survives_a_restart() {
    let data = TempDir::new().unwrap();
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    client.workspace_open(None, Some("w")).unwrap();
    client.pane_open("w", PaneKind::Canvas, "w", Some(1), false).unwrap();
    let resized = client.pane_resize("w", 1, 0.3).unwrap();
    let shape = serde_json::to_value(&resized.layout).unwrap();
    assert_eq!((shape["split"].as_str(), shape["ratio"].as_f64()), (Some("row"), Some(0.3)));
    assert!(resized.event.is_some());
    let events = moved(&client);
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[1]["payload"],
        json!({ "pane": "canvas:w", "x": 0.0, "y": 0.0, "width": 0.3, "height": 1.0 })
    );

    for (pane, ratio) in [(1, 0.3), (2, 0.6), (9, 0.6)] {
        let same = client.pane_resize("w", pane, ratio).unwrap();
        assert_eq!((same.event, &same.layout), (None, &resized.layout), "{pane} {ratio}");
    }
    let code = client.pane_resize("other", 1, 0.5).unwrap_err().code().to_string();
    assert_eq!(code, "not_found");
    assert_eq!(moved(&client).len(), 2, "nothing changed, nothing recorded");
    daemon.stop();

    let daemon = Daemon::start_in(data.path());
    assert_eq!(daemon.client().pane_list("w").unwrap().layout, resized.layout);
    daemon.stop();
}
