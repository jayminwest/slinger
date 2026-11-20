//! Each verb's reply type: a golden reply line is read into it and written
//! back with `write_reply`, as the daemon writes it.

use slinger_control::{
    BlobGetAsk, BlobPutAsk, CanvasChangesAsk, CanvasCreateAsk, CanvasCropAsk, CanvasEdit,
    CanvasFindAsk, CanvasFoldAsk, CanvasImportAsk, CanvasListAsk, CanvasOps, CanvasPut,
    CanvasRedoAsk, CanvasRemove, CanvasShowAsk, CanvasStrokesAsk, CanvasUndoAsk, CanvasUnfoldAsk,
    DaemonInfoAsk, DaemonStopAsk, EventFilter, FeedbackListAsk, FeedbackNote, FeedbackPathAsk,
    FeedbackResolveAsk, FileTouchedAsk, NewEvent, PacketAsk, PaneCloseAsk, PaneListAsk,
    PaneOpenAsk, PaneResizeAsk, PaneViewAsk, ProfileListAsk, ProfileRemoveAsk, ProfileSetAsk,
    SessionListAsk, SessionListenAsk, SessionSendAsk, SessionShowAsk, SessionStartAsk,
    SessionStopAsk, SlingAsk, SlingBackdropsAsk, StatusSetAsk, UiExportAsk, UiListAsk, UiOpen,
    UiPutAsk, UiReplyAsk, UiShowAsk, UiShownAsk, Verb, WorkspaceCloseAsk, WorkspaceListAsk,
    WorkspaceOpenAsk, WorkspaceShowAsk, write_reply,
};

/// Reads `line` as the reply of the verb `V` and writes it back as the
/// daemon does.
fn round<V: Verb>(line: &str) -> String {
    let value: V::Reply = serde_json::from_str(line).unwrap_or_else(|e| panic!("{e}: {line}"));
    let mut out = Vec::new();
    write_reply(&mut out, &Ok(value)).unwrap();
    String::from_utf8(out).unwrap()
}

/// Reads a golden reply line and writes it back.
type Round = fn(&str) -> String;

/// Each verb and how its reply line is read and written, in the order of
/// `fixtures/replies.jsonl`.
pub const REPLIES: &[(&str, Round)] = &[
    (NewEvent::OP, round::<NewEvent>),
    (EventFilter::OP, round::<EventFilter>),
    (BlobPutAsk::OP, round::<BlobPutAsk>),
    (BlobGetAsk::OP, round::<BlobGetAsk>),
    (UiPutAsk::OP, round::<UiPutAsk>),
    (UiListAsk::OP, round::<UiListAsk>),
    (UiShowAsk::OP, round::<UiShowAsk>),
    (UiExportAsk::OP, round::<UiExportAsk>),
    (UiOpen::OP, round::<UiOpen>),
    (UiShownAsk::OP, round::<UiShownAsk>),
    (UiReplyAsk::OP, round::<UiReplyAsk>),
    (CanvasCreateAsk::OP, round::<CanvasCreateAsk>),
    (CanvasListAsk::OP, round::<CanvasListAsk>),
    (CanvasShowAsk::OP, round::<CanvasShowAsk>),
    (CanvasOps::OP, round::<CanvasOps>),
    (CanvasUndoAsk::OP, round::<CanvasUndoAsk>),
    (CanvasRedoAsk::OP, round::<CanvasRedoAsk>),
    (CanvasImportAsk::OP, round::<CanvasImportAsk>),
    (CanvasChangesAsk::OP, round::<CanvasChangesAsk>),
    (WorkspaceOpenAsk::OP, round::<WorkspaceOpenAsk>),
    (WorkspaceListAsk::OP, round::<WorkspaceListAsk>),
    (WorkspaceShowAsk::OP, round::<WorkspaceShowAsk>),
    (WorkspaceCloseAsk::OP, round::<WorkspaceCloseAsk>),
    (PaneListAsk::OP, round::<PaneListAsk>),
    (PaneOpenAsk::OP, round::<PaneOpenAsk>),
    (PaneCloseAsk::OP, round::<PaneCloseAsk>),
    (PaneResizeAsk::OP, round::<PaneResizeAsk>),
    (PaneViewAsk::OP, round::<PaneViewAsk>),
    (PacketAsk::OP, round::<PacketAsk>),
    (CanvasCropAsk::OP, round::<CanvasCropAsk>),
    (CanvasStrokesAsk::OP, round::<CanvasStrokesAsk>),
    (CanvasFindAsk::OP, round::<CanvasFindAsk>),
    (CanvasFoldAsk::OP, round::<CanvasFoldAsk>),
    (CanvasUnfoldAsk::OP, round::<CanvasUnfoldAsk>),
    (SessionListAsk::OP, round::<SessionListAsk>),
    (SessionShowAsk::OP, round::<SessionShowAsk>),
    (StatusSetAsk::OP, round::<StatusSetAsk>),
    (CanvasPut::OP, round::<CanvasPut>),
    (CanvasEdit::OP, round::<CanvasEdit>),
    (CanvasRemove::OP, round::<CanvasRemove>),
    (SessionStartAsk::OP, round::<SessionStartAsk>),
    (SessionStopAsk::OP, round::<SessionStopAsk>),
    (SessionSendAsk::OP, round::<SessionSendAsk>),
    (SessionListenAsk::OP, round::<SessionListenAsk>),
    (SlingBackdropsAsk::OP, round::<SlingBackdropsAsk>),
    (SlingAsk::OP, round::<SlingAsk>),
    (ProfileListAsk::OP, round::<ProfileListAsk>),
    (ProfileSetAsk::OP, round::<ProfileSetAsk>),
    (ProfileRemoveAsk::OP, round::<ProfileRemoveAsk>),
    (FeedbackNote::OP, round::<FeedbackNote>),
    (FeedbackListAsk::OP, round::<FeedbackListAsk>),
    (FeedbackResolveAsk::OP, round::<FeedbackResolveAsk>),
    (FeedbackPathAsk::OP, round::<FeedbackPathAsk>),
    (DaemonInfoAsk::OP, round::<DaemonInfoAsk>),
    (DaemonStopAsk::OP, round::<DaemonStopAsk>),
    (FileTouchedAsk::OP, round::<FileTouchedAsk>),
];
