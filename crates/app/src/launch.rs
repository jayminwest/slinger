//! Whether the app starts `slingerd`, and when (docs/service.md, "Socket").
//! Pure: the app's path and what was found go in, the decision comes out;
//! `backend::daemon` looks at the socket, spawns and reaps.
//!
//! Only a bundled app ([`bundled`]) whose config names no socket and that
//! is not driven starts a daemon ([`managed`]), so a bare `slinger-app`
//! (`cargo run`, the tests, another worktree's build) never touches the
//! user's record. A managed app keeps a [`Launch`]: it starts the bundled
//! daemon when none answers, waits for it, says so when it does not come,
//! and tries again. A daemon stopped on purpose (`slinger daemon stop`
//! leaves a mark) stays stopped until the user asks: the app's own launch
//! counts as asking, and so does `start-slingerd`. A daemon of another
//! build is offered for a restart ([`skew`]), never restarted unasked:
//! `restart-daemon` stops it and [`Launch::restart`] starts the bundled
//! one once it is gone.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::scene::daemon::Daemon;

mod skew;
pub use skew::{Build, Skew, skew};

/// How long a started daemon has to answer before it did not start.
pub const START_WAIT: Duration = Duration::from_secs(10);
/// How long after a failed start the app tries again.
pub const RETRY: Duration = Duration::from_secs(30);
/// `slingerd`'s exit code when another daemon holds the socket or the
/// record: two apps raced to start one, and the other won.
pub const LOCKED: i32 = 4;

/// The daemon bundled beside the app at `exe` (its path with links
/// resolved): `<X>.app/Contents/MacOS/slingerd` for
/// `<X>.app/Contents/MacOS/slinger-app`; `None` for a bare binary.
pub fn bundled(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    let named = |p: &Path, name: &str| p.file_name().is_some_and(|n| n == name);
    let shape = named(exe, "slinger-app")
        && named(macos, "MacOS")
        && named(contents, "Contents")
        && app.extension().is_some_and(|e| e == "app");
    shape.then(|| macos.join("slingerd"))
}

/// What decides whether the app starts a daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The app's own path, links resolved.
    pub exe: PathBuf,
    /// Whether the config sets `[daemon] socket`.
    pub socket_set: bool,
    /// Whether the window is driven (`--drive`).
    pub driven: bool,
    /// The default data directory, if it is known.
    pub data_dir: Option<PathBuf>,
}

/// The daemon a managed app starts, and the record it serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Managed {
    pub program: PathBuf,
    pub data_dir: PathBuf,
}

impl Managed {
    /// The log the started daemon writes: `slingerd.log` in its record.
    pub fn log(&self) -> PathBuf {
        self.data_dir.join("slingerd.log")
    }
}

/// The daemon to start, when the app at `found.exe` starts one: it is
/// bundled and `is_file`, the config names no socket, the window is not
/// driven and the data directory is known. `None`: the app never starts a
/// daemon.
pub fn managed(found: Found, is_file: impl Fn(&Path) -> bool) -> Option<Managed> {
    let program = bundled(&found.exe).filter(|p| is_file(p))?;
    let data_dir = found.data_dir.filter(|_| !found.socket_set && !found.driven)?;
    Some(Managed { program, data_dir })
}

/// One look at the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seen {
    /// Its socket takes a connection. A daemon that takes it and answers
    /// slowly is running (slinger-2f9883).
    pub answers: bool,
    /// Its record holds the mark of a `daemon_stop`.
    pub stopped: bool,
}

/// Where the daemon is, as the app knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    /// Not looked at yet.
    Unknown,
    Up,
    /// Started at `since`, not answering yet.
    Starting {
        since: Instant,
    },
    /// The start at `at` failed: the exit code and the first line it said.
    Failed {
        at: Instant,
        code: Option<i32>,
        said: Option<String>,
    },
    /// Stopped on purpose; not started until asked.
    Stopped,
}

/// The app's part in the daemon's life: one child at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    state: State,
    /// The user asked for a daemon: the app was opened, or `start-slingerd`
    /// ran. Used up once one answers.
    asked: bool,
    /// A child the app started has not been reaped.
    child: bool,
    /// The user asked for a restart and the daemon agreed to stop: `asked`
    /// outlives its last answers, until it is gone.
    restarting: bool,
}

impl Default for Launch {
    fn default() -> Self {
        Self::new()
    }
}

impl Launch {
    /// The app was just opened: that asks for a daemon.
    pub fn new() -> Self {
        Self { state: State::Unknown, asked: true, child: false, restarting: false }
    }

    /// Takes what a look at `now` saw; true: start a daemon now.
    pub fn look(&mut self, seen: Seen, now: Instant) -> bool {
        if seen.answers {
            self.state = State::Up;
            self.asked &= self.restarting;
            return false;
        }
        self.restarting = false;
        let waited = |since: Instant| now.saturating_duration_since(since);
        match self.state {
            State::Starting { since } if waited(since) < START_WAIT => false,
            State::Starting { .. } => {
                self.state = State::Failed { at: now, code: None, said: None };
                false
            }
            State::Failed { at, .. } if waited(at) < RETRY => false,
            _ if seen.stopped && !self.asked => {
                self.state = State::Stopped;
                false
            }
            // The daemon it started closed its socket and is still ending:
            // reaped first, so its exit is not taken for the next start's.
            State::Up if self.child => false,
            _ => {
                self.state = State::Starting { since: now };
                // A child still running is waited for, not started again.
                !std::mem::replace(&mut self.child, true)
            }
        }
    }

    /// The child exited with `code` (`None`: a signal ended it), having
    /// said `said`. Another daemon holding the lock is not a failure: that
    /// one is waited for.
    pub fn exited(&mut self, code: Option<i32>, said: Option<String>, now: Instant) {
        self.child = false;
        if matches!(self.state, State::Starting { .. }) && code != Some(LOCKED) {
            self.state = State::Failed { at: now, code, said };
        }
    }

    /// The user asks for a daemon (`start-slingerd`): the next look starts
    /// one, even a stopped one, without waiting out a retry.
    pub fn ask(&mut self) {
        self.asked = true;
        if matches!(self.state, State::Stopped | State::Failed { .. }) {
            self.state = State::Unknown;
        }
    }

    /// The daemon agreed to stop (`daemon_stop`) because the user asked
    /// for a restart: once it is gone the next look starts the bundled one,
    /// though it leaves the mark of a stop on purpose.
    pub fn restart(&mut self) {
        (self.asked, self.restarting) = (true, true);
    }

    /// Whether the daemon answers, as last seen.
    pub fn is_up(&self) -> bool {
        self.state == State::Up
    }

    /// What the window says of the daemon, if anything: the failure points
    /// at `log`.
    pub fn shown(&self, log: &Path) -> Option<Daemon> {
        match &self.state {
            State::Unknown | State::Up => None,
            State::Starting { .. } => Some(Daemon::Starting),
            State::Failed { code, said, .. } => Some(Daemon::Failed {
                code: *code,
                said: said.clone(),
                log: log.display().to_string(),
            }),
            State::Stopped => Some(Daemon::Stopped),
        }
    }
}

#[cfg(test)]
mod tests;
