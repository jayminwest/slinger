//! An agent's process through the real socket, with a stand-in for Claude
//! Code that speaks its stream format: start, sends, the session id across a
//! restart, a process that dies, and a daemon stop that ends it.

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

/// The stand-in for Claude Code (`tests/claude`, which says what it does).
/// One file kept in the tree, not one written per test: macOS checks a new
/// executable on its first exec, one at a time across the machine, and under
/// load that queue held a stand-in's start past the tests' waits
/// (slinger-d71082).
pub(super) fn fake_claude() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tests/claude")
}

/// Asks for `agent`'s session until `done` holds for it, or fails after 10 s.
pub(super) fn wait_for(client: &Client, agent: &str, done: impl Fn(&Session) -> bool) -> Session {
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
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let claude = fake_claude();
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
fn a_message_sent_mid_turn_and_answered_in_that_turn_leaves_none_pending() {
    // Claude Code takes a message sent while a turn runs into that turn: two
    // messages, one result (slinger-b84853).
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_with_claude(data.path(), &fake_claude());
    let client = daemon.client();
    let home = open_with_agent(&client, &projects, "notes", "scout");
    client.session_start("notes").unwrap();
    client.session_send("notes", "mid-turn").unwrap();
    wait_lines(&home.join("in.txt"), 1);
    assert_eq!(client.session_show("scout").unwrap().pending, Some(1), "its turn runs");
    client.session_send("notes", "two").unwrap();
    let answered = wait_for(&client, "scout", |s| s.pending == Some(0));
    assert_eq!(answered.answer.as_deref(), Some("answer 2"));
    client.session_send("notes", "three").unwrap();
    let answered = wait_for(&client, "scout", |s| s.answer.as_deref() == Some("answer 3"));
    assert_eq!(answered.pending, Some(0), "a later answer is not counted against");
    daemon.stop();
}

#[test]
fn a_process_that_dies_shows_as_exited_and_a_send_is_unavailable() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_with_claude(data.path(), &fake_claude());
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
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_with_claude(data.path(), &fake_claude());
    let client = daemon.client();
    // Its hooks first, running `slinger` on `PATH` (the daemon's default
    // here), then the stream format and the limits.
    let hooks = hooks::settings(Path::new(SLINGER));
    let fixed = format!(
        "--settings {hooks} --print --input-format stream-json --output-format stream-json"
    );
    let denied = "--permission-mode dontAsk";

    let plain = open_with_agent(&client, &projects, "plain", "scout");
    client.session_start("plain").unwrap();
    let id = client.session_show("scout").unwrap().session_id.unwrap();
    assert_eq!(
        wait_lines(&plain.join("args.txt"), 1),
        [format!(
            "{fixed} --setting-sources user,project,local {denied} \
         --allowedTools Bash(slinger *) Edit Write --verbose --session-id {id}"
        )]
    );

    std::fs::create_dir(projects.path().join("wide")).unwrap();
    let wide = projects.path().join("wide").canonicalize().unwrap();
    let file = format!(
        "version = 1\nname = \"wide\"\ncanvas = \"wide\"\n\
         [agent]\nname = \"ada\"\nuser_config = false\n\
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
            "{fixed} --setting-sources project,local {denied} \
         --allowedTools Bash(slinger *) Edit Write Bash(git *) \
         --add-dir /srv/data /srv/docs --verbose --session-id {id}"
        )]
    );
    let env = wait_lines(&wide.join("env.txt"), 1);
    assert_eq!(env, [format!("ada {}", daemon.socket.display())]);
    daemon.stop();
}

#[test]
fn the_cli_the_hooks_run_comes_first_on_the_agents_path() {
    let (data, home) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let ids = data.path().join("sessions.json");
    let mut processes = Processes::new(fake_claude(), ids);
    let daemons = std::env::var("PATH").unwrap();

    // A bare name is looked up on the daemon's PATH, as it is.
    processes.start("scout", None, home.path(), &Limits::default(), false).unwrap();
    assert_eq!(wait_lines(&home.path().join("path.txt"), 1), std::slice::from_ref(&daemons));
    processes.stop("scout");

    std::fs::remove_file(home.path().join("path.txt")).unwrap();
    processes.set_slinger(PathBuf::from("/opt/slinger/bin/slinger"));
    processes.start("scout", None, home.path(), &Limits::default(), false).unwrap();
    let path = wait_lines(&home.path().join("path.txt"), 1);
    assert_eq!(path, [format!("/opt/slinger/bin:{daemons}")]);
    processes.stop("scout");
}

#[test]
fn a_session_that_cannot_be_resumed_is_dropped_and_the_next_start_is_new() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let claude = fake_claude();
    let daemon = Daemon::start_with_claude(data.path(), &claude);
    let client = daemon.client();
    let home = open_with_agent(&client, &projects, "notes", "scout");
    let old = client.session_start("notes").unwrap().session_id.unwrap();
    client.session_send("notes", "one").unwrap();
    wait_for(&client, "scout", |s| s.pending == Some(0));
    client.session_stop("notes").unwrap();

    // Claude Code no longer has the conversation: the resume fails.
    std::fs::remove_file(home.join("known.txt")).unwrap();
    client.session_start("notes").unwrap();
    let exited = wait_for(&client, "scout", |s| s.process.as_deref() == Some("exited"));
    assert_eq!(exited.session_id.as_deref(), Some(old.as_str()));
    assert!(wait_lines(&home.join("args.txt"), 2)[1].ends_with(&format!("--resume {old}")));
    assert_eq!(client.session_send("notes", "lost").unwrap_err().code(), "unavailable");

    let new = client.session_start("notes").unwrap();
    let id = new.session_id.unwrap();
    assert_ne!(id, old, "a new session");
    assert!(wait_lines(&home.join("args.txt"), 3)[2].ends_with(&format!("--session-id {id}")));
    client.session_send("notes", "two").unwrap();
    let answered = wait_for(&client, "scout", |s| s.pending == Some(0));
    assert_eq!(answered.session_id.as_deref(), Some(id.as_str()));
    daemon.stop();

    // The new id is the one kept: a restart resumes it.
    let daemon = Daemon::start_with_claude(data.path(), &claude);
    let client = daemon.client();
    client.session_start("notes").unwrap();
    assert!(wait_lines(&home.join("args.txt"), 4)[3].ends_with(&format!("--resume {id}")));
    client.session_send("notes", "three").unwrap();
    let answered = wait_for(&client, "scout", |s| s.pending == Some(0));
    assert_eq!(answered.process.as_deref(), Some("running"));
    daemon.stop();
}

#[test]
fn a_fresh_start_ends_the_running_process_and_begins_a_new_session() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_with_claude(data.path(), &fake_claude());
    let client = daemon.client();
    let home = open_with_agent(&client, &projects, "notes", "scout");
    let old = client.session_start("notes").unwrap();
    let fresh = client.session_start_fresh("notes").unwrap();
    assert!(!alive(old.pid.unwrap()), "the running one ended");
    assert_ne!(fresh.pid, old.pid);
    let id = fresh.session_id.unwrap();
    assert_ne!(Some(&id), old.session_id.as_ref());
    // The old one can be ended before its shell writes its line, so the new
    // one's line is looked for, not counted to.
    let new = format!("--session-id {id}");
    until(|| lines(&home.join("args.txt")).iter().any(|l| l.ends_with(&new)));
    client.session_send("notes", "hi").unwrap();
    let answered = wait_for(&client, "scout", |s| s.pending == Some(0));
    assert_eq!(answered.session_id.as_deref(), Some(id.as_str()));
    daemon.stop();
}
