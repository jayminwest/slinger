//! The sessions (SPEC section 5): the agents of the open workspaces and the
//! status each one last set.
//!
//! The core decides what a session is and what a status writes
//! ([`slinger_core::session`]). This module keeps the statuses set since the
//! daemon started, so `working` (which the record does not hold) shows; an
//! agent with none here gets its last `agent_blocked` or `agent_done` from
//! the record, so `blocked` and `done` survive a restart. It reaches each
//! agent through the process [`Processes`] holds for it.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use slinger_core::record::{BlobId, Body, Event, FileTouched, Tags};
use slinger_core::session::{self, Report, Session, Status, hooks, stream};

use crate::processes::{ProcessError, Processes};
use crate::store::{Query, Store, StoreError};
use crate::workspaces::WorkspaceError;

/// Why a session request failed.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    Core(#[from] slinger_core::Error),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Process(#[from] ProcessError),
}

impl SessionError {
    /// Stable machine-readable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Core(e) => e.code(),
            Self::Workspace(e) => e.code(),
            Self::Store(e) => e.code(),
            Self::Process(e) => e.code(),
        }
    }

    /// What the caller can do about it, if anything.
    #[must_use]
    pub fn hint(&self) -> Option<String> {
        match self {
            Self::Core(e) => e.hint().map(str::to_string),
            Self::Workspace(e) => e.hint(),
            Self::Store(e) => e.hint(),
            Self::Process(e) => e.hint(),
        }
    }
}

type Result<T> = std::result::Result<T, SessionError>;

/// The statuses set since the daemon started, by agent, and the agents'
/// processes. Safe to share between threads.
#[derive(Debug)]
pub struct Sessions {
    set: Mutex<HashMap<String, Report>>,
    processes: Processes,
    slinging: Mutex<()>,
}

impl Sessions {
    /// No statuses set and no processes held. Each process runs `program`
    /// (Claude Code); `ids` is the file that keeps the session ids.
    #[must_use]
    pub fn new(program: PathBuf, ids: PathBuf) -> Self {
        let processes = Processes::new(program, ids);
        Self { set: Mutex::default(), processes, slinging: Mutex::default() }
    }

    /// The agents' processes.
    #[must_use]
    pub fn processes(&self) -> &Processes {
        &self.processes
    }

    /// The agents' processes, to configure before the daemon serves.
    pub fn processes_mut(&mut self) -> &mut Processes {
        &mut self.processes
    }

    /// Starts the process of the agent of the open workspace `which` names,
    /// in the agent's home and within the workspace's limits on this machine,
    /// and returns its session. A running one is left as it is, unless
    /// `fresh`: then the agent starts a new session, its last one dropped.
    ///
    /// # Errors
    ///
    /// `not_found` for a workspace that is not open, `invalid` for one with
    /// no agent, and the process's error.
    pub fn start(&self, store: &Store, which: &str, fresh: bool) -> Result<Session> {
        let opened = store.workspaces().show(which)?;
        let session = self.show(store, session::agent_of(&opened.workspace)?)?;
        let limits = stream::Limits::of(&opened.workspace, store.machine());
        self.processes.start(&session.agent, &session.home, &limits, fresh)?;
        Ok(session)
    }

    /// Ends the process of the agent of the workspace `which` names, if it
    /// has one, and returns its session.
    ///
    /// # Errors
    ///
    /// As [`Sessions::start`], less the process's.
    pub fn stop(&self, store: &Store, which: &str) -> Result<Session> {
        let session = self.of_workspace(store, which)?;
        self.processes.stop(&session.agent);
        Ok(session)
    }

    /// Delivers `text` as one message to the running process of the agent
    /// of the workspace `which` names, and returns its session.
    ///
    /// # Errors
    ///
    /// `invalid` for blank text, `unavailable` when the process is not
    /// running, and the errors of [`Sessions::stop`].
    pub fn send(&self, store: &Store, which: &str, text: &str) -> Result<Session> {
        self.deliver(store, which, &stream::message(text)?)
    }

    /// Delivers `line`, one message as the stream format writes it, as
    /// [`Sessions::send`] does.
    ///
    /// # Errors
    ///
    /// As [`Sessions::send`], less blank text.
    pub fn deliver(&self, store: &Store, which: &str, line: &str) -> Result<Session> {
        let session = self.of_workspace(store, which)?;
        self.processes.send(&session.agent, line)?;
        Ok(session)
    }

    /// The name of the open workspace whose canvas is `canvas`, and its
    /// agent: where a sling on that canvas goes.
    ///
    /// # Errors
    ///
    /// `not_found` when no open workspace has that canvas, `invalid` when
    /// it has no agent, and the error of a workspace file.
    pub fn on_canvas(&self, store: &Store, canvas: &str) -> Result<(String, String)> {
        let open = store.workspaces().list()?;
        let ws = session::on_canvas(open.iter().map(|o| &o.workspace), canvas)?;
        Ok((ws.name.clone(), session::agent_of(ws)?.to_string()))
    }

    /// Held by a sling from its marker to its `sling_sent`, and by a put
    /// while it looks for the sling it answers: slings on a canvas go one
    /// after another, so their packets never overlap, and a reply that comes
    /// back fast still finds the sling it answers.
    pub fn slinging(&self) -> MutexGuard<'_, ()> {
        self.slinging.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The session of the agent of the open workspace `which` names.
    fn of_workspace(&self, store: &Store, which: &str) -> Result<Session> {
        let opened = store.workspaces().show(which)?;
        let agent = session::agent_of(&opened.workspace)?;
        self.show(store, agent)
    }

    /// The sessions of the open workspaces, by agent.
    ///
    /// # Errors
    ///
    /// The error of a workspace file or of the record.
    pub fn list(&self, store: &Store) -> Result<Vec<Session>> {
        let open = store.workspaces().list()?;
        let set = self.set.lock().unwrap_or_else(PoisonError::into_inner);
        let mut failed = None;
        let sessions = session::list(open.iter().map(|o| (&o.dir, &o.workspace)), |agent| {
            set.get(agent).cloned().or_else(|| {
                last_report(store, agent).unwrap_or_else(|e| {
                    failed.get_or_insert(e);
                    None
                })
            })
        });
        failed.map_or(Ok(sessions), |e| Err(e.into()))
    }

    /// The session of `agent`.
    ///
    /// # Errors
    ///
    /// `not_found` when no open workspace has that agent, and the errors of
    /// [`Sessions::list`].
    pub fn show(&self, store: &Store, agent: &str) -> Result<Session> {
        self.list(store)?.into_iter().find(|s| s.agent == agent).ok_or_else(|| {
            let what = format!("session of agent {agent:?}");
            let hint = Some("see 'slinger session list': an agent joins with its workspace".into());
            slinger_core::Error::NotFound { what, hint }.into()
        })
    }

    /// Sets `agent`'s status. `blocked` and `done` append their event, tagged
    /// with the agent (and so its workspace); it is returned. An agent no
    /// open workspace names may set one too: the record keeps it, and its
    /// session shows it once its workspace opens.
    ///
    /// # Errors
    ///
    /// `invalid` for a blank agent or an unknown status, and the record's error.
    pub fn set(
        &self,
        store: &Store,
        agent: &str,
        status: &str,
        note: Option<String>,
    ) -> Result<(Report, Option<Event>)> {
        session::check_agent(agent)?;
        let status = Status::parse(status)?;
        let mut set = self.set.lock().unwrap_or_else(PoisonError::into_inner);
        let event = match status.body(note.as_deref()) {
            Some(body) => {
                let tags = Tags { agent: Some(agent.into()), ..Tags::on("") };
                Some(store.append(Event::new(0, agent, tags, body))?)
            }
            None => None,
        };
        let report = Report { status, note };
        set.insert(agent.to_string(), report.clone());
        Ok((report, event))
    }

    /// Records that `agent` wrote the file at `path`, or at the path a
    /// Claude Code hook's payload (`hook`) names: appends `file_touched` with
    /// the path and the hash of the file's bytes, never the bytes, tagged as
    /// [`Sessions::set`] tags. A payload that names no file touches none:
    /// `None`.
    ///
    /// # Errors
    ///
    /// `invalid` for a blank agent, a relative path, or both or neither of
    /// `path` and `hook`; `not_found` for a file that is not there; and the
    /// record's error.
    pub fn touched(
        &self,
        store: &Store,
        agent: &str,
        path: Option<PathBuf>,
        hook: Option<&str>,
    ) -> Result<Option<Event>> {
        session::check_agent(agent)?;
        let path = match (path, hook) {
            (Some(path), None) => path,
            (None, Some(payload)) => match hooks::touched(payload) {
                Some(path) => path,
                None => return Ok(None),
            },
            _ => return Err(invalid("pass a file's path or a hook's payload, one of the two")),
        };
        if path.is_relative() {
            return Err(invalid(&format!("{} is not an absolute path", path.display())));
        }
        let hash = BlobId::of(&read(&path)?);
        let body = Body::FileTouched(FileTouched { path: path.display().to_string(), hash });
        let tags = Tags { agent: Some(agent.into()), ..Tags::on("") };
        Ok(Some(store.append(Event::new(0, agent, tags, body))?))
    }
}

/// The bytes of the file at `path`, to hash.
fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| {
        let what = format!("file {}", path.display());
        let hint = match e.kind() {
            io::ErrorKind::NotFound => "name a file that exists".to_string(),
            _ => format!("it cannot be read: {e}"),
        };
        slinger_core::Error::NotFound { what, hint: Some(hint) }.into()
    })
}

fn invalid(message: &str) -> SessionError {
    let hint = Some("see 'slinger file touched --help'".into());
    slinger_core::Error::Invalid { message: message.into(), hint }.into()
}

/// The last status `agent` set in the record, if any.
fn last_report(store: &Store, agent: &str) -> std::result::Result<Option<Report>, StoreError> {
    let events = store.events(&Query { agent: Some(agent.into()), ..Query::default() })?;
    Ok(events.iter().rev().find_map(|e| Report::of(&e.body)))
}
