//! The real binary: it serves, and SIGTERM ends it cleanly, even with a
//! client waiting on a canvas feed. While it serves, its data directory
//! names its socket. With `--log` its stderr and its agents' go to a file,
//! and SIGHUP leaves it serving.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use slinger_control::Client;

#[test]
fn sigterm_stops_the_daemon_cleanly() {
    // Its own directory, so no other run shares the path.
    let socket_dir = tempfile::tempdir().unwrap();
    let socket = socket_dir.path().join("s.sock");
    let data = tempfile::tempdir().unwrap();
    let daemon = Command::new(env!("CARGO_BIN_EXE_slingerd"))
        .arg("--socket")
        .arg(&socket)
        .arg("--data-dir")
        .arg(data.path())
        .arg("--scratch")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let client = Client::new(&socket);
    let deadline = Instant::now() + Duration::from_secs(10);
    let canvases = loop {
        match client.canvas_list() {
            Ok(canvases) => break canvases,
            Err(e) if Instant::now() > deadline => panic!("the daemon never answered: {e}"),
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    assert_eq!(canvases, Vec::<String>::new());
    // Its data directory names the socket it listens on, for clients given none.
    assert_eq!(slinger_control::named_socket(data.path()), Some(socket.clone()));
    client.canvas_create("main").unwrap();
    let waiter = client.clone();
    let waiting =
        std::thread::spawn(move || waiter.canvas_changes("main", 0, Duration::from_secs(30)));
    std::thread::sleep(Duration::from_millis(100));

    let killed = Command::new("kill").arg("-TERM").arg(daemon.id().to_string()).status().unwrap();
    assert!(killed.success());
    let output = daemon.wait_with_output().unwrap();
    assert!(output.status.success(), "{:?}", output.status);
    let feed = waiting.join().unwrap().unwrap();
    assert_eq!((feed.ops.len(), feed.next), (0, 1));
    // Silence is golden: a clean run says nothing.
    assert_eq!((output.stdout.as_slice(), output.stderr.as_slice()), (&b""[..], &b""[..]));
    assert!(!socket.exists());
    assert_eq!(slinger_control::named_socket(data.path()), None, "the name goes with the socket");
    assert!(data.path().join("record.sqlite").exists());
}

/// Waits until `met` holds, or fails after 10 s.
fn until(what: &str, mut met: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !met() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// slingerd with `--log`, in a fresh directory for its socket, on `data`.
#[expect(clippy::unwrap_used, reason = "a test helper fails its test")]
fn start_logged(socket: &Path, data: &Path, log: &Path) -> Child {
    let claude = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tests/claude");
    let daemon = Command::new(env!("CARGO_BIN_EXE_slingerd"))
        .arg("--socket")
        .arg(socket)
        .arg("--data-dir")
        .arg(data)
        .arg("--scratch")
        .arg("--claude")
        .arg(claude)
        .arg("--log")
        .arg(log)
        .env("SLINGER_TEST_STDERR", "stand-in claude says hello")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    until("the daemon to answer", || Client::new(socket).canvas_list().is_ok());
    daemon
}

/// Sends `signal` to `daemon`.
#[expect(clippy::unwrap_used, reason = "a test helper fails its test")]
fn signal(daemon: &Child, signal: &str) {
    let sent = Command::new("kill").arg(signal).arg(daemon.id().to_string()).status().unwrap();
    assert!(sent.success());
}

/// Stops `daemon` with SIGTERM: it exits 0 and says nothing on its stderr,
/// which is the log's.
#[expect(clippy::unwrap_used, reason = "a test helper fails its test")]
fn stop_logged(daemon: Child) {
    signal(&daemon, "-TERM");
    let output = daemon.wait_with_output().unwrap();
    assert!(output.status.success(), "{:?}", output.status);
    assert_eq!((output.stdout.as_slice(), output.stderr.as_slice()), (&b""[..], &b""[..]));
}

/// A project folder named `name` whose workspace file is `file`.
#[expect(clippy::unwrap_used, reason = "a test helper fails its test")]
fn project(parent: &Path, name: &str, file: &str) -> PathBuf {
    let dir = parent.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("slinger-workspace.toml"), file).unwrap();
    dir.canonicalize().unwrap()
}

#[test]
fn with_a_log_its_and_its_agents_stderr_go_there_and_sighup_does_not_stop_it() {
    let socket_dir = tempfile::tempdir().unwrap();
    let socket = socket_dir.path().join("l.sock");
    let (data, projects) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let log = data.path().join("logs/slingerd.log");
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    let old = vec![b'x'; (1 << 20) + 1];
    std::fs::write(&log, &old).unwrap();

    let daemon = start_logged(&socket, data.path(), &log);
    assert_eq!(std::fs::read(data.path().join("logs/slingerd.log.1")).unwrap(), old, "rotated");
    let client = Client::new(&socket);
    let notes = "version = 1\nname = \"notes\"\ncanvas = \"notes\"\n[agent]\nname = \"scout\"\n";
    client.workspace_open(Some(&project(projects.path(), "notes", notes)), None).unwrap();
    let bad = project(projects.path(), "bad", "version = 1\nname = \"bad\"\ncanvas = \"bad\"\n");
    client.workspace_open(Some(&bad), None).unwrap();
    client.session_start("notes").unwrap();
    let read = || std::fs::read_to_string(&log).unwrap();
    until("the agent's stderr in the log", || read().contains("stand-in claude says hello\n"));

    signal(&daemon, "-HUP");
    std::thread::sleep(Duration::from_millis(100));
    assert!(client.canvas_list().is_ok(), "SIGHUP leaves it serving");
    stop_logged(daemon);

    // A workspace whose file is broken is named in the log as the next one starts.
    let broken =
        "version = 1\nname = \"bad\"\ncanvas = \"bad\"\n[agent]\nname = \"s\"\ntools = [\"X\"]\n";
    std::fs::write(bad.join("slinger-workspace.toml"), broken).unwrap();
    stop_logged(start_logged(&socket, data.path(), &log));
    let text = read();
    let pid_lines: Vec<_> = text.lines().filter(|l| l.contains(" slingerd ")).collect();
    let serving = format!("serving {} on {}", data.path().display(), socket.display());
    assert_eq!(pid_lines.len(), 4, "{text}");
    assert!(pid_lines[0].ends_with(&serving) && pid_lines[2].ends_with(&serving), "{text}");
    assert!(pid_lines[1].ends_with(": stopped") && pid_lines[3].ends_with(": stopped"), "{text}");
    assert!(text.contains("slingerd: workspace not loaded: "), "{text}");
}
