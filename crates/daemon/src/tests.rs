//! The daemon through its real socket: typed clients, raw malformed and
//! oversized lines, startup conflicts and a stop in the middle of a request.

use std::io::{BufRead, BufReader, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

use serde_json::json;
use slinger_control::{Client, EventFilter, MAX_LINE, NewEvent, NewTags};
use tempfile::TempDir;

use crate::StoreError;
use crate::testing::when_free;
use crate::{Server, StartError, Stopper, Store, run};

mod canvas;
mod edit;
mod feed;
mod feedback;
mod import;
mod listen;
mod live;
mod look;
mod pane;
mod process;
mod profile;
mod put;
mod read;
mod session;
mod showing;
mod sling;
mod sling_pdf;
mod touched;
mod workspace;

const DAEMON: &str = "slingerd";

/// A socket path in a new directory of its own, so no other test, process or
/// leftover from an earlier run shares it, and it is removed on drop even
/// when the test fails. Short: socket paths have a length limit.
fn socket_path() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let socket = dir.path().join("d.sock");
    (dir, socket)
}

/// Takes `socket`, waiting out a lock a test released a moment ago.
fn bind(socket: &Path) -> Server {
    when_free(|| Server::bind(socket), |e| matches!(e, StartError::Taken(_))).unwrap()
}

/// Opens the record in `data`, waiting out a lock a store or daemon that
/// stopped a moment ago released.
fn open(data: &Path) -> Store {
    when_free(|| Store::open(data, "test-machine"), |e| matches!(e, StoreError::Taken(_))).unwrap()
}

/// Serves `server` with the record in `data` on a new thread. An agent's
/// process cannot start: its program is not there, so no test runs the real
/// Claude Code by accident.
fn serving(server: Server, data: &Path) -> JoinHandle<std::io::Result<()>> {
    let store = open(data).with_claude("/nonexistent/claude");
    std::thread::spawn(move || server.serve(&store))
}

/// A daemon on its own socket for one test.
struct Daemon {
    socket: PathBuf,
    /// The socket's directory: removed on drop.
    socket_dir: TempDir,
    stopper: Stopper,
    thread: JoinHandle<std::io::Result<()>>,
    /// The record's directory when the daemon made it: removed on drop.
    data: Option<TempDir>,
}

impl Daemon {
    fn start() -> Self {
        let data = TempDir::new().unwrap();
        let mut daemon = Self::start_in(data.path());
        daemon.data = Some(data);
        daemon
    }

    fn start_in(data: &Path) -> Self {
        let (socket_dir, socket) = socket_path();
        let server = Server::bind(&socket).unwrap();
        let stopper = server.stopper();
        Self { socket, socket_dir, stopper, thread: serving(server, data), data: None }
    }

    /// A daemon on the record in `data` whose agents' processes run `claude`.
    fn start_with_claude(data: &Path, claude: &Path) -> Self {
        let (socket_dir, socket) = socket_path();
        let server = Server::bind(&socket).unwrap();
        let stopper = server.stopper();
        let store = open(data).with_claude(claude);
        let thread = std::thread::spawn(move || server.serve(&store));
        Self { socket, socket_dir, stopper, thread, data: None }
    }

    fn client(&self) -> Client {
        Client::new(&self.socket)
    }

    /// Stops it and checks that it left nothing but its lock file behind.
    fn stop(self) {
        self.stopper.stop();
        self.thread.join().unwrap().unwrap();
        assert_eq!(left_in(self.socket_dir.path()), ["d.sock.lock"]);
        drop(self.data);
    }

    /// Sends raw bytes, closes the sending side and returns the reply line.
    fn raw(&self, request: &[u8]) -> String {
        let mut stream = UnixStream::connect(&self.socket).unwrap();
        stream.write_all(request).unwrap();
        // A daemon that has the whole line can reply and close first, and
        // macOS then refuses the shutdown; the reply is still there to read.
        match stream.shutdown(Shutdown::Write) {
            Err(e) if e.kind() != std::io::ErrorKind::NotConnected => panic!("{e}"),
            _ => {}
        }
        let mut reply = String::new();
        BufReader::new(stream).read_line(&mut reply).unwrap();
        reply
    }
}

/// The names of the files in `dir`, sorted.
fn left_in(dir: &Path) -> Vec<String> {
    let entries = std::fs::read_dir(dir).unwrap();
    let mut names: Vec<String> =
        entries.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

#[test]
fn a_core_error_keeps_its_code_and_hint() {
    let daemon = Daemon::start();
    let err = Client::new(&daemon.socket).canvas_create(".hidden").unwrap_err();
    assert_eq!(err.code(), "invalid");
    assert!(err.hint().is_some());
    daemon.stop();
}

#[test]
fn malformed_requests_get_an_error_reply_and_the_daemon_keeps_serving() {
    let daemon = Daemon::start();
    for (request, code) in [
        (&b"not json\n"[..], "bad_request"),
        (b"{\"v\":1,\"op\":\"nope\"}\n", "bad_request"),
        (b"{\"op\":\"canvas_list\"}\n", "bad_request"),
        (b"{\"v\":9,\"op\":\"canvas_list\"}\n", "unsupported_version"),
    ] {
        let reply = daemon.raw(request);
        assert!(reply.contains("\"ok\":false"), "{reply}");
        assert!(reply.contains(&format!("\"code\":\"{code}\"")), "{reply}");
    }
    assert_eq!(daemon.raw(b""), "");
    assert_eq!(Client::new(&daemon.socket).canvas_list().unwrap(), Vec::<String>::new());
    daemon.stop();
}

#[test]
fn an_oversized_request_is_refused_after_it_is_read() {
    let daemon = Daemon::start();
    let mut request = vec![b'x'; MAX_LINE * 4];
    request.push(b'\n');
    assert!(daemon.raw(&request).contains("\"code\":\"too_large\""));
    daemon.stop();
}

#[test]
fn one_instance_per_socket_path() {
    let daemon = Daemon::start();
    let err = Server::bind(&daemon.socket).unwrap_err();
    assert!(matches!(err, StartError::Taken(_)));
    assert_eq!(err.code(), "conflict");
    // The loser must not have removed the winner's socket.
    assert_eq!(Client::new(&daemon.socket).canvas_list().unwrap(), Vec::<String>::new());
    daemon.stop();
}

#[test]
fn a_socket_file_left_by_a_dead_daemon_is_replaced() {
    let (socket_dir, socket) = socket_path();
    drop(Server::bind(&socket).unwrap());
    assert!(socket.exists());
    let server = bind(&socket);
    let stopper = server.stopper();
    let data = TempDir::new().unwrap();
    let thread = serving(server, data.path());
    assert_eq!(Client::new(&socket).canvas_list().unwrap(), Vec::<String>::new());
    Daemon { socket, socket_dir, stopper, thread, data: Some(data) }.stop();
}

#[test]
fn a_stop_does_not_lose_the_reply_to_a_request_in_flight() {
    let daemon = Daemon::start();
    let mut stream = UnixStream::connect(&daemon.socket).unwrap();
    stream.write_all(b"{\"v\":1,").unwrap();
    daemon.stopper.stop();
    stream.write_all(b"\"op\":\"canvas_list\"}\n").unwrap();
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).unwrap();
    assert_eq!(reply, "{\"canvases\":[],\"ok\":true}\n");
    daemon.stop();
}

fn new_event(kind: &str, payload: serde_json::Value) -> NewEvent {
    NewEvent { kind: kind.into(), source: "test".into(), tags: NewTags::default(), payload }
}

#[test]
fn an_appended_event_comes_back_numbered_and_survives_a_restart() {
    let data = TempDir::new().unwrap();
    let daemon = Daemon::start_in(data.path());
    let mut new = new_event("agent_done", json!({"summary": "wrote it"}));
    new.tags.agent = Some("scout".into());
    let event = daemon.client().record_append(new).unwrap();
    assert_eq!((event["number"].clone(), event["kind"].clone()), (json!(1), json!("agent_done")));
    assert_eq!(event["tags"], json!({"machine": "test-machine", "agent": "scout"}));
    assert_eq!(event["payload"], json!({"summary": "wrote it"}));
    assert_eq!(event["source"], "test");
    daemon.stop();

    let daemon = Daemon::start_in(data.path());
    assert_eq!(daemon.client().record_list(EventFilter::default()).unwrap(), [event]);
    daemon.stop();
}

#[test]
fn record_list_filters_by_kind_tag_and_number() {
    let daemon = Daemon::start();
    let client = daemon.client();
    for (kind, workspace) in [("agent_done", "a"), ("agent_blocked", "a"), ("agent_done", "b")] {
        let payload = json!({"reason": "why?"});
        let mut new = new_event(kind, payload);
        new.tags.workspace = Some(workspace.into());
        client.record_append(new).unwrap();
    }
    let numbers = |filter: EventFilter| -> Vec<u64> {
        let events = client.record_list(filter).unwrap();
        events.iter().map(|e| e["number"].as_u64().unwrap()).collect()
    };
    let kind = |k: &str| EventFilter { kind: Some(k.into()), ..EventFilter::default() };
    assert_eq!(numbers(EventFilter::default()), [1, 2, 3]);
    assert_eq!(numbers(kind("agent_done")), [1, 3]);
    assert_eq!(numbers(EventFilter { workspace: Some("a".into()), ..kind("agent_done") }), [1]);
    assert_eq!(numbers(EventFilter { after: Some(1), ..kind("agent_done") }), [3]);
    assert_eq!(
        numbers(EventFilter { machine: Some("test-machine".into()), ..Default::default() }),
        [1, 2, 3]
    );
    assert_eq!(numbers(EventFilter { agent: Some("nobody".into()), ..Default::default() }), [0; 0]);
    assert_eq!(numbers(EventFilter { canvas: Some("c".into()), ..Default::default() }), [0; 0]);
    assert_eq!(numbers(EventFilter { after: Some(u64::MAX), ..Default::default() }), [0; 0]);
    daemon.stop();
}

#[test]
fn an_append_the_core_rejects_is_invalid_with_a_hint_and_stores_nothing() {
    let daemon = Daemon::start();
    let client = daemon.client();
    for new in [
        new_event("agent_blocked", json!({})),
        new_event("no_such_kind", json!({})),
        new_event("file_touched", json!({"path": "a", "hash": "nope"})),
    ] {
        let err = client.record_append(new).unwrap_err();
        assert_eq!(err.code(), "invalid");
        assert!(err.hint().is_some());
    }
    assert_eq!(client.record_list(EventFilter::default()).unwrap(), [serde_json::Value::Null; 0]);
    daemon.stop();
}

/// Bytes that are not text and do not repeat: every byte value, in no order.
fn noise(len: usize) -> Vec<u8> {
    let mut x: u32 = 0x9e37_79b9;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x.to_le_bytes()[0]
        })
        .collect()
}

#[test]
fn a_blob_of_megabytes_comes_back_byte_for_byte_and_puts_once() {
    let daemon = Daemon::start();
    let client = daemon.client();
    let dir = TempDir::new().unwrap();
    let (input, output) = (dir.path().join("in"), dir.path().join("out"));
    let bytes = noise(5 * 1024 * 1024);
    std::fs::write(&input, &bytes).unwrap();
    let id = client.blob_put(&input).unwrap();
    assert_eq!(id, slinger_core::record::BlobId::of(&bytes).to_string());
    assert_eq!(client.blob_put(&input).unwrap(), id);
    std::fs::write(&output, b"old contents, longer than nothing").unwrap();
    assert_eq!(client.blob_get(&id, &output).unwrap(), bytes.len() as u64);
    assert!(std::fs::read(&output).unwrap().iter().eq(&bytes), "the bytes differ");
    std::fs::write(&input, b"").unwrap();
    let empty = client.blob_put(&input).unwrap();
    assert_eq!(
        (client.blob_get(&empty, &output).unwrap(), std::fs::read(&output).unwrap()),
        (0, vec![])
    );
    daemon.stop();
}

#[test]
fn a_blob_or_file_that_is_not_there_is_not_found() {
    let daemon = Daemon::start();
    let client = daemon.client();
    let dir = TempDir::new().unwrap();
    let out = dir.path().join("out");
    let unknown = slinger_core::record::BlobId::of(b"never put").to_string();
    let err = client.blob_get(&unknown, &out).unwrap_err();
    assert_eq!(err.code(), "not_found");
    assert!(err.to_string().contains(&unknown) && err.hint().is_some(), "{err}");
    assert!(!out.exists());
    let err = client.blob_put(&dir.path().join("nope")).unwrap_err();
    assert_eq!(err.code(), "not_found");
    assert!(err.hint().is_some());
    daemon.stop();
}

#[test]
fn a_bad_blob_id_or_path_is_invalid_with_a_hint() {
    let daemon = Daemon::start();
    let client = daemon.client();
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("file");
    std::fs::write(&file, "small\n").unwrap();
    let id = client.blob_put(&file).unwrap();
    for err in [
        client.blob_get("sha256:nope", &dir.path().join("out")).unwrap_err(),
        client.blob_put(dir.path()).unwrap_err(),
        client.blob_get(&id, &dir.path().join("no/such/dir")).unwrap_err(),
    ] {
        assert_eq!(err.code(), "invalid", "{err}");
        assert!(err.hint().is_some());
    }
    for request in [
        r#"{"v":1,"op":"blob_put","path":"relative"}"#,
        &format!(r#"{{"v":1,"op":"blob_get","id":"{id}","path":"relative"}}"#),
    ] {
        let reply = daemon.raw(format!("{request}\n").as_bytes());
        assert!(
            reply.contains("\"code\":\"invalid\"") && reply.contains("not absolute"),
            "{reply}"
        );
    }
    daemon.stop();
}

#[test]
fn ui_put_keeps_every_version_and_export_writes_one_back() {
    let daemon = Daemon::start();
    let client = daemon.client();
    let dir = TempDir::new().unwrap();
    let (one, two) = (dir.path().join("one.html"), dir.path().join("two.html"));
    std::fs::write(&one, "<p>one</p>\n").unwrap();
    std::fs::write(&two, "<p>two</p>\n").unwrap();

    let first = client.ui_put("pick-slot", &one, "scout").unwrap();
    assert_eq!((first.ui.version, first.new, first.ui.bytes), (1, true, 11));
    assert_eq!(first.ui.source, "scout");
    assert_eq!(client.ui_put("pick-slot", &two, "scout").unwrap().ui.version, 2);
    let again = client.ui_put("pick-slot", &two, "other").unwrap();
    assert_eq!((again.ui.version, again.new, again.ui.source.as_str()), (2, false, "scout"));
    client.ui_put("another", &one, "scout").unwrap();

    let versions = client.ui_list(Some("pick-slot")).unwrap();
    assert_eq!(versions.iter().map(|v| v.version).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(versions[0], first.ui);
    let latest = client.ui_list(None).unwrap();
    let names: Vec<_> = latest.iter().map(|v| (v.name.as_str(), v.version)).collect();
    assert_eq!(names, [("another", 1), ("pick-slot", 2)]);

    let out = dir.path().join("out/new");
    let exported = client.ui_export("pick-slot@1", &out).unwrap();
    assert_eq!(exported.path, out.join("pick-slot.v1.html"));
    assert_eq!(std::fs::read(&exported.path).unwrap(), std::fs::read(&one).unwrap());
    let by_hash = client.ui_export(&first.ui.hash, &out).unwrap();
    assert_eq!(by_hash.ui.name, "another");

    let events =
        client.record_list(EventFilter { kind: Some("ui_put".into()), ..EventFilter::default() });
    assert_eq!(events.unwrap().len(), 3);
    daemon.stop();
}

#[test]
fn ui_errors_keep_their_codes() {
    let daemon = Daemon::start();
    let client = daemon.client();
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("ui.html");
    std::fs::write(&file, "<p>\0</p>").unwrap();
    for (err, code) in [
        (client.ui_put("pick", &file, "s").unwrap_err(), "invalid"),
        (client.ui_put("Pick", &dir.path().join("none"), "s").unwrap_err(), "not_found"),
        (client.ui_list(Some("pick")).unwrap_err(), "not_found"),
        (client.ui_export("pick@1", dir.path()).unwrap_err(), "not_found"),
        (client.ui_export("pick@x", dir.path()).unwrap_err(), "invalid"),
    ] {
        assert_eq!(err.code(), code, "{err}");
        assert!(err.hint().is_some(), "{err}");
    }
    std::fs::write(&file, "<p></p>").unwrap();
    client.ui_put("pick", &file, "s").unwrap();
    let err = client.ui_export("pick", &file.join("sub")).unwrap_err();
    assert_eq!(err.code(), "invalid", "{err}");
    let reply = daemon.raw(
        br#"{"v":1,"op":"ui_export","ui":"pick","dir":"rel"}
"#,
    );
    assert!(reply.contains("not absolute"), "{reply}");
    daemon.stop();
}

fn daemon(args: &[&str]) -> (u8, String, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(std::iter::once(DAEMON).chain(args.iter().copied()), &mut out, &mut err);
    (code, String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap())
}

#[test]
fn a_taken_socket_is_a_conflict_with_a_hint() {
    let running = Daemon::start();
    let (code, out, err) = daemon(&["--socket", running.socket.to_str().unwrap()]);
    assert_eq!((code, out.as_str()), (4, ""));
    assert!(err.starts_with("✗ socket "), "{err}");
    assert!(err.contains(" is in use by another "), "{err}");
    assert!(err.contains("\n  stop that daemon"), "{err}");
    running.stop();
}

#[test]
fn a_socket_path_it_cannot_create_is_an_error() {
    let (code, _, err) = daemon(&["--socket", "/dev/null/x.sock"]);
    assert_eq!(code, 1);
    assert!(err.starts_with("✗ cannot open socket /dev/null/x.sock"), "{err}");
}

#[test]
fn a_data_dir_it_cannot_create_is_an_error() {
    let (_socket_dir, socket) = socket_path();
    let args = ["--socket", socket.to_str().unwrap(), "--data-dir", "/dev/null/slinger"];
    let (code, out, err) = daemon(&args);
    assert_eq!((code, out.as_str()), (1, ""));
    assert!(err.starts_with("✗ cannot create data directory /dev/null/slinger"), "{err}");
    assert!(err.contains("\n  pass another --data-dir"), "{err}");
    // The lock is free again: a daemon with a good data dir can take the socket.
    drop(bind(&socket));
}

#[test]
fn bad_arguments_are_usage_errors() {
    let (code, out, err) = daemon(&["--bogus"]);
    assert_eq!((code, out.as_str()), (2, ""));
    assert!(err.starts_with("✗ unexpected argument '--bogus'"), "{err}");
    assert!(err.ends_with(" --help'\n"), "{err}");
}

#[test]
fn version_and_help_print_and_succeed() {
    let version = format!("{}\n", env!("CARGO_PKG_VERSION"));
    assert_eq!(daemon(&["--version"]), (0, version, String::new()));
    let (code, out, _) = daemon(&["--help"]);
    assert_eq!(code, 0);
    assert!(out.contains("--socket <PATH>"), "{out}");
    assert!(out.contains("--data-dir <PATH>"), "{out}");
}
