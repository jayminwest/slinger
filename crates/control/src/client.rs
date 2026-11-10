//! The socket client: one connection per request, one typed method per verb.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{Greeting, RemoteError, Request, wire};

/// The daemon's program name, for messages.
const DAEMON: &str = "slingerd";

/// How long a client waits for the daemon to answer.
const TIMEOUT: Duration = Duration::from_secs(5);

/// A client of the daemon on one socket path. Making one does no IO.
#[derive(Debug, Clone)]
pub struct Client {
    socket: PathBuf,
}

impl Client {
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self { socket: socket.into() }
    }

    /// Asks for a greeting.
    ///
    /// # Errors
    ///
    /// The daemon's error, or `unavailable` when it does not answer.
    pub fn hello(&self, name: &str) -> Result<String, RemoteError> {
        let reply: Greeting = self.call(&Request::Hello { name: name.to_string() })?;
        Ok(reply.greeting)
    }

    fn call<T: DeserializeOwned>(&self, request: &Request) -> Result<T, RemoteError> {
        let answer = self.exchange(request).map_err(|e| self.unavailable(&e.to_string()))?;
        let doc: Value = serde_json::from_str(&answer).map_err(internal)?;
        match doc.get("ok").and_then(Value::as_bool) {
            Some(true) => serde_json::from_value(doc).map_err(internal),
            Some(false) => Err(serde_json::from_value(doc).map_err(internal)?),
            None => Err(internal("no `ok` field")),
        }
    }

    /// Sends the request line and returns the reply line.
    fn exchange(&self, request: &Request) -> std::io::Result<String> {
        let mut stream = UnixStream::connect(&self.socket)?;
        stream.set_read_timeout(Some(TIMEOUT))?;
        stream.set_write_timeout(Some(TIMEOUT))?;
        stream.write_all(wire::request_line(request)?.as_bytes())?;
        let mut answer = String::new();
        if BufReader::new(stream).read_line(&mut answer)? == 0 {
            return Err(std::io::Error::other("the connection closed before a reply"));
        }
        Ok(answer)
    }

    fn unavailable(&self, why: &str) -> RemoteError {
        let socket = self.socket.display();
        RemoteError::new(
            "unavailable",
            format!("{DAEMON} does not answer on {socket}: {why}"),
            Some(format!("start it with '{DAEMON} --socket {socket}'")),
        )
    }
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
    fn answering(reply: &'static str) -> (Client, PathBuf) {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let socket = std::env::temp_dir().join(format!("ctl-{}-{n}.sock", std::process::id()));
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            stream.write_all(reply.as_bytes()).unwrap();
        });
        (Client::new(&socket), socket)
    }

    fn hello(reply: &'static str) -> Result<String, RemoteError> {
        let (client, socket) = answering(reply);
        let result = client.hello("ada");
        std::fs::remove_file(socket).unwrap();
        result
    }

    #[test]
    fn returns_the_reply_or_the_daemons_error() {
        assert_eq!(hello("{\"ok\":true,\"greeting\":\"hello, ada\"}\n").unwrap(), "hello, ada");
        let err = hello("{\"ok\":false,\"code\":\"invalid\",\"error\":\"no\",\"hint\":\"h\"}\n");
        assert_eq!(err.unwrap_err(), RemoteError::new("invalid", "no", Some("h".into())));
    }

    #[test]
    fn a_reply_it_cannot_read_is_internal() {
        for reply in ["nope\n", "{}\n", "{\"ok\":true}\n", "{\"ok\":false}\n"] {
            assert_eq!(hello(reply).unwrap_err().code(), "internal", "{reply}");
        }
    }

    #[test]
    fn no_daemon_or_no_reply_is_unavailable_with_a_start_hint() {
        let err = Client::new("/nonexistent/x.sock").hello("ada").unwrap_err();
        assert_eq!(err.code(), "unavailable");
        assert!(err.hint().unwrap().ends_with(" --socket /nonexistent/x.sock'"));
        assert_eq!(hello("").unwrap_err().code(), "unavailable");
    }
}
