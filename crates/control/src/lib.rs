//! The control protocol of `slingerd` (docs/service.md): the requests, the
//! replies, the wire format and the socket client.
//!
//! This crate owns the wire format. The daemon and its clients see typed
//! values only, so a change to the format is a change here and nowhere else.
//! It does not link the core: a client stays small and cannot call around the
//! daemon.

mod agent;
mod client;
mod feed;
mod feedback;
mod imported;
mod look;
mod packet;
mod pane;
mod place;
mod profile;
mod session;
mod sling;
mod ui;
mod wire;
mod workspace;

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use agent::{
    AgentAsk, CanvasEdit, CanvasEdited, CanvasPlaced, CanvasPut, CanvasRemove, CanvasRemoved, Put,
};
pub use client::Client;
pub use feed::{CanvasFeed, MAX_WAIT_MS};
pub use feedback::{
    FeedbackAdded, FeedbackAsk, FeedbackEntries, FeedbackEntry, FeedbackFile, FeedbackNote,
    FeedbackResolution, FeedbackResolved,
};
pub use imported::CanvasImported;
pub use look::{CanvasImage, CanvasImages, CanvasStrokes};
pub use packet::{CanvasPacket, PacketAsk, PacketImage};
pub use pane::{Pane, PaneAsk, PaneKind, PaneLayout, PaneSplit, PaneTree};
pub use place::{
    default_data_dir, default_socket, name_socket, named_socket, running_socket, socket_name_file,
    unname_socket,
};
pub use profile::{Profile, ProfileAsk, Profiles};
pub use session::{
    FileTouched, Listened, Session, SessionAsk, SessionInfo, Sessions, StatusSet, Taken, Touched,
};
pub use sling::{Backdrop, BackdropArea, Backdrops, SlingAsk, Slung};
pub use ui::{LIVE_SIZE, LiveItem, UiAsk, UiOpen, UiOpened, UiReplied, UiShown};
pub use wire::{MAX_LINE, VERSION, read_request, write_reply};
pub use workspace::{
    Workspace, WorkspaceAgent, WorkspaceClosed, WorkspaceFileError, WorkspaceInfo, WorkspaceOpened,
    WorkspaceRoot, Workspaces,
};

/// A request to the daemon. One variant per verb; `op` on the wire names it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
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
    /// Make a new canvas: empty, or holding the operations in the file at
    /// `path` (a JSON list, as `ops` of `canvas_apply`), applied as `by`
    /// (`user` or `agent:<name>`; absent: the user) in one write.
    CanvasCreate {
        canvas: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<PathBuf>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        by: Option<String>,
    },
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
    /// Make a new canvas from the tldraw file at `path`, named `canvas` or
    /// after the file.
    CanvasImport {
        path: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        canvas: Option<String>,
    },
    // The workspace verbs are read here, but a client writes them with
    // `workspace::Ask`: serializing them here would grow every binary that
    // sends any request, the app's included (its size budget is nearly full).
    /// Open a workspace: the directory `dir`, or one with no root named `name`.
    #[serde(skip_serializing)]
    WorkspaceOpen {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dir: Option<PathBuf>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// The open workspaces.
    #[serde(skip_serializing)]
    WorkspaceList,
    /// One open workspace, by name or by the directory of its file.
    #[serde(skip_serializing)]
    WorkspaceShow { workspace: String },
    /// Close a workspace, by name or by the directory of its file.
    #[serde(skip_serializing)]
    WorkspaceClose { workspace: String },
    /// The packet of a canvas page (docs/design/packet.md), stored as a blob
    /// and written into a directory. A client writes it with
    /// `packet::Read`, so the app, which never sends it, carries no code for it.
    #[serde(skip_serializing)]
    CanvasRead(PacketAsk),
    /// The operations of a canvas numbered above `after`. With `wait`, the
    /// reply waits up to that many milliseconds for one (docs/service.md).
    /// A client writes it with its own type, as the workspace verbs.
    #[serde(skip_serializing)]
    CanvasChanges {
        canvas: String,
        #[serde(default)]
        after: u64,
        #[serde(default)]
        wait: u64,
    },
    /// Images of what `of` names, an item id or a box (docs/service.md),
    /// written as the PNG file at `path` or, for tiles, beside it. A client
    /// writes it, and `CanvasStrokes`, with `look::Ask`.
    #[serde(skip_serializing)]
    CanvasCrop { canvas: String, of: String, path: PathBuf },
    /// The raw points of a stroke, or of every stroke of a cluster.
    #[serde(skip_serializing)]
    CanvasStrokes { canvas: String, item: String },
    /// The session verbs (`session_list`, `session_show`, `status_set`,
    /// `session_start`, `session_stop`, `session_send`, `sling_send`, `sling_backdrops`,
    /// `session_listen`, `file_touched`), read by their own `op`s. One variant for them all,
    /// written only by clients that send them: each variant here adds code to
    /// the app, which sends none of them (its size budget is nearly full).
    #[serde(untagged, skip_serializing)]
    Session(SessionAsk),
    /// The agent's canvas verbs (`canvas_put`, `canvas_edit`,
    /// `canvas_remove`), read by their own `op`s and written only by clients
    /// that send them, as the session verbs are.
    #[serde(untagged, skip_serializing)]
    Agent(AgentAsk),
    /// The pane verbs (`pane_list`, `pane_open`, `pane_close`), read by
    /// their own `op`s and written by clients with `PaneAsk` itself.
    #[serde(untagged, skip_serializing)]
    Pane(PaneAsk),
    /// The showing verbs (`ui_open`, `ui_shown`, `ui_reply`), read by their
    /// own `op`s and written by clients with `UiAsk` itself.
    #[serde(untagged, skip_serializing)]
    Showing(UiAsk),
    /// The feedback verbs (`feedback_add`, `feedback_list`,
    /// `feedback_resolve`, `feedback_path`), read by their own `op`s and
    /// written by clients with `FeedbackAsk` itself.
    #[serde(untagged, skip_serializing)]
    Feedback(FeedbackAsk),
    /// The profile verbs (`profile_list`, `profile_set`, `profile_remove`),
    /// read by their own `op`s and written by clients with `ProfileAsk`.
    #[serde(untagged, skip_serializing)]
    Profile(ProfileAsk),
}

/// A successful answer. One variant per verb, holding that verb's reply type.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Reply {
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
    CanvasImport(CanvasImported),
    WorkspaceOpen(WorkspaceOpened),
    WorkspaceList(Workspaces),
    WorkspaceShow(WorkspaceInfo),
    WorkspaceClose(WorkspaceClosed),
    CanvasRead(CanvasPacket),
    CanvasChanges(CanvasFeed),
    CanvasCrop(CanvasImages),
    CanvasStrokes(CanvasStrokes),
    SessionList(Sessions),
    SessionShow(SessionInfo),
    SessionStart(SessionInfo),
    SessionStop(SessionInfo),
    SessionSend(SessionInfo),
    StatusSet(StatusSet),
    SlingSend(Slung),
    SlingBackdrops(Backdrops),
    SessionListen(Listened),
    FileTouched(FileTouched),
    CanvasPut(CanvasPlaced),
    CanvasEdit(CanvasEdited),
    CanvasRemove(CanvasRemoved),
    Pane(PaneLayout),
    UiOpen(UiOpened),
    UiShown(UiShown),
    UiReply(UiReplied),
    FeedbackAdd(FeedbackAdded),
    FeedbackList(FeedbackEntries),
    FeedbackResolve(FeedbackResolution),
    FeedbackPath(FeedbackFile),
    Profiles(Profiles),
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
    /// A key the caller makes unique: sent again under it, the operations
    /// are not applied twice (docs/service.md, "Canvas operations").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub once: Option<String>,
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
    /// The absolute path of the canvas's file; none from a daemon older
    /// than the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remote_error_shows_its_message_and_keeps_its_code() {
        let err = RemoteError::new("invalid", "name is empty", Some("pass a name".into()));
        assert_eq!(err.to_string(), "name is empty");
        assert_eq!((err.code(), err.hint()), ("invalid", Some("pass a name")));
    }
}
