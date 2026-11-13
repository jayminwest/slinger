//! `session_listen` through the real socket: a sling waits for a listener,
//! the one that asks takes it as text with its images as files, a listener
//! outranks a running process, and a listener that is gone, its reply
//! unread or its process ended, loses nothing.

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::json;
use slinger_control::{CanvasOps, Client, EventFilter, Profile, Put, Session, SlingAsk, Slung};
use tempfile::TempDir;

use super::Daemon;
use super::process::{fake_claude, wait_for, wait_lines};
use super::session::open_with_agent;

const NONE: Duration = Duration::ZERO;
const LONG: Duration = Duration::from_secs(30);

/// Draws one stroke of the user's at `x` on the canvas `notes`.
fn draw(client: &Client, x: i64) {
    let item = json!({ "page": "p1", "kind": "stroke", "width": 20,
        "pts": [x, 0, 100, 50, 100, -50] });
    let ops = json!({ "op": "add", "item": item });
    let ops = CanvasOps { canvas: "notes".into(), by: "user".into(), ops, once: None };
    client.canvas_apply(ops).unwrap();
}

fn sling(client: &Client, prompt: &str) -> Slung {
    let ask = SlingAsk { canvas: "notes".into(), prompt: prompt.into(), ..SlingAsk::default() };
    client.sling_send(ask).unwrap()
}

/// What a session says of its listener: deliver, listening, pending.
fn heard(s: &Session) -> (&str, Option<bool>, Option<u32>) {
    (s.deliver.as_str(), s.listening, s.pending)
}

#[test]
fn a_sling_waits_for_a_listener_who_takes_it_as_text_with_its_images_as_files() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    open_with_agent(&client, &projects, "notes", "scout");
    assert_eq!(heard(&client.session_show("scout").unwrap()), ("process", None, None));
    let none = client.session_listen("notes", NONE).unwrap();
    assert_eq!((none.agent.as_str(), none.sling), ("scout", None));

    // Nobody listens and no process runs: the sling waits.
    client.session_stop("notes").unwrap();
    draw(&client, 0);
    let slung = sling(&client, "what is this?");
    assert_eq!((slung.deliver.as_deref(), slung.session_id.as_str()), (Some("listen"), ""));
    let waiting = client.session_show("scout").unwrap();
    assert_eq!(heard(&waiting), ("listen", Some(false), Some(1)));
    assert_eq!(waiting.status, None);

    let taken = client.session_listen("notes", LONG).unwrap().sling.unwrap();
    assert_eq!((taken.event, taken.canvas.as_str()), (slung.event, "notes"));
    assert!(taken.text.starts_with("slinger-sling 1 canvas=notes agent=scout\n"), "{}", taken.text);
    let at = format!(" at={}", taken.dir.display());
    assert!(taken.text.lines().nth(1).unwrap().contains(&at), "{}", taken.text);
    assert!(taken.text.contains("This is a sling from Slinger"), "the whole note");
    assert!(taken.text.contains("`slinger session listen notes --pid $PPID`"));
    assert!(taken.text.ends_with("# prompt\nwhat is this?"));
    assert!(taken.text.contains("img=c1.png") && taken.dir.join("c1.png").is_file());
    let working = client.session_show("scout").unwrap();
    assert_eq!(heard(&working), ("listen", Some(true), Some(1)), "it works on the sling");
    assert_eq!(working.status.as_deref(), Some("working"));

    answers_and_is_in_the_record(&client, &slung);
    daemon.stop();
}

/// The listener's put answers `slung`, asking again says it is dealt with,
/// and the record holds the sling with no conversation.
fn answers_and_is_in_the_record(client: &Client, slung: &Slung) {
    let placed = client.canvas_put("notes", "scout", "c1", Put::Text("a stroke".into())).unwrap();
    let filter = EventFilter { kind: Some("reply_landed".into()), ..EventFilter::default() };
    let landed = client.record_list(filter).unwrap();
    assert_eq!(landed[0]["number"], json!(placed.event.unwrap()));
    assert_eq!(landed[0]["payload"]["sling"], json!(slung.event));
    assert_eq!(client.session_listen("notes", NONE).unwrap().sling, None);
    assert_eq!(heard(&client.session_show("scout").unwrap()), ("listen", Some(true), Some(0)));

    // The record holds the sling with no conversation, and the packet as stored: without `at`.
    let filter = EventFilter { kind: Some("sling_sent".into()), ..EventFilter::default() };
    let sent = client.record_list(filter).unwrap();
    assert_eq!(sent[0]["payload"]["packet"], json!(slung.packet));
    assert_eq!(sent[0]["payload"].get("session"), None);
    let scratch = TempDir::new().unwrap();
    client.blob_get(&slung.packet, &scratch.path().join("p")).unwrap();
    assert!(!std::fs::read_to_string(scratch.path().join("p")).unwrap().contains(" at="));

    let err = client.session_listen("nowhere", NONE).unwrap_err();
    assert_eq!(err.code(), "not_found", "{err}");
}

#[test]
fn a_listener_outranks_a_running_process_and_a_profile_names_only_a_process() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_with_claude(data.path(), &fake_claude());
    let client = daemon.client();
    let home = open_with_agent(&client, &projects, "notes", "scout");
    let dev = Profile { name: "dev".into(), dir: None, tools: Vec::new() };
    client.profile_set("notes", dev).unwrap();
    let under =
        SlingAsk { canvas: "notes".into(), profile: Some("dev".into()), ..SlingAsk::default() };
    let err = client.sling_send(under.clone()).unwrap_err();
    assert_eq!(err.code(), "unavailable", "no process, and a listener has no profile: {err}");
    assert!(err.hint().unwrap().contains("session start"));

    // No listener: the running process takes the sling.
    client.session_start("notes").unwrap();
    assert_eq!(sling(&client, "to the process").deliver.as_deref(), Some("process"));
    assert_eq!(wait_lines(&home.join("in.txt"), 1).len(), 1);

    std::thread::scope(|scope| {
        let theirs = daemon.client();
        let ask = scope.spawn(move || theirs.session_listen("notes", LONG).unwrap());
        wait_for(&client, "scout", |s| s.listening == Some(true));
        assert_eq!(sling(&client, "to the listener").deliver.as_deref(), Some("listen"));
        let taken = ask.join().unwrap().sling.unwrap();
        assert!(taken.text.ends_with("# prompt\nto the listener"));
    });
    // While it works on that one, the next waits for it too.
    assert_eq!(sling(&client, "and this").deliver.as_deref(), Some("listen"));
    assert_eq!(heard(&client.session_show("scout").unwrap()), ("listen", Some(true), Some(2)));
    // A start hands the agent back to its process; the sling that waits stays.
    client.session_start("notes").unwrap();
    assert_eq!(heard(&client.session_show("scout").unwrap()), ("listen", Some(false), Some(1)));
    assert_eq!(sling(&client, "to the process again").deliver.as_deref(), Some("process"));
    assert_eq!(wait_lines(&home.join("in.txt"), 2).len(), 2);
    let left = client.session_listen("notes", NONE).unwrap().sling.unwrap();
    assert!(left.text.ends_with("# prompt\nand this"));
    daemon.stop();
}

#[test]
fn a_listener_that_is_gone_leaves_its_sling_waiting_and_a_stop_ends_an_open_ask() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    open_with_agent(&client, &projects, "notes", "scout");
    // It asks, then goes away without reading the reply.
    let mut gone = UnixStream::connect(&daemon.socket).unwrap();
    gone.write_all(b"{\"v\":1,\"op\":\"session_listen\",\"workspace\":\"notes\",\"wait\":30000}\n")
        .unwrap();
    wait_for(&client, "scout", |s| s.listening == Some(true));
    drop(gone);
    let slung = sling(&client, "anyone?");
    let taken = client.session_listen("notes", LONG).unwrap().sling.unwrap();
    assert_eq!(taken.event, slung.event, "the sling is not lost");

    std::thread::scope(|scope| {
        let theirs = daemon.client();
        let ask = scope.spawn(move || theirs.session_listen("notes", LONG).unwrap());
        wait_for(&client, "scout", |s| s.pending == Some(0));
        daemon.stop();
        assert_eq!(ask.join().unwrap().sling, None);
    });
}

#[test]
fn a_listener_whose_process_ends_mid_sling_is_gone_and_one_that_says_done_is_not() {
    let (data, projects, home) =
        (TempDir::new().unwrap(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    open_with_agent(&client, &projects, "notes", "scout");
    // The session that listens: any process that runs until it is ended.
    let mut session = Command::new(fake_claude())
        .current_dir(home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    client.status_set("scout", "blocked", Some("need the key")).unwrap();
    draw(&client, 0);
    let slung = sling(&client, "first");
    let taken = client.session_listen_in("notes", LONG, Some(session.id())).unwrap();
    assert_eq!(taken.sling.unwrap().event, slung.event);
    let working = client.session_show("scout").unwrap();
    assert_eq!(heard(&working), ("listen", Some(true), Some(1)));
    assert_eq!(working.status.as_deref(), Some("working"));

    // It is closed before it says it is done: nobody listens, the sling waits
    // again and nobody works on it: the status is the one from before
    // (slinger-feaf29).
    session.kill().unwrap();
    session.wait().unwrap();
    let waiting = client.session_show("scout").unwrap();
    assert_eq!(heard(&waiting), ("listen", Some(false), Some(1)));
    let before = (waiting.status.as_deref(), waiting.note.as_deref());
    assert_eq!(before, (Some("blocked"), Some("need the key")));
    let err = client.session_listen_in("notes", NONE, Some(session.id())).unwrap_err();
    assert_eq!(err.code(), "invalid", "{err}");
    let again = client.session_listen("notes", NONE).unwrap().sling.unwrap();
    assert_eq!(again.event, slung.event, "the next listener takes it");
    let working = client.session_show("scout").unwrap().status;
    assert_eq!(working.as_deref(), Some("working"), "and works on it");

    // Saying done (or blocked) ends the work, and the listener stays to ask again.
    client.status_set("scout", "done", None).unwrap();
    let done = client.session_show("scout").unwrap();
    assert_eq!(heard(&done), ("listen", Some(true), Some(0)));
    daemon.stop();
}
