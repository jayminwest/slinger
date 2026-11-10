//! The control protocol of `slingerd` (docs/service.md): the requests, the
//! replies, the wire format and the socket client.
//!
//! This crate owns the wire format. The daemon and its clients see typed
//! values only, so a change to the format is a change here and nowhere else.
//! It does not link the core: a client stays small and cannot call around the
//! daemon.

mod client;
mod wire;

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use client::Client;
pub use wire::{MAX_LINE, VERSION, read_request, write_reply};

/// A request to the daemon. One variant per verb; `op` on the wire names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Ask for a greeting.
    Hello { name: String },
    /// Append an event to this machine's record.
    RecordAppend(NewEvent),
    /// Read events from this machine's record.
    RecordList(EventFilter),
    /// Store the bytes of the file at `path` as a blob. Bytes travel as a
    /// file on this machine, never in the line (docs/service.md).
    BlobPut { path: PathBuf },
    /// Write the bytes of the blob `id` to the file at `path`.
    BlobGet { id: String, path: PathBuf },
    /// Store the HTML file at `path` as the next version of the UI `name`.
    UiPut { name: String, path: PathBuf, source: String },
    /// The latest version of every UI.
    UiList,
    /// Every version of the UI `name`.
    UiShow { name: String },
    /// Write the version `ui` names (`name`, `name@3` or a blob id) into the
    /// directory `dir`, as `<name>.v<version>.html`.
    UiExport { ui: String, dir: PathBuf },
    /// Make a new, empty canvas.
    CanvasCreate { canvas: String },
    /// List the canvases.
    CanvasList,
    /// One page of a canvas: the first, or the one `page` names (id or name).
    CanvasShow {
        canvas: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        page: Option<String>,
    },
    /// Apply operations to a canvas, as one undo step.
    CanvasApply(CanvasOps),
    /// Undo the last step of `by` on a canvas.
    CanvasUndo(CanvasStep),
    /// Redo the last step `by` undid on a canvas.
    CanvasRedo(CanvasStep),
}

/// A successful answer. One variant per verb, holding that verb's reply type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Reply {
    Hello(Greeting),
    RecordAppend(Appended),
    RecordList(Events),
    BlobPut(Stored),
    BlobGet(Written),
    UiPut(UiPut),
    UiList(Uis),
    UiShow(Uis),
    UiExport(Exported),
    CanvasCreate(CanvasInfo),
    CanvasList(Canvases),
    CanvasShow(CanvasPage),
    CanvasApply(CanvasChange),
    CanvasUndo(CanvasChange),
    CanvasRedo(CanvasChange),
}

/// The reply to [`Request::Hello`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Greeting {
    pub greeting: String,
}

/// An event to append. The daemon sets its number, time and machine tag,
/// and checks `payload` against `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewEvent {
    pub kind: String,
    /// The program or agent that writes it.
    pub source: String,
    #[serde(default)]
    pub tags: NewTags,
    #[serde(default = "empty_object")]
    pub payload: Value,
}

/// The tags a writer sets. The daemon adds `machine`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewTags {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<String>,
}

/// Which events to read. Each field set narrows; none set reads them all.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<String>,
    /// Only events numbered above this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<u64>,
}

/// The reply to [`Request::RecordAppend`]: the event as stored, in the
/// record's event format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appended {
    pub event: Value,
}

/// The reply to [`Request::RecordList`]: the events, lowest number first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Events {
    pub events: Vec<Value>,
}

/// The reply to [`Request::BlobPut`]: the blob's id, `sha256:<hex>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stored {
    pub id: String,
}

/// The reply to [`Request::BlobGet`]: how many bytes were written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Written {
    pub size: u64,
}

/// One version of an agent UI, as the record holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ui {
    pub name: String,
    pub version: u64,
    /// The code's blob id.
    pub hash: String,
    /// The code's size.
    pub bytes: u64,
    /// Who put it.
    pub source: String,
    /// When, in milliseconds since the Unix epoch, UTC.
    pub time: u64,
}

/// The reply to [`Request::UiPut`]: the version that holds the bytes, and
/// whether this put made it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiPut {
    pub ui: Ui,
    pub new: bool,
}

/// The reply to [`Request::UiList`] and [`Request::UiShow`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Uis {
    pub uis: Vec<Ui>,
}

/// The reply to [`Request::UiExport`]: the version written and its file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exported {
    pub ui: Ui,
    pub path: PathBuf,
}

/// Operations to apply to a canvas (docs/service.md, "Canvas operations").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasOps {
    pub canvas: String,
    /// Who asks: `user` or `agent:<name>`.
    #[serde(default = "user")]
    pub by: String,
    /// One operation object, or a list of them.
    pub ops: Value,
}

/// Who takes an undo or redo step on which canvas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasStep {
    pub canvas: String,
    /// `user` or `agent:<name>`.
    #[serde(default = "user")]
    pub by: String,
}

/// One canvas, as listed. The reply to [`Request::CanvasCreate`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasInfo {
    pub canvas: String,
}

/// The reply to [`Request::CanvasList`]: the canvases, by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Canvases {
    pub canvases: Vec<CanvasInfo>,
}

/// The reply to [`Request::CanvasShow`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasPage {
    pub canvas: String,
    /// The sequence number the canvas's next operation gets.
    pub next: u64,
    /// The id of the page shown.
    pub page: String,
    /// Every page, in order: `{"id", "name"}`.
    pub pages: Vec<Value>,
    /// The page's items in drawing order, as the canvas file writes them.
    pub items: Vec<Value>,
}

/// What a change to a canvas wrote: the reply to apply, undo and redo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasChange {
    /// The sequence number of each operation written, in order.
    pub seq: Vec<u64>,
    /// The ids of the items and pages it made, in order.
    pub made: Vec<String>,
}

fn user() -> String {
    "user".into()
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

/// A failure as it crosses the socket: the core's error, or one of the
/// protocol's own (`bad_request`, `too_large`, `unsupported_version`,
/// `unavailable`, `internal`). Callers branch on [`RemoteError::code`], never
/// on the message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteError {
    code: String,
    error: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
}

impl RemoteError {
    pub fn new(code: &str, error: impl Into<String>, hint: Option<String>) -> Self {
        Self { code: code.to_string(), error: error.into(), hint }
    }

    /// Stable machine-readable code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// What the caller can do about it, if anything.
    #[must_use]
    pub fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }
}

impl fmt::Display for RemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.error)
    }
}

impl std::error::Error for RemoteError {}

const SOCKET_FILE: &str = "slinger.sock";

/// Where the daemon listens when no path is given: `$XDG_RUNTIME_DIR`, else
/// the temporary directory (per user on macOS).
#[must_use]
pub fn default_socket() -> PathBuf {
    socket_in(std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from), std::env::temp_dir())
}

fn socket_in(runtime: Option<PathBuf>, temp: PathBuf) -> PathBuf {
    runtime.filter(|dir| !dir.as_os_str().is_empty()).unwrap_or(temp).join(SOCKET_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_socket_prefers_the_runtime_dir() {
        let (run, temp) = (PathBuf::from("/run/user/1"), PathBuf::from("/tmp"));
        assert_eq!(socket_in(Some(run.clone()), temp.clone()), run.join(SOCKET_FILE));
        assert_eq!(socket_in(Some("".into()), temp.clone()), temp.join(SOCKET_FILE));
        assert_eq!(socket_in(None, temp.clone()), temp.join(SOCKET_FILE));
        assert!(default_socket().ends_with(SOCKET_FILE));
    }

    #[test]
    fn a_remote_error_shows_its_message_and_keeps_its_code() {
        let err = RemoteError::new("invalid", "name is empty", Some("pass a name".into()));
        assert_eq!(err.to_string(), "name is empty");
        assert_eq!((err.code(), err.hint()), ("invalid", Some("pass a name")));
    }
}
