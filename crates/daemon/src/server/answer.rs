//! Request in, reply out: one arm per verb. Each arm hands the verb's request
//! to its handler, which calls the core and returns the verb's own reply type
//! ([`Verb::Reply`]), so no handler can answer with another verb's reply.

use serde_json::Value;
use slinger_control::{RemoteError, Request, Verb};

use super::daemon::{self, Here};
use super::{
    agent, canvas, feedback, fold, import, look, pane, profile, read, record, session, showing,
    sling, uis, workspace,
};
use crate::store::Store;

/// The reply to `request`, as the value [`slinger_control::write_reply`] writes.
pub(super) fn answer(request: Request, store: &Store, here: &Here) -> Result<Value, RemoteError> {
    match request {
        Request::RecordAppend(ask) => by(store, ask, record::append),
        Request::RecordList(ask) => by(store, ask, record::list),
        Request::BlobPut(ask) => by(store, ask, record::blob_put),
        Request::BlobGet(ask) => by(store, ask, record::blob_get),
        Request::UiPut(ask) => by(store, ask, uis::put),
        Request::UiList(ask) => by(store, ask, uis::list),
        Request::UiShow(ask) => by(store, ask, uis::show),
        Request::UiExport(ask) => by(store, ask, uis::export),
        Request::CanvasCreate(ask) => by(store, ask, import::create),
        Request::CanvasList(ask) => by(store, ask, canvas::list),
        Request::CanvasShow(ask) => by(store, ask, canvas::show),
        Request::CanvasApply(ask) => by(store, ask, canvas::apply),
        Request::CanvasUndo(ask) => by(store, ask, canvas::undo),
        Request::CanvasRedo(ask) => by(store, ask, canvas::redo),
        Request::CanvasImport(ask) => by(store, ask, import::canvas),
        Request::WorkspaceOpen(ask) => by(store, ask, workspace::open),
        Request::WorkspaceList(ask) => by(store, ask, workspace::list),
        Request::WorkspaceShow(ask) => by(store, ask, workspace::show),
        Request::WorkspaceClose(ask) => by(store, ask, workspace::close),
        Request::CanvasRead(ask) => by(store, ask, read::packet),
        Request::CanvasChanges(ask) => by(store, ask, canvas::changes),
        Request::CanvasCrop(ask) => by(store, ask, look::crop),
        Request::CanvasStrokes(ask) => by(store, ask, look::strokes),
        Request::CanvasFind(ask) => by(store, ask, look::find),
        Request::CanvasFold(ask) => by(store, ask, fold::fold),
        Request::CanvasUnfold(ask) => by(store, ask, fold::unfold),
        Request::SessionList(ask) => by(store, ask, session::list),
        Request::SessionShow(ask) => by(store, ask, session::show),
        Request::StatusSet(ask) => by(store, ask, session::set),
        Request::SessionStart(ask) => by(store, ask, session::start),
        Request::SessionStop(ask) => by(store, ask, session::stop),
        Request::SessionSend(ask) => by(store, ask, session::send),
        Request::SlingSend(ask) => by(store, ask, sling::send),
        Request::SlingBackdrops(ask) => by(store, ask, sling::backdrops),
        Request::SessionListen(ask) => by(store, ask, session::listen),
        Request::FileTouched(ask) => by(store, ask, session::touched),
        Request::CanvasPut(ask) => by(store, ask, agent::put),
        Request::CanvasEdit(ask) => by(store, ask, agent::edit),
        Request::CanvasRemove(ask) => by(store, ask, agent::remove),
        Request::PaneList(ask) => by(store, ask, pane::list),
        Request::PaneOpen(ask) => by(store, ask, pane::open),
        Request::PaneClose(ask) => by(store, ask, pane::close),
        Request::PaneResize(ask) => by(store, ask, pane::resize),
        Request::PaneView(ask) => by(store, ask, pane::view),
        Request::UiOpen(ask) => by(store, ask, showing::open),
        Request::UiShown(ask) => by(store, ask, showing::shown),
        Request::UiReply(ask) => by(store, ask, showing::reply),
        Request::FeedbackAdd(ask) => by(store, ask, feedback::add),
        Request::FeedbackList(ask) => by(store, ask, feedback::list),
        Request::FeedbackResolve(ask) => by(store, ask, feedback::resolve),
        Request::FeedbackPath(ask) => by(store, ask, |store, _| Ok(feedback::path(store))),
        Request::ProfileList(ask) => by(store, ask, profile::list),
        Request::ProfileSet(ask) => by(store, ask, profile::set),
        Request::ProfileRemove(ask) => by(store, ask, profile::remove),
        Request::DaemonInfo(ask) => by(store, ask, |store, _| Ok(daemon::info(store, here))),
        Request::DaemonStop(ask) => by(store, ask, |store, _| daemon::stop(store, here)),
    }
}

/// Runs the handler of the verb `V` and makes its reply the value written.
fn by<V: Verb>(
    store: &Store,
    ask: V,
    handler: impl FnOnce(&Store, V) -> Result<V::Reply, RemoteError>,
) -> Result<Value, RemoteError> {
    let reply = handler(store, ask)?;
    serde_json::to_value(reply)
        .map_err(|e| RemoteError::new("internal", format!("cannot write the reply: {e}"), None))
}
