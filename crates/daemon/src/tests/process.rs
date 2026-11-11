//! An agent's process through the real socket, with a stand-in for Claude
//! Code that speaks its stream format: start, sends, the session id across a
//! restart, a process that dies, and a daemon stop that ends it.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use slinger_control::{Client, Session};
use slinger_core::session::hooks;
use slinger_core::session::stream::Limits;
use tempfile::TempDir;

use super::Daemon;
use super::session::open_with_agent;
use crate::processes::Processes;
use crate::store::SLINGER;

/// Claude Code's stream mode, as far as the daemon uses it. It runs in the
/// agent's home and leaves its evidence there: its arguments (`args.txt`, one
/// run per line), its environment (`env.txt`, and its `PATH` in `path.txt`)
/// and each line it was sent
/// (`in.txt`). For each line it names its session and answers.
const FAKE_CLAUDE: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> args.txt
printf '%s %s\n' "$SLINGER_AGENT" "$SLINGER_SOCKET" > env.txt
printf '%s\n' "$PATH" > path.txt
id=
while [ $# -gt 0 ]; do
  case "$1" in --session-id|--resume) id="$2"; shift ;; esac
  shift
done
n=0
while IFS= read -r line; do
  n=$((n + 1))
  printf '%s\n' "$line" >> in.txt
  printf '{"type":"system","subtype":"init","session_id":"%s"}\n' "$id"
  printf '{"type":"result","subtype":"success","session_id":"%s","result":"answer %s"}\n' "$id" "$n"
done
"#;

/// Writes the stand-in into `dir` and returns its path.
pub(super) fn fake_claude(dir: &Path) -> PathBuf {
    let path = dir.join("claude");
    std::fs::write(&path, FAKE_CLAUDE).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Asks for `agent`'s session until `done` holds for it, or fails after 10 s.
fn wait_for(client: &Client, agent: &str, done: impl Fn(&Session) -> bool) -> Session {
    let mut last = None;
    until(|| {
        let session = client.session_show(agent).unwrap();
        let met = done(&session);
        last = Some(session);
        met
    });
    last.unwrap()
}

/// The lines of the file at `path` once it has `n` of them, or fails after 10 s.
pub(super) fn wait_lines(path: &Path, n: usize) -> Vec<String> {
    until(|| lines(path).len() >= n);
    lines(path)
}

/// Waits until `met` holds, or fails after 10 s.
fn until(mut met: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !met() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The lines of the file at `path`; none while it is not there.
fn lines(path: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines().map(str::to_string).collect()
}

/// Whether a process with this id still exists.
fn alive(pid: u32) -> bool {
    let mut kill = Command::new("kill");
    kill.arg("-0").arg(pid.to_string()).stderr(std::process::Stdio::null());
    kill.status().unwrap().success()
}

#[test]
fn three_sends_go_to_one_session_and_a_restart_resumes_it() {
    let (data, projects, bin) =
        (TempDir::new().unwrap(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let claude = fake_claude(bin.path());
    let daemon = Daemon::start_with_claude(data.path(), &claude);
    let client = daemon.client();
    let home = open_with_agent(&client, &projects, "notes", "scout");
    let started = client.session_start("notes").unwrap();
    assert_eq!((started.agent.as_str(), started.process.as_deref()), ("scout", Some("running")));
    let id = started.session_id.clone().unwrap();
    assert_eq!(client.session_start("notes").unwrap().pid, started.pid, "a running one stays");

    for text in ["one", "two", "three"] {
        client.session_send("notes", text).unwrap();
    }
    let sent = wait_lines(&home.join("in.txt"), 3);
    let answered = wait_for(&client, "scout", |s| s.pending == Some(0));
    assert_eq!(answered.answer.as_deref(), Some("answer 3"), "the output is read");
    assert_eq!(answered.session_id.as_deref(), Some(id.as_str()));
    let sent: Vec<serde_json::Value> =
        sent.iter().map(|l| serde_json::from_str(l).unwrap()).collect();
    let texts: Vec<&str> = sent.iter().map(|m| m["message"]["content"].as_str().unwrap()).collect();
    assert_eq!(texts, ["one", "two", "three"]);
    let socket = daemon.socket.display().to_string();
    assert_eq!(lines(&home.join("env.txt")), [format!("scout {socket}")]);
    assert!(lines(&home.join("args.txt"))[0].ends_with(&format!("--session-id {id}")));

    let pid = answered.pid.unwrap();
    daemon.stop();
    assert!(!alive(pid), "a daemon stop ends the process");

    let daemon = Daemon::start_with_claude(data.path(), &claude);
    let client = daemon.client();
    let shown = client.session_show("scout").unwrap();
    assert_eq!((shown.process, shown.session_id.as_deref()), (None, Some(id.as_str())));
    client.session_start("notes").unwrap();
    assert!(wait_lines(&home.join("args.txt"), 2)[1].ends_with(&format!("--resume {id}")));
    let stopped = client.session_stop("notes").unwrap();
    assert_eq!((stopped.process, stopped.pid), (None, None));
    client.session_stop("notes").unwrap();
    daemon.stop();
}

#[test]
fn a_process_that_dies_shows_as_exited_and_a_send_is_unavailable() {
    let (data, projects, bin) =
        (TempDir::new().unwrap(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_with_claude(data.path(), &fake_claude(bin.path()));
    let client = daemon.client();
    open_with_agent(&client, &projects, "notes", "scout");
    let err = client.session_send("notes", "hi").unwrap_err();
    assert_eq!(err.code(), "unavailable", "never started");
    assert!(err.hint().unwrap().contains("session start"));

    let pid = client.session_start("notes").unwrap().pid.unwrap();
    client.session_send("notes", "hi").unwrap();
    assert!(Command::new("kill").arg("-KILL").arg(pid.to_string()).status().unwrap().success());
    let exited = wait_for(&client, "scout", |s| s.process.as_deref() == Some("exited"));
    assert_eq!((exited.pid, exited.pending), (None, None));
    let err = client.session_send("notes", "again").unwrap_err();
    assert_eq!(err.code(), "unavailable");
    assert!(err.to_string().contains("exited"), "{err}");

    let restarted = client.session_start("notes").unwrap();
    assert_eq!(restarted.process.as_deref(), Some("running"));
    client.session_send("notes", "back").unwrap();
    daemon.stop();
}

#[test]
fn a_start_needs_an_open_workspace_with_an_agent_and_a_program_to_run() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_with_claude(data.path(), &projects.path().join("no-such-claude"));
    let client = daemon.client();
    assert_eq!(client.session_start("nowhere").unwrap_err().code(), "not_found");
    let name = client.workspace_open(Some(projects.path()), None).unwrap().workspace.name;
    let err = client.session_start(&name).unwrap_err();
    assert_eq!(err.code(), "invalid", "{err}");
    open_with_agent(&client, &projects, "notes", "scout");
    let err = client.session_start("notes").unwrap_err();
    assert_eq!(err.code(), "not_found", "{err}");
    assert!(err.hint().unwrap().contains("--claude"));
    assert_eq!(client.session_send("notes", " ").unwrap_err().code(), "invalid");
    daemon.stop();
}

#[test]
fn a_session_starts_within_its_workspaces_limits() {
    let (data, projects, bin) =
        (TempDir::new().unwrap(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_with_claude(data.path(), &fake_claude(bin.path()));
    let client = daemon.client();
    // Its hooks first, running `slinger` on `PATH` (the daemon's default
    // here), then the stream format and the limits.
    let hooks = hooks::settings(Path::new(SLINGER));
    let fixed = format!(
        "--settings {hooks} --print --input-format stream-json --output-format stream-json"
    );
    let denied = "--permission-mode dontAsk --tools";

    let plain = open_with_agent(&client, &projects, "plain", "scout");
    client.session_start("plain").unwrap();
    let id = client.session_show("scout").unwrap().session_id.unwrap();
    assert_eq!(
        wait_lines(&plain.join("args.txt"), 1),
        [format!(
            "{fixed} --setting-sources project,local --strict-mcp-config {denied} \
         Read,Glob,Grep,Bash --allowedTools Bash(slinger *) --verbose --session-id {id}"
        )]
    );

    std::fs::create_dir(projects.path().join("wide")).unwrap();
    let wide = projects.path().join("wide").canonicalize().unwrap();
    let file = format!(
        "version = 1\nname = \"wide\"\ncanvas = \"wide\"\n\
         [agent]\nname = \"ada\"\nuser_config = true\n\
         dirs = [\"/srv/docs\"]\ntools = [\"Edit\", \"Bash(git *)\"]\n\
         [[root]]\nmachine = \"test-machine\"\npath = \"{}\"\nprimary = true\n\
         [[root]]\nmachine = \"test-machine\"\npath = \"/srv/data\"\n\
         [[root]]\nmachine = \"elsewhere\"\npath = \"/srv/far\"\n",
        wide.display()
    );
    std::fs::write(wide.join("slinger-workspace.toml"), file).unwrap();
    client.workspace_open(Some(&wide), None).unwrap();
    client.session_start("wide").unwrap();
    let id = client.session_show("ada").unwrap().session_id.unwrap();
    assert_eq!(
        wait_lines(&wide.join("args.txt"), 1),
        [format!(
            "{fixed} --setting-sources user,project,local --strict-mcp-config {denied} \
         Read,Glob,Grep,Bash,Edit --allowedTools Bash(slinger *) Edit Bash(git *) \
         --add-dir /srv/data /srv/docs --verbose --session-id {id}"
        )]
    );
    let env = wait_lines(&wide.join("env.txt"), 1);
    assert_eq!(env, [format!("ada {}", daemon.socket.display())]);
    daemon.stop();
}

#[test]
fn the_cli_the_hooks_run_comes_first_on_the_agents_path() {
    let (data, home, bin) =
        (TempDir::new().unwrap(), TempDir::new().unwrap(), TempDir::new().unwrap());
    let ids = data.path().join("sessions.json");
    let mut processes = Processes::new(fake_claude(bin.path()), ids);
    let daemons = std::env::var("PATH").unwrap();

    // A bare name is looked up on the daemon's PATH, as it is.
    processes.start("scout", home.path(), &Limits::default()).unwrap();
    assert_eq!(wait_lines(&home.path().join("path.txt"), 1), std::slice::from_ref(&daemons));
    processes.stop("scout");

    std::fs::remove_file(home.path().join("path.txt")).unwrap();
    processes.set_slinger(PathBuf::from("/opt/slinger/bin/slinger"));
    processes.start("scout", home.path(), &Limits::default()).unwrap();
    let path = wait_lines(&home.path().join("path.txt"), 1);
    assert_eq!(path, [format!("/opt/slinger/bin:{daemons}")]);
    processes.stop("scout");
}
