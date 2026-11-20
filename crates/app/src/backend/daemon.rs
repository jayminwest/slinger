//! The [`Launcher`]: a bundled app's daemon, started when none answers
//! (docs/service.md, "Socket"). [`crate::launch`] decides; this thread looks
//! at the socket about once a second, spawns the bundled `slingerd` and
//! reaps it, and tells the window what to say ([`Daemon`]).
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

use slinger_control as control;

use super::Settings;
use super::wake::Wake;
use crate::launch::{self, Found, Launch, Managed, Seen};
use crate::scene::daemon::Daemon;

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
/// on `socket`, and whether it was stopped on purpose. Only a refused
/// connect or no socket file is no daemon: one that takes the connection
/// is running, however slowly it answers (slinger-2f9883).
fn look(data_dir: &Path, socket: &Path) -> Seen {
    let socket = control::live_socket(data_dir).unwrap_or_else(|| socket.to_path_buf());
    let answers = match UnixStream::connect(&socket) {
        Ok(_) => true,
        Err(e) => !matches!(e.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused),
    };
    Seen { answers, stopped: control::stop_mark(data_dir).exists() }
}

/// The thread that keeps a bundled app's daemon running, and what it last
/// said. The thread ends once the launcher is dropped.
#[derive(Debug)]
pub struct Launcher {
    asked: Arc<Asked>,
    news: Receiver<Said>,
}

/// What the window says of the daemon now; `None`: nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said(pub Option<Daemon>);

/// Whether the user asked for a daemon since the thread last looked, and
/// whether the launcher is gone; the bell that wakes the thread.
#[derive(Debug, Default)]
struct Asked {
    now: Mutex<(bool, bool)>,
    bell: Condvar,
}

impl Asked {
    fn lock(&self) -> MutexGuard<'_, (bool, bool)> {
        self.now.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn ring(&self, change: impl FnOnce(&mut (bool, bool))) {
        change(&mut self.lock());
        self.bell.notify_all();
    }

    /// Waits up to `every`; whether the user asked meanwhile, or `None`
    /// once the launcher is gone.
    fn next(&self, every: Duration) -> Option<bool> {
        let (mut now, _) = self
            .bell
            .wait_timeout_while(self.lock(), every, |&mut (asked, gone)| !asked && !gone)
            .unwrap_or_else(PoisonError::into_inner);
        let (asked, gone) = std::mem::take(&mut *now);
        (!gone).then_some(asked)
    }
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
            let (mut launch, mut child, mut told) = (Launch::new(), None::<Box<dyn Started>>, None);
            loop {
                if let Some(exit) = child.as_mut().and_then(|c| c.exited()) {
                    child = None;
                    launch.exited(exit.0, exit.1, Instant::now());
                }
                if launch.look(look(&managed.data_dir, &socket), Instant::now()) {
                    match spawn(&managed) {
                        Ok(started) => child = Some(started),
                        Err(e) => launch.exited(None, Some(e.to_string()), Instant::now()),
                    }
                }
                let shown = launch.shown(&managed.log());
                if shown != told {
                    if tell.send(Said(shown.clone())).is_err() {
                        return;
                    }
                    told = shown;
                }
                match theirs.next(every) {
                    None => return,
                    Some(true) => launch.ask(),
                    Some(false) => {}
                }
            }
        });
        Self { asked, news }
    }

    /// The user asks for a daemon (`start-slingerd`): one starts at once,
    /// even one stopped on purpose.
    pub fn ask(&self) {
        self.asked.ring(|now| now.0 = true);
    }

    /// What the window says of the daemon now, if that changed since the
    /// last call. Never blocks.
    pub fn latest(&self) -> Option<Said> {
        self.news.try_iter().last()
    }
}

impl Drop for Launcher {
    fn drop(&mut self) {
        self.asked.ring(|now| now.1 = true);
    }
}

#[cfg(test)]
mod tests;
