//! The fold verbs as they cross the socket: `canvas_fold` and
//! `canvas_unfold`. The core decides what a fold takes and who may.

use slinger_control::{CanvasEdited, FoldAsk, RemoteError, Reply};
use slinger_core::canvas::file::Placed;

use super::drawn;
use crate::store::Store;

pub(super) fn answer(store: &Store, ask: FoldAsk) -> Result<Reply, RemoteError> {
    let (ask, open) = match ask {
        FoldAsk::CanvasFold(ask) => (ask, false),
        FoldAsk::CanvasUnfold(ask) => (ask, true),
    };
    let once = ask.once.as_deref();
    let folded = store.canvases().fold(&ask.canvas, &ask.by, &ask.item, open, once);
    let Placed { written, item, page, frame: [x, y, w, h] } = folded.map_err(|e| drawn(&e))?;
    Ok(Reply::Fold(CanvasEdited { item, page, x, y, w, h, seq: written.seq }))
}
