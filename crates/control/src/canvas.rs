//! The canvas verbs: `canvas_create`, `canvas_list`, `canvas_show`,
//! `canvas_apply`, `canvas_undo`, `canvas_redo` and `canvas_import`
//! (docs/service.md, "Canvas operations").

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::absolute;
use crate::{Client, RemoteError};

/// `canvas_create`: make a new canvas: empty, or holding the operations in
/// the file at `path` (a JSON list, as `ops` of `canvas_apply`), applied as
/// `by` (`user` or `agent:<name>`; absent: the user) in one write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasCreateAsk {
    pub canvas: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
}

/// `canvas_list`: the canvases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasListAsk {}

/// `canvas_show`: one page of a canvas: the first, or the one `page` names
/// (id or name).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasShowAsk {
    pub canvas: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
}

/// `canvas_apply`: operations to apply to a canvas as one undo step
/// (docs/service.md, "Canvas operations").
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

/// `canvas_undo`: undo the last step of `by` on a canvas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CanvasUndoAsk(pub CanvasStep);

/// `canvas_redo`: redo the last step `by` undid on a canvas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CanvasRedoAsk(pub CanvasStep);

/// `canvas_import`: make a new canvas from the tldraw file at `path`, named
/// `canvas` or after the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasImportAsk {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<String>,
}

/// One canvas, as listed. The reply to `canvas_create`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasInfo {
    pub canvas: String,
}

/// The reply to `canvas_list`: the canvases, by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Canvases {
    pub canvases: Vec<CanvasInfo>,
}

/// The reply to `canvas_show`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasPage {
    pub canvas: String,
    /// The sequence number the canvas's next operation gets.
    pub next: u64,
    /// The id of the page shown.
    pub page: String,
    /// Every page, in order: `{"id", "name"}`, and `"figures"` (the names
    /// of the figures on it) when it has any.
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
    /// The ids of the items and pages it made, in order. A reply without
    /// them (a fold's) made none.
    #[serde(default)]
    pub made: Vec<String>,
}

/// The reply to `canvas_import`: the new canvas and what the import made
/// and skipped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasImported {
    pub canvas: String,
    /// How many pages it has.
    pub pages: u64,
    /// How many items it made, by kind: `stroke`, `text`, `box`, `arrow`.
    pub items: BTreeMap<String, u64>,
    /// Shapes it made no item for, by tldraw shape type.
    pub skipped: BTreeMap<String, u64>,
}

pub(crate) fn user() -> String {
    "user".into()
}

impl Client {
    /// Makes a new, empty canvas.
    ///
    /// # Errors
    ///
    /// `invalid` for an id that is not one, `conflict` when it exists, and
    /// `unavailable` when the daemon does not answer.
    pub fn canvas_create(&self, canvas: &str) -> Result<(), RemoteError> {
        self.call(CanvasCreateAsk { canvas: canvas.into(), path: None, by: None })?;
        Ok(())
    }

    /// Makes a new canvas that holds the operations in the file `ops` (a
    /// JSON list, as [`CanvasOps::ops`]) applied as `by` (`user` or
    /// `agent:<name>`; `None`: the user), so only that author's edits and
    /// removes reach them later: the whole canvas, or none. Its first page
    /// is `p1`.
    ///
    /// # Errors
    ///
    /// As [`Client::canvas_create`], `not_found` for a file that is not
    /// there, and `invalid` for one that is not JSON, an operation the
    /// canvas refuses or a `by` that is not an author.
    pub fn canvas_create_with(
        &self,
        canvas: &str,
        ops: &Path,
        by: Option<&str>,
    ) -> Result<(), RemoteError> {
        let (path, by) = (Some(absolute(ops)?), by.map(str::to_string));
        self.call(CanvasCreateAsk { canvas: canvas.into(), path, by })?;
        Ok(())
    }

    /// The ids of the canvases, sorted.
    ///
    /// # Errors
    ///
    /// The daemon's error, or `unavailable` when it does not answer.
    pub fn canvas_list(&self) -> Result<Vec<String>, RemoteError> {
        let reply = self.call(CanvasListAsk {})?;
        Ok(reply.canvases.into_iter().map(|c| c.canvas).collect())
    }

    /// One page of a canvas: the first, or the one `page` names by id or name.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas or page that is not there, and `unavailable`
    /// when the daemon does not answer.
    pub fn canvas_show(&self, canvas: &str, page: Option<&str>) -> Result<CanvasPage, RemoteError> {
        let page = page.map(str::to_string);
        self.call(CanvasShowAsk { canvas: canvas.to_string(), page })
    }

    /// Applies operations to a canvas as one undo step, and returns what was
    /// written. The operations are on disk when this returns.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas or item that is not there, `invalid` for an
    /// operation that is not one or that the canvas refuses, and
    /// `unavailable` when the daemon does not answer.
    pub fn canvas_apply(&self, ops: CanvasOps) -> Result<CanvasChange, RemoteError> {
        self.call(ops)
    }

    /// Undoes the last step of `step.by` on the canvas.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas that is not there, and `unavailable` when the
    /// daemon does not answer.
    pub fn canvas_undo(&self, step: CanvasStep) -> Result<CanvasChange, RemoteError> {
        self.call(CanvasUndoAsk(step))
    }

    /// Makes a new canvas from the tldraw file `file`, named `canvas` or,
    /// without one, after the file.
    ///
    /// # Errors
    ///
    /// `not_found` for a file that is not there, `invalid` for one that is
    /// not a tldraw file or a name that is not a canvas id, `conflict` when
    /// the canvas exists, and `unavailable` when the daemon does not answer.
    pub fn canvas_import(
        &self,
        file: &Path,
        canvas: Option<&str>,
    ) -> Result<CanvasImported, RemoteError> {
        let (path, canvas) = (absolute(file)?, canvas.map(str::to_string));
        self.call(CanvasImportAsk { path, canvas })
    }

    /// Redoes the last step `step.by` undid on the canvas.
    ///
    /// # Errors
    ///
    /// As [`Client::canvas_undo`].
    pub fn canvas_redo(&self, step: CanvasStep) -> Result<CanvasChange, RemoteError> {
        self.call(CanvasRedoAsk(step))
    }
}
