//! The daemon through its real socket: typed clients, raw malformed and
//! oversized lines, startup conflicts and a stop in the middle of a request.

use std::io::{BufRead, BufReader, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread::JoinHandle;

use slinger_control::{Client, MAX_LINE};

use crate::{Server, StartError, Stopper, run};

const DAEMON: &str = "slingerd";

/// A socket path that no other test uses. Short: socket paths have a length limit.
fn socket_path() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("d-{}-{n}.sock", std::process::id()))
}

/// A daemon on its own socket for one test.
struct Daemon {
    socket: PathBuf,
    stopper: Stopper,
    thread: JoinHandle<std::io::Result<()>>,
}

impl Daemon {
    fn start() -> Self {
        let socket = socket_path();
        let server = Server::bind(&socket).unwrap();
        let stopper = server.stopper();
        Self { socket, stopper, thread: std::thread::spawn(move || server.serve()) }
    }

    /// Stops it and checks that it left nothing but its lock file behind.
    fn stop(self) {
        self.stopper.stop();
        self.thread.join().unwrap().unwrap();
        assert!(!self.socket.exists());
        std::fs::remove_file(format!("{}.lock", self.socket.display())).unwrap();
    }

    /// Sends raw bytes, closes the sending side and returns the reply line.
    fn raw(&self, request: &[u8]) -> String {
        let mut stream = UnixStream::connect(&self.socket).unwrap();
        stream.write_all(request).unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut reply = String::new();
        BufReader::new(stream).read_line(&mut reply).unwrap();
        reply
    }
}

#[test]
fn hello_goes_through_the_core_and_back() {
    let daemon = Daemon::start();
    assert_eq!(Client::new(&daemon.socket).hello(" ada ").unwrap(), "hello, ada");
    daemon.stop();
}

#[test]
fn a_core_error_keeps_its_code_and_hint() {
    let daemon = Daemon::start();
    let err = Client::new(&daemon.socket).hello("  ").unwrap_err();
    assert_eq!((err.code(), err.to_string().as_str()), ("invalid", "name is empty"));
    assert!(err.hint().is_some());
    daemon.stop();
}

#[test]
fn malformed_requests_get_an_error_reply_and_the_daemon_keeps_serving() {
    let daemon = Daemon::start();
    for (request, code) in [
        (&b"not json\n"[..], "bad_request"),
        (b"{\"v\":1,\"op\":\"nope\"}\n", "bad_request"),
        (b"{\"op\":\"hello\",\"name\":\"ada\"}\n", "bad_request"),
        (b"{\"v\":9,\"op\":\"hello\",\"name\":\"ada\"}\n", "unsupported_version"),
    ] {
        let reply = daemon.raw(request);
        assert!(reply.contains("\"ok\":false"), "{reply}");
        assert!(reply.contains(&format!("\"code\":\"{code}\"")), "{reply}");
    }
    assert_eq!(daemon.raw(b""), "");
    assert_eq!(Client::new(&daemon.socket).hello("ada").unwrap(), "hello, ada");
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
    assert_eq!(Client::new(&daemon.socket).hello("ada").unwrap(), "hello, ada");
    daemon.stop();
}

#[test]
fn a_socket_file_left_by_a_dead_daemon_is_replaced() {
    let socket = socket_path();
    drop(Server::bind(&socket).unwrap());
    assert!(socket.exists());
    let server = Server::bind(&socket).unwrap();
    let stopper = server.stopper();
    let thread = std::thread::spawn(move || server.serve());
    assert_eq!(Client::new(&socket).hello("ada").unwrap(), "hello, ada");
    Daemon { socket, stopper, thread }.stop();
}

#[test]
fn a_stop_does_not_lose_the_reply_to_a_request_in_flight() {
    let daemon = Daemon::start();
    let mut stream = UnixStream::connect(&daemon.socket).unwrap();
    stream.write_all(b"{\"v\":1,\"op\":\"hello\",").unwrap();
    daemon.stopper.stop();
    stream.write_all(b"\"name\":\"ada\"}\n").unwrap();
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).unwrap();
    assert_eq!(reply, "{\"greeting\":\"hello, ada\",\"ok\":true}\n");
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
}
