//! The PDFs the canvas panes draw, and opening a PDF as a canvas
//! (docs/design/pdf.md). Each blob a `pdf` item a pane draws names is
//! fetched once, on a thread of its own ([`Pdfs`]), into a file the
//! canvas view draws from ([`canvas::build_with`]); until it has come the
//! item draws as a grey frame, and a blob that does not come (the record
//! does not hold it, O8) stays that way and is not asked for again. The
//! `open-pdf` command's file becomes a canvas on a thread of its own
//! ([`backend::pdf::open`]); what went wrong shows as a problem until the
//! next. Runs on the main thread and never waits.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

use slinger_control::Client;

use super::Canvases;
use crate::backend::Problem;
use crate::backend::pdf::{self, Opening, Pages};
use crate::panes::Panes;
use crate::scene::CanvasPane;
use crate::scene::canvas::Mark;

/// The PDFs fetched, and the last PDF opened.
pub(super) struct Pdfs {
    daemon: Client,
    /// Where the PDFs are fetched to, and an open writes its operations.
    dir: PathBuf,
    ask: Sender<String>,
    heard: Receiver<(String, Option<PathBuf>)>,
    /// Each blob asked for, and its file once it has come.
    files: HashMap<String, Option<PathBuf>>,
    opened: Option<Receiver<Result<String, Problem>>>,
    problem: Option<Problem>,
}

impl Pdfs {
    /// Fetches through `daemon` into `dir`, on a thread that ends with this.
    pub(super) fn start(daemon: Client, dir: PathBuf) -> Self {
        let (ask, asks) = mpsc::channel::<String>();
        let (tell, heard) = mpsc::channel();
        let (fetcher, to) = (daemon.clone(), dir.clone());
        let spawned = thread::Builder::new().name("pdfs".into()).spawn(move || {
            for hash in asks {
                let file = pdf::fetch(&fetcher, &hash, &to).ok();
                if tell.send((hash, file)).is_err() {
                    break;
                }
            }
        });
        // Without the thread no PDF comes: each item keeps its grey frame.
        drop(spawned);
        Self { daemon, dir, ask, heard, files: HashMap::new(), opened: None, problem: None }
    }

    /// The file of the PDF that is blob `hash`, once it has come.
    pub(super) fn file(&self, hash: &str) -> Option<PathBuf> {
        self.files.get(hash)?.clone()
    }

    /// Takes the PDFs fetched and the end of an open. Returns whether
    /// anything came.
    pub(super) fn poll(&mut self) -> bool {
        let mut came = false;
        for (hash, file) in self.heard.try_iter() {
            came |= file.is_some();
            self.files.insert(hash, file);
        }
        match self.opened.as_ref().map(Receiver::try_recv) {
            Some(Ok(done)) => (self.problem, self.opened, came) = (done.err(), None, true),
            Some(Err(TryRecvError::Disconnected)) => self.opened = None,
            Some(Err(TryRecvError::Empty)) | None => {}
        }
        came
    }

    /// Asks for the PDF of each `pdf` item `scenes` draw, once.
    pub(super) fn want(&mut self, scenes: &[CanvasPane]) {
        let draws = scenes.iter().flat_map(|s| &s.scene.draws);
        for draw in draws {
            if let Mark::Pdf { hash, .. } = &draw.mark
                && !self.files.contains_key(hash)
            {
                self.files.insert(hash.clone(), None);
                // The thread is gone only if it could not start.
                let _ = self.ask.send(hash.clone());
            }
        }
    }

    /// What went wrong with the last PDF opened, if anything.
    pub(super) fn problem(&self) -> Option<&Problem> {
        self.problem.as_ref()
    }
}

impl Canvases {
    /// Opens the PDF `file` as a new canvas, its pages' sizes read by
    /// `sizes`, in a pane beside the focused pane of `panes` in `workspace`
    /// ([`backend::pdf::open`]), on a thread of its own. One open at a
    /// time: another while one runs does nothing.
    pub fn open_pdf(
        &mut self,
        panes: &Panes,
        workspace: Option<&str>,
        file: PathBuf,
        sizes: Pages,
    ) {
        let pdfs = &mut self.pdfs;
        if pdfs.opened.is_some() {
            return;
        }
        let workspace = workspace.map(str::to_string);
        let ask = Opening { file, workspace, beside: Some(panes.number()) };
        let (tell, opened) = mpsc::channel();
        let (daemon, dir) = (pdfs.daemon.clone(), pdfs.dir.clone());
        let spawned = thread::Builder::new().name("open pdf".into()).spawn(move || {
            // The window may be gone: no one to tell.
            let _ = tell.send(pdf::open(&daemon, sizes, &ask, &dir));
        });
        pdfs.problem = None;
        match spawned {
            Ok(_) => pdfs.opened = Some(opened),
            Err(e) => pdfs.problem = Some(Problem { message: e.to_string(), hint: None }),
        }
    }
}

#[cfg(test)]
mod tests;
