//! Live items through the real socket: `ui_open` as a canvas item records
//! the showing and puts its item beside the one named, in one step; a
//! refused open records nothing and puts nothing.

use serde_json::{Value, json};
use slinger_control::{CanvasOps, CanvasStep, UiOpen};
use tempfile::TempDir;

use super::Daemon;
use super::session::open_with_agent;
use super::showing::{events, put};

fn ask(item: &str, size: Option<[i64; 2]>) -> UiOpen {
    UiOpen {
        ui: "pick".into(),
        agent: "scout".into(),
        host: "canvas".into(),
        data: json!({"slots": ["Tue"]}),
        canvas: Some("notes".into()),
        item: Some(item.into()),
        size,
        ..UiOpen::default()
    }
}

/// A daemon whose workspace `notes` has agent `scout`, UI `pick` and the
/// user's box `i1`; the UI's hash. The directories live as long as it.
fn notes() -> (Daemon, [TempDir; 3], String) {
    let dirs = [TempDir::new().unwrap(), TempDir::new().unwrap(), TempDir::new().unwrap()];
    let daemon = Daemon::start_in(dirs[0].path());
    let client = daemon.client();
    open_with_agent(&client, &dirs[1], "notes", "scout");
    let ui = put(&client, &dirs[2], "pick", "<p>pick</p>");
    let bx = json!({ "op": "add", "item": { "page": "p1", "kind": "box", "x": 0, "y": 0, "w": 1000, "h": 500 } });
    client.canvas_apply(CanvasOps { canvas: "notes".into(), by: "user".into(), ops: bx }).unwrap();
    (daemon, dirs, ui.hash)
}

#[test]
fn a_refused_live_item_records_nothing_and_puts_nothing() {
    let (daemon, _dirs, _) = notes();
    let client = daemon.client();
    let code = |open: UiOpen| client.ui_open(open).unwrap_err().code().to_string();
    let refused = [
        ask("i9", None),
        UiOpen { canvas: Some("gone".into()), ..ask("i1", None) },
        ask("i1", Some([0, 100])),
        UiOpen { item: None, ..ask("i1", None) },
    ];
    let codes: Vec<String> = refused.into_iter().map(code).collect();
    assert_eq!(codes, ["not_found", "not_found", "invalid", "invalid"]);
    assert_eq!(events(&client, "ui_opened"), Vec::<Value>::new(), "a refused open records nothing");
    assert_eq!(client.canvas_show("notes", None).unwrap().items.len(), 1);
    daemon.stop();
}

#[test]
fn an_agent_opens_a_ui_as_a_live_item_beside_an_item() {
    let (daemon, _dirs, hash) = notes();
    let client = daemon.client();
    let opened = client.ui_open(ask("i1", None)).unwrap();
    assert_eq!(opened.open, format!("pick#{}", opened.event));
    assert_eq!((opened.workspace.as_str(), opened.pane), ("notes", None));
    let item = opened.item.unwrap();
    assert_eq!(
        (item.canvas.as_str(), item.item.as_str(), item.page.as_str()),
        ("notes", "i2", "p1")
    );
    assert_eq!([item.x, item.y, item.w, item.h], [1240, 0, 3200, 2400]);
    let shown = client.canvas_show("notes", None).unwrap();
    let live = &shown.items[1];
    assert_eq!(
        (&live["kind"], &live["ui"], &live["hash"], &live["open"], &live["by"]),
        (&json!("live"), &json!("pick"), &json!(hash), &json!(opened.open), &json!("agent:scout"))
    );
    assert_eq!(events(&client, "ui_opened")[0]["payload"]["host"], "canvas");
    assert_eq!(client.pane_list("notes").unwrap().layout.panes().len(), 1, "no pane opens");

    let context = client.ui_shown(&opened.open).unwrap();
    assert_eq!(
        (context.data, context.agent.as_deref()),
        (json!({"slots": ["Tue"]}), Some("scout"))
    );
    assert!(!client.ui_reply(&opened.open, json!({"slot": "Tue"})).unwrap().delivered);
    assert_eq!(events(&client, "ui_reply").len(), 1);

    let undone =
        client.canvas_undo(CanvasStep { canvas: "notes".into(), by: "agent:scout".into() });
    assert!(undone.is_ok(), "the put is one undo step of its agent: {undone:?}");
    assert_eq!(client.canvas_show("notes", None).unwrap().items.len(), 1);
    daemon.stop();
}
