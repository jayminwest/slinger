//! The control socket: one instance per path, one thread per connection, and a
//! stop that lets every accepted request finish.

mod agent;
mod feedback;
mod import;
mod pane;
mod profile;
mod read;
mod session;
mod showing;
mod sling;

#[cfg(test)]
pub(crate) use sling::VERBS;

use std::fs::{self, Permissions};
use std::io::{self, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::Scope;
use std::time::Duration;

use slinger_control as control;

use control::{
    Appended, CanvasChange, CanvasFeed, CanvasImage, CanvasImages, CanvasInfo, CanvasPage,
    CanvasStrokes, Canvases, EventFilter, Events, Exported, NewEvent, RemoteError, Reply, Request,
    Stored, Uis, Written,
};
use slinger_core::canvas::file::Written as Lines;
use slinger_core::record::{self, BlobId, Body, Event, Tags};
use slinger_core::ui;
use slinger_core::workspace::FILE_NAME;

use crate::canvases::CanvasError;
use crate::lock::Lock;
use crate::store::{Query, Store, StoreError};
use crate::workspaces::{Broken, Listed, Opened, WorkspaceError};

/// How long one connection may take to send its request or take its reply. A
/// stalled client costs one thread for this long and cannot hold up a stop.
const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Why the daemon could not start.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// Another daemon serves this socket path.
    #[error("socket {} is in use by another slingerd", .0.display())]
    Taken(PathBuf),
    #[error("cannot open socket {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
}

impl StartError {
    /// Stable machine-readable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Taken(_) => "conflict",
            Self::Io { .. } => "internal",
        }
    }

    /// What the caller can do about it, if anything.
    #[must_use]
    pub fn hint(&self) -> Option<String> {
        match self {
            Self::Taken(_) => Some("stop that daemon, or pass another --socket path".into()),
            Self::Io { .. } => None,
        }
    }
}

/// A bound control socket. Dropping it without [`Server::serve`] frees the path.
#[derive(Debug)]
pub struct Server {
    listener: UnixListener,
    socket: PathBuf,
    /// Held for the server's life: the lock, not the socket file, says who owns the path.
    _lock: Lock,
    stopping: Arc<AtomicBool>,
}

/// Stops a running [`Server`] from another thread.
#[derive(Debug, Clone)]
pub struct Stopper {
    socket: PathBuf,
    stopping: Arc<AtomicBool>,
}

impl Stopper {
    /// Ends [`Server::serve`]. Requests on connections made before this call
    /// still get their replies.
    pub fn stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        // Wake the accept loop. If this fails the server is already gone.
        let _ = UnixStream::connect(&self.socket);
    }
}

impl Server {
    /// Takes the socket path. The socket is created for its owner only (0600).
    ///
    /// # Errors
    ///
    /// [`StartError::Taken`] when another daemon serves the path.
    pub fn bind(socket: &Path) -> Result<Self, StartError> {
        let io = |source| StartError::Io { path: socket.to_owned(), source };
        if let Some(dir) = socket.parent() {
            fs::create_dir_all(dir).map_err(io)?;
        }
        let mut path = socket.as_os_str().to_owned();
        path.push(".lock");
        let lock = Lock::take(Path::new(&path)).map_err(io)?;
        let lock = lock.ok_or_else(|| StartError::Taken(socket.to_owned()))?;
        // The lock is ours, so a socket file here is left over from a daemon that died.
        match fs::remove_file(socket) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(io(e)),
            _ => {}
        }
        let listener = UnixListener::bind(socket).map_err(io)?;
        fs::set_permissions(socket, Permissions::from_mode(0o600)).map_err(io)?;
        Ok(Self {
            listener,
            socket: socket.to_owned(),
            _lock: lock,
            stopping: Arc::new(AtomicBool::new(false)),
        })
    }

    /// The handle that ends [`Server::serve`].
    #[must_use]
    pub fn stopper(&self) -> Stopper {
        Stopper { socket: self.socket.clone(), stopping: Arc::clone(&self.stopping) }
    }

    /// Serves `store` until [`Stopper::stop`], then waits for the open
    /// connections, ends the agents' processes and removes the socket file.
    ///
    /// # Errors
    ///
    /// The error that ended the accept loop, or the socket file's removal.
    pub fn serve(self, store: &Store) -> io::Result<()> {
        store.sessions().processes().serve_on(&self.socket);
        let accepted = std::thread::scope(|scope| {
            let accepted = self.accept(scope, store);
            // A request waiting on a canvas feed or a sling answers now, so the scope ends.
            store.stop_waits();
            accepted
        });
        // The agents' processes end with the daemon.
        store.sessions().processes().stop_all();
        fs::remove_file(&self.socket)?;
        accepted
    }

    /// Serves each connection on a thread of `scope` until a stop.
    fn accept<'s>(&'s self, scope: &'s Scope<'s, '_>, store: &'s Store) -> io::Result<()> {
        for stream in self.listener.incoming() {
            // The first connection accepted after a stop is the last one
            // served, so every connection queued before the stop is served.
            let last = self.stopping.load(Ordering::SeqCst);
            let stream = stream?;
            scope.spawn(move || converse(&stream, store));
            if last {
                break;
            }
        }
        Ok(())
    }
}

/// One connection: one request, one reply. A failure here is the client's to
/// notice (it gets no reply); the daemon has no one else to tell.
fn converse(stream: &UnixStream, store: &Store) -> io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let reply = match control::read_request(&mut BufReader::new(stream)) {
        Ok(None) => return Ok(()),
        Ok(Some(request)) => answer(request, store),
        Err(error) => Err(error),
    };
    let written = control::write_reply(&mut &*stream, &reply);
    written.inspect_err(|_| session::unheard(store, &reply))
}

/// Request in, core call, reply out. One arm per verb, and no logic of its own.
fn answer(request: Request, store: &Store) -> Result<Reply, RemoteError> {
    match request {
        Request::RecordAppend(new) => {
            let event =
                store.append(event(new).map_err(|e| remote(&e))?).map_err(|e| stored(&e))?;
            Ok(Reply::RecordAppend(Appended { event: record::to_value(&event) }))
        }
        Request::RecordList(filter) => {
            let events = store.events(&query(filter)).map_err(|e| stored(&e))?;
            Ok(Reply::RecordList(Events { events: events.iter().map(record::to_value).collect() }))
        }
        Request::BlobPut { path } => {
            let bytes = fs::read(absolute(&path)?).map_err(|e| unreadable(&path, &e))?;
            let id = store.put_blob(&bytes).map_err(|e| stored(&e))?;
            Ok(Reply::BlobPut(Stored { id: id.to_string() }))
        }
        Request::BlobGet { id, path } => {
            let id: BlobId = id.parse().map_err(|e| remote(&e))?;
            let bytes = store.blob(id).map_err(|e| stored(&e))?.ok_or_else(|| missing(id))?;
            fs::write(absolute(&path)?, &bytes).map_err(|e| unusable(&path, &e))?;
            Ok(Reply::BlobGet(Written { size: bytes.len() as u64 }))
        }
        Request::UiPut { name, path, source } => ui_put(store, &name, &path, source),
        Request::UiList => Ok(Reply::UiList(uis(catalog(store)?.latest()))),
        Request::UiShow { name } => {
            Ok(Reply::UiShow(uis(catalog(store)?.versions(&name).map_err(|e| remote(&e))?)))
        }
        Request::UiExport { ui, dir } => ui_export(store, &ui, &dir),
        Request::CanvasCreate { canvas, path, by } => {
            import::create(store, canvas, path.as_deref(), by.as_deref())
        }
        Request::CanvasList => {
            let ids = store.canvases().list().map_err(|e| drawn(&e))?;
            let canvases = ids.into_iter().map(|canvas| CanvasInfo { canvas }).collect();
            Ok(Reply::CanvasList(Canvases { canvases }))
        }
        Request::CanvasShow { canvas, page } => {
            let (shown, next) =
                store.canvases().show(&canvas, page.as_deref()).map_err(|e| drawn(&e))?;
            let (page, pages, items) = (shown.page, shown.pages, shown.items);
            let path = Some(store.canvases().path(&canvas).to_string_lossy().into_owned());
            Ok(Reply::CanvasShow(CanvasPage { canvas, next, page, pages, items, path }))
        }
        Request::CanvasApply(o) => {
            let applied = store.canvases().apply(&o.canvas, &o.by, &o.ops, o.once.as_deref());
            Ok(Reply::CanvasApply(change(applied.map_err(|e| drawn(&e))?)))
        }
        Request::CanvasUndo(s) => {
            let lines = store.canvases().undo(&s.canvas, &s.by).map_err(|e| drawn(&e))?;
            Ok(Reply::CanvasUndo(change(lines)))
        }
        Request::CanvasRedo(s) => {
            let lines = store.canvases().redo(&s.canvas, &s.by).map_err(|e| drawn(&e))?;
            Ok(Reply::CanvasRedo(change(lines)))
        }
        Request::CanvasImport { path, canvas } => import::canvas(store, &path, canvas),
        Request::CanvasRead(ask) => read::packet(store, ask),
        Request::CanvasChanges { canvas, after, wait } => {
            let wait = Duration::from_millis(wait.min(control::MAX_WAIT_MS));
            let (ops, next) =
                store.canvases().changes(&canvas, after, wait).map_err(|e| drawn(&e))?;
            Ok(Reply::CanvasChanges(CanvasFeed { next, ops }))
        }
        Request::WorkspaceOpen { dir, name } => {
            let workspaces = store.workspaces();
            let (opened, new) = workspaces
                .open(dir.as_deref(), name.as_deref(), store.canvases())
                .map_err(|e| kept(&e))?;
            Ok(Reply::WorkspaceOpen(control::WorkspaceOpened { workspace: wire_ws(opened), new }))
        }
        Request::WorkspaceList => {
            let all = store.workspaces().list().map_err(|e| kept(&e))?;
            let workspaces = all.into_iter().map(|w| match w {
                Listed::Served(opened) => wire_ws(opened),
                Listed::Broken(broken) => wire_broken(broken),
            });
            let workspaces = workspaces.collect();
            Ok(Reply::WorkspaceList(control::Workspaces { workspaces }))
        }
        Request::WorkspaceShow { workspace } => {
            let opened = store.workspaces().show(&workspace).map_err(|e| kept(&e))?;
            Ok(Reply::WorkspaceShow(control::WorkspaceInfo { workspace: wire_ws(opened) }))
        }
        Request::WorkspaceClose { workspace } => {
            let dir = store.workspaces().close(&workspace).map_err(|e| kept(&e))?;
            Ok(Reply::WorkspaceClose(control::WorkspaceClosed { dir }))
        }
        Request::CanvasCrop { canvas, of, path } => crop(store, &canvas, &of, &path),
        Request::CanvasStrokes { canvas, item } => {
            let strokes = store.canvases().strokes(&canvas, &item).map_err(|e| drawn(&e))?;
            Ok(Reply::CanvasStrokes(CanvasStrokes { strokes }))
        }
        Request::Session(ask) => session::answer(store, ask),
        Request::Agent(ask) => agent::answer(store, ask),
        Request::Pane(ask) => pane::answer(store, ask),
        Request::Showing(ask) => showing::answer(store, ask),
        Request::Feedback(ask) => feedback::answer(store, ask),
        Request::Profile(ask) => profile::answer(store, ask),
    }
}

/// A workspace error as it crosses the socket.
fn kept(error: &WorkspaceError) -> RemoteError {
    RemoteError::new(error.code(), error.to_string(), error.hint())
}

fn wire_ws(Opened { dir, workspace: ws }: Opened) -> control::Workspace {
    let agent = ws.agent.map(|a| control::WorkspaceAgent {
        profiles: a.profiles.into_iter().map(|p| p.name).collect(),
        name: a.name,
        durable: a.durable,
    });
    let roots = ws.roots.into_iter();
    let roots = roots.map(|r| control::WorkspaceRoot {
        machine: r.machine,
        path: r.path,
        primary: r.primary,
    });
    control::Workspace {
        name: ws.name,
        tag: ws.tag,
        canvas: ws.canvas,
        dir,
        agent,
        roots: roots.collect(),
        error: None,
    }
}

/// An open workspace whose file the daemon cannot use, with its error.
fn wire_broken(Broken { dir, name, canvas, error }: Broken) -> control::Workspace {
    let (file, field) = error.file().map_or_else(
        || (dir.join(FILE_NAME), None),
        |(file, field)| (file.to_owned(), field.map(str::to_string)),
    );
    let error = control::WorkspaceFileError {
        code: error.code().into(),
        message: error.to_string(),
        hint: error.hint(),
        file,
        field,
    };
    control::Workspace {
        name: name.unwrap_or_default(),
        tag: String::new(),
        canvas: canvas.unwrap_or_default(),
        dir,
        agent: None,
        roots: Vec::new(),
        error: Some(error),
    }
}

fn change(lines: Lines) -> CanvasChange {
    CanvasChange { seq: lines.seq, made: lines.made }
}

/// Writes a crop's images: `path` names the one image, and tiles go beside
/// it as `<stem>-<n>.png`. Bytes travel as files, as a blob's do.
fn crop(store: &Store, canvas: &str, of: &str, path: &Path) -> Result<Reply, RemoteError> {
    let path = absolute(path)?;
    let (Some(dir), Some(name)) = (path.parent(), path.file_stem().and_then(|s| s.to_str())) else {
        return Err(unusable(path, &io::Error::other("it names no file")));
    };
    let images = store.canvases().crop(canvas, of, name).map_err(|e| drawn(&e))?;
    fs::create_dir_all(dir).map_err(|e| unusable(dir, &e))?;
    let images = images
        .into_iter()
        .map(|image| {
            let path = dir.join(&image.name);
            fs::write(&path, &image.png).map_err(|e| unusable(&path, &e))?;
            let (x, y, width, height) = (image.x, image.y, image.width, image.height);
            Ok(CanvasImage { path, x, y, width, height })
        })
        .collect::<Result<_, RemoteError>>()?;
    Ok(Reply::CanvasCrop(CanvasImages { images }))
}

/// A canvas error as it crosses the socket.
fn drawn(error: &CanvasError) -> RemoteError {
    RemoteError::new(error.code(), error.to_string(), error.hint())
}

/// Stores the code as a blob, then appends the next version unless the
/// name's latest already holds it. The append decides on the log as it is
/// inside the write, so two puts at once cannot take one version number.
fn ui_put(store: &Store, name: &str, path: &Path, source: String) -> Result<Reply, RemoteError> {
    let bytes = fs::read(absolute(path)?).map_err(|e| unreadable(path, &e))?;
    ui::check(name, &bytes).map_err(|e| remote(&e))?;
    let hash = store.put_blob(&bytes).map_err(|e| stored(&e))?;
    let size = bytes.len() as u64;
    let put = store
        .append_if(&ui_events(), |events| match ui::Catalog::new(events).put(name, hash, size) {
            ui::Put::New(body) => Ok(Event::new(0, source, Tags::on(""), Body::UiPut(body))),
            ui::Put::Same(version) => Err(version),
        })
        .map_err(|e| stored(&e))?;
    let (version, new) = match put {
        Ok(event) => (ui::Version::of(&event), true),
        Err(version) => (Some(version), false),
    };
    let version = version.ok_or_else(|| RemoteError::new("internal", "ui_put not stored", None))?;
    Ok(Reply::UiPut(control::UiPut { ui: wire_ui(&version), new }))
}

/// Writes the version `reference` names into `dir`, creating it.
fn ui_export(store: &Store, reference: &str, dir: &Path) -> Result<Reply, RemoteError> {
    let catalog = catalog(store)?;
    let version = catalog.resolve(reference).map_err(|e| remote(&e))?;
    let hash = version.hash;
    let bytes = store.blob(hash).map_err(|e| stored(&e))?.ok_or_else(|| missing(hash))?;
    fs::create_dir_all(absolute(dir)?).map_err(|e| unusable(dir, &e))?;
    let path = dir.join(version.file_name());
    fs::write(&path, &bytes).map_err(|e| unusable(&path, &e))?;
    Ok(Reply::UiExport(Exported { ui: wire_ui(version), path }))
}

fn ui_events() -> Query {
    Query { kind: Some("ui_put".into()), ..Query::default() }
}

fn catalog(store: &Store) -> Result<ui::Catalog, RemoteError> {
    Ok(ui::Catalog::new(&store.events(&ui_events()).map_err(|e| stored(&e))?))
}

fn uis<'a>(versions: impl IntoIterator<Item = &'a ui::Version>) -> Uis {
    Uis { uis: versions.into_iter().map(wire_ui).collect() }
}

fn wire_ui(v: &ui::Version) -> control::Ui {
    control::Ui {
        name: v.name.clone(),
        version: v.version,
        hash: v.hash.to_string(),
        bytes: v.bytes,
        source: v.source.clone(),
        time: v.time,
    }
}

/// A client's path, which must not depend on the daemon's working directory.
fn absolute(path: &Path) -> Result<&Path, RemoteError> {
    if path.is_absolute() {
        return Ok(path);
    }
    let what = format!("path {} is not absolute", path.display());
    Err(RemoteError::new("invalid", what, Some("pass the full path from /".into())))
}

/// A file to put that the daemon could not read.
fn unreadable(path: &Path, error: &io::Error) -> RemoteError {
    if error.kind() == io::ErrorKind::NotFound {
        let what = format!("file {} not found", path.display());
        let hint = "check the path: slingerd reads it on this machine".into();
        return RemoteError::new("not_found", what, Some(hint));
    }
    unusable(path, error)
}

/// A blob's file the daemon could not read or write.
fn unusable(path: &Path, error: &io::Error) -> RemoteError {
    let path = path.display();
    let hint = "pass a file slingerd's user can read and write".into();
    RemoteError::new("invalid", format!("cannot use file {path}: {error}"), Some(hint))
}

fn missing(id: BlobId) -> RemoteError {
    let hint = "store it first with 'slinger blob put'".to_string();
    remote(&slinger_core::Error::NotFound { what: format!("blob {id}"), hint: Some(hint) })
}

/// The event a writer asks for. The store sets its number, time and machine.
fn event(new: NewEvent) -> slinger_core::Result<Event> {
    let body = Body::new(&new.kind, new.payload)?;
    let tags = Tags {
        machine: String::new(),
        workspace: new.tags.workspace,
        agent: new.tags.agent,
        canvas: new.tags.canvas,
    };
    Ok(Event::new(0, new.source, tags, body))
}

fn query(filter: EventFilter) -> Query {
    Query {
        from: filter.after.map(|n| n.saturating_add(1)),
        kind: filter.kind,
        machine: filter.machine,
        workspace: filter.workspace,
        agent: filter.agent,
        canvas: filter.canvas,
        ..Query::default()
    }
}

/// The core's error as it crosses the socket: same code, message and hint.
fn remote(error: &slinger_core::Error) -> RemoteError {
    RemoteError::new(error.code(), error.to_string(), error.hint().map(str::to_string))
}

/// The store's error as it crosses the socket.
fn stored(error: &StoreError) -> RemoteError {
    RemoteError::new(error.code(), error.to_string(), error.hint())
}
