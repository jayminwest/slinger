//! Canvas files on disk (SPEC section 5, K5): the daemon is their only writer.
//!
//! Each canvas is `<id>.canvas` in the canvases directory, in the format of
//! docs/design/canvas-file.md. A canvas is read on first use and then held
//! open. Every change to one canvas takes that canvas's lock, so changes are
//! numbered and appended in the order they apply, and the lines are synced to
//! disk before the caller answers. The core decides what each change writes
//! ([`Log`]); this module only reads, appends and syncs.

use std::collections::HashMap;
use std::fs::{self, DirBuilder, File};
use std::io::{self, Write};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde_json::Value;
use slinger_core::canvas::file::{Log, Shown, Written, check_id, parse_author};
use slinger_core::canvas::{Canvas, Time};

/// A canvas file's name ends in this.
const EXTENSION: &str = "canvas";

/// Why a canvas request failed.
#[derive(Debug, thiserror::Error)]
pub enum CanvasError {
    #[error(transparent)]
    Core(#[from] slinger_core::Error),
    #[error("cannot {doing} {}: {source}", path.display())]
    Io { doing: &'static str, path: PathBuf, source: io::Error },
}

impl CanvasError {
    /// Stable machine-readable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Core(e) => e.code(),
            Self::Io { .. } => "internal",
        }
    }

    /// What the caller can do about it, if anything.
    #[must_use]
    pub fn hint(&self) -> Option<String> {
        match self {
            Self::Core(e) => e.hint().map(str::to_string),
            Self::Io { .. } => None,
        }
    }
}

type Result<T> = std::result::Result<T, CanvasError>;

/// One canvas: empty until first used, and again after a failed write.
type Slot = Arc<Mutex<Option<Open>>>;

/// A canvas held open: its log, and its file opened for append.
#[derive(Debug)]
struct Open {
    log: Log,
    file: File,
}

/// The canvases in one directory. Safe to share between threads.
#[derive(Debug)]
pub struct Canvases {
    dir: PathBuf,
    slots: Mutex<HashMap<String, Slot>>,
}

impl Canvases {
    /// The canvases in `dir`. Does no IO: the directory is made with the
    /// first canvas.
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, slots: Mutex::new(HashMap::new()) }
    }

    /// Makes a new, empty canvas.
    ///
    /// # Errors
    ///
    /// `invalid` for an id that is not one, `conflict` when the canvas
    /// exists, and [`CanvasError::Io`] when its file cannot be written.
    pub fn create(&self, id: &str) -> Result<()> {
        let (log, header) = Log::create(id)?;
        self.make(id, log, &header)
    }

    /// Makes a new canvas that holds `canvas` whole, in one write.
    ///
    /// # Errors
    ///
    /// As [`Canvases::create`].
    pub fn import(&self, id: &str, canvas: &Canvas) -> Result<()> {
        let (log, bytes) = Log::import(id, canvas, now())?;
        self.make(id, log, &bytes)
    }

    /// Writes a new canvas's file whole and holds it open.
    fn make(&self, id: &str, log: Log, bytes: &str) -> Result<()> {
        let slot = self.slot(id)?;
        let mut open = lock(&slot);
        let path = self.path(id);
        if open.is_some() || path.exists() {
            return Err(slinger_core::Error::Conflict {
                message: format!("canvas {id} exists"),
                hint: Some("pick another id, or use it with 'slinger canvas show'".into()),
            }
            .into());
        }
        let dir = &self.dir;
        DirBuilder::new().recursive(true).mode(0o700).create(dir).map_err(failed("make", dir))?;
        // A whole file or none: write it aside, then rename it into place.
        // Ids never start with '.', so the temporary name is no canvas's.
        let temp = dir.join(format!(".{id}.{EXTENSION}.new"));
        write_synced(&temp, bytes.as_bytes()).map_err(failed("write", &temp))?;
        fs::rename(&temp, &path).map_err(failed("write", &path))?;
        File::open(dir).and_then(|d| d.sync_all()).map_err(failed("sync", dir))?;
        let file = append(&path).map_err(failed("open", &path))?;
        *open = Some(Open { log, file });
        Ok(())
    }

    /// The ids of the canvases, sorted. No directory is no canvases.
    ///
    /// # Errors
    ///
    /// [`CanvasError::Io`] when the directory cannot be read.
    pub fn list(&self) -> Result<Vec<String>> {
        let entries = match fs::read_dir(&self.dir) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            other => other.map_err(failed("read", &self.dir))?,
        };
        let mut ids: Vec<String> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let stem = path.file_stem()?.to_str()?;
                let canvas = path.extension()? == EXTENSION && check_id(stem).is_ok();
                canvas.then(|| stem.to_owned())
            })
            .collect();
        ids.sort();
        Ok(ids)
    }

    /// One page of a canvas (see [`Log::show`]) and the number of the next
    /// line the canvas will write.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas or page that is not there, and the errors of
    /// reading the file.
    pub fn show(&self, id: &str, page: Option<&str>) -> Result<(Shown, u64)> {
        self.with(id, |log| Ok((log.show(page)?, log.next())))
    }

    /// Applies `ops` as `by` (`user` or `agent:<name>`) and returns what it
    /// wrote, after the lines are on disk.
    ///
    /// # Errors
    ///
    /// The errors of [`Log::apply`] and of reading and writing the file.
    pub fn apply(&self, id: &str, by: &str, ops: &Value) -> Result<Written> {
        let by = parse_author(by)?;
        self.change(id, |log| Ok(log.apply(&by, now(), ops)?))
    }

    /// Undoes the last step of `by` on the canvas.
    ///
    /// # Errors
    ///
    /// The errors of reading and writing the file.
    pub fn undo(&self, id: &str, by: &str) -> Result<Written> {
        let by = parse_author(by)?;
        self.change(id, |log| Ok(log.undo(&by, now())))
    }

    /// Redoes the last step `by` undid on the canvas.
    ///
    /// # Errors
    ///
    /// The errors of reading and writing the file.
    pub fn redo(&self, id: &str, by: &str) -> Result<Written> {
        let by = parse_author(by)?;
        self.change(id, |log| Ok(log.redo(&by, now())))
    }

    /// Runs `f` on the open canvas, then appends and syncs what it wrote. A
    /// failed write closes the canvas, so the next request reads the file.
    fn change(&self, id: &str, f: impl FnOnce(&mut Log) -> Result<Written>) -> Result<Written> {
        let slot = self.slot(id)?;
        let mut guard = lock(&slot);
        let open = self.load(id, &mut guard)?;
        let written = f(&mut open.log)?;
        if written.bytes.is_empty() {
            return Ok(written);
        }
        let synced =
            open.file.write_all(written.bytes.as_bytes()).and_then(|()| open.file.sync_data());
        if let Err(source) = synced {
            *guard = None;
            return Err(failed("write", &self.path(id))(source));
        }
        Ok(written)
    }

    fn with<T>(&self, id: &str, f: impl FnOnce(&Log) -> Result<T>) -> Result<T> {
        let slot = self.slot(id)?;
        let mut guard = lock(&slot);
        f(&self.load(id, &mut guard)?.log)
    }

    /// The open canvas, read from its file if it is not open yet.
    fn load<'a>(&self, id: &str, slot: &'a mut Option<Open>) -> Result<&'a mut Open> {
        if let Some(open) = slot {
            return Ok(open);
        }
        let path = self.path(id);
        let bytes = match fs::read(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(slinger_core::Error::NotFound {
                    what: format!("canvas {id}"),
                    hint: Some(format!("make it with 'slinger canvas create {id}'")),
                }
                .into());
            }
            other => other.map_err(failed("read", &path))?,
        };
        let (log, torn) = Log::open(&bytes)?;
        let file = append(&path).map_err(failed("open", &path))?;
        if torn > 0 {
            let whole = (bytes.len() - torn) as u64;
            file.set_len(whole).and_then(|()| file.sync_all()).map_err(failed("cut", &path))?;
            say_cut(&path, torn);
        }
        Ok(slot.insert(Open { log, file }))
    }

    /// The lock for one canvas. Checks the id first, so it is never a path.
    fn slot(&self, id: &str) -> Result<Slot> {
        check_id(id)?;
        let mut slots = self.slots.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(Arc::clone(slots.entry(id.to_owned()).or_default()))
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.{EXTENSION}"))
    }
}

/// What a failed read or write of `path` becomes.
fn failed(doing: &'static str, path: &Path) -> impl FnOnce(io::Error) -> CanvasError {
    let path = path.to_owned();
    move |source| CanvasError::Io { doing, path, source }
}

fn lock(slot: &Slot) -> MutexGuard<'_, Option<Open>> {
    slot.lock().unwrap_or_else(PoisonError::into_inner)
}

fn append(path: &Path) -> io::Result<File> {
    File::options().append(true).open(path)
}

fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[expect(clippy::print_stderr, reason = "stderr is the daemon's diagnostic channel")]
fn say_cut(path: &Path, torn: usize) {
    eprintln!("slingerd: cut a torn last line ({torn} bytes) off {}", path.display());
}

fn now() -> Time {
    Time(crate::store::now_ms())
}
