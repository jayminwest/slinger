//! The daemon's own verbs as they cross the socket: `daemon_info` and
//! `daemon_stop`. They describe and end this process, which the core cannot
//! know, so they have no core half. `serve` hands every connection a `Here`
//! (the socket, when it began to serve, the `Stopper`). A stop writes
//! `stopped` in the data directory (`control::stop_mark`), then stops as
//! SIGTERM does; the next daemon's `serve` removes the mark as it starts.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use slinger_control::{self as control, DaemonInfo, DaemonStopped, RemoteError};

use crate::server::Stopper;
use crate::store::Store;

/// What the server knows of the process that `daemon_info` and
/// `daemon_stop` need and the store does not: its socket, when it began to
/// serve and how to stop it.
#[derive(Debug)]
pub(super) struct Here {
    pub(super) stopper: Stopper,
    pub(super) socket: PathBuf,
    pub(super) started_ms: u64,
}

impl Here {
    /// The facts of a server that begins to serve now.
    pub(super) fn now(stopper: Stopper, socket: &Path) -> Self {
        let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        let started_ms = u64::try_from(since.as_millis()).unwrap_or(u64::MAX);
        Self { stopper, socket: std::path::absolute(socket).unwrap_or(socket.into()), started_ms }
    }
}

pub(super) fn stop(store: &Store, here: &Here) -> Result<DaemonStopped, RemoteError> {
    let pid = std::process::id();
    // The mark first: a stop that cannot leave it does not happen, so a
    // launcher never takes a deliberate stop for a crash.
    let mark = control::stop_mark(store.dir());
    fs::write(&mark, format!("{pid}\n")).map_err(|e| {
        let what = format!("cannot write {}: {e}", mark.display());
        let hint = "check that slingerd's user can write its data directory".into();
        RemoteError::new("internal", what, Some(hint))
    })?;
    // The reply still goes out: the server serves every connection it took
    // before it ends.
    here.stopper.stop();
    Ok(DaemonStopped { pid })
}

pub(super) fn info(store: &Store, here: &Here) -> DaemonInfo {
    let pid = std::process::id();
    let text = |p: &Path| p.to_string_lossy().into_owned();
    let processes = store.sessions().processes();
    let exe = std::env::current_exe().map(|p| text(&p)).unwrap_or_default();
    DaemonInfo {
        version: control::PACKAGE.into(),
        build: control::stamp(),
        protocol: control::VERSION,
        pid,
        started_ms: here.started_ms,
        exe,
        socket: text(&here.socket),
        data_dir: text(store.dir()),
        claude: processes.claude().as_deref().map(text),
        slinger: text(processes.slinger()),
        log: store.log().map(text),
    }
}
