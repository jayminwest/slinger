//! The control socket: one instance per path, one thread per connection, and a
//! stop that lets every accepted request finish.

use std::fs::{self, File, Permissions, TryLockError};
use std::io::{self, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use slinger_control as control;

use control::{Greeting, RemoteError, Reply, Request};

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
    _lock: File,
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
        let mut lock_path = socket.as_os_str().to_owned();
        lock_path.push(".lock");
        let lock = File::options().create(true).append(true).open(lock_path).map_err(io)?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(StartError::Taken(socket.to_owned())),
            Err(TryLockError::Error(e)) => return Err(io(e)),
        }
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

    /// Serves until [`Stopper::stop`], then waits for the open connections and
    /// removes the socket file.
    ///
    /// # Errors
    ///
    /// The error that ended the accept loop, or the socket file's removal.
    pub fn serve(self) -> io::Result<()> {
        let accepted = std::thread::scope(|scope| {
            for stream in self.listener.incoming() {
                // The first connection accepted after a stop is the last one
                // served, so every connection queued before the stop is served.
                let last = self.stopping.load(Ordering::SeqCst);
                let stream = stream?;
                scope.spawn(move || converse(&stream));
                if last {
                    break;
                }
            }
            Ok(())
        });
        fs::remove_file(&self.socket)?;
        accepted
    }
}

/// One connection: one request, one reply. A failure here is the client's to
/// notice (it gets no reply); the daemon has no one else to tell.
fn converse(stream: &UnixStream) -> io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let reply = match control::read_request(&mut BufReader::new(stream)) {
        Ok(None) => return Ok(()),
        Ok(Some(request)) => answer(request),
        Err(error) => Err(error),
    };
    control::write_reply(&mut &*stream, &reply)
}

/// Request in, core call, reply out. One arm per verb, and no logic of its own.
fn answer(request: Request) -> Result<Reply, RemoteError> {
    match request {
        Request::Hello { name } => slinger_core::greet(&name)
            .map(|greeting| Reply::Hello(Greeting { greeting }))
            .map_err(|e| remote(&e)),
    }
}

/// The core's error as it crosses the socket: same code, message and hint.
fn remote(error: &slinger_core::Error) -> RemoteError {
    RemoteError::new(error.code(), error.to_string(), error.hint().map(str::to_string))
}
