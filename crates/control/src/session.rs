//! The session and status verbs: their requests, replies and client methods
//! (docs/service.md, "Sessions").
//!
//! [`crate::Request`] reads them as one variant, [`SessionAsk`], and does not
//! write them: a binary that never sends one (the app) carries no code for
//! them.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::client::TIMEOUT;
use crate::{Client, MAX_WAIT_MS, RemoteError, SlingAsk};

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
    /// `fresh` starts a new session instead, ending a running process.
    /// `profile` runs it under that profile of the agent's.
    SessionStart {
        workspace: String,
        #[serde(default, skip_serializing_if = "is_false")]
        fresh: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        profile: Option<String>,
    },
    /// End the process of a workspace's agent.
    SessionStop { workspace: String },
    /// Deliver one message to the running process of a workspace's agent.
    SessionSend { workspace: String, text: String },
    /// Send a canvas's new ink and a prompt to the agent of its workspace.
    SlingSend(SlingAsk),
    /// Take the next sling that waits for a listener of a workspace's
    /// agent. With `wait`, the reply waits up to that many milliseconds for
    /// one (at most [`crate::MAX_WAIT_MS`]).
    SessionListen {
        workspace: String,
        #[serde(default)]
        wait: u64,
    },
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
    /// How a sling reaches it: `process`, a process the daemon holds, or
    /// `listen`, a session that asks for its slings (`session_listen`).
    pub deliver: String,
    /// Under `listen`, whether a listener is there now. Absent under `process`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listening: Option<bool>,
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
    /// Under `listen`: the slings that wait, and the one the listener took
    /// and has not asked past.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<u32>,
    /// The text of the process's last answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    /// The conversation the process holds, which the next start resumes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// The profile the process runs under; absent: the agent's own limits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
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

/// The reply to [`SessionAsk::SessionListen`]: the agent listened for, and
/// the sling taken. No sling: none came within the wait.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listened {
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sling: Option<Taken>,
}

/// One sling as a listener takes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Taken {
    /// The number of its `sling_sent` event.
    pub event: u64,
    /// The canvas it was slung on.
    pub canvas: String,
    /// The directory that holds the packet's images, which `text` names.
    pub dir: PathBuf,
    /// The sling's message: the packet, the note and the prompt.
    pub text: String,
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
    /// session it last held. A running one is left as it is. A session that
    /// could not be resumed (its process exited with an error) is dropped,
    /// and the start after it begins a new one.
    ///
    /// # Errors
    ///
    /// `not_found` for a workspace that is not open or a program the daemon
    /// cannot find, `invalid` for a workspace with no agent, and
    /// `unavailable` when the daemon does not answer.
    pub fn session_start(&self, workspace: &str) -> Result<Session, RemoteError> {
        self.session_start_with(workspace, None, false)
    }

    /// Starts `workspace`'s agent in a new session: its last one is dropped,
    /// and a running process ends first.
    ///
    /// # Errors
    ///
    /// As [`Client::session_start`].
    pub fn session_start_fresh(&self, workspace: &str) -> Result<Session, RemoteError> {
        self.session_start_with(workspace, None, true)
    }

    /// Starts `workspace`'s agent under `profile`, or within its own limits
    /// with none. Each profile has a conversation of its own, which a start
    /// resumes. A process running under another profile ends first.
    ///
    /// # Errors
    ///
    /// `not_found` for a profile the agent does not have or a directory
    /// that is not there, and the errors of [`Client::session_start`].
    pub fn session_start_with(
        &self,
        workspace: &str,
        profile: Option<&str>,
        fresh: bool,
    ) -> Result<Session, RemoteError> {
        let profile = profile.map(str::to_string);
        let ask = SessionAsk::SessionStart { workspace: workspace.into(), fresh, profile };
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

    /// Takes the next sling that waits for a listener of `workspace`'s
    /// agent, waiting up to `wait` (at most [`MAX_WAIT_MS`]) for one. The
    /// agent counts as listened for from this call on: slings with no
    /// profile go to its listener, not to its process.
    ///
    /// # Errors
    ///
    /// `not_found` for a workspace that is not open, `invalid` for one with
    /// no agent, and `unavailable` when the daemon does not answer.
    pub fn session_listen(&self, workspace: &str, wait: Duration) -> Result<Listened, RemoteError> {
        let wait = u64::try_from(wait.as_millis()).unwrap_or(u64::MAX).min(MAX_WAIT_MS);
        let ask = SessionAsk::SessionListen { workspace: workspace.into(), wait };
        self.call_within(&ask, TIMEOUT + Duration::from_millis(wait))
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

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_false(b: &bool) -> bool {
    !b
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
                SessionAsk::SessionStart { workspace: "w".into(), fresh: false, profile: None },
                r#"{"v":1,"op":"session_start","workspace":"w"}"#,
            ),
            (
                SessionAsk::SessionStart { workspace: "w".into(), fresh: true, profile: None },
                r#"{"v":1,"op":"session_start","workspace":"w","fresh":true}"#,
            ),
            (
                SessionAsk::SessionStart {
                    workspace: "w".into(),
                    fresh: false,
                    profile: Some("dev".into()),
                },
                r#"{"v":1,"op":"session_start","workspace":"w","profile":"dev"}"#,
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
                    profile: Some("dev".into()),
                }),
                r#"{"v":1,"op":"sling_send","canvas":"c","page":"p1","select":["c4","t9"],"prompt":"why?","profile":"dev"}"#,
            ),
            (
                SessionAsk::SlingSend(SlingAsk { canvas: "c".into(), ..SlingAsk::default() }),
                r#"{"v":1,"op":"sling_send","canvas":"c","prompt":""}"#,
            ),
            (
                SessionAsk::SessionListen { workspace: "w".into(), wait: 250 },
                r#"{"v":1,"op":"session_listen","workspace":"w","wait":250}"#,
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
