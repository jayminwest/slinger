//! Canvas files on disk (SPEC section 5, K5): the daemon is their only writer.
//!
//! Each canvas is `<id>.canvas` in the canvases directory, in the format of
//! docs/design/canvas-file.md. A canvas is read on first use and then held
//! open. Every change to one canvas takes that canvas's lock, so changes are
//! numbered and appended in the order they apply, and the lines are synced to
//! disk before the caller answers. The core decides what each change writes
//! ([`Log`]); this module only reads, appends and syncs. A change wakes the
//! clients that wait on that canvas's feed ([`Canvases::changes`]).

mod feed;

use std::collections::HashMap;
use std::fs::{self, DirBuilder, File};
use std::io::{self, Write};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

use serde_json::Value;
use slinger_core::canvas::file::{
    Edit, How, Log, Placed, Put, Shown, Written, check_id, parse_author,
};
use slinger_core::canvas::{Author, Canvas, Time};
use slinger_core::packet::{Area, Packet};
use slinger_core::render::{Backdrop, Image};

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
type Slot = Arc<Shared>;

/// One canvas's lock, and the signal that it changed (or the daemon stops).
#[derive(Debug, Default)]
struct Shared {
    open: Mutex<Option<Open>>,
    changed: Condvar,
}

/// A canvas held open: its log, and its file opened to read and append.
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
    /// Set once the daemon stops: waits end at once.
    stopping: AtomicBool,
}

impl Canvases {
    /// The canvases in `dir`. Does no IO: the directory is made with the
    /// first canvas.
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, slots: Mutex::new(HashMap::new()), stopping: AtomicBool::new(false) }
    }

    /// Ends every wait on a feed, now and from now on: each gets its reply.
    pub fn stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        let slots: Vec<Slot> =
            self.slots.lock().unwrap_or_else(PoisonError::into_inner).values().cloned().collect();
        for slot in slots {
            // Under the lock, so a request between its check and its wait still wakes.
            let _open = lock(&slot);
            slot.changed.notify_all();
        }
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

    /// Makes a workspace's new canvas: one with its "send to agent" button
    /// ([`Log::start`]).
    ///
    /// # Errors
    ///
    /// As [`Canvases::create`].
    pub fn start(&self, id: &str) -> Result<()> {
        let (log, bytes) = Log::start(id, now())?;
        self.make(id, log, &bytes)
    }

    /// Makes a new canvas that starts with `ops` applied as the user, in
    /// one write: the whole canvas or none ([`Log::create_with`]).
    ///
    /// # Errors
    ///
    /// As [`Canvases::create`], and those of [`Log::apply`].
    pub fn create_with(&self, id: &str, ops: &Value) -> Result<()> {
        let (log, bytes) = Log::create_with(id, &Author::User, now(), ops)?;
        self.make(id, log, &bytes)
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

    /// The packet of one page (see [`Log::packet`]): the lines after
    /// `after` are new, and the images of new ink are drawn over `under`.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas or page that is not there, `invalid` for a
    /// selected id that is not one, and the errors of reading the file.
    pub fn packet(
        &self,
        id: &str,
        page: Option<&str>,
        after: u64,
        select: &[String],
        under: &[Backdrop],
    ) -> Result<Packet> {
        self.with(id, |log| Ok(log.packet(id, page, after, select, under)?))
    }

    /// The areas of PDF pages under the new ink of one page (see
    /// [`Log::backdrops`]).
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas or page that is not there, and the errors
    /// of reading the file.
    pub fn backdrops(&self, id: &str, page: Option<&str>, after: u64) -> Result<Vec<Area>> {
        self.with(id, |log| Ok(log.backdrops(page, after)?))
    }

    /// Images of what `of` names on a canvas, named after `name` (see
    /// [`Log::crop`]).
    ///
    /// # Errors
    ///
    /// The errors of [`Log::crop`] and of reading the file.
    pub fn crop(&self, id: &str, of: &str, name: &str) -> Result<Vec<Image>> {
        self.with(id, |log| Ok(log.crop(of, name)?))
    }

    /// The strokes `item` names on a canvas (see [`Log::strokes`]).
    ///
    /// # Errors
    ///
    /// The errors of [`Log::strokes`] and of reading the file.
    pub fn strokes(&self, id: &str, item: &str) -> Result<Vec<Value>> {
        self.with(id, |log| Ok(log.strokes(item)?))
    }

    /// Applies `ops` as `by` (`user` or `agent:<name>`) and returns what it
    /// wrote, after the lines are on disk. Under `once`, a key it applied
    /// under lately applies nothing again ([`Log::apply_once`]).
    ///
    /// # Errors
    ///
    /// The errors of [`Log::apply`] and of reading and writing the file.
    pub fn apply(&self, id: &str, by: &str, ops: &Value, once: Option<&str>) -> Result<Written> {
        let by = parse_author(by)?;
        self.change(id, |log| match once {
            Some(once) => Ok(log.apply_once(&by, now(), ops, once)?),
            None => Ok(log.apply(&by, now(), ops)?),
        })
    }

    /// Puts `what` beside the item `beside` names, as `agent`, drawn as
    /// `how` says (see [`Log::put_how`]), after the line is on disk.
    ///
    /// # Errors
    ///
    /// The errors of [`Log::put_how`] and of reading and writing the file.
    pub fn put(
        &self,
        id: &str,
        agent: &str,
        beside: &str,
        what: &Put,
        how: &How,
    ) -> Result<Placed> {
        let by = Author::Agent(agent.into());
        self.change(id, |log| Ok(log.put_how(&by, now(), beside, what, how)?))
    }

    /// Puts `what` as [`Canvases::put`] does, once [`Log::check_put`] says it
    /// would go: `ready` runs then, under the canvas's lock, and fills in
    /// what the put needs made first (a live item's showing). `None`, and
    /// nothing put, when `ready` says no; so what `ready` records is never
    /// left without its item by a check that fails.
    ///
    /// # Errors
    ///
    /// The errors of [`Log::put`] and of reading and writing the file.
    pub fn put_when(
        &self,
        id: &str,
        agent: &str,
        beside: &str,
        mut what: Put,
        ready: impl FnOnce(&mut Put) -> bool,
    ) -> Result<Option<Placed>> {
        let by = Author::Agent(agent.into());
        self.change(id, |log| {
            log.check_put(beside, &what)?;
            if !ready(&mut what) {
                return Ok(None);
            }
            Ok(Some(log.put(&by, now(), beside, &what)?))
        })
    }

    /// Changes the item `item` names, as `agent` (see [`Log::edit`]).
    ///
    /// # Errors
    ///
    /// The errors of [`Log::edit`] and of reading and writing the file.
    pub fn edit(&self, id: &str, agent: &str, item: &str, edit: &Edit) -> Result<Placed> {
        let by = Author::Agent(agent.into());
        self.change(id, |log| Ok(log.edit(&by, now(), item, edit)?))
    }

    /// Removes the item `item` names, as `agent` (see [`Log::remove`]).
    ///
    /// # Errors
    ///
    /// The errors of [`Log::remove`] and of reading and writing the file.
    pub fn remove(&self, id: &str, agent: &str, item: &str) -> Result<Written> {
        let by = Author::Agent(agent.into());
        self.change(id, |log| Ok(log.remove(&by, now(), item)?))
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
    fn change<T: Wrote>(&self, id: &str, f: impl FnOnce(&mut Log) -> Result<T>) -> Result<T> {
        let slot = self.slot(id)?;
        let mut guard = lock(&slot);
        let open = self.load(id, &mut guard)?;
        let done = f(&mut open.log)?;
        let bytes = done.written().bytes.as_bytes();
        if bytes.is_empty() {
            return Ok(done);
        }
        let synced = open.file.write_all(bytes).and_then(|()| open.file.sync_data());
        if let Err(source) = synced {
            *guard = None;
            return Err(failed("write", &self.path(id))(source));
        }
        slot.changed.notify_all();
        Ok(done)
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

    /// The file of canvas `id`, whether or not it exists.
    pub fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.{EXTENSION}"))
    }
}

/// What a change returns: the lines it wrote, and maybe more.
trait Wrote {
    fn written(&self) -> &Written;
}

impl Wrote for Written {
    fn written(&self) -> &Written {
        self
    }
}

impl Wrote for Placed {
    fn written(&self) -> &Written {
        &self.written
    }
}

impl Wrote for Option<Placed> {
    fn written(&self) -> &Written {
        const NOTHING: &Written =
            &Written { bytes: String::new(), seq: Vec::new(), made: Vec::new() };
        self.as_ref().map_or(NOTHING, |placed| &placed.written)
    }
}

/// What a failed read or write of `path` becomes.
fn failed(doing: &'static str, path: &Path) -> impl FnOnce(io::Error) -> CanvasError {
    let path = path.to_owned();
    move |source| CanvasError::Io { doing, path, source }
}

fn lock(slot: &Slot) -> MutexGuard<'_, Option<Open>> {
    slot.open.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The file to append to, and to read the feed from.
fn append(path: &Path) -> io::Result<File> {
    File::options().read(true).append(true).open(path)
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
