//! The verbs: each request type, the `op` that names it on the wire and the
//! type of its reply, declared once in the table below.
//!
//! The table makes [`Request`], the one enum the daemon reads, and a [`Verb`]
//! impl for each request type, which [`crate::Client`] uses to read each
//! reply as its own verb's type and the daemon to write it. A new verb is a
//! row here and its request and reply types in their module
//! (docs/service.md, "Adding a verb").

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// A request type, with the `op` that names it and the type of its reply.
pub trait Verb: Into<Request> {
    /// The verb's name on the wire.
    const OP: &'static str;
    /// What the daemon answers on success.
    type Reply: Serialize + DeserializeOwned;
}

macro_rules! verbs {
    ($($op:literal $variant:ident($ask:ident) -> $reply:ident,)*) => {
        /// A request to the daemon. One variant per verb; `op` on the wire
        /// names it.
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(tag = "op")]
        pub enum Request {
            $(#[serde(rename = $op)] $variant(crate::$ask),)*
        }

        $(
            impl Verb for crate::$ask {
                const OP: &'static str = $op;
                type Reply = crate::$reply;
            }

            impl From<crate::$ask> for Request {
                fn from(ask: crate::$ask) -> Self {
                    Self::$variant(ask)
                }
            }
        )*

        /// Every verb's `op`, in the order of the table.
        pub const OPS: &[&str] = &[$($op),*];
    };
}

verbs! {
    "record_append" RecordAppend(NewEvent) -> Appended,
    "record_list" RecordList(EventFilter) -> Events,
    "blob_put" BlobPut(BlobPutAsk) -> Stored,
    "blob_get" BlobGet(BlobGetAsk) -> Written,
    "ui_put" UiPut(UiPutAsk) -> UiPut,
    "ui_list" UiList(UiListAsk) -> Uis,
    "ui_show" UiShow(UiShowAsk) -> Uis,
    "ui_export" UiExport(UiExportAsk) -> Exported,
    "canvas_create" CanvasCreate(CanvasCreateAsk) -> CanvasInfo,
    "canvas_list" CanvasList(CanvasListAsk) -> Canvases,
    "canvas_show" CanvasShow(CanvasShowAsk) -> CanvasPage,
    "canvas_apply" CanvasApply(CanvasOps) -> CanvasChange,
    "canvas_undo" CanvasUndo(CanvasUndoAsk) -> CanvasChange,
    "canvas_redo" CanvasRedo(CanvasRedoAsk) -> CanvasChange,
    "canvas_import" CanvasImport(CanvasImportAsk) -> CanvasImported,
    "workspace_open" WorkspaceOpen(WorkspaceOpenAsk) -> WorkspaceOpened,
    "workspace_list" WorkspaceList(WorkspaceListAsk) -> Workspaces,
    "workspace_show" WorkspaceShow(WorkspaceShowAsk) -> WorkspaceInfo,
    "workspace_close" WorkspaceClose(WorkspaceCloseAsk) -> WorkspaceClosed,
    "canvas_read" CanvasRead(PacketAsk) -> CanvasPacket,
    "canvas_changes" CanvasChanges(CanvasChangesAsk) -> CanvasFeed,
    "canvas_crop" CanvasCrop(CanvasCropAsk) -> CanvasImages,
    "canvas_strokes" CanvasStrokes(CanvasStrokesAsk) -> CanvasStrokes,
    "canvas_find" CanvasFind(CanvasFindAsk) -> CanvasFound,
    "canvas_fold" CanvasFold(CanvasFoldAsk) -> CanvasEdited,
    "canvas_unfold" CanvasUnfold(CanvasUnfoldAsk) -> CanvasEdited,
    "canvas_draw" CanvasDraw(CanvasDraw) -> CanvasDrawn,
    "canvas_redraw" CanvasRedraw(CanvasRedraw) -> CanvasDrawn,
    "canvas_figure" CanvasFigure(CanvasFigureAsk) -> CanvasFigures,
    "session_list" SessionList(SessionListAsk) -> Sessions,
    "session_show" SessionShow(SessionShowAsk) -> SessionInfo,
    "status_set" StatusSet(StatusSetAsk) -> StatusSet,
    "session_start" SessionStart(SessionStartAsk) -> SessionInfo,
    "session_stop" SessionStop(SessionStopAsk) -> SessionInfo,
    "session_send" SessionSend(SessionSendAsk) -> SessionInfo,
    "sling_send" SlingSend(SlingAsk) -> Slung,
    "sling_backdrops" SlingBackdrops(SlingBackdropsAsk) -> Backdrops,
    "session_listen" SessionListen(SessionListenAsk) -> Listened,
    "file_touched" FileTouched(FileTouchedAsk) -> FileTouched,
    "canvas_put" CanvasPut(CanvasPut) -> CanvasPlaced,
    "canvas_edit" CanvasEdit(CanvasEdit) -> CanvasEdited,
    "canvas_remove" CanvasRemove(CanvasRemove) -> CanvasRemoved,
    "pane_list" PaneList(PaneListAsk) -> PaneLayout,
    "pane_open" PaneOpen(PaneOpenAsk) -> PaneLayout,
    "pane_close" PaneClose(PaneCloseAsk) -> PaneLayout,
    "pane_resize" PaneResize(PaneResizeAsk) -> PaneLayout,
    "pane_view" PaneView(PaneViewAsk) -> PaneLayout,
    "ui_open" UiOpen(UiOpen) -> UiOpened,
    "ui_shown" UiShown(UiShownAsk) -> UiShown,
    "ui_reply" UiReply(UiReplyAsk) -> UiReplied,
    "feedback_add" FeedbackAdd(FeedbackNote) -> FeedbackAdded,
    "feedback_list" FeedbackList(FeedbackListAsk) -> FeedbackEntries,
    "feedback_resolve" FeedbackResolve(FeedbackResolveAsk) -> FeedbackResolution,
    "feedback_path" FeedbackPath(FeedbackPathAsk) -> FeedbackFile,
    "profile_list" ProfileList(ProfileListAsk) -> Profiles,
    "profile_set" ProfileSet(ProfileSetAsk) -> Profiles,
    "profile_remove" ProfileRemove(ProfileRemoveAsk) -> Profiles,
    "daemon_info" DaemonInfo(DaemonInfoAsk) -> DaemonInfo,
    "daemon_stop" DaemonStop(DaemonStopAsk) -> DaemonStopped,
}
