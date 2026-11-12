//! The pen's changes the daemon has not written yet, kept on disk
//! (slinger-e76db0, slinger-e7fc87): a stroke drawn, an item erased, moved
//! or put, or a page changed while the daemon is down survives the app
//! quitting, and goes to the daemon after the next start. One JSON file
//! holds them all, oldest first, and is replaced whole on each change:
//!
//! ```json
//! {"version": 2, "changes": [{"key": 7, "after": null,
//!   "once": "4021-1762790400.250000000.7", "canvas": "notes",
//!   "change": "add", "page": "p1", "colour": [0, 0, 255], "width": 2.0,
//!   "points": [x0, y0, x1, y1]}], "turning": [[9, 1]]}
//! ```
//!
//! Each change is as [`change`] writes it. `key` is the ink's key for it,
//! by which a later change names an item it adds; `once` is the key it is
//! applied once under (`null` for none), so one that was on its way when
//! the app quit is not written twice (slinger-1d05ae). A version 1 file
//! (`strokes`, strokes only) reads too. `turning` (left out when empty,
//! and in files kept before it) pairs the key of each page add kept with
//! the pane that turns to the page once it is added (slinger-857848), by
//! [`PaneId::number`](crate::layout::PaneId::number). No change left: no
//! file.
//!
//! One app at a time keeps the file (slinger-1d05ae): the first app to
//! start holds a lock on `unsent-ink.json.lock` for as long as it runs (the
//! system lets go of it however the app ends), and another app running
//! beside it keeps nothing and says so, so neither writes over the other's
//! changes. The next app to start takes the file, changes and all.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{Problem, env_path};
use crate::scene::ink::Kept;

mod change;

/// A pane waiting for a new page: the key of the kept change that adds it,
/// and the pane's [`PaneId::number`](crate::layout::PaneId::number).
pub type Turning = (u64, u32);

/// The file the unsent changes are kept in.
#[derive(Debug)]
pub struct Unsent {
    path: PathBuf,
    /// The lock on it, once this app holds it ([`Unsent::claim`]).
    lock: Option<File>,
}

impl Unsent {
    /// `unsent-ink.json` in `$XDG_DATA_HOME/slinger`, else in
    /// `~/Library/Application Support/slinger`, next to the daemon's record.
    /// `None` when neither `XDG_DATA_HOME` nor `HOME` is set.
    pub fn locate() -> Option<Self> {
        let dir = env_path("XDG_DATA_HOME")
            .or_else(|| Some(env_path("HOME")?.join("Library/Application Support")))?;
        Some(Self::at(dir.join("slinger").join("unsent-ink.json")))
    }

    /// The changes kept in the file at `path`, not held yet.
    pub fn at(path: PathBuf) -> Self {
        Self { path, lock: None }
    }

    /// Holds the file for this app as long as it runs, and returns the
    /// changes it keeps, oldest first, and the panes waiting for a page one
    /// of them adds. Another app running holds it: a problem, and this app
    /// keeps nothing ([`Unsent::save`]).
    pub fn claim(&mut self) -> Result<(Vec<Kept>, Vec<Turning>), Problem> {
        self.lock = hold(&self.path).map_err(|e| problem(&self.path, &e))?;
        if self.lock.is_none() {
            return Err(self.unheld());
        }
        let file = self.read()?;
        let pair =
            |p: &Value| Some((p.get(0)?.as_u64()?, u32::try_from(p.get(1)?.as_u64()?).ok()?));
        let turning = file.get("turning").and_then(Value::as_array).into_iter().flatten();
        Ok((changes(&file), turning.filter_map(pair).collect()))
    }

    /// The changes kept in the file, oldest first; none without a
    /// file. A change the file holds but this app cannot read is left out.
    #[cfg(test)]
    pub fn load(&self) -> Result<Vec<Kept>, Problem> {
        Ok(changes(&self.read()?))
    }

    /// What the file holds; `null` without a file.
    fn read(&self) -> Result<Value, Problem> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Value::Null),
            Err(e) => return Err(problem(&self.path, &e)),
        };
        serde_json::from_str(&text).map_err(|e| problem(&self.path, &e))
    }

    /// Keeps `kept`, oldest first, and the panes waiting for a page one of
    /// them adds, in place of what the file held: written whole beside it,
    /// then moved over it, so a quit midway leaves the old file or the new
    /// one. No change removes the file. Nothing is kept unless this app
    /// holds the file ([`Unsent::claim`]), so it never writes over another
    /// app's.
    pub fn save(&self, kept: &[Kept], turning: &[Turning]) -> Result<(), Problem> {
        if self.lock.is_none() {
            return Err(self.unheld());
        }
        if kept.is_empty() {
            return match fs::remove_file(&self.path) {
                Err(e) if e.kind() != ErrorKind::NotFound => Err(problem(&self.path, &e)),
                _ => Ok(()),
            };
        }
        let changes: Vec<Value> = kept.iter().map(change::write).collect();
        let mut file = json!({"version": 2, "changes": changes});
        if !turning.is_empty() {
            file["turning"] = json!(turning);
        }
        let text = file.to_string();
        // Held, so its directory is there.
        let beside = self.path.with_extension("json.new");
        fs::write(&beside, text)
            .and_then(|()| fs::rename(&beside, &self.path))
            .map_err(|e| problem(&self.path, &e))
    }

    /// Why this app keeps nothing: another app running holds the file.
    fn unheld(&self) -> Problem {
        let message = format!("{}: another slinger app keeps it", self.path.display());
        Problem::new(message, Some("changes not yet sent are kept by one app at a time"))
    }
}

/// The changes `file` keeps, oldest first, leaving out any this app cannot
/// read.
fn changes(file: &Value) -> Vec<Kept> {
    let changes = file.get("changes").or_else(|| file.get("strokes"));
    let changes = changes.and_then(Value::as_array);
    changes.into_iter().flatten().filter_map(change::read).collect()
}

/// The lock on the file at `path`, taken: `None` when a running app holds it.
fn hold(path: &Path) -> io::Result<Option<File>> {
    let mut name = OsString::from(path.as_os_str());
    name.push(".lock");
    fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    let file = OpenOptions::new().create(true).truncate(false).write(true).open(name)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(e),
    }
}

fn problem(path: &Path, e: &dyn std::fmt::Display) -> Problem {
    let hint = "changes not yet sent cannot be kept: make the file writable, or remove it";
    Problem::new(format!("{}: {e}", path.display()), Some(hint))
}

#[cfg(test)]
mod tests;
