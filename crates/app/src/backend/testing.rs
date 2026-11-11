//! What the app's tests share: a stand-in for Claude Code, a wait, and a
//! user's stroke on a canvas.

use std::fs;
use std::path::{Path, PathBuf};
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

/// Writes the stand-in for Claude Code into `dir`; its path.
pub(crate) fn fake_claude(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("claude");
    fs::write(&path, FAKE_CLAUDE).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Waits until `met` holds, or fails after 10 s.
pub(crate) fn wait_until(mut met: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !met() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A user's stroke on page p1 of `canvas`, from (100, 100) to (110, 110).
pub(crate) fn user_stroke(daemon: &Client, canvas: &str) {
    let ops = serde_json::json!({"op": "add", "item": {"page": "p1", "kind": "stroke",
        "color": "#000000", "width": 20, "pts": [1000, 1000, 100, 100]}});
    let by = "user".to_string();
    daemon.canvas_apply(control::CanvasOps { canvas: canvas.into(), by, ops }).unwrap();
}
