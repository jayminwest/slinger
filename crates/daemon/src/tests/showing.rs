//! Showings through the real socket, with the stand-in for Claude Code: an
//! agent opens a UI in a pane with data, the pane names the showing, and
//! each reply is recorded and reaches the agent's session as one JSON line;
//! a reply for a session that does not run stays in the record.

use serde_json::{Value, json};
use slinger_control::{Client, EventFilter, UiOpen};
use tempfile::TempDir;

use super::Daemon;
use super::process::{fake_claude, wait_lines};
use super::session::open_with_agent;

fn put(client: &Client, dir: &TempDir, name: &str, html: &str) -> slinger_control::Ui {
    let file = dir.path().join(format!("{name}.html"));
    std::fs::write(&file, html).unwrap();
    client.ui_put(name, &file, "scout").unwrap().ui
}

fn ask(ui: &str, data: Value) -> UiOpen {
    UiOpen {
        ui: ui.into(),
        workspace: "notes".into(),
        agent: "scout".into(),
        host: "pane".into(),
        data,
        beside: None,
        below: false,
    }
}

fn events(client: &Client, kind: &str) -> Vec<Value> {
    client.record_list(EventFilter { kind: Some(kind.into()), ..EventFilter::default() }).unwrap()
}

/// What the pane target names: the showing `open`, with its version 1 and
/// `data`, or plainly the latest version, 2.
fn shows(client: &Client, open: &str, data: Value) {
    let shown = client.ui_shown(open).unwrap();
    assert_eq!((shown.ui.version, shown.open.as_deref()), (1, Some(open)));
    assert_eq!((shown.data, shown.agent.as_deref()), (data, Some("scout")));
    let plain = client.ui_shown("pick-slot").unwrap();
    assert_eq!((plain.ui.version, plain.open, plain.data), (2, None, Value::Null));
}

/// Sends two replies to the showing `open`, each taken by its running
/// session, and reads back the lines the stand-in at `home` heard.
fn reply_twice(client: &Client, open: &str, home: &std::path::Path) -> Vec<Value> {
    for slot in ["Tue 10:00", "Wed 14:00"] {
        let replied = client.ui_reply(open, json!({"slot": slot})).unwrap();
        assert_eq!((replied.agent.as_str(), replied.delivered), ("scout", true));
    }
    let heard = wait_lines(&home.join("in.txt"), 2);
    let content = |l: &String| {
        let message: Value = serde_json::from_str(l).unwrap();
        serde_json::from_str(message["message"]["content"].as_str().unwrap()).unwrap()
    };
    heard.iter().map(content).collect()
}

#[test]
fn an_agent_opens_a_ui_and_each_reply_reaches_its_session_as_one_json_line() {
    let (data, projects, bin) =
        (TempDir::new().unwrap(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_with_claude(data.path(), &fake_claude(bin.path()));
    let client = daemon.client();
    let home = open_with_agent(&client, &projects, "notes", "scout");
    let first = put(&client, &bin, "pick-slot", "<p>one</p>");
    put(&client, &bin, "pick-slot", "<p>two</p>");

    let slots = json!({"slots": ["Tue 10:00", "Wed 14:00"]});
    let opened = client.ui_open(ask("pick-slot@1", slots.clone())).unwrap();
    assert_eq!(opened.open, format!("pick-slot#{}", opened.event));
    assert_eq!((opened.ui.version, opened.ui.hash.as_str()), (1, first.hash.as_str()));
    assert_eq!((opened.workspace.as_str(), opened.pane), ("notes", 2));
    let panes = client.pane_list("notes").unwrap();
    assert_eq!(panes.layout.panes()[1].target, opened.open, "the pane names the showing");
    let recorded = &events(&client, "ui_opened")[0];
    assert_eq!(
        (recorded["tags"]["agent"].as_str(), recorded["tags"]["workspace"].as_str()),
        (Some("scout"), Some("notes"))
    );
    assert_eq!(recorded["payload"]["data"], slots);

    shows(&client, &opened.open, slots);

    client.session_start("notes").unwrap();
    let lines = reply_twice(&client, &opened.open, &home);
    let line = |slot| {
        json!({"v": 1, "kind": "ui_reply", "ui": "pick-slot", "version": 1,
               "hash": first.hash, "open": opened.open, "value": {"slot": slot}})
    };
    assert_eq!(lines, [line("Tue 10:00"), line("Wed 14:00")], "in order, one line each");
    let replies = events(&client, "ui_reply");
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[1]["payload"]["value"], json!({"slot": "Wed 14:00"}));
    assert_eq!(replies[1]["tags"]["agent"], "scout");

    client.session_stop("notes").unwrap();
    let kept = client.ui_reply(&opened.open, json!("late")).unwrap();
    assert!(!kept.delivered, "no session runs");
    assert_eq!(events(&client, "ui_reply")[2]["payload"]["value"], "late", "kept in the record");
    daemon.stop();
}

#[test]
fn a_refused_open_records_nothing_and_replies_are_bounded() {
    let (data, projects, bin) =
        (TempDir::new().unwrap(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    open_with_agent(&client, &projects, "notes", "scout");
    put(&client, &bin, "pick", "<p>");

    let code = |open: UiOpen| client.ui_open(open).unwrap_err().code().to_string();
    let refused = [
        ask("nope", Value::Null),
        UiOpen { workspace: "gone".into(), ..ask("pick", Value::Null) },
        UiOpen { beside: Some(9), ..ask("pick", Value::Null) },
        UiOpen { agent: " ".into(), ..ask("pick", Value::Null) },
        UiOpen { host: "canvas".into(), ..ask("pick", Value::Null) },
        ask("pick", json!("x".repeat(16384))),
    ];
    let codes: Vec<String> = refused.into_iter().map(code).collect();
    assert_eq!(codes, ["not_found", "not_found", "not_found", "invalid", "invalid", "invalid"]);
    assert_eq!(events(&client, "ui_opened"), Vec::<Value>::new());
    assert_eq!(client.pane_list("notes").unwrap().layout.panes().len(), 1);

    let open = client.ui_open(ask("pick", Value::Null)).unwrap().open;
    for _ in 0..100 {
        assert!(!client.ui_reply(&open, json!(1)).unwrap().delivered, "never started");
    }
    let limit = client.ui_reply(&open, json!(1)).unwrap_err();
    assert_eq!(limit.code(), "invalid");
    assert!(limit.hint().is_some());
    assert_eq!(client.ui_reply(&open, json!("x".repeat(16384))).unwrap_err().code(), "invalid");
    assert_eq!(events(&client, "ui_reply").len(), 100);

    let reply = |open: &str| client.ui_reply(open, json!(1)).unwrap_err().code().to_string();
    let codes: Vec<String> = ["pick#1000", "pick#1", "pick"].map(reply).into();
    assert_eq!(codes, ["not_found", "not_found", "invalid"], "missing, a put, a reference");
    assert_eq!(client.ui_shown("pick#1000").unwrap_err().code(), "not_found");
    daemon.stop();
}
