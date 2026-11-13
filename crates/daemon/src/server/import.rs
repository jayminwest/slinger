//! A file on this machine in, a new canvas out: `canvas_import` (a tldraw
//! file) and `canvas_create` (a file of operations, or none).

use std::fs;
use std::path::Path;

use slinger_control::{CanvasImported, CanvasInfo, RemoteError, Reply};
use slinger_core::canvas::Time;
use slinger_core::canvas::file::id_from_name;
use slinger_core::tldraw;

use super::{absolute, drawn, remote, unreadable};
use crate::store::{Store, now_ms};

/// Makes the canvas `canvas`: empty, or holding the operations in the file
/// at `path` applied as the user, the whole canvas or none.
pub(super) fn create(
    store: &Store,
    canvas: String,
    path: Option<&Path>,
) -> Result<Reply, RemoteError> {
    let canvases = store.canvases();
    let made = match path {
        None => canvases.create(&canvas),
        Some(path) => {
            let bytes = fs::read(absolute(path)?).map_err(|e| unreadable(path, &e))?;
            let ops = serde_json::from_slice(&bytes).map_err(|e| {
                let what = format!("{} is not JSON: {e}", path.display());
                RemoteError::new("invalid", what, Some("write a list of canvas operations".into()))
            })?;
            canvases.create_with(&canvas, &ops)
        }
    };
    made.map_err(|e| drawn(&e))?;
    Ok(Reply::CanvasCreate(CanvasInfo { canvas }))
}

/// Reads the file, imports it and stores the canvas as `canvas`, or under an
/// id made from the file's name.
pub(super) fn canvas(
    store: &Store,
    path: &Path,
    canvas: Option<String>,
) -> Result<Reply, RemoteError> {
    let bytes = fs::read(absolute(path)?).map_err(|e| unreadable(path, &e))?;
    let id = canvas
        .unwrap_or_else(|| id_from_name(&path.file_stem().unwrap_or_default().to_string_lossy()));
    let import = tldraw::import(&bytes, Time(now_ms())).map_err(|e| remote(&e))?;
    store.canvases().import(&id, &import.canvas).map_err(|e| drawn(&e))?;
    let report = import.report;
    let count = |n: usize| n as u64;
    let items = [
        ("stroke", report.strokes),
        ("text", report.texts),
        ("box", report.boxes),
        ("arrow", report.arrows),
    ];
    Ok(Reply::CanvasImport(CanvasImported {
        canvas: id,
        pages: count(report.pages),
        items: items.into_iter().map(|(k, n)| (k.to_owned(), count(n))).collect(),
        skipped: report.skipped.into_iter().map(|(k, n)| (k, count(n))).collect(),
    }))
}
