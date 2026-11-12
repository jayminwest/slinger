//! The pen's strokes the daemon has not written yet, kept on disk
//! (slinger-e76db0): a stroke drawn while the daemon is down survives the
//! app quitting, and goes to the daemon after the next start. One JSON file
//! holds them all, oldest first, and is replaced whole on each change:
//!
//! ```json
//! {"version": 1, "strokes": [{"canvas": "notes", "page": "p1",
//!   "colour": [0, 0, 255], "width": 2.0, "points": [x0, y0, x1, y1],
//!   "once": "4021-1762790400000000000.7"}]}
//! ```
//!
//! Points are canvas units, as the ink holds them; `once` is the key the
//! stroke is applied once under, so one that was on its way when the app
//! quit is not written twice (slinger-1d05ae). No stroke left: no file.
//!
//! Each app running keeps a file of its own (slinger-1d05ae): the first is
//! `unsent-ink.json`, the others `unsent-ink.2.json` and up, to [`FILES`].
//! An app holds its file by a lock on `<file>.lock` for as long as it runs;
//! the system lets go of it however the app ends. At its start an app takes
//! the first file no running app holds, and the strokes of every other one
//! no running app holds (an app that quit left them), which then move into
//! its own file.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{Problem, env_path};
use crate::config::Rgb;
use crate::layout::Point;
use crate::scene::ink::Stroke;

/// How many apps running at once can each keep their strokes.
pub const FILES: usize = 16;

/// A stroke kept, and the key it is applied once under, if it has one.
#[derive(Debug, Clone, PartialEq)]
pub struct KeptStroke {
    pub stroke: Stroke,
    pub once: Option<String>,
}

/// The file this app keeps its unsent strokes in.
#[derive(Debug)]
pub struct Unsent {
    /// The first file: the others are named after it.
    first: PathBuf,
    /// This app's file.
    path: PathBuf,
    /// The lock on it, once taken ([`Unsent::claim`]).
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

    /// The files whose first is at `path`, none taken yet.
    pub fn at(path: PathBuf) -> Self {
        Self { first: path.clone(), path, lock: None }
    }

    /// Takes the first file no running app holds, as this app's for as long
    /// as it runs, and returns the strokes it keeps: its own, oldest first,
    /// then those of each other file no running app holds, which move into
    /// it. A file held, or one that cannot be read, is left as it is.
    pub fn claim(&mut self) -> Result<Vec<KeptStroke>, Problem> {
        let free = (1..=FILES).map(|n| self.nth(n)).find_map(|path| match hold(&path) {
            Ok(Some(lock)) => Some(Ok((path, lock))),
            Ok(None) => None,
            Err(e) => Some(Err(problem(&path, &e))),
        });
        let Some((path, lock)) = free.transpose()? else {
            let message =
                format!("{}: all {FILES} files are held by running apps", self.first.display());
            return Err(Problem::new(message, Some("quit another slinger app, then this one")));
        };
        (self.path, self.lock) = (path, Some(lock));
        let mut kept = self.load()?;
        let mut taken = Vec::new();
        for path in (1..=FILES).map(|n| self.nth(n)).filter(|p| *p != self.path && p.exists()) {
            let Ok(Some(lock)) = hold(&path) else { continue };
            let Ok(theirs) = Self::at(path.clone()).load() else { continue };
            kept.extend(theirs);
            taken.push((path, lock));
        }
        if !taken.is_empty() {
            self.save(&kept)?;
            for (path, _lock) in taken {
                // Gone already is as good.
                let _ = fs::remove_file(path);
            }
        }
        Ok(kept)
    }

    /// The strokes kept in this app's file, oldest first; none without a
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

    /// Keeps `kept`, oldest first, in place of what this app's file held:
    /// written whole beside it, then moved over it, so a quit midway leaves
    /// the old file or the new one. None removes the file. Nothing is kept
    /// until the file is this app's ([`Unsent::claim`]), so it never
    /// overwrites another app's.
    pub fn save(&self, kept: &[KeptStroke]) -> Result<(), Problem> {
        if self.lock.is_none() {
            let message = format!("{}: not this app's file", self.path.display());
            return Err(Problem::new(message, Some("quit another slinger app, then this one")));
        }
        if kept.is_empty() {
            return match fs::remove_file(&self.path) {
                Err(e) if e.kind() != ErrorKind::NotFound => Err(problem(&self.path, &e)),
                _ => Ok(()),
            };
        }
        let strokes: Vec<Value> = kept.iter().map(write).collect();
        let text = json!({"version": 1, "strokes": strokes}).to_string();
        let beside = self.path.with_extension("json.new");
        let dir = self.path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(dir)
            .and_then(|()| fs::write(&beside, text))
            .and_then(|()| fs::rename(&beside, &self.path))
            .map_err(|e| problem(&self.path, &e))
    }

    /// The `n`th file, from 1: the first, or `<name>.<n>.json` beside it.
    fn nth(&self, n: usize) -> PathBuf {
        if n == 1 {
            return self.first.clone();
        }
        let stem = self.first.file_stem().unwrap_or_default().to_string_lossy();
        self.first.with_file_name(format!("{stem}.{n}.json"))
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
    let mut o = json!({"canvas": stroke.canvas, "page": stroke.page, "colour": [r, g, b],
        "width": stroke.width, "points": points});
    if let Some(once) = &kept.once {
        o["once"] = once.as_str().into();
    }
    o
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
