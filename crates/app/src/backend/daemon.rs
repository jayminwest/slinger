//! The [`Launcher`]: a bundled app's daemon, started when none answers
//! (docs/service.md, "Socket"). [`crate::launch`] decides; this thread looks
//! at the socket about once a second, spawns the bundled `slingerd` and
//! reaps it, and tells the window what to say ([`Daemon`]). Each time the
//! daemon comes to answer, or answers on another socket, it asks
//! `daemon_info` and tells the window when the build is another
//! ([`Offer`]); `restart-daemon` stops it ([`Launcher::restart`]) and the
//! same loop starts the bundled one.
//!
//! The daemon it starts takes the defaults (no `--socket`, no
//! `--data-dir`, never `--scratch`) and writes its log in its record. It
//! runs in a process group of its own, so it outlives the app and a signal
//! to the app's group never reaches it; the app sends it nothing when it
//! quits. What it says before its log opens (a taken socket, a bad data
//! directory) comes back on a pipe and shows as why it did not start.

use std::io::{self, ErrorKind, Read};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use slinger_control::{self as control, Client, DaemonInfo, RemoteError};

use super::Settings;
use super::wake::Wake;
use crate::launch::{self, Build, Found, Launch, Managed, Seen, Skew};
use crate::scene::daemon::{self as said, Daemon};
use crate::scene::sidebar::launched::Offer;

/// How often the launcher looks at the socket.
const EVERY: Duration = Duration::from_secs(1);

/// The daemon this app starts, if it starts one ([`launch::managed`]):
/// the app's path read with links resolved, as macOS names a program run
/// through a link by the link.
pub fn managed(settings: &Settings, driven: bool) -> Option<Managed> {
    let exe = std::env::current_exe().ok()?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let socket_set = settings.config.socket.is_some();
    let data_dir = control::default_data_dir();
    launch::managed(Found { exe, socket_set, driven, data_dir }, Path::is_file)
}

/// A daemon the launcher started.
pub trait Started: Send {
    /// Its exit code (`None`: a signal ended it) and the first line it
    /// said, once it has exited; `None` while it runs.
    fn exited(&mut self) -> Option<(Option<i32>, Option<String>)>;
}

/// Starts the daemon: the bundled program, or a test's stand-in.
pub type Spawn = Box<dyn FnMut(&Managed) -> io::Result<Box<dyn Started>> + Send>;

/// Starts `managed`'s program with `--log` in its record, its stderr piped
/// back, in a process group of its own.
///
/// # Errors
///
/// The program could not be run.
pub fn spawn(managed: &Managed) -> io::Result<Box<dyn Started>> {
    let child = Command::new(&managed.program)
        .arg("--log")
        .arg(managed.log())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()?;
    Ok(Box::new(child))
}

impl Started for Child {
    fn exited(&mut self) -> Option<(Option<i32>, Option<String>)> {
        let status = self.try_wait().ok()??;
        let mut said = String::new();
        if let Some(mut stderr) = self.stderr.take() {
            // Only what it said is lost if the pipe cannot be read.
            let _ = stderr.read_to_string(&mut said);
        }
        let first = said.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string);
        Some((status.code(), first))
    }
}

/// Whether the daemon of `data_dir` answers, on the socket it named, else
/// on `socket`, and whether it was stopped on purpose; and the socket
/// looked at. Only a refused connect or no socket file is no daemon: one
/// that takes the connection is running, however slowly it answers
/// (slinger-2f9883).
fn look(data_dir: &Path, socket: &Path) -> (Seen, PathBuf) {
    let socket = control::live_socket(data_dir).unwrap_or_else(|| socket.to_path_buf());
    let answers = match UnixStream::connect(&socket) {
        Ok(_) => true,
        Err(e) => !matches!(e.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused),
    };
    (Seen { answers, stopped: control::stop_mark(data_dir).exists() }, socket)
}

/// The skew of the daemon that said `info` to `daemon_info`: `None` when
/// it said nothing to go by (ask again later).
fn skew(info: &Result<DaemonInfo, RemoteError>) -> Option<Skew> {
    let stamp = control::stamp();
    let mine = Build { version: control::PACKAGE, stamp: &stamp };
    let theirs = info.as_ref().map(|i| Build { version: &i.version, stamp: &i.build });
    launch::skew(mine, theirs.map_err(RemoteError::code))
}

/// The offer for the daemon that said `info` and is `skew`: none for this
/// app's own build.
fn offer(info: &Result<DaemonInfo, RemoteError>, skew: Skew) -> Option<Offer> {
    let pid = info.as_ref().ok().map(|i| i.pid);
    (skew != Skew::Same).then_some(Offer { pid, skew })
}

/// The thread that keeps a bundled app's daemon running, and what it last
/// said. The thread ends once the launcher is dropped.
#[derive(Debug)]
pub struct Launcher {
    asked: Arc<Asked>,
    news: Receiver<Said>,
}

/// What the launcher tells the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    /// What the window says of the daemon now; `None`: nothing, it answers.
    Daemon(Option<Daemon>),
    /// The offer to restart the daemon that answers; `None`: its build is
    /// this app's, or none answers.
    Offer(Option<Offer>),
    /// The daemon did not stop for a restart: what to flash.
    Refused(String),
}

/// What the user asked since the thread last looked, and whether the
/// launcher is gone.
#[derive(Debug, Default, Clone, Copy)]
struct Asks {
    start: bool,
    restart: bool,
    gone: bool,
}

/// The [`Asks`] and the bell that wakes the thread.
#[derive(Debug, Default)]
struct Asked {
    now: Mutex<Asks>,
    bell: Condvar,
}

impl Asked {
    fn lock(&self) -> MutexGuard<'_, Asks> {
        self.now.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn ring(&self, change: impl FnOnce(&mut Asks)) {
        change(&mut self.lock());
        self.bell.notify_all();
    }

    /// Waits up to `every`; what the user asked meanwhile, or `None` once
    /// the launcher is gone.
    fn next(&self, every: Duration) -> Option<Asks> {
        let (mut now, _) = self
            .bell
            .wait_timeout_while(self.lock(), every, |a| !a.start && !a.restart && !a.gone)
            .unwrap_or_else(PoisonError::into_inner);
        let asks = std::mem::take(&mut *now);
        (!asks.gone).then_some(asks)
    }
}

/// What the thread last told the window, and the socket whose daemon's
/// build it last read.
#[derive(Default)]
struct Told {
    daemon: Option<Daemon>,
    offer: Option<Offer>,
    read: Option<PathBuf>,
}

impl Launcher {
    /// Keeps `managed`'s bundled daemon running, on the default socket.
    pub fn start(managed: Managed, wake: &Wake) -> Self {
        Self::with(managed, control::default_socket(), Box::new(spawn), EVERY, wake)
    }

    /// Looks every `every` at the daemon of `managed`'s record (on
    /// `socket` while it names none) and starts one with `spawn`.
    fn with(
        managed: Managed,
        socket: PathBuf,
        mut spawn: Spawn,
        every: Duration,
        wake: &Wake,
    ) -> Self {
        let asked = Arc::new(Asked::default());
        let (tell, news) = wake.channel();
        let theirs = Arc::clone(&asked);
        std::thread::spawn(move || {
            let (mut launch, mut child) = (Launch::new(), None::<Box<dyn Started>>);
            let mut told = Told::default();
            loop {
                if let Some(exit) = child.as_mut().and_then(|c| c.exited()) {
                    child = None;
                    launch.exited(exit.0, exit.1, Instant::now());
                }
                let (seen, at) = look(&managed.data_dir, &socket);
                if launch.look(seen, Instant::now()) {
                    match spawn(&managed) {
                        Ok(started) => child = Some(started),
                        Err(e) => launch.exited(None, Some(e.to_string()), Instant::now()),
                    }
                }
                let shown = launch.shown(&managed.log());
                if shown != told.daemon {
                    if tell.send(Said::Daemon(shown.clone())).is_err() {
                        return;
                    }
                    told.daemon = shown;
                }
                // The build of a daemon that came to answer, or moved.
                let mut offered = told.offer.clone();
                if !launch.is_up() {
                    (told.read, offered) = (None, None);
                } else if told.read.as_ref() != Some(&at) {
                    let info = Client::new(&at).daemon_info();
                    if let Some(skew) = skew(&info) {
                        (told.read, offered) = (Some(at.clone()), offer(&info, skew));
                    }
                }
                if offered != told.offer {
                    if tell.send(Said::Offer(offered.clone())).is_err() {
                        return;
                    }
                    told.offer = offered;
                }
                let Some(asks) = theirs.next(every) else { return };
                if asks.restart {
                    match Client::new(&at).daemon_stop() {
                        Ok(_) => launch.restart(),
                        Err(e) => {
                            let why = said::refused(e.code(), &e.to_string());
                            if tell.send(Said::Refused(why)).is_err() {
                                return;
                            }
                        }
                    }
                } else if asks.start {
                    launch.ask();
                }
            }
        });
        Self { asked, news }
    }

    /// The user asks for a daemon (`start-slingerd`): one starts at once,
    /// even one stopped on purpose.
    pub fn ask(&self) {
        self.asked.ring(|now| now.start = true);
    }

    /// The user asks for a restart (`restart-daemon`): the daemon that
    /// answers is stopped, and the bundled one started once it is gone.
    /// Its agents' processes end with it.
    pub fn restart(&self) {
        self.asked.ring(|now| now.restart = true);
    }

    /// What the launcher told since the last call, oldest first. Never blocks.
    pub fn news(&self) -> Vec<Said> {
        self.news.try_iter().collect()
    }
}

impl Drop for Launcher {
    fn drop(&mut self) {
        self.asked.ring(|now| now.gone = true);
    }
}

#[cfg(test)]
mod tests;
