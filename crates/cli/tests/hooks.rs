//! A session reports to Slinger through its hooks, without the agent's help:
//! the daemon starts a stand-in for Claude Code with the hooks it makes, the
//! stand-in runs them as Claude Code would, and they run the real `slinger`
//! binary, which finds the agent and the socket in the process's environment.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

use slinger_control::{Client, EventFilter};

/// Claude Code's turn, as far as its hooks go. It reads each hook's command
/// from `--settings` by its event, and runs it with `sh -c`, its payload on
/// stdin. Per message: `UserPromptSubmit`; then, once the test creates `go`
/// in its home, a write of `out.txt` and its `PostToolUse`; the answer; and
/// `Stop`.
const FAKE_CLAUDE: &str = r#"#!/bin/sh
settings=
while [ $# -gt 0 ]; do
  case "$1" in --settings) settings="$2"; shift ;; esac
  shift
done
hook() {
  printf '%s' "$settings" |
    sed -n "s/.*\"$1\":\[{\"hooks\":\[{\"command\":\"\([^\"]*\)\".*/\1/p"
}
while IFS= read -r line; do
  printf '{"hook_event_name":"UserPromptSubmit","cwd":"%s","prompt":"hi"}' "$PWD" |
    sh -c "$(hook UserPromptSubmit)"
  while [ ! -f go ]; do sleep 0.02; done
  rm go
  printf 'written by the agent' > out.txt
  printf '{"hook_event_name":"PostToolUse","cwd":"%s","tool_name":"Write","tool_input":{"file_path":"out.txt","content":"x"}}' "$PWD" |
    sh -c "$(hook PostToolUse)"
  printf '{"type":"result","subtype":"success","session_id":"s","result":"ok"}\n'
  printf '{"hook_event_name":"Stop","cwd":"%s"}' "$PWD" | sh -c "$(hook Stop)"
done
"#;

/// Waits until `met` holds, or fails after 10 s.
fn until(mut met: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !met() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_turn_moves_the_status_from_working_to_done_and_records_the_file_it_wrote() {
    let (data, work) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    // The socket in a directory of its own, removed on drop.
    let sockets = tempfile::tempdir().unwrap();
    let socket = sockets.path().join("d.sock");
    let claude = work.path().join("claude");
    std::fs::write(&claude, FAKE_CLAUDE).unwrap();
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    let store = slingerd::Store::open(data.path(), "my-laptop")
        .unwrap()
        .with_claude(&claude)
        .with_slinger(env!("CARGO_BIN_EXE_slinger"));
    let server = slingerd::Server::bind(&socket).unwrap();
    let stopper = server.stopper();
    let serving = std::thread::spawn(move || server.serve(&store));

    let client = Client::new(&socket);
    let status = |client: &Client| client.session_show("scout").unwrap().status;
    let home = work.path().canonicalize().unwrap().join("notes");
    std::fs::create_dir(&home).unwrap();
    let file = "version = 1\nname = \"notes\"\ncanvas = \"notes\"\n[agent]\nname = \"scout\"\n";
    std::fs::write(home.join("slinger-workspace.toml"), file).unwrap();
    client.workspace_open(Some(&home), None).unwrap();
    client.session_start("notes").unwrap();
    assert_eq!(status(&client), None);

    client.session_send("notes", "write something").unwrap();
    until(|| status(&client).as_deref() == Some("working"));
    std::fs::write(home.join("go"), "").unwrap();
    until(|| status(&client).as_deref() == Some("done"));

    let events = client.record_list(EventFilter::default()).unwrap();
    let kinds: Vec<&str> = events.iter().filter_map(|e| e["kind"].as_str()).collect();
    assert_eq!(kinds, ["file_touched", "agent_done"]);
    let written = home.join("out.txt");
    assert_eq!(events[0]["payload"]["path"], written.display().to_string());
    assert_eq!(events[0]["payload"]["hash"], client.blob_put(&written).unwrap());
    assert!(events.iter().all(|e| e["tags"]["agent"] == "scout"));

    client.session_stop("notes").unwrap();
    stopper.stop();
    serving.join().unwrap().unwrap();
    assert!(!Path::new(&socket).exists());
}
