//! The fold verbs as they cross the socket: `canvas_fold` and
//! `canvas_unfold`. The core decides what a fold takes and who may.

use slinger_control::{CanvasEdited, CanvasFold, CanvasFoldAsk, CanvasUnfoldAsk, RemoteError};
use slinger_core::canvas::file::Placed;

use super::drawn;
use crate::store::Store;

pub(super) fn fold(store: &Store, CanvasFoldAsk(ask): CanvasFoldAsk) -> Edited {
    fold_or_open(store, &ask, false)
}

pub(super) fn unfold(store: &Store, CanvasUnfoldAsk(ask): CanvasUnfoldAsk) -> Edited {
    fold_or_open(store, &ask, true)
}

type Edited = Result<CanvasEdited, RemoteError>;

fn fold_or_open(store: &Store, ask: &CanvasFold, open: bool) -> Edited {
    let once = ask.once.as_deref();
    let folded = store.canvases().fold(&ask.canvas, &ask.by, &ask.item, open, once);
    let Placed { written, item, page, frame: [x, y, w, h] } = folded.map_err(|e| drawn(&e))?;
    Ok(CanvasEdited { item, page, x, y, w, h, seq: written.seq })
}
