//! The socket client: one connection per request, one typed method per verb.

use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{
    Appended, CanvasChange, CanvasImported, CanvasInfo, CanvasOps, CanvasPage, CanvasStep,
    Canvases, EventFilter, Events, Exported, NewEvent, RemoteError, Request, Stored, Ui, UiPut,
    Uis, Written, wire,
};

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

    /// Appends an event and returns it as stored.
    ///
    /// # Errors
    ///
    /// `invalid` for an unknown kind or a payload that does not fit it, and
    /// `unavailable` when the daemon does not answer.
    pub fn record_append(&self, event: NewEvent) -> Result<Value, RemoteError> {
        let reply: Appended = self.call(&Request::RecordAppend(event))?;
        Ok(reply.event)
    }

    /// The events `filter` selects, lowest number first. None is not an error.
    ///
    /// # Errors
    ///
    /// The daemon's error, or `unavailable` when it does not answer.
    pub fn record_list(&self, filter: EventFilter) -> Result<Vec<Value>, RemoteError> {
        let reply: Events = self.call(&Request::RecordList(filter))?;
        Ok(reply.events)
    }

    /// Stores the bytes of the file at `file` and returns the blob's id.
    /// Storing bytes already held returns the same id. A relative path is
    /// taken from this process's working directory.
    ///
    /// # Errors
    ///
    /// `not_found` when there is no such file, `invalid` when the daemon
    /// cannot read it, and `unavailable` when the daemon does not answer.
    pub fn blob_put(&self, file: &Path) -> Result<String, RemoteError> {
        let reply: Stored = self.call(&Request::BlobPut { path: absolute(file)? })?;
        Ok(reply.id)
    }

    /// Writes the bytes of the blob `id` to the file at `file`, replacing it,
    /// and returns how many there were.
    ///
    /// # Errors
    ///
    /// `not_found` when the record holds no such blob, `invalid` for an id
    /// that is not one or a file the daemon cannot write, and `unavailable`
    /// when the daemon does not answer.
    pub fn blob_get(&self, id: &str, file: &Path) -> Result<u64, RemoteError> {
        let request = Request::BlobGet { id: id.to_string(), path: absolute(file)? };
        let reply: Written = self.call(&request)?;
        Ok(reply.size)
    }

    /// Stores the HTML file at `file` as the next version of the UI `name`,
    /// put by `source`. Bytes equal to the latest version's add none.
    ///
    /// # Errors
    ///
    /// `invalid` for a name or code the rules refuse (docs/design/live-items.md),
    /// `not_found` when there is no such file, and `unavailable` when the
    /// daemon does not answer.
    pub fn ui_put(&self, name: &str, file: &Path, source: &str) -> Result<UiPut, RemoteError> {
        let path = absolute(file)?;
        self.call(&Request::UiPut { name: name.into(), path, source: source.into() })
    }

    /// The latest version of every UI, by name. With `name`, every version of
    /// that UI, oldest first.
    ///
    /// # Errors
    ///
    /// `not_found` for a name with no versions, and `unavailable` when the
    /// daemon does not answer.
    pub fn ui_list(&self, name: Option<&str>) -> Result<Vec<Ui>, RemoteError> {
        let request = name.map_or(Request::UiList, |name| Request::UiShow { name: name.into() });
        let reply: Uis = self.call(&request)?;
        Ok(reply.uis)
    }

    /// Writes the version `ui` names into the directory `dir`, creating it.
    ///
    /// # Errors
    ///
    /// `not_found` when nothing matches `ui`, `invalid` for a reference of
    /// no known form or a directory the daemon cannot write, and
    /// `unavailable` when the daemon does not answer.
    pub fn ui_export(&self, ui: &str, dir: &Path) -> Result<Exported, RemoteError> {
        self.call(&Request::UiExport { ui: ui.into(), dir: absolute(dir)? })
    }

    /// Makes a new, empty canvas.
    ///
    /// # Errors
    ///
    /// `invalid` for an id that is not one, `conflict` when it exists, and
    /// `unavailable` when the daemon does not answer.
    pub fn canvas_create(&self, canvas: &str) -> Result<(), RemoteError> {
        let _: CanvasInfo =
            self.call(&Request::CanvasCreate { canvas: canvas.into(), path: None, by: None })?;
        Ok(())
    }

    /// Makes a new canvas that holds the operations in the file `ops` (a
    /// JSON list, as [`CanvasOps::ops`]) applied as `by` (`user` or
    /// `agent:<name>`; `None`: the user), so only that author's edits and
    /// removes reach them later: the whole canvas, or none. Its first page
    /// is `p1`.
    ///
    /// # Errors
    ///
    /// As [`Client::canvas_create`], `not_found` for a file that is not
    /// there, and `invalid` for one that is not JSON, an operation the
    /// canvas refuses or a `by` that is not an author.
    pub fn canvas_create_with(
        &self,
        canvas: &str,
        ops: &Path,
        by: Option<&str>,
    ) -> Result<(), RemoteError> {
        let (path, by) = (Some(absolute(ops)?), by.map(str::to_string));
        let _: CanvasInfo =
            self.call(&Request::CanvasCreate { canvas: canvas.into(), path, by })?;
        Ok(())
    }

    /// The ids of the canvases, sorted.
    ///
    /// # Errors
    ///
    /// The daemon's error, or `unavailable` when it does not answer.
    pub fn canvas_list(&self) -> Result<Vec<String>, RemoteError> {
        let reply: Canvases = self.call(&Request::CanvasList)?;
        Ok(reply.canvases.into_iter().map(|c| c.canvas).collect())
    }

    /// One page of a canvas: the first, or the one `page` names by id or name.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas or page that is not there, and `unavailable`
    /// when the daemon does not answer.
    pub fn canvas_show(&self, canvas: &str, page: Option<&str>) -> Result<CanvasPage, RemoteError> {
        let page = page.map(str::to_string);
        self.call(&Request::CanvasShow { canvas: canvas.to_string(), page })
    }

    /// Applies operations to a canvas as one undo step, and returns what was
    /// written. The operations are on disk when this returns.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas or item that is not there, `invalid` for an
    /// operation that is not one or that the canvas refuses, and
    /// `unavailable` when the daemon does not answer.
    pub fn canvas_apply(&self, ops: CanvasOps) -> Result<CanvasChange, RemoteError> {
        self.call(&Request::CanvasApply(ops))
    }

    /// Undoes the last step of `step.by` on the canvas.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas that is not there, and `unavailable` when the
    /// daemon does not answer.
    pub fn canvas_undo(&self, step: CanvasStep) -> Result<CanvasChange, RemoteError> {
        self.call(&Request::CanvasUndo(step))
    }

    /// Makes a new canvas from the tldraw file `file`, named `canvas` or,
    /// without one, after the file.
    ///
    /// # Errors
    ///
    /// `not_found` for a file that is not there, `invalid` for one that is
    /// not a tldraw file or a name that is not a canvas id, `conflict` when
    /// the canvas exists, and `unavailable` when the daemon does not answer.
    pub fn canvas_import(
        &self,
        file: &Path,
        canvas: Option<&str>,
    ) -> Result<CanvasImported, RemoteError> {
        let (path, canvas) = (absolute(file)?, canvas.map(str::to_string));
        self.call(&Request::CanvasImport { path, canvas })
    }

    /// As [`Client::canvas_import`], but returns only the new canvas's id:
    /// a binary that never reads what the import made and skipped (the app,
    /// whose size budget is nearly full) carries no code for it.
    ///
    /// # Errors
    ///
    /// As [`Client::canvas_import`].
    pub fn canvas_import_id(
        &self,
        file: &Path,
        canvas: Option<&str>,
    ) -> Result<String, RemoteError> {
        let (path, canvas) = (absolute(file)?, canvas.map(str::to_string));
        let info: CanvasInfo = self.call(&Request::CanvasImport { path, canvas })?;
        Ok(info.canvas)
    }

    /// Redoes the last step `step.by` undid on the canvas.
    ///
    /// # Errors
    ///
    /// As [`Client::canvas_undo`].
    pub fn canvas_redo(&self, step: CanvasStep) -> Result<CanvasChange, RemoteError> {
        self.call(&Request::CanvasRedo(step))
    }

    /// Sends `request` (a [`Request`], or a type that writes the same line)
    /// and reads the reply.
    pub(crate) fn call<R: Serialize, T: DeserializeOwned>(
        &self,
        request: &R,
    ) -> Result<T, RemoteError> {
        self.call_within(request, TIMEOUT)
    }

    /// [`Client::call`] for a request the daemon may take `timeout` to answer.
    ///
    /// Only writing the request and reading the reply's type depend on the
    /// types; the rest is [`Client::answer`], so each new pair of them does
    /// not build another copy of the socket IO into every binary (slinger-548ea9).
    pub(crate) fn call_within<R: Serialize, T: DeserializeOwned>(
        &self,
        request: &R,
        timeout: Duration,
    ) -> Result<T, RemoteError> {
        let line = wire::request_line(request).map_err(|e| self.unavailable(&e, timeout))?;
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
        let mut stream = UnixStream::connect(&self.socket)?;
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
        let err = client
            .call_within::<_, Canvases>(&Request::CanvasList, Duration::from_millis(200))
            .unwrap_err();
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
