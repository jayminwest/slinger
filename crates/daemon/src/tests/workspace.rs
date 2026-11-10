//! Workspaces through the real socket: a directory opened with its file and
//! its canvas, one with no root, a restart, the rules over the open ones, a
//! close that keeps the file, and the workspace tag on new events.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use slinger_control::{NewEvent, NewTags, Workspace};
use tempfile::TempDir;

use super::Daemon;

const FILE: &str = "slinger-workspace.toml";

/// A new directory `name` inside `parent`, as the daemon names it (links resolved).
fn project(parent: &TempDir, name: &str) -> PathBuf {
    let dir = parent.path().join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn opening_a_directory_makes_its_file_and_canvas_and_survives_a_restart() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let dir = project(&projects, "Notes");
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    let opened = client.workspace_open(Some(&dir), None).unwrap();
    assert!(opened.new);
    let ws = opened.workspace;
    assert_eq!(
        (ws.name.as_str(), ws.tag.as_str(), ws.canvas.as_str()),
        ("Notes", "Notes", "notes")
    );
    assert_eq!(ws.dir, dir);
    assert_eq!(ws.roots.len(), 1);
    assert!(ws.roots[0].primary);
    assert_eq!(
        (ws.roots[0].machine.as_str(), ws.roots[0].path.as_str()),
        ("test-machine", text(&dir))
    );
    let file = std::fs::read_to_string(dir.join(FILE)).unwrap();
    assert!(file.contains("name = \"Notes\"") && file.contains("primary = true"), "{file}");
    assert_eq!(client.canvas_list().unwrap(), ["notes"]);
    assert_eq!(client.canvas_show("notes", None).unwrap().items.len(), 0);

    let again = client.workspace_open(Some(&dir.join("..").join("Notes")), None).unwrap();
    assert_eq!((again.new, &again.workspace), (false, &ws));
    assert_eq!(client.workspace_list().unwrap(), std::slice::from_ref(&ws));
    daemon.stop();
    still_open_after_a_restart(data.path(), &dir, &ws);
}

fn still_open_after_a_restart(data: &Path, dir: &Path, ws: &Workspace) {
    let daemon = Daemon::start_in(data);
    let client = daemon.client();
    assert_eq!(client.workspace_list().unwrap(), std::slice::from_ref(ws));
    assert_eq!(&client.workspace_show(&ws.name).unwrap(), ws);
    assert_eq!(&client.workspace_show(&format!("{}/", dir.display())).unwrap(), ws);
    assert!(!client.workspace_open(Some(dir), None).unwrap().new);
    assert_eq!(client.canvas_list().unwrap(), [ws.canvas.as_str()]);
    daemon.stop();
}

#[test]
fn a_workspace_with_no_root_lives_in_the_data_directory() {
    let data = TempDir::new().unwrap();
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    let opened = client.workspace_open(None, Some("Scratch Pad")).unwrap();
    assert!(opened.new);
    let ws = opened.workspace;
    assert_eq!((ws.name.as_str(), ws.canvas.as_str()), ("Scratch Pad", "scratch-pad"));
    assert_eq!(ws.roots, []);
    assert_eq!(ws.dir, data.path().join("workspaces/scratch-pad"));
    assert!(ws.dir.join(FILE).is_file());
    assert_eq!(client.canvas_list().unwrap(), ["scratch-pad"]);
    assert!(!client.workspace_open(None, Some("Scratch Pad")).unwrap().new);

    client.workspace_close("Scratch Pad").unwrap();
    assert_eq!(client.workspace_list().unwrap(), []);
    let reopened = client.workspace_open(None, Some("Scratch Pad")).unwrap();
    assert_eq!((reopened.new, reopened.workspace), (false, ws));
    daemon.stop();
}

#[test]
fn a_second_workspace_on_one_primary_root_is_a_conflict() {
    let projects = TempDir::new().unwrap();
    let (first, other) = (project(&projects, "first"), project(&projects, "other"));
    let daemon = Daemon::start();
    let client = daemon.client();
    client.workspace_open(Some(&first), None).unwrap();
    let claims_first = format!(
        "version = 1\nname = \"other\"\n[[root]]\nmachine = \"test-machine\"\npath = \"{}\"\nprimary = true\n",
        first.display()
    );
    std::fs::write(other.join(FILE), &claims_first).unwrap();
    let err = client.workspace_open(Some(&other), None).unwrap_err();
    assert_eq!(err.code(), "conflict");
    assert!(err.to_string().contains("\"first\""), "{err}");
    assert!(err.hint().unwrap().contains("\"first\""));
    assert_eq!(client.workspace_list().unwrap().len(), 1);
    assert_eq!(std::fs::read_to_string(other.join(FILE)).unwrap(), claims_first);
    daemon.stop();
}

#[test]
fn a_name_or_a_canvas_taken_by_an_open_workspace_is_a_conflict() {
    let (a, b) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let (one, two) = (project(&a, "notes"), project(&b, "notes"));
    let daemon = Daemon::start();
    let client = daemon.client();
    client.workspace_open(Some(&one), None).unwrap();
    let err = client.workspace_open(Some(&two), None).unwrap_err();
    assert_eq!(err.code(), "conflict");
    assert!(!two.join(FILE).exists(), "a refused open writes no file");
    let renamed = client.workspace_open(Some(&two), Some("notes two")).unwrap();
    assert_eq!(renamed.workspace.canvas, "notes-two");
    let wrong_name = client.workspace_open(Some(&two), Some("other")).unwrap_err();
    assert_eq!(wrong_name.code(), "conflict");
    assert!(wrong_name.hint().unwrap().contains("\"notes two\""));
    daemon.stop();
}

#[test]
fn closing_keeps_the_file_and_the_canvas() {
    let projects = TempDir::new().unwrap();
    let dir = project(&projects, "keep");
    let daemon = Daemon::start();
    let client = daemon.client();
    client.workspace_open(Some(&dir), None).unwrap();
    assert_eq!(client.workspace_close("keep").unwrap(), dir);
    assert!(dir.join(FILE).is_file());
    assert_eq!(client.canvas_list().unwrap(), ["keep"]);
    assert_eq!(client.workspace_list().unwrap(), []);
    assert_eq!(client.workspace_show("keep").unwrap_err().code(), "not_found");
    let gone = client.workspace_close("keep").unwrap_err();
    assert_eq!(gone.code(), "not_found");
    assert!(gone.hint().unwrap().contains("workspace list"));
    assert!(!client.workspace_open(Some(&dir), None).unwrap().new);
    daemon.stop();
}

#[test]
fn a_broken_file_fails_the_list_and_can_still_be_closed() {
    let projects = TempDir::new().unwrap();
    let dir = project(&projects, "fragile");
    let daemon = Daemon::start();
    let client = daemon.client();
    client.workspace_open(Some(&dir), None).unwrap();
    std::fs::write(dir.join(FILE), "not a workspace").unwrap();
    let err = client.workspace_list().unwrap_err();
    assert_eq!(err.code(), "invalid");
    assert!(err.to_string().contains(FILE), "{err}");
    assert!(err.hint().unwrap().contains("workspace close"), "{:?}", err.hint());
    assert_eq!(client.workspace_show("fragile").unwrap_err().code(), "not_found");
    assert_eq!(client.workspace_close(text(&dir)).unwrap(), dir);
    assert_eq!(client.workspace_list().unwrap(), []);
    daemon.stop();
}

#[test]
fn bad_requests_to_open_get_their_codes() {
    let projects = TempDir::new().unwrap();
    let file = projects.path().join("plain.txt");
    std::fs::write(&file, "").unwrap();
    let daemon = Daemon::start();
    let client = daemon.client();
    assert_eq!(client.workspace_open(None, None).unwrap_err().code(), "invalid");
    let missing = projects.path().join("missing");
    assert_eq!(client.workspace_open(Some(&missing), None).unwrap_err().code(), "not_found");
    assert_eq!(client.workspace_open(Some(&file), None).unwrap_err().code(), "invalid");
    assert_eq!(client.workspace_open(None, Some("  ")).unwrap_err().code(), "invalid");
    let reply = daemon.raw(b"{\"v\":1,\"op\":\"workspace_open\",\"dir\":\"relative\"}\n");
    assert!(reply.contains("\"code\":\"invalid\""), "{reply}");
    daemon.stop();
}

#[test]
fn new_events_carry_the_tag_of_their_workspace() {
    let projects = TempDir::new().unwrap();
    let dir = project(&projects, "tagged");
    std::fs::write(
        dir.join(FILE),
        "version = 1\nname = \"tagged\"\ntag = \"t1\"\n[agent]\nname = \"scout\"\n",
    )
    .unwrap();
    let daemon = Daemon::start();
    let client = daemon.client();
    let append = |tags: NewTags| {
        let new = NewEvent {
            kind: "agent_done".into(),
            source: "scout".into(),
            tags,
            payload: json!({}),
        };
        client.record_append(new).unwrap()["tags"]["workspace"].clone()
    };
    let on_canvas = || NewTags { canvas: Some("main".into()), ..NewTags::default() };
    assert_eq!(append(on_canvas()), Value::Null, "no workspace is open yet");
    assert_eq!(client.workspace_open(Some(&dir), None).unwrap().workspace.canvas, "main");
    assert_eq!(append(on_canvas()), "t1");
    assert_eq!(append(NewTags { agent: Some("scout".into()), ..NewTags::default() }), "t1");
    assert_eq!(append(NewTags { agent: Some("nobody".into()), ..NewTags::default() }), Value::Null);
    let set = NewTags { workspace: Some("mine".into()), ..on_canvas() };
    assert_eq!(append(set), "mine");
    daemon.stop();
}

#[test]
fn a_raw_request_opens_and_lists_a_workspace() {
    let data = TempDir::new().unwrap();
    let daemon = Daemon::start_in(data.path());
    let reply = |line: Value| -> Value {
        serde_json::from_str(&daemon.raw(format!("{line}\n").as_bytes())).unwrap()
    };
    let opened = reply(json!({ "v": 1, "op": "workspace_open", "name": "loose" }));
    let dir = data.path().join("workspaces/loose");
    let ws = json!({ "name": "loose", "tag": "loose", "canvas": "loose", "dir": dir, "roots": [] });
    assert_eq!(opened, json!({ "ok": true, "workspace": ws, "new": true }));
    let listed = reply(json!({ "v": 1, "op": "workspace_list" }));
    assert_eq!(listed, json!({ "ok": true, "workspaces": [ws] }));
    let closed = reply(json!({ "v": 1, "op": "workspace_close", "workspace": "loose" }));
    assert_eq!(closed, json!({ "ok": true, "dir": dir }));
    daemon.stop();
}
