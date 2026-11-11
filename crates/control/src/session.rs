//! The session and status verbs: their requests, replies and client methods
//! (docs/service.md, "Sessions").
//!
//! [`crate::Request`] reads them as one variant, [`SessionAsk`], and does not
//! write them: a binary that never sends one (the app) carries no code for
//! them.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{Client, RemoteError, SlingAsk};

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
    /// Start the process of a workspace's agent, or resume its session.
    SessionStart { workspace: String },
    /// End the process of a workspace's agent.
    SessionStop { workspace: String },
    /// Deliver one message to the running process of a workspace's agent.
    SessionSend { workspace: String, text: String },
    /// Send a canvas's new ink and a prompt to the agent of its workspace.
    SlingSend(SlingAsk),
    /// An agent wrote a file: the absolute `path`, or the one a Claude Code
    /// `PostToolUse` hook's payload (`hook`) names. One of the two.
    FileTouched {
        agent: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<PathBuf>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hook: Option<String>,
    },
}

/// What [`SessionAsk::FileTouched`] reports: a file by its path, or the
/// payload of the hook that saw it written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Touched<'a> {
    Path(&'a std::path::Path),
    Hook(&'a str),
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
    /// The agent's process: `running`, or `exited` when it ended without a
    /// `session_stop`. Absent when the daemon holds none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process: Option<String>,
    /// The process id of a running process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    /// Messages sent to the running process that it has not answered yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<u32>,
    /// The text of the process's last answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    /// The conversation the process holds, which the next start resumes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// The reply to [`SessionAsk::SessionList`]: the sessions, by agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sessions {
    pub sessions: Vec<Session>,
}

/// The reply to [`SessionAsk::SessionShow`], and to a start, stop or send:
/// the session after it.
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

/// The reply to [`SessionAsk::FileTouched`]: the file, its hash and the
/// number of the `file_touched` event. All absent when a hook's payload
/// names no file: nothing was touched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileTouched {
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
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

    /// Starts the process of `workspace`'s agent in its home, resuming the
    /// session it last held. A running one is left as it is.
    ///
    /// # Errors
    ///
    /// `not_found` for a workspace that is not open or a program the daemon
    /// cannot find, `invalid` for a workspace with no agent, and
    /// `unavailable` when the daemon does not answer.
    pub fn session_start(&self, workspace: &str) -> Result<Session, RemoteError> {
        let ask = SessionAsk::SessionStart { workspace: workspace.into() };
        Ok(self.call::<_, SessionInfo>(&ask)?.session)
    }

    /// Ends the process of `workspace`'s agent. Stopping one that is not
    /// running succeeds.
    ///
    /// # Errors
    ///
    /// As [`Client::session_start`].
    pub fn session_stop(&self, workspace: &str) -> Result<Session, RemoteError> {
        let ask = SessionAsk::SessionStop { workspace: workspace.into() };
        Ok(self.call::<_, SessionInfo>(&ask)?.session)
    }

    /// Delivers `text` to the running process of `workspace`'s agent as one
    /// message. It returns once the message is written; the answer is the
    /// agent's to deliver.
    ///
    /// # Errors
    ///
    /// `unavailable` when the process is not running (never started, or it
    /// exited) or the daemon does not answer, `invalid` for blank text, and
    /// the errors of [`Client::session_start`].
    pub fn session_send(&self, workspace: &str, text: &str) -> Result<Session, RemoteError> {
        let ask = SessionAsk::SessionSend { workspace: workspace.into(), text: text.into() };
        Ok(self.call::<_, SessionInfo>(&ask)?.session)
    }

    /// Reports that `agent` wrote a file. The daemon hashes it and appends a
    /// `file_touched` event with its path and hash, never its contents.
    ///
    /// # Errors
    ///
    /// `invalid` for a blank agent or a relative path, `not_found` for a
    /// file that is not there, and `unavailable` when the daemon does not
    /// answer.
    pub fn file_touched(&self, agent: &str, what: Touched<'_>) -> Result<FileTouched, RemoteError> {
        let (path, hook) = match what {
            Touched::Path(path) => (Some(path.to_owned()), None),
            Touched::Hook(payload) => (None, Some(payload.to_string())),
        };
        self.call(&SessionAsk::FileTouched { agent: agent.into(), path, hook })
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
            (
                SessionAsk::SessionStart { workspace: "w".into() },
                r#"{"v":1,"op":"session_start","workspace":"w"}"#,
            ),
            (
                SessionAsk::SessionStop { workspace: "w".into() },
                r#"{"v":1,"op":"session_stop","workspace":"w"}"#,
            ),
            (
                SessionAsk::SessionSend { workspace: "w".into(), text: "hi".into() },
                r#"{"v":1,"op":"session_send","workspace":"w","text":"hi"}"#,
            ),
            (
                SessionAsk::SlingSend(SlingAsk {
                    canvas: "c".into(),
                    page: Some("p1".into()),
                    select: vec!["c4".into(), "t9".into()],
                    prompt: "why?".into(),
                }),
                r#"{"v":1,"op":"sling_send","canvas":"c","page":"p1","select":["c4","t9"],"prompt":"why?"}"#,
            ),
            (
                SessionAsk::SlingSend(SlingAsk { canvas: "c".into(), ..SlingAsk::default() }),
                r#"{"v":1,"op":"sling_send","canvas":"c","prompt":""}"#,
            ),
            (
                SessionAsk::FileTouched { agent: "a".into(), path: Some("/f".into()), hook: None },
                r#"{"v":1,"op":"file_touched","agent":"a","path":"/f"}"#,
            ),
            (
                SessionAsk::FileTouched { agent: "a".into(), path: None, hook: Some("{}".into()) },
                r#"{"v":1,"op":"file_touched","agent":"a","hook":"{}"}"#,
            ),
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
            r#"{"v":1,"op":"session_send","workspace":"w"}"#,
            r#"{"v":1,"op":"sling_send","prompt":"p"}"#,
            r#"{"v":1,"op":"session_nope"}"#,
        ] {
            let err = crate::read_request(&mut line.as_bytes()).unwrap_err();
            assert_eq!(err.code(), "bad_request", "{line}");
        }
    }
}
