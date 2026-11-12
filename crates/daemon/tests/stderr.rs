//! The real binary: a connection thread can write to stderr while the daemon
//! serves (slinger-63289a). A torn canvas file is cut, said on stderr, and
//! the request that opened it is answered. Every wait has a deadline, and
//! the daemon is killed on every path, so a regression fails, never hangs.

use std::io::Write;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use slinger_control::Client;

/// A running slingerd, killed if dropped before [`Daemon::stop`].
struct Daemon(Option<Child>);

#[expect(clippy::unwrap_used, clippy::panic, reason = "a test helper fails its test")]
impl Daemon {
    fn start(socket: &Path, data: &Path) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_slingerd"))
            .arg("--socket")
            .arg(socket)
            .arg("--data-dir")
            .arg(data)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let daemon = Self(Some(child));
        let deadline = Instant::now() + Duration::from_secs(10);
        while let Err(e) = Client::new(socket).canvas_list() {
            assert!(Instant::now() < deadline, "the daemon never answered: {e}");
            std::thread::sleep(Duration::from_millis(20));
        }
        daemon
    }

    /// Stops it with SIGTERM and returns its output. One still running after
    /// ten seconds (a thread stuck on stderr keeps `serve` from returning)
    /// is killed, and the test fails.
    fn stop(mut self) -> Output {
        let mut child = self.0.take().unwrap();
        let pid = child.id().to_string();
        let termed = Command::new("kill").arg("-TERM").arg(pid).status().unwrap();
        assert!(termed.success());
        let deadline = Instant::now() + Duration::from_secs(10);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() > deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                let stderr = String::from_utf8_lossy(&output.stderr);
                panic!("slingerd did not stop on SIGTERM; stderr: {stderr}");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        child.wait_with_output().unwrap()
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn a_connection_thread_can_write_to_stderr() {
    let socket_dir = tempfile::tempdir().unwrap();
    let socket = socket_dir.path().join("e.sock");
    let data = tempfile::tempdir().unwrap();
    let first = Daemon::start(&socket, data.path());
    Client::new(&socket).canvas_create("main").unwrap();
    assert!(first.stop().status.success());

    let file = data.path().join("canvases/main.canvas");
    let mut canvas = std::fs::OpenOptions::new().append(true).open(&file).unwrap();
    canvas.write_all(b"{\"torn").unwrap();
    drop(canvas);

    let second = Daemon::start(&socket, data.path());
    let (sent, answered) = mpsc::channel();
    let client = Client::new(&socket);
    std::thread::spawn(move || sent.send(client.canvas_show("main", None).map(|_| ())));
    let shown = answered.recv_timeout(Duration::from_secs(10));
    let output = second.stop();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(matches!(shown, Ok(Ok(()))), "{shown:?}; stderr: {stderr}");
    assert!(stderr.contains("cut a torn last line (6 bytes)"), "{stderr}");
}
