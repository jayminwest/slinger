//! `canvas_import`: a tldraw file on this machine in, a new canvas out.

use std::fs;
use std::path::Path;

use slinger_control::{CanvasImported, RemoteError, Reply};
use slinger_core::canvas::Time;
use slinger_core::canvas::file::id_from_name;
use slinger_core::tldraw;

use super::{absolute, drawn, remote, unreadable};
use crate::store::{Store, now_ms};

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
