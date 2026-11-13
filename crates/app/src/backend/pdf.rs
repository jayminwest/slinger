//! PDFs and the daemon (docs/design/pdf.md). [`fetch`] writes the bytes of
//! a PDF's blob to a file of the app's own, once per blob id, for the
//! canvas view to draw from. [`open`] makes a PDF a new canvas: the blob,
//! the canvas, one `pdf` item per page stacked on its first page, and a
//! pane that shows it. Each blocks until the daemon answers or its client
//! gives up. Only `platform` reads a PDF: `open` is handed its page sizes'
//! reader ([`Pages`]).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;
use slinger_control::{CanvasOps, Client, PaneKind};

use super::Problem;
use crate::mirror;
use crate::scene::pdf::{BATCH, canvas_id, pages};

/// Each page's size of the PDF at a path, in page points as displayed, or
/// why it cannot be read, for the user (`platform::pdf::pages`).
pub type Pages = fn(&Path) -> Result<Vec<(f64, f64)>, String>;

/// The most canvas ids [`open`] tries for one PDF (`-2`, `-3`, ...).
const TRIES: u32 = 999;

/// The part files [`fetch`] has written in this process.
static PARTS: AtomicU64 = AtomicU64::new(0);

/// The directory the PDFs are fetched to: the app's own, in the temporary
/// directory.
pub fn cache() -> PathBuf {
    std::env::temp_dir().join("slinger-app-pdf")
}

/// The file in `dir` that holds the PDF that is blob `hash`, fetched from
/// the daemon unless it is there: `<hex>.pdf`, written whole or not at all,
/// however many fetch it at once.
///
/// # Errors
///
/// The daemon's (`not_found` for a blob the record does not hold, O8), a
/// `hash` that is not a blob id, and a problem when `dir` cannot be written.
pub fn fetch(daemon: &Client, hash: &str, dir: &Path) -> Result<PathBuf, Problem> {
    let hex = hash.strip_prefix("sha256:").filter(|h| {
        h.len() == 64 && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    });
    let Some(hex) = hex else {
        return Err(Problem::new(format!("{hash} is not a blob id"), None));
    };
    let file = dir.join(format!("{hex}.pdf"));
    if file.is_file() {
        return Ok(file);
    }
    // Each fetch writes a part file of its own, so two at once (the Pdfs
    // thread and a sling's backdrops) never write or rename each other's:
    // the same bytes land whole under the one name, whichever is last.
    let n = PARTS.fetch_add(1, Ordering::Relaxed);
    let part = dir.join(format!("{hex}.{}-{n}.part", std::process::id()));
    fs::create_dir_all(dir).map_err(|e| unwritable(dir, &e))?;
    let fetched = daemon.blob_get(hash, &part).map_err(Problem::from);
    let fetched = fetched.and_then(|_| fs::rename(&part, &file).map_err(|e| unwritable(&file, &e)));
    if fetched.is_err() {
        let _ = fs::remove_file(&part);
    }
    fetched.map(|()| file)
}

/// What [`open`] is asked: the PDF's file, and the workspace whose window
/// shows it beside its pane number `beside` (no workspace: no pane).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opening {
    pub file: PathBuf,
    pub workspace: Option<String>,
    pub beside: Option<u32>,
}

/// Makes the PDF `ask.file` a new canvas, and opens a pane on it beside
/// `ask.beside` in `ask.workspace`: reads its page sizes with `sizes`,
/// stores it as a blob, makes the canvas ([`canvas_id`], the next id when
/// one is taken), adds one `pdf` item per page to its first page in page
/// order, [`BATCH`] to a `canvas_apply`. Returns the canvas's id.
///
/// # Errors
///
/// A PDF that cannot be read, with nothing made; then the daemon's, with
/// the blob, and maybe the canvas, made.
pub fn open(daemon: &Client, sizes: Pages, ask: &Opening) -> Result<String, Problem> {
    let sizes = sizes(&ask.file).map_err(|why| Problem::new(why, None))?;
    if sizes.is_empty() {
        let name = ask.file.display();
        return Err(Problem::new(format!("{name} has no pages"), None));
    }
    let hash = daemon.blob_put(&ask.file)?;
    let canvas = create(daemon, &ask.file)?;
    let first = daemon.canvas_show(&canvas, None)?.page;
    for batch in pages(&sizes, &hash, &ask.file).chunks(BATCH) {
        let ops = Value::Array(batch.iter().map(|p| mirror::add_pdf(&first, p)).collect());
        let (canvas, by) = (canvas.clone(), "user".to_string());
        daemon.canvas_apply(CanvasOps { canvas, by, ops, once: None })?;
    }
    if let Some(workspace) = &ask.workspace {
        daemon.pane_open(workspace, PaneKind::Canvas, &canvas, ask.beside, false)?;
    }
    Ok(canvas)
}

/// Makes the canvas for the PDF `file`: the first of its ids not taken.
fn create(daemon: &Client, file: &Path) -> Result<String, Problem> {
    for n in 1..=TRIES {
        let id = canvas_id(file, n);
        match daemon.canvas_create(&id) {
            Ok(()) => return Ok(id),
            Err(e) if e.code() == "conflict" => {}
            Err(e) => return Err(e.into()),
        }
    }
    let hint = Some("rename the file, or remove some of the canvases made from it");
    Err(Problem::new(format!("every canvas id for {} is taken", file.display()), hint))
}

fn unwritable(path: &Path, e: &std::io::Error) -> Problem {
    let hint = Some("check the temporary directory is writable");
    Problem::new(format!("{}: {e}", path.display()), hint)
}

#[cfg(test)]
mod tests;
