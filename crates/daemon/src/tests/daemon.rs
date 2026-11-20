//! The daemon's own verbs through the real socket: `daemon_info` names this
//! process and what it serves, `daemon_stop` replies, ends the server as a
//! signal does and leaves its mark.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use slinger_control::{self as control, DaemonStopped};
use tempfile::TempDir;

use super::Daemon;
use super::process::fake_claude;
use crate::store::SLINGER;
use crate::{Search, Server};

fn now_ms() -> u64 {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    u64::try_from(since.as_millis()).unwrap()
}

/// A daemon on the record in `data` that looks for Claude Code with
/// `search` and logs to `log`.
fn start(data: &Path, search: Search, log: Option<&Path>) -> Daemon {
    let (socket_dir, socket) = super::socket_path();
    let server = Server::bind(&socket).unwrap();
    let stopper = server.stopper();
    let store = super::open(data).with_search(None, search).with_log(log.map(Into::into));
    let thread = std::thread::spawn(move || server.serve(&store));
    Daemon { socket, socket_dir, stopper, thread, data: None }
}

#[test]
fn info_names_this_process_what_it_serves_and_the_claude_it_would_run_now() {
    let (data, home) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    // A PATH without claude, and a home that has none yet.
    let bare = home.path().join("bin");
    std::fs::create_dir(&bare).unwrap();
    let search = Search::new(Some(bare.into()), Some(home.path().to_owned()), None);
    let log = data.path().join("slingerd.log");
    let before = now_ms();
    let daemon = start(data.path(), search, Some(&log));
    let client = daemon.client();

    let info = client.daemon_info().unwrap();
    assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(info.build, control::stamp());
    assert_eq!(info.protocol, control::VERSION);
    assert_eq!(info.pid, std::process::id());
    assert!((before..=now_ms()).contains(&info.started_ms), "{}", info.started_ms);
    let exe = std::env::current_exe().unwrap();
    assert_eq!(info.exe, exe.display().to_string());
    assert_eq!(info.socket, daemon.socket.display().to_string());
    assert_eq!(info.data_dir, data.path().display().to_string());
    assert_eq!(info.claude, None);
    assert_eq!(info.slinger, SLINGER);
    assert_eq!(info.log, Some(log.display().to_string()));

    // Installed after the daemon started: info names it at once, as the
    // next agent's start would find it. A link to the stand-in, not a new
    // executable.
    let local = home.path().join(".local/bin/claude");
    std::fs::create_dir_all(local.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(fake_claude(), &local).unwrap();
    assert_eq!(client.daemon_info().unwrap().claude, Some(local.display().to_string()));
    daemon.stop();
}

#[test]
fn a_daemon_with_no_log_says_none() {
    let daemon = Daemon::start();
    assert_eq!(daemon.client().daemon_info().unwrap().log, None);
    daemon.stop();
}

#[test]
fn stop_replies_ends_the_server_leaves_its_mark_and_answers_what_came_before() {
    let daemon = Daemon::start();
    let data = daemon.data.as_ref().unwrap().path().to_owned();
    // A request begun before the stop.
    let mut stream = UnixStream::connect(&daemon.socket).unwrap();
    stream.write_all(b"{\"v\":1,").unwrap();

    let stopped = daemon.client().daemon_stop().unwrap();
    assert_eq!(stopped, DaemonStopped { pid: std::process::id() });
    stream.write_all(b"\"op\":\"canvas_list\"}\n").unwrap();
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).unwrap();
    assert_eq!(reply, "{\"canvases\":[],\"ok\":true}\n");

    // Serve returns on its own; the socket file is gone.
    let Daemon { socket, socket_dir, thread, data: kept, .. } = daemon;
    thread.join().unwrap().unwrap();
    assert!(!socket.exists());
    assert_eq!(super::left_in(socket_dir.path()), ["d.sock.lock"]);
    let mark = std::fs::read_to_string(control::stop_mark(&data)).unwrap();
    assert_eq!(mark, format!("{}\n", std::process::id()));
    drop(kept);
}
