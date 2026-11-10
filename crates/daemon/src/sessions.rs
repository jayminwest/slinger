//! The sessions (SPEC section 5): the agents of the open workspaces and the
//! status each one last set.
//!
//! The core decides what a session is and what a status writes
//! ([`slinger_core::session`]). This module keeps the statuses set since the
//! daemon started, so `working` (which the record does not hold) shows; an
//! agent with none here gets its last `agent_blocked` or `agent_done` from
//! the record, so `blocked` and `done` survive a restart.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use slinger_core::record::{Event, Tags};
use slinger_core::session::{self, Report, Session, Status};

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
}

impl SessionError {
    /// Stable machine-readable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Core(e) => e.code(),
            Self::Workspace(e) => e.code(),
            Self::Store(e) => e.code(),
        }
    }

    /// What the caller can do about it, if anything.
    #[must_use]
    pub fn hint(&self) -> Option<String> {
        match self {
            Self::Core(e) => e.hint().map(str::to_string),
            Self::Workspace(e) => e.hint(),
            Self::Store(e) => e.hint(),
        }
    }
}

type Result<T> = std::result::Result<T, SessionError>;

/// The statuses set since the daemon started, by agent. Safe to share
/// between threads.
#[derive(Debug, Default)]
pub struct Sessions {
    set: Mutex<HashMap<String, Report>>,
}

impl Sessions {
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
}

/// The last status `agent` set in the record, if any.
fn last_report(store: &Store, agent: &str) -> std::result::Result<Option<Report>, StoreError> {
    let events = store.events(&Query { agent: Some(agent.into()), ..Query::default() })?;
    Ok(events.iter().rev().find_map(|e| Report::of(&e.body)))
}
