//! The fold verbs: `canvas_fold` and `canvas_unfold` (docs/service.md,
//! "Folding a box"; docs/design/fold.md). A folded box shows as its
//! miniature and keeps what lay inside it; an unfold opens it in its old room.
//!
//! [`crate::Request`] reads them as one variant, [`FoldAsk`], and does not
//! write them: a client writes [`FoldAsk`] itself. The reply is a
//! [`CanvasEdited`]: the box and the frame it has now.

use serde::{Deserialize, Serialize};

use crate::{CanvasChange, CanvasEdited, Client, RemoteError};

/// A fold request, as a client writes it and the daemon reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FoldAsk {
    /// Fold a box to its miniature, taking what lies inside it. A box
    /// already folded writes nothing.
    CanvasFold(CanvasFold),
    /// Open a folded box in its old room. A box that is open writes
    /// nothing; one whose room is taken is `conflict`.
    CanvasUnfold(CanvasFold),
}

/// What [`FoldAsk`] asks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasFold {
    pub canvas: String,
    /// The box: `i4`, or a packet's `b4`.
    pub item: String,
    /// Who asks: `user` or `agent:<name>`. An agent folds only a box it made.
    #[serde(default = "crate::user")]
    pub by: String,
    /// A key the caller makes unique, as `canvas_apply` takes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub once: Option<String>,
}

impl Client {
    /// Folds or unfolds a box, as `ask` says, as one undo step.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas or item that is not there, `invalid` for an
    /// id that is not one, an item that is not a box, a box of no area or a
    /// box `by` may not change, `conflict` for an unfold whose room is taken
    /// (the message names what is in the way; nothing is written), and
    /// `unavailable` when the daemon does not answer.
    pub fn canvas_fold(&self, ask: &FoldAsk) -> Result<CanvasEdited, RemoteError> {
        self.call(ask)
    }

    /// [`Client::canvas_fold`], read as the step it wrote: for a client that
    /// needs no frame and already reads [`CanvasChange`] (`made` is empty).
    ///
    /// # Errors
    ///
    /// As [`Client::canvas_fold`]'s.
    pub fn canvas_fold_step(&self, ask: &FoldAsk) -> Result<CanvasChange, RemoteError> {
        self.call(ask)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;
    use crate::wire::request_line;

    fn fold(by: &str, once: Option<&str>) -> CanvasFold {
        let (canvas, item, by) = ("main".into(), "b4".into(), by.into());
        CanvasFold { canvas, item, by, once: once.map(Into::into) }
    }

    #[test]
    fn a_fold_and_an_unfold_read_back_as_themselves_by_their_own_ops() {
        for (ask, line) in [
            (
                FoldAsk::CanvasFold(fold("user", None)),
                r#"{"v":1,"op":"canvas_fold","canvas":"main","item":"b4","by":"user"}"#,
            ),
            (
                FoldAsk::CanvasUnfold(fold("agent:scout", Some("k1"))),
                r#"{"v":1,"op":"canvas_unfold","canvas":"main","item":"b4","by":"agent:scout","once":"k1"}"#,
            ),
        ] {
            assert_eq!(request_line(&ask).unwrap(), format!("{line}\n"));
            let read = crate::read_request(&mut line.as_bytes()).unwrap();
            assert_eq!(read, Some(Request::Fold(ask)), "{line}");
        }
    }

    #[test]
    fn a_fold_without_by_is_the_users() {
        let line = r#"{"v":1,"op":"canvas_fold","canvas":"main","item":"b4"}"#;
        let read = crate::read_request(&mut line.as_bytes()).unwrap();
        assert_eq!(read, Some(Request::Fold(FoldAsk::CanvasFold(fold("user", None)))));
    }

    #[test]
    fn a_fold_reply_reads_as_the_box_or_as_the_step() {
        let reply = r#"{"item":"i4","page":"p1","x":0,"y":0,"w":250,"h":125,"seq":[7]}"#;
        let edited: CanvasEdited = serde_json::from_str(reply).unwrap();
        assert_eq!((edited.item.as_str(), edited.w, edited.seq), ("i4", 250, vec![7]));
        let step: CanvasChange = serde_json::from_str(reply).unwrap();
        assert_eq!(step, CanvasChange { seq: vec![7], made: vec![] });
    }
}
