//! The agent UI sandbox (docs/design/live-items.md, K4): a UI that tries
//! every way out reaches neither the network nor a file. It runs the real
//! app (`slinger-app --preview`) against a daemon of its own, so it needs a
//! macOS login session: `WebKit` draws in a window.
//!
//! The UI (`fixtures/escape.html`) shows its context, tries each way out
//! against a TCP listener this test owns and against `/etc/hosts`, then
//! replies with what each attempt saw. The test passes when the reply came,
//! no attempt got out, the policy on the response was in force, and the
//! listener saw no connection.

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use serde_json::{Value, json};
use slinger_control::Client;

#[test]
fn a_ui_reaches_neither_the_network_nor_a_file() {
    // Counts every connection to a listener on a free local port.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let connections = Arc::new(AtomicU32::new(0));
    let count = Arc::clone(&connections);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            count.fetch_add(1, Ordering::SeqCst);
            drop(stream);
        }
    });
    // A daemon of the test's own.
    let socket = std::env::temp_dir().join(format!("sb-{}.sock", std::process::id()));
    let data = tempfile::tempdir().unwrap();
    let store = slingerd::Store::open(data.path(), "my-laptop").unwrap();
    let server = slingerd::Server::bind(&socket).unwrap();
    let stopper = server.stopper();
    let serving = std::thread::spawn(move || server.serve(&store));
    let reply = {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/escape.html");
        Client::new(&socket).ui_put("escape", &fixture, "test").unwrap();
        let config = tempfile::tempdir().unwrap();
        let file = config.path().join("config.toml");
        std::fs::write(&file, format!("[daemon]\nsocket = \"{}\"\n", socket.display())).unwrap();
        let mut app = Command::new(env!("CARGO_BIN_EXE_slinger-app"))
            .args(["--preview", "escape", "--data", &json!({"port": port}).to_string()])
            .env("SLINGER_CONFIG", &file)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut err = app.stderr.take().unwrap();
        let said = std::thread::spawn(move || {
            let mut text = String::new();
            let _ = std::io::Read::read_to_string(&mut err, &mut text);
            text
        });
        let (lines, heard) = mpsc::channel();
        let out = BufReader::new(app.stdout.take().unwrap());
        std::thread::spawn(move || {
            out.lines().map_while(Result::ok).for_each(|l| drop(lines.send(l)));
        });
        let line = heard.recv_timeout(Duration::from_secs(60));
        // A slow attempt still has time to reach the listener.
        std::thread::sleep(Duration::from_secs(2));
        app.kill().unwrap();
        let status = app.wait().unwrap();
        let said = said.join().unwrap();
        line.unwrap_or_else(|e| panic!("no reply within 60 s ({e}; {status}): {said}"))
    };
    stopper.stop();
    serving.join().unwrap().unwrap();
    let _ = std::fs::remove_file(format!("{}.lock", socket.display()));
    let reply: Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(
        (&reply["kind"], &reply["ui"], &reply["version"]),
        (&json!("ui_reply"), &json!("escape"), &json!(1))
    );
    let value = &reply["value"];
    assert_eq!(value["context"]["data"], json!({"port": port}), "the UI read its context");
    assert_eq!(value["context"]["ui"]["hash"], reply["hash"]);
    let seen = value["seen"].as_object().unwrap();
    for (attempt, outcome) in seen {
        let outcome = outcome.to_string();
        assert!(!outcome.contains("reached"), "{attempt} got out: {outcome}");
    }
    assert_eq!(seen.len(), 15, "{seen:?}");
    assert_eq!(seen["eval"], json!("failed: EvalError"), "the policy on the response holds");
    assert!(seen["violations"].as_u64().unwrap() > 0, "the policy reported what it blocked");
    assert_eq!(connections.load(Ordering::SeqCst), 0, "the listener saw a connection");
}
