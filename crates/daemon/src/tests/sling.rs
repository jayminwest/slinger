//! `sling_send` through the real socket, with the stand-in for Claude Code:
//! slings go into one conversation, each is in the record with its packet,
//! old ink is not sent again, a sling starts an agent that does not run, and
//! a sling that cannot go leaves no trace.
//! `listen.rs` has the slings a listener takes.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use slinger_control::{CanvasOps, Client, EventFilter, Put, SlingAsk, Slung};
use slinger_core::sling::Verb;
use tempfile::TempDir;

use super::Daemon;
use super::process::{fake_claude, wait_for, wait_lines};
use super::session::open_with_agent;
use crate::server::VERBS;

const LONG: Duration = Duration::from_secs(30);

/// Draws one stroke of the user's at `x` on the canvas `notes`; far enough
/// apart, each is a cluster of its own, `c<seq>`.
fn draw(client: &Client, x: i64) -> u64 {
    let item = json!({ "page": "p1", "kind": "stroke", "width": 20,
        "pts": [x, 0, 100, 50, 100, -50] });
    let ops = json!({ "op": "add", "item": item });
    let ops = CanvasOps { canvas: "notes".into(), by: "user".into(), ops, once: None };
    client.canvas_apply(ops).unwrap().seq[0]
}

fn sling(client: &Client, select: &[&str], prompt: &str) -> Slung {
    let select = select.iter().map(|s| (*s).to_string()).collect();
    let ask =
        SlingAsk { canvas: "notes".into(), select, prompt: prompt.into(), ..SlingAsk::default() };
    client.sling_send(ask).unwrap()
}

fn sent(client: &Client) -> Vec<Value> {
    let filter = EventFilter { kind: Some("sling_sent".into()), ..EventFilter::default() };
    client.record_list(filter).unwrap()
}

/// The text blocks of each message the stand-in got, joined, and how many
/// images each had.
fn messages(home: &Path, n: usize) -> Vec<(String, usize)> {
    let lines = wait_lines(&home.join("in.txt"), n);
    let message = |line: &String| {
        let value: Value = serde_json::from_str(line).unwrap();
        let blocks = value["message"]["content"].as_array().unwrap().clone();
        let text = blocks.iter().filter_map(|b| b["text"].as_str()).collect::<String>();
        (text, blocks.iter().filter(|b| b["type"] == "image").count())
    };
    lines.iter().map(message).collect()
}

/// Each sling has its `sling_sent`, tagged and naming its packet, images
/// and conversation, and its own packet blob.
fn in_the_record(client: &Client, slung: &[Slung], id: &str) {
    let events = sent(client);
    assert_eq!(events.len(), slung.len());
    for (event, s) in events.iter().zip(slung) {
        assert_eq!(event["number"], json!(s.event));
        let tags = &event["tags"];
        assert_eq!(
            (&tags["agent"], &tags["canvas"], &tags["workspace"]),
            (&json!("scout"), &json!("notes"), &json!("notes"))
        );
        let payload = &event["payload"];
        assert_eq!((&payload["packet"], &payload["session"]), (&json!(s.packet), &json!(id)));
        assert_eq!(payload["images"].as_array().unwrap().len(), 1);
        let scratch = TempDir::new().unwrap();
        client.blob_get(&s.packet, &scratch.path().join("p")).unwrap();
    }
    let packets: std::collections::BTreeSet<&str> = slung.iter().map(|s| &*s.packet).collect();
    assert_eq!(packets.len(), slung.len(), "a packet blob each");
}

/// The first message of a conversation gets the whole note, the later ones a line.
fn first_gets_the_whole_note(got: &[(String, usize)]) {
    assert!(got[0].0.starts_with("slinger-sling 1 canvas=notes selected=c1\n"), "{}", got[0].0);
    assert!(got[0].0.contains("This is a sling from Slinger"), "the first gets the whole note");
    assert!(got[0].0.contains("slinger canvas crop notes"), "it names the verbs served");
    assert!(!got[1].0.contains("This is a sling from Slinger"), "a later one gets one line");
    assert!(got[2].0.ends_with("# prompt\nsling 2"));
    assert!(got.iter().all(|(_, images)| *images == 1), "each new cluster is an image");
}

#[test]
fn three_slings_go_into_one_conversation_and_each_is_in_the_record() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_with_claude(data.path(), &fake_claude());
    let client = daemon.client();
    let home = open_with_agent(&client, &projects, "notes", "scout");
    let id = client.session_start("notes").unwrap().session_id.unwrap();

    let mut slung = Vec::new();
    for (n, x) in [0, 5000, 10000].into_iter().enumerate() {
        let seq = draw(&client, x);
        let s = sling(&client, &[&format!("i{seq}")], &format!("sling {n}"));
        assert_eq!((s.from, s.to, s.strokes), (seq, seq, 1), "only the ink after the last sling");
        assert_eq!((s.agent.as_str(), s.session_id.as_str()), ("scout", id.as_str()));
        slung.push(s);
    }
    in_the_record(&client, &slung, &id);
    let got = messages(&home, 3);
    first_gets_the_whole_note(&got);
    for (s, (text, _)) in slung.iter().zip(&got) {
        let text = (text.chars().count() as u64).div_ceil(4);
        assert!(s.tokens.unwrap() > text, "the wrapper and the image count: {s:?}");
    }
    assert!(slung[0].tokens > slung[1].tokens, "the whole note costs more");

    // Old ink selected again: nothing new is sent, and the selection is named.
    let again = sling(&client, &["i1"], "and this?");
    assert_eq!((again.strokes, again.from, again.to), (0, 4, 3));
    let fourth = &messages(&home, 4)[3];
    assert!(fourth.0.starts_with("slinger-sling 1 canvas=notes selected=c1\n"));
    assert_eq!(fourth.1, 0);

    // The agent's put answers the latest sling.
    let placed = client.canvas_put("notes", "scout", "c1", Put::Text("a stroke".into())).unwrap();
    let filter = EventFilter { kind: Some("reply_landed".into()), ..EventFilter::default() };
    let landed = client.record_list(filter).unwrap();
    assert_eq!(landed[0]["number"], json!(placed.event.unwrap()));
    assert_eq!(landed[0]["payload"]["sling"], json!(again.event));
    daemon.stop();
}

#[test]
fn a_sling_starts_an_agent_that_does_not_run_unless_a_listener_is_there() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_with_claude(data.path(), &fake_claude());
    let client = daemon.client();
    let home = open_with_agent(&client, &projects, "notes", "scout");
    assert_eq!(client.session_show("scout").unwrap().process, None, "opening starts nothing");

    // Never started: the sling starts it, and goes into its new conversation.
    draw(&client, 0);
    let first = sling(&client, &[], "first");
    assert_eq!(first.deliver.as_deref(), Some("process"));
    let running = client.session_show("scout").unwrap();
    assert_eq!(running.process.as_deref(), Some("running"));
    assert_eq!(running.session_id.as_deref(), Some(first.session_id.as_str()));
    assert!(messages(&home, 1)[0].0.contains("This is a sling from Slinger"), "the whole note");

    // Stopped: the next sling starts it again, resuming that conversation.
    client.session_stop("notes").unwrap();
    draw(&client, 5000);
    let again = sling(&client, &[], "again");
    assert_eq!((again.deliver.as_deref(), &again.session_id), (Some("process"), &first.session_id));
    assert!(messages(&home, 2)[1].0.ends_with("# prompt\nagain"));
    let args = std::fs::read_to_string(home.join("args.txt")).unwrap();
    assert!(args.lines().nth(1).unwrap().contains("--resume"), "{args}");

    // Stopped with a listener there: the listener takes it, and nothing starts.
    client.session_stop("notes").unwrap();
    std::thread::scope(|scope| {
        let theirs = daemon.client();
        let ask = scope.spawn(move || theirs.session_listen("notes", LONG).unwrap());
        wait_for(&client, "scout", |s| s.listening == Some(true));
        assert_eq!(sling(&client, &[], "to the listener").deliver.as_deref(), Some("listen"));
        assert!(ask.join().unwrap().sling.is_some());
    });
    assert_eq!(client.session_show("scout").unwrap().process, None);
    daemon.stop();
}

#[test]
fn a_sling_that_cannot_go_leaves_no_trace() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_with_claude(data.path(), &fake_claude());
    let client = daemon.client();
    let ask = SlingAsk { canvas: "notes".into(), ..SlingAsk::default() };
    let err = client.sling_send(ask.clone()).unwrap_err();
    assert_eq!(err.code(), "not_found", "no open workspace has the canvas: {err}");
    open_with_agent(&client, &projects, "notes", "scout");
    draw(&client, 0);
    // A profile the agent does not have: no process could be under it.
    let under = SlingAsk { profile: Some("dev".into()), ..ask.clone() };
    assert_eq!(client.sling_send(under).unwrap_err().code(), "not_found");
    assert_eq!(sent(&client), Vec::<Value>::new());

    // The ink a failed sling did not send is still new for the next one.
    client.session_start("notes").unwrap();
    let s = client.sling_send(ask).unwrap();
    assert_eq!((s.from, s.to, s.strokes), (1, 1, 1));
    let bad = SlingAsk { canvas: "notes".into(), select: vec!["x".into()], ..SlingAsk::default() };
    assert_eq!(client.sling_send(bad).unwrap_err().code(), "invalid");
    daemon.stop();
}

/// Each verb the message names is one this daemon answers: its request is
/// read, not refused as unknown.
#[test]
fn every_verb_the_note_names_is_served() {
    let daemon = Daemon::start();
    for verb in VERBS {
        let request = match verb {
            Verb::Put => json!({ "v": 1, "op": "canvas_put", "canvas": "none", "agent": "a",
                "beside": "i1", "text": "t" }),
            Verb::Crop => json!({ "v": 1, "op": "canvas_crop", "canvas": "none", "of": "i1",
                "path": "/tmp/none.png" }),
            Verb::Ui => json!({ "v": 1, "op": "ui_open", "ui": "none", "workspace": "none",
                "agent": "a" }),
        };
        let reply: Value =
            serde_json::from_str(&daemon.raw(format!("{request}\n").as_bytes())).unwrap();
        assert_eq!(reply["code"], "not_found", "{verb:?}: {reply}");
    }
    daemon.stop();
}
