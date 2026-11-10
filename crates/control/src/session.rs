//! The session and status verbs: their requests, replies and client methods
//! (docs/service.md, "Sessions").
//!
//! [`crate::Request`] reads them as one variant, [`SessionAsk`], and does not
//! write them: a binary that never sends one (the app) carries no code for
//! them.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{Client, RemoteError};

/// A session request, as a client writes it and the daemon reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SessionAsk {
    /// The sessions: the agents of the open workspaces.
    SessionList,
    /// The session of one agent.
    SessionShow { agent: String },
    /// Set an agent's status (`working`, `blocked` or `done`), with a note.
    StatusSet {
        agent: String,
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
}

/// One agent the daemon can reach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub agent: String,
    /// The agent's home: the directory of its workspace's file.
    pub home: PathBuf,
    /// How a message reaches it: `process` in v1.
    pub deliver: String,
    /// `working`, `blocked` or `done`; absent until the agent sets one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// What the agent said with its status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The reply to [`SessionAsk::SessionList`]: the sessions, by agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sessions {
    pub sessions: Vec<Session>,
}

/// The reply to [`SessionAsk::SessionShow`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session: Session,
}

/// The reply to [`SessionAsk::StatusSet`]: the status as set, and the
/// number of the event it appended (none for `working`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusSet {
    pub agent: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<u64>,
}

impl Client {
    /// The sessions: one per agent of an open workspace, by agent.
    ///
    /// # Errors
    ///
    /// The error of a workspace file the daemon cannot read, and
    /// `unavailable` when the daemon does not answer.
    pub fn session_list(&self) -> Result<Vec<Session>, RemoteError> {
        let reply: Sessions = self.call(&SessionAsk::SessionList)?;
        Ok(reply.sessions)
    }

    /// The session of `agent`.
    ///
    /// # Errors
    ///
    /// `not_found` when no open workspace has that agent, and `unavailable`
    /// when the daemon does not answer.
    pub fn session_show(&self, agent: &str) -> Result<Session, RemoteError> {
        let reply: SessionInfo = self.call(&SessionAsk::SessionShow { agent: agent.into() })?;
        Ok(reply.session)
    }

    /// Sets `agent`'s status, with a note. `blocked` and `done` append an
    /// event to the record.
    ///
    /// # Errors
    ///
    /// `invalid` for a blank agent or a status that is not one, and
    /// `unavailable` when the daemon does not answer.
    pub fn status_set(
        &self,
        agent: &str,
        status: &str,
        note: Option<&str>,
    ) -> Result<StatusSet, RemoteError> {
        let (agent, status, note) = (agent.into(), status.into(), note.map(str::to_string));
        self.call(&SessionAsk::StatusSet { agent, status, note })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;
    use crate::wire::request_line;

    #[test]
    fn a_session_request_reads_back_as_itself_by_its_own_op() {
        let set = |note: Option<&str>| SessionAsk::StatusSet {
            agent: "a".into(),
            status: "done".into(),
            note: note.map(str::to_string),
        };
        for (ask, line) in [
            (SessionAsk::SessionList, r#"{"v":1,"op":"session_list"}"#),
            (
                SessionAsk::SessionShow { agent: "a".into() },
                r#"{"v":1,"op":"session_show","agent":"a"}"#,
            ),
            (set(None), r#"{"v":1,"op":"status_set","agent":"a","status":"done"}"#),
            (set(Some("n")), r#"{"v":1,"op":"status_set","agent":"a","status":"done","note":"n"}"#),
        ] {
            assert_eq!(request_line(&ask).unwrap(), format!("{line}\n"));
            let read = crate::read_request(&mut line.as_bytes()).unwrap();
            assert_eq!(read, Some(Request::Session(ask)), "{line}");
        }
    }

    #[test]
    fn a_session_request_missing_a_field_or_an_unknown_op_is_a_bad_request() {
        for line in [
            r#"{"v":1,"op":"session_show"}"#,
            r#"{"v":1,"op":"status_set","agent":"a"}"#,
            r#"{"v":1,"op":"session_nope"}"#,
        ] {
            let err = crate::read_request(&mut line.as_bytes()).unwrap_err();
            assert_eq!(err.code(), "bad_request", "{line}");
        }
    }
}
