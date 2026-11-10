//! The reply to `canvas_import` (docs/service.md).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The reply to [`Request::CanvasImport`](crate::Request::CanvasImport): the
/// new canvas and what the import made and skipped.
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
