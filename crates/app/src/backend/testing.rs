//! What the app's tests share: stand-ins for Claude Code, one for Flick, a
//! wait, and a user's stroke on a canvas.
//!
//! The tests run beside other builds: a wait stops on the whole of what it
//! waits for, never its first sign, and gives up only after long enough.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use slinger_control::{self as control, Client};

/// A stand-in for Claude Code: names a session, then writes each
/// message it reads to `in.txt` in its home and answers it.
const FAKE_CLAUDE: &str = r#"#!/bin/sh
id=00000000-0000-4000-8000-000000000001
printf '{"type":"system","subtype":"init","session_id":"%s"}\n' "$id"
while IFS= read -r line; do
  printf '%s\n' "$line" >> in.txt
  printf '{"type":"result","subtype":"success","session_id":"%s","result":"ok"}\n' "$id"
done
"#;

/// A stand-in for Claude Code that holds each answer until a file `go`
/// shows up in its home, and takes the file. Its context holds 2008 tokens
/// from the start, as a resumed conversation's does. It waits only while
/// the test that started it runs, or a failed test leaves it behind
/// (slinger-0e8be0).
const WAITING_CLAUDE: &str = r#"#!/bin/sh
id=00000000-0000-4000-8000-000000000001
printf '{"type":"system","subtype":"init","session_id":"%s"}\n' "$id"
printf '{"type":"assistant","message":{"usage":{"input_tokens":3,"cache_read_input_tokens":2000,"output_tokens":5}}}\n'
while IFS= read -r line; do
  printf '%s\n' "$line" >> in.txt
  while [ ! -e go ]; do kill -0 "$PPID" 2>/dev/null || exit 1; sleep 0.02; done
  rm go
  printf '{"type":"result","subtype":"success","session_id":"%s","result":"ok"}\n' "$id"
done
"#;

/// Writes the stand-in for Claude Code into `dir`; its path.
pub(crate) fn fake_claude(dir: &Path) -> PathBuf {
    write_claude(dir, FAKE_CLAUDE)
}

/// Writes the stand-in for Claude Code that answers once told to (a file
/// `go` in its home) into `dir`; its path.
pub(crate) fn waiting_claude(dir: &Path) -> PathBuf {
    write_claude(dir, WAITING_CLAUDE)
}

fn write_claude(dir: &Path, script: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("claude");
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Waits until `met` holds, or fails after 30 s: long enough for a machine
/// busy with other builds.
pub(crate) fn wait_until(mut met: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !met() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// What the stand-in for Claude Code in `home` read, once it holds `want`.
/// It writes a message a line at a time: wait for the part that ends it.
pub(crate) fn delivered(home: &Path, want: &str) -> String {
    let mut message = String::new();
    wait_until(|| {
        message = fs::read_to_string(home.join("in.txt")).unwrap_or_default();
        message.contains(want)
    });
    message
}

/// A user's stroke on page p1 of `canvas`, from (100, 100) to (110, 110).
pub(crate) fn user_stroke(daemon: &Client, canvas: &str) {
    let ops = serde_json::json!({"op": "add", "item": {"page": "p1", "kind": "stroke",
        "color": "#000000", "width": 20, "pts": [1000, 1000, 100, 100]}});
    let by = "user".to_string();
    daemon.canvas_apply(control::CanvasOps { canvas: canvas.into(), by, ops, once: None }).unwrap();
}

/// A stand-in Flick on a socket in `dir`: it answers each request line with
/// `answer(line)`, after it records the line: once its answer is in, so is
/// the request. Returns the socket and the requests heard so far.
pub(crate) fn fake_flick(
    dir: &Path,
    answer: fn(&str) -> String,
) -> (PathBuf, Arc<Mutex<Vec<String>>>) {
    let socket = dir.join("flick.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let theirs = Arc::clone(&heard);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut line = String::new();
            if BufReader::new(&stream).read_line(&mut line).is_err() {
                continue;
            }
            let line = line.trim_end().to_string();
            let answered = answer(&line);
            theirs.lock().unwrap().push(line);
            let _ = writeln!(stream, "{answered}");
        }
    });
    (socket, heard)
}

/// A Flick with dictation set up: every verb is ok.
pub(crate) fn ready(_line: &str) -> String {
    r#"{"ok":"dictation: recording"}"#.to_string()
}
