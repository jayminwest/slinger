//! The pen's strokes the daemon has not written yet, kept on disk
//! (slinger-e76db0): a stroke drawn while the daemon is down survives the
//! app quitting, and goes to the daemon after the next start. One JSON file
//! holds them all, oldest first, and is replaced whole on each change:
//!
//! ```json
//! {"version": 1, "strokes": [{"canvas": "notes", "page": "p1",
//!   "colour": [0, 0, 255], "width": 2.0, "points": [x0, y0, x1, y1],
//!   "once": "4021-1762790400.250000000.7"}]}
//! ```
//!
//! Points are canvas units, as the ink holds them; `once` is the key the
//! stroke is applied once under (`null` for none), so one that was on its
//! way when the app quit is not written twice (slinger-1d05ae). No stroke
//! left: no file.
//!
//! One app at a time keeps the file (slinger-1d05ae): the first app to
//! start holds a lock on `unsent-ink.json.lock` for as long as it runs (the
//! system lets go of it however the app ends), and another app running
//! beside it keeps nothing and says so, so neither writes over the other's
//! strokes. The next app to start takes the file, strokes and all.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{Problem, env_path};
use crate::config::Rgb;
use crate::layout::Point;
use crate::scene::ink::Stroke;

/// A stroke kept, and the key it is applied once under, if it has one.
#[derive(Debug, Clone, PartialEq)]
pub struct KeptStroke {
    pub stroke: Stroke,
    pub once: Option<String>,
}

/// The file the unsent strokes are kept in.
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

    /// The strokes kept in the file at `path`, not held yet.
    pub fn at(path: PathBuf) -> Self {
        Self { path, lock: None }
    }

    /// Holds the file for this app as long as it runs, and returns the
    /// strokes it keeps, oldest first. Another app running holds it: a
    /// problem, and this app keeps nothing ([`Unsent::save`]).
    pub fn claim(&mut self) -> Result<Vec<KeptStroke>, Problem> {
        self.lock = hold(&self.path).map_err(|e| problem(&self.path, &e))?;
        if self.lock.is_none() {
            return Err(self.unheld());
        }
        self.load()
    }

    /// The strokes kept in the file, oldest first; none without a
    /// file. A stroke the file holds but this app cannot read is left out.
    pub fn load(&self) -> Result<Vec<KeptStroke>, Problem> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(problem(&self.path, &e)),
        };
        let file: Value = serde_json::from_str(&text).map_err(|e| problem(&self.path, &e))?;
        let strokes = file.get("strokes").and_then(Value::as_array);
        Ok(strokes.into_iter().flatten().filter_map(read).collect())
    }

    /// Keeps `kept`, oldest first, in place of what the file held: written
    /// whole beside it, then moved over it, so a quit midway leaves the old
    /// file or the new one. None removes the file. Nothing is kept unless
    /// this app holds the file ([`Unsent::claim`]), so it never writes over
    /// another app's.
    pub fn save(&self, kept: &[KeptStroke]) -> Result<(), Problem> {
        if self.lock.is_none() {
            return Err(self.unheld());
        }
        if kept.is_empty() {
            return match fs::remove_file(&self.path) {
                Err(e) if e.kind() != ErrorKind::NotFound => Err(problem(&self.path, &e)),
                _ => Ok(()),
            };
        }
        let strokes: Vec<Value> = kept.iter().map(write).collect();
        let text = json!({"version": 1, "strokes": strokes}).to_string();
        // Held, so its directory is there.
        let beside = self.path.with_extension("json.new");
        fs::write(&beside, text)
            .and_then(|()| fs::rename(&beside, &self.path))
            .map_err(|e| problem(&self.path, &e))
    }

    /// Why this app keeps nothing: another app running holds the file.
    fn unheld(&self) -> Problem {
        let message = format!("{}: another slinger app keeps it", self.path.display());
        Problem::new(message, Some("strokes not yet sent are kept by one app at a time"))
    }
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
    let hint = "strokes not yet sent cannot be kept: make the file writable, or remove it";
    Problem::new(format!("{}: {e}", path.display()), Some(hint))
}

fn write(kept: &KeptStroke) -> Value {
    let stroke = &kept.stroke;
    let Rgb { r, g, b } = stroke.colour;
    let points: Vec<f64> = stroke.points.iter().flat_map(|p| [p.x, p.y]).collect();
    json!({"canvas": stroke.canvas, "page": stroke.page, "colour": [r, g, b],
        "width": stroke.width, "points": points, "once": kept.once})
}

fn read(o: &Value) -> Option<KeptStroke> {
    let text = |key: &str| o.get(key)?.as_str().map(str::to_string);
    let byte = |v: &Value| u8::try_from(v.as_u64()?).ok();
    let [r, g, b] = o.get("colour")?.as_array()?.as_slice() else { return None };
    let numbers: Vec<f64> =
        o.get("points")?.as_array()?.iter().map(Value::as_f64).collect::<Option<_>>()?;
    if numbers.is_empty() || !numbers.len().is_multiple_of(2) {
        return None;
    }
    let stroke = Stroke {
        canvas: text("canvas")?,
        page: text("page")?,
        colour: Rgb { r: byte(r)?, g: byte(g)?, b: byte(b)? },
        width: o.get("width")?.as_f64()?,
        points: numbers.as_chunks::<2>().0.iter().map(|[x, y]| Point { x: *x, y: *y }).collect(),
    };
    Some(KeptStroke { stroke, once: text("once") })
}

#[cfg(test)]
mod tests;
