//! The open workspaces (SPEC section 10, D16): which directories hold the
//! definition files of the workspaces this daemon serves.
//!
//! The core owns the definition file's format and the rules
//! ([`slinger_core::workspace`]); this module reads and writes the files and
//! keeps the list. The list is `open.json` in the workspaces directory (in
//! the data directory): `{"v":1,"dirs":[...]}`, the directories in the order
//! they were opened. A workspace with no root keeps its file in that
//! directory too, in `<slug>/`. Definition files are read on every request,
//! so an edit to one shows at once. Each file stands alone: one the daemon
//! cannot use is listed as [`Broken`], with its error, and every verb on
//! that workspace fails with that error; the others serve as before.

use std::fs::{self, DirBuilder, File};
use std::io::{self, Write};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde_json::{Value, json};
use slinger_core::record::Tags;
use slinger_core::workspace::panes::Layout;
use slinger_core::workspace::{self, FILE_NAME, Workspace};

use crate::canvases::{CanvasError, Canvases};

/// The list of open workspaces, in the workspaces directory.
const LIST: &str = "open.json";
/// The list's format version.
const LIST_VERSION: u64 = 1;

/// Why a workspace request failed.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error(transparent)]
    Core(#[from] slinger_core::Error),
    /// A definition file the daemon could not use, and the field in it
    /// that broke a rule, when one did.
    #[error("{}: {source}", path.display())]
    File { path: PathBuf, field: Option<&'static str>, source: slinger_core::Error },
    #[error(transparent)]
    Canvas(#[from] CanvasError),
    #[error("cannot {doing} {}: {source}", path.display())]
    Io { doing: &'static str, path: PathBuf, source: io::Error },
}

impl WorkspaceError {
    /// Stable machine-readable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Core(e) | Self::File { source: e, .. } => e.code(),
            Self::Canvas(e) => e.code(),
            Self::Io { .. } => "internal",
        }
    }

    /// What the caller can do about it, if anything.
    #[must_use]
    pub fn hint(&self) -> Option<String> {
        match self {
            Self::Core(e) => e.hint().map(str::to_string),
            Self::File { path, source, .. } => {
                let dir = path.parent().unwrap_or(path).display();
                let fix = source.hint().map_or_else(String::new, |h| format!("{h}; "));
                Some(format!("{fix}fix the file, or close it: 'slinger workspace close {dir}'"))
            }
            Self::Canvas(e) => e.hint(),
            Self::Io { .. } => None,
        }
    }

    /// The definition file this error is about, and the field in it that
    /// broke a rule, if any.
    #[must_use]
    pub fn file(&self) -> Option<(&Path, Option<&'static str>)> {
        match self {
            Self::File { path, field, .. } => Some((path, *field)),
            _ => None,
        }
    }
}

type Result<T> = std::result::Result<T, WorkspaceError>;

/// An open workspace: the directory of its file, and what the file says.
#[derive(Debug, Clone, PartialEq)]
pub struct Opened {
    pub dir: PathBuf,
    pub workspace: Workspace,
}

/// An open workspace whose definition file the daemon cannot use: listed,
/// never served. Its name and canvas are what the file says, if it says
/// them ([`workspace::peek`]).
#[derive(Debug)]
pub struct Broken {
    pub dir: PathBuf,
    pub name: Option<String>,
    pub canvas: Option<String>,
    pub error: WorkspaceError,
}

/// One open workspace as [`Workspaces::list`] has it: served, or broken.
#[derive(Debug)]
pub enum Listed {
    Served(Opened),
    Broken(Broken),
}

impl Listed {
    /// The workspace, if it is served.
    #[must_use]
    pub fn served(self) -> Option<Opened> {
        match self {
            Self::Served(opened) => Some(opened),
            Self::Broken(_) => None,
        }
    }

    /// The workspace, if its file cannot be used.
    #[must_use]
    pub fn broken(self) -> Option<Broken> {
        match self {
            Self::Served(_) => None,
            Self::Broken(broken) => Some(broken),
        }
    }

    /// Its name; empty for a broken file that says none.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Served(o) => &o.workspace.name,
            Self::Broken(b) => b.name.as_deref().unwrap_or_default(),
        }
    }
}

/// The open workspaces. Safe to share between threads.
#[derive(Debug)]
pub struct Workspaces {
    dir: PathBuf,
    machine: String,
    /// The directories of the open workspaces' files; read on first use.
    open: Mutex<Option<Vec<PathBuf>>>,
}

impl Workspaces {
    /// The workspaces listed in `dir`, on `machine`. Does no IO.
    #[must_use]
    pub fn new(dir: PathBuf, machine: impl Into<String>) -> Self {
        Self { dir, machine: machine.into(), open: Mutex::new(None) }
    }

    /// Opens the directory `dir` (absolute) as a workspace, or, with no
    /// `dir`, the workspace with no root named `name`. Makes the definition
    /// file when there is none (`name` names a new one) and the default
    /// canvas when it is missing. Returns the workspace and whether its file
    /// is new. Opening an open workspace returns it.
    ///
    /// # Errors
    ///
    /// `invalid` with neither, or for a `dir` that is not an absolute
    /// directory; `not_found` for a `dir` that is not there; `conflict` when
    /// the file names another workspace than `name`, when the workspace
    /// breaks a rule over the open ones, or when it takes the name or the
    /// canvas a broken one's file states; and the errors of the files.
    pub fn open(
        &self,
        dir: Option<&Path>,
        name: Option<&str>,
        canvases: &Canvases,
    ) -> Result<(Opened, bool)> {
        let mut guard = self.lock()?;
        let open = guard.get_or_insert_default();
        let home = match (dir, name) {
            (Some(dir), _) => directory(dir)?,
            (None, Some(name)) => self.dir.join(workspace::slug(name)),
            (None, None) => {
                return Err(invalid("open needs a directory or a name", "pass <DIR> or --name"));
            }
        };
        let file = home.join(FILE_NAME);
        let (ws, new) = match read(&file) {
            Err(WorkspaceError::File { source: slinger_core::Error::NotFound { .. }, .. })
                if !open.contains(&home) =>
            {
                let ws = match dir {
                    Some(_) => Workspace::for_dir(&self.machine, &utf8(&home)?, name),
                    None => Workspace::named(name.unwrap_or_default()),
                };
                (ws, true)
            }
            read => (read?, false),
        };
        if let Some(name) = name.filter(|n| *n != ws.name) {
            return Err(slinger_core::Error::Conflict {
                message: format!("{} is workspace {:?}, not {name:?}", home.display(), ws.name),
                hint: Some(format!("open it as {:?}, or without --name", ws.name)),
            }
            .into());
        }
        let opened = Opened { dir: home, workspace: ws };
        if open.contains(&opened.dir) {
            return Ok((opened, false));
        }
        let mut all = Vec::new();
        for listed in load(open) {
            match listed {
                Listed::Served(o) => all.push(o.workspace),
                Listed::Broken(b) => {
                    let dir = b.dir.display().to_string();
                    let (name, canvas) = (b.name.as_deref(), b.canvas.as_deref());
                    workspace::check_reserved(&opened.workspace, &dir, name, canvas)?;
                }
            }
        }
        all.push(opened.workspace.clone());
        workspace::check(&all)?;
        if new {
            mkdir(&opened.dir)?;
            write_synced(&file, opened.workspace.to_toml().as_bytes())?;
        }
        match canvases.start(&opened.workspace.canvas) {
            Err(CanvasError::Core(slinger_core::Error::Conflict { .. })) | Ok(()) => {}
            Err(e) => return Err(e.into()),
        }
        open.push(opened.dir.clone());
        self.save(open)?;
        Ok((opened, new))
    }

    /// The open workspaces, sorted by name: each one served, or broken
    /// when its file cannot be used.
    ///
    /// # Errors
    ///
    /// The error of reading the list.
    pub fn list(&self) -> Result<Vec<Listed>> {
        let guard = self.lock()?;
        let mut all = load(guard.as_deref().unwrap_or_default());
        all.sort_by(|a, b| a.name().cmp(b.name()));
        Ok(all)
    }

    /// The open workspaces whose files the daemon can use, by name.
    ///
    /// # Errors
    ///
    /// As [`Workspaces::list`].
    pub fn usable(&self) -> Result<Vec<Opened>> {
        Ok(self.list()?.into_iter().filter_map(Listed::served).collect())
    }

    /// The open workspace `which` names: its name, or the absolute directory
    /// of its file.
    ///
    /// # Errors
    ///
    /// `not_found` for no such open workspace, and the error of its file.
    pub fn show(&self, which: &str) -> Result<Opened> {
        let guard = self.lock()?;
        let open = guard.as_deref().unwrap_or_default();
        let dir = find(open, which).ok_or_else(|| not_open(which))?;
        Ok(Opened { workspace: read(&dir.join(FILE_NAME))?, dir: dir.clone() })
    }

    /// The panes of the open workspace `which` names, as
    /// [`Workspaces::show`], after `change`. A change that leaves them as
    /// they were writes nothing; one that changes them writes the file whole
    /// before this returns. One change at a time, so none is lost.
    ///
    /// # Errors
    ///
    /// The errors of [`Workspaces::show`], `change`'s, and the error of
    /// writing the file.
    pub fn panes<T>(
        &self,
        which: &str,
        change: impl FnOnce(&mut Layout) -> slinger_core::Result<T>,
    ) -> Result<(Opened, Layout, T)> {
        let guard = self.lock()?;
        let open = guard.as_deref().unwrap_or_default();
        let dir = find(open, which).ok_or_else(|| not_open(which))?;
        let file = dir.join(FILE_NAME);
        let mut ws = read(&file)?;
        let before = ws.panes();
        let mut layout = before.clone();
        let changed = change(&mut layout)?;
        if layout != before {
            ws.set_panes(&layout);
            write_synced(&file, ws.to_toml().as_bytes())?;
        }
        Ok((Opened { dir: dir.clone(), workspace: ws }, layout, changed))
    }

    /// The open workspace `which` names, as [`Workspaces::show`], after
    /// `change`. A change that leaves its file as it was writes nothing;
    /// one that changes it writes the file whole before this returns, and
    /// one that fails writes nothing. One change at a time, as
    /// [`Workspaces::panes`].
    ///
    /// # Errors
    ///
    /// The errors of [`Workspaces::show`], `change`'s, and the error of
    /// writing the file.
    pub fn edit<T>(
        &self,
        which: &str,
        change: impl FnOnce(&mut Workspace) -> slinger_core::Result<T>,
    ) -> Result<(Opened, T)> {
        let guard = self.lock()?;
        let open = guard.as_deref().unwrap_or_default();
        let dir = find(open, which).ok_or_else(|| not_open(which))?;
        let file = dir.join(FILE_NAME);
        let mut ws = read(&file)?;
        let before = ws.clone();
        let changed = change(&mut ws)?;
        if ws != before {
            write_synced(&file, ws.to_toml().as_bytes())?;
        }
        Ok((Opened { dir: dir.clone(), workspace: ws }, changed))
    }

    /// Closes the open workspace `which` names, as [`Workspaces::show`], and
    /// returns the directory of its file. The file and the canvas stay.
    ///
    /// # Errors
    ///
    /// `not_found` for no such open workspace, and the error of writing the
    /// list.
    pub fn close(&self, which: &str) -> Result<PathBuf> {
        let mut guard = self.lock()?;
        let open = guard.get_or_insert_default();
        let dir = find(open, which).ok_or_else(|| not_open(which))?.clone();
        open.retain(|d| *d != dir);
        self.save(open)?;
        Ok(dir)
    }

    /// Sets the workspace tag of an event that has none, from its canvas or
    /// agent ([`workspace::tag_for`]). A file that cannot be read tags nothing.
    pub fn tag(&self, tags: &mut Tags) {
        if tags.workspace.is_some() || (tags.canvas.is_none() && tags.agent.is_none()) {
            return;
        }
        let Ok(guard) = self.lock() else { return };
        let all: Vec<Workspace> = guard
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter_map(|dir| read(&dir.join(FILE_NAME)).ok())
            .collect();
        let tag = workspace::tag_for(&all, tags.canvas.as_deref(), tags.agent.as_deref());
        tags.workspace = tag.map(str::to_string);
    }

    /// The list, read from its file on first use.
    fn lock(&self) -> Result<MutexGuard<'_, Option<Vec<PathBuf>>>> {
        let mut guard = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        if guard.is_none() {
            *guard = Some(self.read_list()?);
        }
        Ok(guard)
    }

    fn read_list(&self) -> Result<Vec<PathBuf>> {
        let path = self.dir.join(LIST);
        let text = match fs::read_to_string(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            other => other.map_err(failed("read", &path))?,
        };
        let doc: Value = serde_json::from_str(&text).map_err(|e| WorkspaceError::Io {
            doing: "read",
            path: path.clone(),
            source: io::Error::new(io::ErrorKind::InvalidData, e),
        })?;
        let dirs = doc["dirs"].as_array().map(Vec::as_slice).unwrap_or_default();
        Ok(dirs.iter().filter_map(Value::as_str).map(PathBuf::from).collect())
    }

    /// Writes the list whole, or not at all.
    fn save(&self, open: &[PathBuf]) -> Result<()> {
        mkdir(&self.dir)?;
        let dirs: Vec<_> = open.iter().map(|d| d.to_string_lossy()).collect();
        let text = format!("{}\n", json!({ "v": LIST_VERSION, "dirs": dirs }));
        write_synced(&self.dir.join(LIST), text.as_bytes())?;
        File::open(&self.dir).and_then(|d| d.sync_all()).map_err(failed("sync", &self.dir))
    }
}

/// The open workspaces' files, read, each on its own.
fn load(open: &[PathBuf]) -> Vec<Listed> {
    let one = |dir: &PathBuf| {
        let file = dir.join(FILE_NAME);
        let error = match read(&file) {
            Ok(workspace) => return Listed::Served(Opened { dir: dir.clone(), workspace }),
            Err(error) => error,
        };
        let (name, canvas) =
            fs::read_to_string(&file).map(|t| workspace::peek(&t)).unwrap_or_default();
        Listed::Broken(Broken { dir: dir.clone(), name, canvas, error })
    };
    open.iter().map(one).collect()
}

/// Reads and parses one definition file.
fn read(path: &Path) -> Result<Workspace> {
    let file = |field, source| WorkspaceError::File { path: path.to_owned(), field, source };
    let text = match fs::read_to_string(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let what = format!("workspace file {}", path.display());
            return Err(file(None, slinger_core::Error::NotFound { what, hint: None }));
        }
        other => other.map_err(failed("read", path))?,
    };
    Workspace::read(&text).map_err(|r| file(r.field, r.error))
}

/// The open directory `which` names: by its path when it starts with `/`,
/// else by the name in its file, even a file that breaks a rule, so a verb
/// on it fails with its file's error. A file with no name matches no name.
fn find<'a>(open: &'a [PathBuf], which: &str) -> Option<&'a PathBuf> {
    if which.starts_with('/') {
        let which = Path::new(which.trim_end_matches('/'));
        let real = fs::canonicalize(which).ok();
        return open.iter().find(|d| *d == which || Some(*d) == real.as_ref());
    }
    let named = |d: &&PathBuf| {
        let text = fs::read_to_string(d.join(FILE_NAME));
        text.is_ok_and(|t| workspace::peek(&t).0.as_deref() == Some(which))
    };
    open.iter().find(named)
}

/// The directory a client named, as one path: absolute, links resolved.
fn directory(dir: &Path) -> Result<PathBuf> {
    if !dir.is_absolute() {
        let message = format!("path {} is not absolute", dir.display());
        return Err(invalid(&message, "pass the full path from /"));
    }
    let real = fs::canonicalize(dir).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            let what = format!("directory {}", dir.display());
            let hint = Some("check the path: slingerd reads it on this machine".into());
            return WorkspaceError::Core(slinger_core::Error::NotFound { what, hint });
        }
        failed("read", dir)(e)
    })?;
    if !real.is_dir() {
        let message = format!("{} is not a directory", dir.display());
        return Err(invalid(&message, "pass the directory to open as a workspace"));
    }
    Ok(real)
}

fn utf8(path: &Path) -> Result<String> {
    path.to_str().map(str::to_string).ok_or_else(|| {
        let message = format!("{} is not UTF-8", path.display());
        invalid(&message, "rename the directory, or open another")
    })
}

fn not_open(which: &str) -> WorkspaceError {
    let what = format!("open workspace {which:?}");
    let hint = Some("see 'slinger workspace list', or open it first".into());
    slinger_core::Error::NotFound { what, hint }.into()
}

fn invalid(message: &str, hint: &str) -> WorkspaceError {
    slinger_core::Error::Invalid { message: message.into(), hint: Some(hint.into()) }.into()
}

fn mkdir(dir: &Path) -> Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(dir).map_err(failed("make", dir))
}

/// The file whole or not at all: written aside, synced, then renamed in.
fn write_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    let temp = path.with_file_name(format!(".{name}.new"));
    let written =
        File::create(&temp).and_then(|mut f| f.write_all(bytes).and_then(|()| f.sync_all()));
    written.map_err(failed("write", &temp))?;
    fs::rename(&temp, path).map_err(failed("write", path))
}

/// What a failed read or write of `path` becomes.
fn failed(doing: &'static str, path: &Path) -> impl FnOnce(io::Error) -> WorkspaceError {
    let path = path.to_owned();
    move |source| WorkspaceError::Io { doing, path, source }
}
