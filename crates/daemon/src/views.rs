//! Where each pane of each workspace looks (docs/service.md, "Panes"): one
//! file, `views.json` in the workspaces directory (in the data directory),
//! whose text the core owns ([`slinger_core::workspace::view`]). Never in a
//! workspace's file nor a canvas file: a view is where this machine's window
//! looks, and it changes often. A change appends nothing to the record.
//!
//! The file is read on first use and kept in memory; a missing or unreadable
//! file is no views. A change that leaves the views as they were writes
//! nothing; one that changes them writes the file whole and synced before it
//! returns. One change at a time, so none is lost. Each workspace's views are
//! keyed by the directory of its file, as the open list names it.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use slinger_core::workspace::view::{self, Kept};

use crate::workspaces::{WorkspaceError, mkdir, write_synced};

/// The views file's name, in the workspaces directory.
pub const FILE: &str = "views.json";

/// The kept views. Safe to share between threads.
#[derive(Debug)]
pub struct Views {
    path: PathBuf,
    /// What the file holds; read on first use.
    kept: Mutex<Option<Kept>>,
}

impl Views {
    /// The views kept in the file at `path`. Does no IO.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path, kept: Mutex::new(None) }
    }

    /// The views of the workspace whose file is in `dir`.
    #[must_use]
    pub fn of(&self, dir: &Path) -> view::Views {
        let mut guard = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        guard.get_or_insert_with(|| self.read()).of(&key(dir))
    }

    /// The views of the workspace whose file is in `dir`, after `change`.
    /// A change that fails keeps them as they were.
    ///
    /// # Errors
    ///
    /// `change`'s, and the error of writing the file.
    pub fn change<T>(
        &self,
        dir: &Path,
        change: impl FnOnce(&mut view::Views) -> slinger_core::Result<T>,
    ) -> Result<(view::Views, T), WorkspaceError> {
        let mut guard = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        let kept = guard.get_or_insert_with(|| self.read());
        let key = key(dir);
        let before = kept.of(&key);
        let mut views = before.clone();
        let changed = change(&mut views)?;
        if views != before {
            let mut after = kept.clone();
            after.put(&key, views.clone());
            if let Some(parent) = self.path.parent() {
                mkdir(parent)?;
            }
            write_synced(&self.path, after.to_text().as_bytes())?;
            *kept = after;
        }
        Ok((views, changed))
    }

    fn read(&self) -> Kept {
        fs::read_to_string(&self.path).map(|text| Kept::read(&text)).unwrap_or_default()
    }
}

fn key(dir: &Path) -> String {
    dir.to_string_lossy().into_owned()
}
