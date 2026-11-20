//! Each verb's reply type: a golden reply line is read into it and written
//! back with `write_reply`, as the daemon writes it.

use serde::de::DeserializeOwned;
use slinger_control::{
    Appended, Backdrops, CanvasChange, CanvasEdited, CanvasFeed, CanvasFound, CanvasImages,
    CanvasImported, CanvasInfo, CanvasPacket, CanvasPage, CanvasPlaced, CanvasRemoved,
    CanvasStrokes, Canvases, DaemonInfo, DaemonStopped, Events, Exported, FeedbackAdded,
    FeedbackEntries, FeedbackFile, FeedbackResolution, FileTouched, Listened, PaneLayout, Profiles,
    Reply, SessionInfo, Sessions, Slung, StatusSet, Stored, UiOpened, UiPut, UiReplied, UiShown,
    Uis, WorkspaceClosed, WorkspaceInfo, WorkspaceOpened, Workspaces, Written, write_reply,
};

/// Reads `line` as a `T` and writes it back as the reply `wrap` makes.
fn round<T: DeserializeOwned>(line: &str, wrap: fn(T) -> Reply) -> String {
    let value: T = serde_json::from_str(line).unwrap_or_else(|e| panic!("{e}: {line}"));
    let mut out = Vec::new();
    write_reply(&mut out, &Ok(wrap(value))).unwrap();
    String::from_utf8(out).unwrap()
}

/// Reads a golden reply line and writes it back.
type Round = fn(&str) -> String;

/// Each verb and how its reply line is read and written, in the order of
/// `fixtures/replies.jsonl`.
pub const REPLIES: &[(&str, Round)] = &[
    ("record_append", |l| round::<Appended>(l, Reply::RecordAppend)),
    ("record_list", |l| round::<Events>(l, Reply::RecordList)),
    ("blob_put", |l| round::<Stored>(l, Reply::BlobPut)),
    ("blob_get", |l| round::<Written>(l, Reply::BlobGet)),
    ("ui_put", |l| round::<UiPut>(l, Reply::UiPut)),
    ("ui_list", |l| round::<Uis>(l, Reply::UiList)),
    ("ui_show", |l| round::<Uis>(l, Reply::UiShow)),
    ("ui_export", |l| round::<Exported>(l, Reply::UiExport)),
    ("ui_open", |l| round::<UiOpened>(l, Reply::UiOpen)),
    ("ui_shown", |l| round::<UiShown>(l, Reply::UiShown)),
    ("ui_reply", |l| round::<UiReplied>(l, Reply::UiReply)),
    ("canvas_create", |l| round::<CanvasInfo>(l, Reply::CanvasCreate)),
    ("canvas_list", |l| round::<Canvases>(l, Reply::CanvasList)),
    ("canvas_show", |l| round::<CanvasPage>(l, Reply::CanvasShow)),
    ("canvas_apply", |l| round::<CanvasChange>(l, Reply::CanvasApply)),
    ("canvas_undo", |l| round::<CanvasChange>(l, Reply::CanvasUndo)),
    ("canvas_redo", |l| round::<CanvasChange>(l, Reply::CanvasRedo)),
    ("canvas_import", |l| round::<CanvasImported>(l, Reply::CanvasImport)),
    ("canvas_changes", |l| round::<CanvasFeed>(l, Reply::CanvasChanges)),
    ("workspace_open", |l| round::<WorkspaceOpened>(l, Reply::WorkspaceOpen)),
    ("workspace_list", |l| round::<Workspaces>(l, Reply::WorkspaceList)),
    ("workspace_show", |l| round::<WorkspaceInfo>(l, Reply::WorkspaceShow)),
    ("workspace_close", |l| round::<WorkspaceClosed>(l, Reply::WorkspaceClose)),
    ("pane_list", |l| round::<PaneLayout>(l, Reply::Pane)),
    ("pane_open", |l| round::<PaneLayout>(l, Reply::Pane)),
    ("pane_close", |l| round::<PaneLayout>(l, Reply::Pane)),
    ("pane_resize", |l| round::<PaneLayout>(l, Reply::Pane)),
    ("pane_view", |l| round::<PaneLayout>(l, Reply::Pane)),
    ("canvas_read", |l| round::<CanvasPacket>(l, Reply::CanvasRead)),
    ("canvas_crop", |l| round::<CanvasImages>(l, Reply::CanvasCrop)),
    ("canvas_strokes", |l| round::<CanvasStrokes>(l, Reply::CanvasStrokes)),
    ("canvas_find", |l| round::<CanvasFound>(l, Reply::CanvasFind)),
    ("canvas_fold", |l| round::<CanvasEdited>(l, Reply::Fold)),
    ("canvas_unfold", |l| round::<CanvasEdited>(l, Reply::Fold)),
    ("session_list", |l| round::<Sessions>(l, Reply::SessionList)),
    ("session_show", |l| round::<SessionInfo>(l, Reply::SessionShow)),
    ("status_set", |l| round::<StatusSet>(l, Reply::StatusSet)),
    ("canvas_put", |l| round::<CanvasPlaced>(l, Reply::CanvasPut)),
    ("canvas_edit", |l| round::<CanvasEdited>(l, Reply::CanvasEdit)),
    ("canvas_remove", |l| round::<CanvasRemoved>(l, Reply::CanvasRemove)),
    ("session_start", |l| round::<SessionInfo>(l, Reply::SessionStart)),
    ("session_stop", |l| round::<SessionInfo>(l, Reply::SessionStop)),
    ("session_send", |l| round::<SessionInfo>(l, Reply::SessionSend)),
    ("session_listen", |l| round::<Listened>(l, Reply::SessionListen)),
    ("sling_backdrops", |l| round::<Backdrops>(l, Reply::SlingBackdrops)),
    ("sling_send", |l| round::<Slung>(l, Reply::SlingSend)),
    ("profile_list", |l| round::<Profiles>(l, Reply::Profiles)),
    ("profile_set", |l| round::<Profiles>(l, Reply::Profiles)),
    ("profile_remove", |l| round::<Profiles>(l, Reply::Profiles)),
    ("feedback_add", |l| round::<FeedbackAdded>(l, Reply::FeedbackAdd)),
    ("feedback_list", |l| round::<FeedbackEntries>(l, Reply::FeedbackList)),
    ("feedback_resolve", |l| round::<FeedbackResolution>(l, Reply::FeedbackResolve)),
    ("feedback_path", |l| round::<FeedbackFile>(l, Reply::FeedbackPath)),
    ("daemon_info", |l| round::<DaemonInfo>(l, Reply::DaemonInfo)),
    ("daemon_stop", |l| round::<DaemonStopped>(l, Reply::DaemonStopped)),
    ("file_touched", |l| round::<FileTouched>(l, Reply::FileTouched)),
];
