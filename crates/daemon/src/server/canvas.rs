//! `canvas_list`, `canvas_show`, `canvas_apply`, `canvas_undo`,
//! `canvas_redo` and `canvas_changes`: canvases as they cross the socket.
//! The core decides what each operation does; making a canvas from a file is
//! in `import`.

use std::time::Duration;

use slinger_control::{
    self as control, CanvasChange, CanvasChangesAsk, CanvasFeed, CanvasInfo, CanvasListAsk,
    CanvasOps, CanvasPage, CanvasRedoAsk, CanvasShowAsk, CanvasUndoAsk, Canvases, RemoteError,
};
use slinger_core::canvas::file::Written;

use super::drawn;
use crate::store::Store;

pub(super) fn list(store: &Store, _: CanvasListAsk) -> Result<Canvases, RemoteError> {
    let ids = store.canvases().list().map_err(|e| drawn(&e))?;
    Ok(Canvases { canvases: ids.into_iter().map(|canvas| CanvasInfo { canvas }).collect() })
}

pub(super) fn show(
    store: &Store,
    CanvasShowAsk { canvas, page }: CanvasShowAsk,
) -> Result<CanvasPage, RemoteError> {
    let (shown, next) = store.canvases().show(&canvas, page.as_deref()).map_err(|e| drawn(&e))?;
    let (page, pages, items) = (shown.page, shown.pages, shown.items);
    let path = Some(store.canvases().path(&canvas).to_string_lossy().into_owned());
    Ok(CanvasPage { canvas, next, page, pages, items, path })
}

pub(super) fn apply(
    store: &Store,
    CanvasOps { canvas, by, ops, once }: CanvasOps,
) -> Result<CanvasChange, RemoteError> {
    let applied = store.canvases().apply(&canvas, &by, &ops, once.as_deref());
    Ok(change(applied.map_err(|e| drawn(&e))?))
}

pub(super) fn undo(
    store: &Store,
    CanvasUndoAsk(s): CanvasUndoAsk,
) -> Result<CanvasChange, RemoteError> {
    Ok(change(store.canvases().undo(&s.canvas, &s.by).map_err(|e| drawn(&e))?))
}

pub(super) fn redo(
    store: &Store,
    CanvasRedoAsk(s): CanvasRedoAsk,
) -> Result<CanvasChange, RemoteError> {
    Ok(change(store.canvases().redo(&s.canvas, &s.by).map_err(|e| drawn(&e))?))
}

pub(super) fn changes(store: &Store, ask: CanvasChangesAsk) -> Result<CanvasFeed, RemoteError> {
    let CanvasChangesAsk { canvas, after, wait } = ask;
    let wait = Duration::from_millis(wait.min(control::MAX_WAIT_MS));
    let (ops, next) = store.canvases().changes(&canvas, after, wait).map_err(|e| drawn(&e))?;
    Ok(CanvasFeed { next, ops })
}

fn change(lines: Written) -> CanvasChange {
    CanvasChange { seq: lines.seq, made: lines.made }
}
