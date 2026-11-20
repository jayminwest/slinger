//! The control socket: one instance per path, one thread per connection, and a
//! stop that lets every accepted request finish.

mod agent;
mod answer;
mod canvas;
mod daemon;
mod feedback;
mod fold;
mod import;
mod look;
mod pane;
mod profile;
mod read;
mod record;
mod session;
mod showing;
mod sling;
mod uis;
mod workspace;

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

use control::{RemoteError, Request};
use slinger_core::record::BlobId;

use crate::canvases::CanvasError;
use crate::lock::Lock;
use crate::store::{Store, StoreError};
use crate::workspaces::WorkspaceError;

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
    /// The socket's file name alone is too long for a socket's address.
    #[error("cannot open socket {}: {source}", path.display())]
    TooLong { path: PathBuf, source: io::Error },
}

impl StartError {
    /// Stable machine-readable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Taken(_) => "conflict",
            Self::Io { .. } => "internal",
            Self::TooLong { .. } => "invalid",
        }
    }

    /// What the caller can do about it, if anything.
    #[must_use]
    pub fn hint(&self) -> Option<String> {
        match self {
            Self::Taken(_) => Some("stop that daemon, or pass another --socket path".into()),
            Self::Io { .. } => None,
            Self::TooLong { .. } => Some("pass a --socket path with a shorter file name".into()),
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
        let _ = control::connect_socket(&self.socket);
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
        let listener = control::bind_socket(socket).map_err(|e| match e.kind() {
            io::ErrorKind::InvalidInput => {
                StartError::TooLong { path: socket.to_owned(), source: e }
            }
            _ => io(e),
        })?;
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
        let here = daemon::Here::now(self.stopper(), &self.socket);
        let accepted = std::thread::scope(|scope| {
            let accepted = self.accept(scope, store, &here);
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
    fn accept<'s>(
        &'s self,
        scope: &'s Scope<'s, '_>,
        store: &'s Store,
        here: &'s daemon::Here,
    ) -> io::Result<()> {
        for stream in self.listener.incoming() {
            // The first connection accepted after a stop is the last one
            // served, so every connection queued before the stop is served.
            let last = self.stopping.load(Ordering::SeqCst);
            let stream = stream?;
            scope.spawn(move || converse(&stream, store, here));
            if last {
                break;
            }
        }
        Ok(())
    }
}

/// One connection: one request, one reply. A failure here is the client's to
/// notice (it gets no reply); the daemon has no one else to tell.
fn converse(stream: &UnixStream, store: &Store, here: &daemon::Here) -> io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let (reply, listen) = match control::read_request(&mut BufReader::new(stream)) {
        Ok(None) => return Ok(()),
        Ok(Some(request)) => {
            let listen = matches!(request, Request::SessionListen(_));
            (answer::answer(request, store, here), listen)
        }
        Err(error) => (Err(error), false),
    };
    let written = control::write_reply(&mut &*stream, &reply);
    written.inspect_err(|_| {
        if listen {
            session::unheard(store, &reply);
        }
    })
}

/// A workspace error as it crosses the socket.
fn kept(error: &WorkspaceError) -> RemoteError {
    RemoteError::new(error.code(), error.to_string(), error.hint())
}

/// A canvas error as it crosses the socket.
fn drawn(error: &CanvasError) -> RemoteError {
    RemoteError::new(error.code(), error.to_string(), error.hint())
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

/// The core's error as it crosses the socket: same code, message and hint.
fn remote(error: &slinger_core::Error) -> RemoteError {
    RemoteError::new(error.code(), error.to_string(), error.hint().map(str::to_string))
}

/// The store's error as it crosses the socket.
fn stored(error: &StoreError) -> RemoteError {
    RemoteError::new(error.code(), error.to_string(), error.hint())
}
