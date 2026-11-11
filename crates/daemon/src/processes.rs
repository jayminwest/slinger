//! The Claude Code processes the daemon holds, one per agent (SPEC D10): it
//! starts each in its agent's home, writes messages to its input, reads its
//! output, and ends it on a stop or when the daemon stops.
//!
//! The core owns the stream format ([`slinger_core::session::stream`]). This
//! module owns the processes and the file of session ids, `sessions.json` in
//! the data directory: the id each agent's conversation last had, so the next
//! start resumes it, even after a restart. An id is kept once the process's
//! output names it; by then the conversation exists to resume. Slinger does
//! not keep a process alive (D20): one that dies shows as `exited` until the
//! next start. Each process gets the hooks the core makes
//! ([`slinger_core::session::hooks`]), so it reports its status and the files
//! it writes through the CLI.

use std::collections::{BTreeMap, HashMap};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use slinger_core::session::hooks;
use slinger_core::session::stream::{self, Limits, Start};

/// How long a stop waits for a process to end after its input closes, before
/// it kills the process.
const GRACE: Duration = Duration::from_secs(3);

/// The environment variable that names the agent to its process, so the
/// agent's own `slinger` calls act as it (docs/cli.md).
const AGENT_ENV: &str = "SLINGER_AGENT";
/// The environment variable that names the daemon's socket to the process.
const SOCKET_ENV: &str = "SLINGER_SOCKET";

/// Why a process could not be started or reached.
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("the agent {agent:?} has no running process: {why}")]
    NotRunning { agent: String, why: &'static str },
    #[error("cannot start {}: {source}", program.display())]
    Spawn { program: PathBuf, source: io::Error },
    #[error("cannot keep the session ids in {}: {source}", path.display())]
    Ids { path: PathBuf, source: io::Error },
}

impl ProcessError {
    /// Stable machine-readable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotRunning { .. } => "unavailable",
            Self::Spawn { source, .. } if source.kind() == io::ErrorKind::NotFound => "not_found",
            Self::Spawn { .. } | Self::Ids { .. } => "internal",
        }
    }

    /// What the caller can do about it, if anything.
    #[must_use]
    pub fn hint(&self) -> Option<String> {
        match self {
            Self::NotRunning { .. } => {
                Some("start it with 'slinger session start <WORKSPACE>'".into())
            }
            Self::Spawn { .. } => {
                Some("install Claude Code, or pass its path with 'slingerd --claude <PATH>'".into())
            }
            Self::Ids { .. } => None,
        }
    }
}

type Result<T> = std::result::Result<T, ProcessError>;

/// An agent's process as a session shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    /// Whether the process still runs; `false` once it ended without a stop.
    pub running: bool,
    pub pid: u32,
    /// Messages written that the process has not answered yet.
    pub pending: u32,
    /// The text of its last answer, if it gave one.
    pub answer: Option<String>,
}

/// The processes, by agent. Safe to share between threads.
#[derive(Debug)]
pub struct Processes {
    program: PathBuf,
    /// The CLI the processes' hooks run (docs/service.md, "Hooks").
    slinger: PathBuf,
    /// The daemon's socket, named to each process. Set once it serves.
    socket: OnceLock<PathBuf>,
    ids: Arc<Ids>,
    held: Mutex<HashMap<String, Held>>,
}

impl Processes {
    /// No processes yet. `program` is Claude Code's path (a bare name is
    /// looked up on `PATH`); `ids` is the file of session ids.
    #[must_use]
    pub fn new(program: PathBuf, ids: PathBuf) -> Self {
        let ids = Arc::new(Ids { path: ids, lock: Mutex::new(()) });
        let slinger = crate::store::SLINGER.into();
        Self { program, slinger, socket: OnceLock::new(), ids, held: Mutex::default() }
    }

    /// Starts `program` from now on, in place of the one [`Processes::new`] took.
    pub fn set_program(&mut self, program: PathBuf) {
        self.program = program;
    }

    /// Has the hooks of every process started from now on run `slinger`.
    pub fn set_slinger(&mut self, slinger: PathBuf) {
        self.slinger = slinger;
    }

    /// Names `socket` to every process started from now on.
    pub fn serve_on(&self, socket: &Path) {
        // A second call names the same socket: the first one stands.
        let _ = self.socket.set(socket.to_owned());
    }

    /// Starts `agent`'s process in `home` within `limits`, resuming its last
    /// session. A running one is left as it is; one that exited is replaced.
    ///
    /// # Errors
    ///
    /// [`ProcessError::Spawn`] when the program cannot run, and
    /// [`ProcessError::Ids`] when the file of ids cannot be read.
    pub fn start(&self, agent: &str, home: &Path, limits: &Limits) -> Result<()> {
        let mut held = self.lock();
        if held.get_mut(agent).is_some_and(Held::running) {
            return Ok(());
        }
        held.remove(agent);
        let kept = self.ids.get(agent)?;
        let resume = kept.is_some();
        let id = match kept {
            Some(id) => id,
            None => new_id().map_err(|source| self.ids.failed(source))?,
        };
        let start = if resume { Start::Resume(&id) } else { Start::Fresh(&id) };
        let mut command = Command::new(&self.program);
        // Its hooks report its status and the files it writes. Passed as an
        // argument, they are this process's alone: no settings file changes.
        command.arg("--settings").arg(hooks::settings(&self.slinger));
        command.args(stream::args(start, limits)).current_dir(home).env(AGENT_ENV, agent);
        if let Some(socket) = self.socket.get() {
            command.env(SOCKET_ENV, socket);
        }
        // Its chatter on stderr goes where the daemon's goes: the service log.
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
        let spawned = command.spawn();
        let mut child = spawned
            .map_err(|source| ProcessError::Spawn { program: self.program.clone(), source })?;
        let (stdin, stdout) = (child.stdin.take(), child.stdout.take());
        let heard = Arc::new(Mutex::new(Heard::default()));
        if let Some(out) = stdout {
            listen(out, agent, &heard, &self.ids);
        }
        held.insert(agent.to_string(), Held { child, id, stdin, heard });
        Ok(())
    }

    /// Writes `line` (one message, as the stream format makes it) to
    /// `agent`'s running process.
    ///
    /// # Errors
    ///
    /// [`ProcessError::NotRunning`] when there is no process, it exited, or
    /// it does not take the line.
    pub fn send(&self, agent: &str, line: &str) -> Result<()> {
        let mut held = self.lock();
        let not = |why| ProcessError::NotRunning { agent: agent.to_string(), why };
        let process = held.get_mut(agent).ok_or_else(|| not("it was not started"))?;
        if !process.running() {
            return Err(not("it exited"));
        }
        let stdin = process.stdin.as_mut().ok_or_else(|| not("its input is closed"))?;
        stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|_| not("it exited"))?;
        process.heard().pending += 1;
        Ok(())
    }

    /// Ends `agent`'s process, if the daemon holds one: its input closes,
    /// and it is killed if it has not ended within a grace period.
    pub fn stop(&self, agent: &str) {
        let process = self.lock().remove(agent);
        drop(process);
    }

    /// Ends every process, as [`Processes::stop`].
    pub fn stop_all(&self) {
        let mut all: Vec<Held> = self.lock().drain().map(|(_, p)| p).collect();
        // Every input closes first, so they end together, in one grace period.
        for process in &mut all {
            drop(process.stdin.take());
        }
        drop(all);
    }

    /// `agent`'s process, if the daemon holds one.
    pub fn shown(&self, agent: &str) -> Option<Shown> {
        let mut held = self.lock();
        let process = held.get_mut(agent)?;
        let running = process.running();
        let heard = process.heard();
        let pending = if running { heard.pending } else { 0 };
        let answer = heard.answer.clone();
        Some(Shown { running, pid: process.child.id(), pending, answer })
    }

    /// The session id `agent`'s conversation has: the one its process uses,
    /// else the one the next start resumes.
    ///
    /// # Errors
    ///
    /// [`ProcessError::Ids`] when the file of ids cannot be read.
    pub fn session_id(&self, agent: &str) -> Result<Option<String>> {
        if let Some(process) = self.lock().get(agent) {
            let heard = process.heard().session_id.clone();
            return Ok(Some(heard.unwrap_or_else(|| process.id.clone())));
        }
        self.ids.get(agent)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Held>> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One process. Dropping it ends the process.
#[derive(Debug)]
struct Held {
    child: Child,
    /// The session id it started with: the one it resumes, or a new one.
    id: String,
    /// `None` once closed.
    stdin: Option<ChildStdin>,
    heard: Arc<Mutex<Heard>>,
}

impl Held {
    /// Whether the process runs and its output is open.
    fn running(&mut self) -> bool {
        !self.heard().ended && matches!(self.child.try_wait(), Ok(None))
    }

    fn heard(&self) -> std::sync::MutexGuard<'_, Heard> {
        self.heard.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        // End of input is how a stream-input process is told to finish.
        drop(self.stdin.take());
        let deadline = Instant::now() + GRACE;
        while matches!(self.child.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        // Already ended, or it gets no more grace. Either way it is reaped.
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Its reader is not joined: a process it left behind may still hold
        // the output open. The reader ends when the output closes.
    }
}

/// What the reader has heard from a process.
#[derive(Debug, Default)]
struct Heard {
    session_id: Option<String>,
    pending: u32,
    answer: Option<String>,
    /// Its output closed: it ended, or soon will.
    ended: bool,
}

/// Reads the process's output on a thread of its own until it closes, so the
/// process never blocks on a full pipe: the session id it names (kept in the
/// file of ids) and each answer.
fn listen(out: ChildStdout, agent: &str, heard: &Arc<Mutex<Heard>>, ids: &Arc<Ids>) {
    let (agent, heard, ids) = (agent.to_string(), Arc::clone(heard), Arc::clone(ids));
    std::thread::spawn(move || {
        let mut lines = BufReader::new(out);
        let mut line = Vec::new();
        while lines.read_until(b'\n', &mut line).is_ok_and(|n| n > 0) {
            let said = stream::read(&String::from_utf8_lossy(&line));
            line.clear();
            let mut heard = heard.lock().unwrap_or_else(PoisonError::into_inner);
            if said.answer.is_some() {
                heard.pending = heard.pending.saturating_sub(1);
                heard.answer = said.answer;
            }
            if let Some(id) = said.session_id
                && heard.session_id.as_ref() != Some(&id)
            {
                // A failure to keep it costs the next start its resume, and
                // nothing now; there is no caller to tell.
                let _ = ids.set(&agent, &id);
                heard.session_id = Some(id);
            }
        }
        heard.lock().unwrap_or_else(PoisonError::into_inner).ended = true;
    });
}

/// The file of session ids: `{agent: id}` as JSON.
#[derive(Debug)]
struct Ids {
    path: PathBuf,
    /// Held across a read and its write, so two writers do not lose one.
    lock: Mutex<()>,
}

impl Ids {
    fn get(&self, agent: &str) -> Result<Option<String>> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(self.load()?.remove(agent))
    }

    fn set(&self, agent: &str, id: &str) -> Result<()> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut all = self.load()?;
        all.insert(agent.to_string(), id.to_string());
        let text = serde_json::to_string_pretty(&all).map_err(|e| self.failed(e.into()))?;
        let next = self.path.with_extension("json.next");
        std::fs::write(&next, text + "\n")
            .and_then(|()| std::fs::rename(&next, &self.path))
            .map_err(|source| self.failed(source))
    }

    fn load(&self) -> Result<BTreeMap<String, String>> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| self.failed(e.into())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(self.failed(e)),
        }
    }

    fn failed(&self, source: io::Error) -> ProcessError {
        ProcessError::Ids { path: self.path.clone(), source }
    }
}

/// A new session id from the system's random source.
fn new_id() -> io::Result<String> {
    let mut random = [0; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
    Ok(stream::session_id(random))
}
