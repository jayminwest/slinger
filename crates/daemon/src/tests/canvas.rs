//! Canvases through the real socket: files that survive a restart, one
//! writer that numbers concurrent changes without a gap, and damage on disk.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use slinger_control::{CanvasOps, CanvasStep};
use tempfile::TempDir;

use super::Daemon;

fn ops(canvas: &str, by: &str, ops: Value) -> CanvasOps {
    CanvasOps { canvas: canvas.into(), by: by.into(), ops, once: None }
}

fn bx(x: i64) -> Value {
    json!({ "op": "add", "item": { "page": "p1", "kind": "box", "x": x, "y": 0, "w": 10, "h": 10 } })
}

fn user(canvas: &str) -> CanvasStep {
    CanvasStep { canvas: canvas.into(), by: "user".into() }
}

#[test]
fn a_canvas_survives_a_restart_with_every_item_and_page() {
    let data = TempDir::new().unwrap();
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    let page = json!({ "op": "page.add", "name": "notes" });
    let change = client.canvas_apply(ops("main", "user", json!([bx(0), page]))).unwrap();
    assert_eq!((change.seq, change.made), (vec![1, 2], vec!["i1".to_string(), "p2".into()]));
    let agent = client.canvas_apply(ops("main", "agent:ada", bx(50))).unwrap();
    assert_eq!(agent.seq, [3]);
    let before = client.canvas_show("main", None).unwrap();
    daemon.stop();

    let daemon = Daemon::start_in(data.path());
    let after = daemon.client().canvas_show("main", None).unwrap();
    assert_eq!(after, before);
    assert_eq!(after.next, 4);
    assert_eq!(after.items.len(), 2);
    assert_eq!(after.items[1]["by"], "agent:ada");
    assert_eq!(after.pages[1], json!({ "id": "p2", "name": "notes" }));
    assert_eq!(daemon.client().canvas_list().unwrap(), ["main"]);
    daemon.stop();
}

#[test]
fn a_change_sent_again_under_its_key_is_applied_once_and_answered_the_same() {
    let data = TempDir::new().unwrap();
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    let once = |x| CanvasOps { once: Some("app-1.7".into()), ..ops("main", "user", bx(x)) };
    let first = client.canvas_apply(once(0)).unwrap();
    let again = client.canvas_apply(once(0)).unwrap();
    assert_eq!((&again.seq, &again.made), (&first.seq, &first.made));
    let other = CanvasOps { once: Some("app-1.8".into()), ..ops("main", "user", bx(0)) };
    assert_eq!(client.canvas_apply(other).unwrap().made, ["i2"]);
    let shown = client.canvas_show("main", None).unwrap();
    assert_eq!((shown.items.len(), shown.next), (2, 3), "written once");
    daemon.stop();
}

#[test]
fn a_shown_canvas_names_the_file_that_holds_it() {
    let data = TempDir::new().unwrap();
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    let path = client.canvas_show("main", None).unwrap().path.unwrap();
    assert_eq!(std::path::Path::new(&path), data.path().join("canvases/main.canvas"));
    assert!(std::fs::metadata(&path).unwrap().is_file());
    daemon.stop();
}

#[test]
fn clients_that_apply_at_once_all_succeed_and_the_numbers_have_no_gap() {
    let data = TempDir::new().unwrap();
    let daemon = Daemon::start_in(data.path());
    daemon.client().canvas_create("busy").unwrap();
    let (clients, each) = (8, 10);
    let seqs: Vec<u64> = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..clients)
            .map(|c| {
                let client = daemon.client();
                scope.spawn(move || {
                    (0..each)
                        .flat_map(|i| {
                            let change = client.canvas_apply(ops("busy", "user", bx(c * 100 + i)));
                            change.unwrap().seq
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        threads.into_iter().flat_map(|t| t.join().unwrap()).collect()
    });
    let total = u64::try_from(clients * each).unwrap();
    assert_eq!(seqs.iter().copied().collect::<BTreeSet<_>>(), (1..=total).collect());
    assert_eq!(seqs.len(), usize::try_from(total).unwrap());
    daemon.stop();

    let daemon = Daemon::start_in(data.path());
    let shown = daemon.client().canvas_show("busy", None).unwrap();
    assert_eq!((shown.items.len(), shown.next), (usize::try_from(total).unwrap(), total + 1));
    daemon.stop();
}

#[test]
fn undo_and_redo_are_written_like_any_change() {
    let data = TempDir::new().unwrap();
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    client.canvas_apply(ops("main", "user", bx(0))).unwrap();
    assert_eq!(client.canvas_undo(user("main")).unwrap().seq, [2]);
    assert_eq!(client.canvas_undo(user("main")).unwrap().seq, [0_u64; 0]);
    assert_eq!(client.canvas_redo(user("main")).unwrap().seq, [3]);
    daemon.stop();

    let daemon = Daemon::start_in(data.path());
    let shown = daemon.client().canvas_show("main", None).unwrap();
    assert_eq!((shown.items.len(), shown.next), (1, 4));
    // Undo is not on disk: after a restart there is nothing to undo.
    assert_eq!(daemon.client().canvas_undo(user("main")).unwrap().seq, [0_u64; 0]);
    daemon.stop();
}

#[test]
fn unknown_taken_and_bad_canvases_get_their_codes() {
    let daemon = Daemon::start();
    let client = daemon.client();
    assert_eq!(client.canvas_list().unwrap(), Vec::<String>::new());
    let missing = client.canvas_show("nope", None).unwrap_err();
    assert_eq!(missing.code(), "not_found");
    assert!(missing.hint().unwrap().contains("canvas create nope"));
    assert_eq!(client.canvas_apply(ops("nope", "user", bx(0))).unwrap_err().code(), "not_found");
    assert_eq!(client.canvas_undo(user("nope")).unwrap_err().code(), "not_found");
    client.canvas_create("b").unwrap();
    client.canvas_create("a").unwrap();
    assert_eq!(client.canvas_create("a").unwrap_err().code(), "conflict");
    assert_eq!(client.canvas_list().unwrap(), ["a", "b"]);
    for id in ["../a", "A", ""] {
        assert_eq!(client.canvas_create(id).unwrap_err().code(), "invalid", "{id}");
        assert_eq!(client.canvas_show(id, None).unwrap_err().code(), "invalid", "{id}");
    }
    assert_eq!(client.canvas_show("a", Some("p9")).unwrap_err().code(), "not_found");
    assert_eq!(client.canvas_apply(ops("a", "robot", bx(0))).unwrap_err().code(), "invalid");
    let theirs = client.canvas_apply(ops("a", "user", bx(0))).unwrap();
    let remove = json!({ "op": "remove", "ids": theirs.made });
    let refused = client.canvas_apply(ops("a", "agent:ada", remove)).unwrap_err();
    assert_eq!(refused.code(), "invalid");
    assert!(refused.hint().is_some());
    daemon.stop();
}

#[test]
fn a_torn_last_line_is_cut_and_a_damaged_file_is_refused() {
    let data = TempDir::new().unwrap();
    let dir = data.path().join("canvases");
    std::fs::create_dir_all(&dir).unwrap();
    let header = "{\"slinger\":\"canvas\",\"v\":1,\"id\":\"torn\",\"first\":1}\n";
    std::fs::write(dir.join("torn.canvas"), format!("{header}{{\"n\":1,\"t\":")).unwrap();
    std::fs::write(dir.join("bad.canvas"), "not a canvas\n").unwrap();
    std::fs::write(
        dir.join("new.canvas"),
        "{\"slinger\":\"canvas\",\"v\":9,\"id\":\"new\",\"first\":1}\n",
    )
    .unwrap();
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    assert_eq!(client.canvas_apply(ops("torn", "user", bx(0))).unwrap().seq, [1]);
    let file = std::fs::read_to_string(dir.join("torn.canvas")).unwrap();
    assert!(file.starts_with(header) && file.ends_with("}}\n"), "{file}");
    assert_eq!(file.lines().count(), 2);
    assert_eq!(client.canvas_show("bad", None).unwrap_err().code(), "canvas_corrupt");
    assert_eq!(client.canvas_show("new", None).unwrap_err().code(), "canvas_too_new");
    assert_eq!(client.canvas_list().unwrap(), ["bad", "new", "torn"]);
    daemon.stop();
}

#[test]
fn a_raw_request_reaches_the_canvas() {
    let daemon = Daemon::start();
    let reply = |line: Value| -> Value {
        serde_json::from_str(&daemon.raw(format!("{line}\n").as_bytes())).unwrap()
    };
    let created = reply(json!({ "v": 1, "op": "canvas_create", "canvas": "main" }));
    assert_eq!(created, json!({ "ok": true, "canvas": "main" }));
    let applied = reply(json!({ "v": 1, "op": "canvas_apply", "canvas": "main", "ops": [bx(0)] }));
    assert_eq!(applied, json!({ "ok": true, "seq": [1], "made": ["i1"] }));
    let listed = reply(json!({ "v": 1, "op": "canvas_list" }));
    assert_eq!(listed, json!({ "ok": true, "canvases": [{ "canvas": "main" }] }));
    daemon.stop();
}

#[test]
fn a_canvas_made_from_a_file_of_operations_is_made_whole_or_not_at_all() {
    let data = TempDir::new().unwrap();
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    let file = data.path().join("ops.json");
    // More than one request line holds: the file is read on this machine.
    let many: Vec<Value> = (0..1000).map(bx).collect();
    std::fs::write(&file, Value::Array(many.clone()).to_string()).unwrap();
    client.canvas_create_with("big", &file).unwrap();
    let shown = client.canvas_show("big", None).unwrap();
    assert_eq!((shown.page.as_str(), shown.items.len(), shown.next), ("p1", 1000, 1001));
    assert_eq!(shown.items[999]["by"], "user");
    assert_eq!(client.canvas_create_with("big", &file).unwrap_err().code(), "conflict");

    let mut torn = many;
    torn.push(json!({ "op": "add", "item": { "page": "p9", "kind": "box", "x": 0, "y": 0, "w": 1, "h": 1 } }));
    std::fs::write(&file, Value::Array(torn).to_string()).unwrap();
    assert_eq!(client.canvas_create_with("torn", &file).unwrap_err().code(), "not_found");
    std::fs::write(&file, "[{").unwrap();
    assert_eq!(client.canvas_create_with("torn", &file).unwrap_err().code(), "invalid");
    let gone = data.path().join("gone.json");
    assert_eq!(client.canvas_create_with("torn", &gone).unwrap_err().code(), "not_found");
    assert_eq!(client.canvas_list().unwrap(), ["big"], "no partial canvas");
    daemon.stop();
    let daemon = Daemon::start_in(data.path());
    assert_eq!(daemon.client().canvas_show("big", None).unwrap().items.len(), 1000);
    daemon.stop();
}
