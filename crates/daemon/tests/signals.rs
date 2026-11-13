//! The real binary: it serves, and SIGTERM ends it cleanly, even with a
//! client waiting on a canvas feed. While it serves, its data directory
//! names its socket.

use std::process::{Command, Stdio};
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
