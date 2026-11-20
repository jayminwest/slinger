//! The socket client: one connection per request, one typed method per verb.
//!
//! A reply slower than 5 s (a daemon under disk load) is still an answer;
//! the client waits 30 s at most, and no reply in that time is `unavailable`
//! with a hint to retry or restart, not to start the daemon (the tests, with a
//! fake daemon).

use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use crate::{RemoteError, Verb, wire};

/// The daemon's program name, for messages.
const DAEMON: &str = "slingerd";

/// How long a client waits for the daemon to answer. A daemon that took the
/// connection is there: its reply can wait on durable writes behind a busy
/// disk (11 s seen under load, slinger-9b1389), and is not to be taken for
/// no daemon. Bounded, so a hung one is still noticed.
pub(crate) const TIMEOUT: Duration = Duration::from_secs(30);

/// A client of the daemon on one socket path. Making one does no IO.
#[derive(Debug, Clone)]
pub struct Client {
    socket: PathBuf,
}

impl Client {
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self { socket: socket.into() }
    }

    /// Sends `ask` and reads the reply as its verb's.
    pub(crate) fn call<V: Verb>(&self, ask: V) -> Result<V::Reply, RemoteError> {
        self.call_within(ask, TIMEOUT)
    }

    /// [`Client::call`] for a request the daemon may take `timeout` to answer.
    ///
    /// Only reading the reply's type depends on `V`; the rest is
    /// [`Client::answer`], so each verb does not build another copy of the
    /// socket IO (slinger-548ea9).
    pub(crate) fn call_within<V: Verb>(
        &self,
        ask: V,
        timeout: Duration,
    ) -> Result<V::Reply, RemoteError> {
        let line = wire::request_line(&ask.into()).map_err(|e| self.unavailable(&e, timeout))?;
        serde_json::from_value(self.answer(&line, timeout)?).map_err(internal)
    }

    /// Sends the request line and returns the reply's document when it is a
    /// success, or the daemon's error.
    #[inline(never)]
    fn answer(&self, line: &str, timeout: Duration) -> Result<Value, RemoteError> {
        let answer = self.exchange(line, timeout).map_err(|e| self.unavailable(&e, timeout))?;
        let doc: Value = serde_json::from_str(&answer).map_err(internal)?;
        match doc.get("ok").and_then(Value::as_bool) {
            Some(true) => Ok(doc),
            Some(false) => Err(serde_json::from_value(doc).map_err(internal)?),
            None => Err(internal("no `ok` field")),
        }
    }

    /// Sends the request line and returns the reply line.
    fn exchange(&self, line: &str, timeout: Duration) -> std::io::Result<String> {
        let mut stream = crate::connect_socket(&self.socket)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(TIMEOUT))?;
        stream.write_all(line.as_bytes())?;
        let mut answer = String::new();
        if BufReader::new(stream).read_line(&mut answer)? == 0 {
            return Err(std::io::Error::other("the connection closed before a reply"));
        }
        Ok(answer)
    }

    /// No daemon, or one that took the request and gave no reply in
    /// `timeout`. Only a connected socket times out (`SO_RCVTIMEO`, which macOS
    /// reports as `WouldBlock`): that daemon is running, so starting it is no
    /// help (slinger-2f9883).
    fn unavailable(&self, e: &std::io::Error, timeout: Duration) -> RemoteError {
        let socket = self.socket.display();
        if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) {
            return RemoteError::new(
                "unavailable",
                format!("{DAEMON} on {socket} did not reply within {timeout:?}"),
                Some(format!("it is running but busy or stuck: try again, or restart {DAEMON}")),
            );
        }
        RemoteError::new(
            "unavailable",
            format!("{DAEMON} does not answer on {socket}: {e}"),
            Some(format!("start it with '{DAEMON} --socket {socket}'")),
        )
    }
}

/// `path` from the root: the daemon runs in another directory.
pub(crate) fn absolute(path: &Path) -> Result<PathBuf, RemoteError> {
    std::path::absolute(path).map_err(|e| {
        RemoteError::new("invalid", format!("cannot resolve {}: {e}", path.display()), None)
    })
}

/// The daemon sent a reply this client cannot read: the two disagree on the protocol.
fn internal(what: impl std::fmt::Display) -> RemoteError {
    RemoteError::new("internal", format!("bad reply from {DAEMON}: {what}"), None)
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixListener;

    use super::*;

    /// A one-shot fake daemon that answers any request with `reply`. The real
    /// daemon's tests cover the real answers; this covers replies it never sends.
    /// Its socket is in a new directory, removed when the directory drops.
    fn answering(reply: &'static str) -> (Client, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("d.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            stream.write_all(reply.as_bytes()).unwrap();
        });
        (Client::new(&socket), dir)
    }

    fn list(reply: &'static str) -> Result<Vec<String>, RemoteError> {
        let (client, _dir) = answering(reply);
        client.canvas_list()
    }

    #[test]
    fn returns_the_reply_or_the_daemons_error() {
        let reply = "{\"ok\":true,\"canvases\":[{\"canvas\":\"ada\"}]}\n";
        assert_eq!(list(reply).unwrap(), ["ada"]);
        let err = list("{\"ok\":false,\"code\":\"invalid\",\"error\":\"no\",\"hint\":\"h\"}\n");
        assert_eq!(err.unwrap_err(), RemoteError::new("invalid", "no", Some("h".into())));
    }

    #[test]
    fn a_reply_it_cannot_read_is_internal() {
        for reply in ["nope\n", "{}\n", "{\"ok\":true}\n", "{\"ok\":false}\n"] {
            assert_eq!(list(reply).unwrap_err().code(), "internal", "{reply}");
        }
    }

    #[test]
    fn no_daemon_or_no_reply_is_unavailable_with_a_start_hint() {
        let err = Client::new("/nonexistent/x.sock").canvas_list().unwrap_err();
        assert_eq!(err.code(), "unavailable");
        assert!(err.hint().unwrap().ends_with(" --socket /nonexistent/x.sock'"));
        assert_eq!(list("").unwrap_err().code(), "unavailable");
    }

    /// slinger-2f9883: a daemon that took the request and gave no reply is
    /// running, so the hint does not say to start it.
    #[test]
    fn no_reply_in_time_is_unavailable_without_a_start_hint() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("d.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let (done, wait) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            BufReader::new(&stream).read_line(&mut String::new()).unwrap();
            let _ = wait.recv();
        });
        let client = Client::new(&socket);
        let err =
            client.call_within(crate::CanvasListAsk {}, Duration::from_millis(200)).unwrap_err();
        drop(done);
        assert_eq!(err.code(), "unavailable");
        assert!(err.to_string().ends_with(" did not reply within 200ms"), "{err}");
        assert!(!err.hint().unwrap().contains("start it"), "{:?}", err.hint());
    }

    /// slinger-9b1389: under load a daemon's reply can take longer than the
    /// 5 s a client used to wait, and that daemon is not unavailable.
    #[test]
    fn a_daemon_slow_to_reply_still_answers() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("d.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            BufReader::new(&stream).read_line(&mut String::new()).unwrap();
            std::thread::sleep(Duration::from_secs(6));
            stream.write_all(b"{\"ok\":true,\"canvases\":[]}\n").unwrap();
        });
        assert_eq!(Client::new(&socket).canvas_list(), Ok(vec![]));
    }
}
