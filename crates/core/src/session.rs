//! Sessions (SPEC section 5, section 7 `status set`): the agents the server
//! can reach, how a message gets to each, and the status each one reports.
//!
//! A session is the agent of an open workspace. Its status is what the agent
//! last set: `blocked` and `done` are in the record (`agent_blocked`,
//! `agent_done`), so they are rebuilt from it; `working` has no event in v1
//! and lives only as long as the server that was told. No IO: callers hand in
//! the workspaces and the events.

pub mod hooks;
pub mod stream;

use std::path::PathBuf;

use crate::record::{AgentBlocked, AgentDone, Body};
use crate::workspace::Workspace;
use crate::{Error, Result};

/// What an agent says it is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Working,
    /// It needs something from the user.
    Blocked,
    /// It finished its task.
    Done,
}

impl Status {
    /// Every status, in the order a person reads them.
    pub const ALL: [Self; 3] = [Self::Working, Self::Blocked, Self::Done];

    /// Its name on the wire and in the CLI.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Done => "done",
        }
    }

    /// The status `name` names.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] for any other name.
    pub fn parse(name: &str) -> Result<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name).ok_or_else(|| Error::Invalid {
            message: format!("{name:?} is not a status"),
            hint: Some("pass working, blocked or done".into()),
        })
    }

    /// The event setting this status appends to the record: `agent_blocked`
    /// with the note as its reason, `agent_done` with it as its summary.
    /// `None` for `working`, which the record does not hold.
    #[must_use]
    pub fn body(self, note: Option<&str>) -> Option<Body> {
        match self {
            Self::Working => None,
            Self::Blocked => {
                Some(Body::AgentBlocked(AgentBlocked { reason: note.unwrap_or_default().into() }))
            }
            Self::Done => Some(Body::AgentDone(AgentDone { summary: note.map(str::to_string) })),
        }
    }
}

/// A status as an agent set it, with its note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub status: Status,
    /// What the agent said with it. `None` when it said nothing.
    pub note: Option<String>,
}

impl Report {
    /// The status an event reports: `agent_blocked` and `agent_done` do, no
    /// other kind does. The inverse of [`Status::body`].
    #[must_use]
    pub fn of(body: &Body) -> Option<Self> {
        let (status, note) = match body {
            Body::AgentBlocked(b) => (Status::Blocked, Some(b.reason.clone())),
            Body::AgentDone(d) => (Status::Done, d.summary.clone()),
            _ => return None,
        };
        Some(Self { status, note: note.filter(|n| !n.is_empty()) })
    }
}

/// How a message reaches a session. One kind in v1: a process the server
/// holds (SPEC D10). No trait until a second kind arrives (B7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Process,
}

impl Delivery {
    /// Its name on the wire.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Process => "process",
        }
    }
}

/// One agent the server can reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// The agent's name, as its workspace's file gives it.
    pub agent: String,
    /// The agent's home: the directory of its workspace's file (the primary
    /// root, or the server's own directory for a workspace with no root).
    pub home: PathBuf,
    pub deliver: Delivery,
    /// What the agent last set. `None` until it sets one.
    pub status: Option<Report>,
}

/// The sessions of `workspaces` (each with the directory of its file), sorted
/// by agent: one per agent name. An agent named by several workspaces is one
/// session, at home in the first of them. `status` gives an agent's last
/// report.
#[must_use]
pub fn list<'a>(
    workspaces: impl IntoIterator<Item = (&'a PathBuf, &'a Workspace)>,
    mut status: impl FnMut(&str) -> Option<Report>,
) -> Vec<Session> {
    let mut sessions: Vec<Session> = Vec::new();
    for (dir, ws) in workspaces {
        let Some(agent) = &ws.agent else { continue };
        if sessions.iter().any(|s| s.agent == agent.name) {
            continue;
        }
        sessions.push(Session {
            agent: agent.name.clone(),
            home: dir.clone(),
            deliver: Delivery::Process,
            status: status(&agent.name),
        });
    }
    sessions.sort_by(|a, b| a.agent.cmp(&b.agent));
    sessions
}

/// Checks the name an agent gives for itself.
///
/// # Errors
///
/// [`Error::Invalid`] for a blank name.
pub fn check_agent(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::Invalid {
            message: "the agent's name is empty".into(),
            hint: Some("pass --agent <NAME>, or set SLINGER_AGENT".into()),
        });
    }
    Ok(())
}

/// The name of `workspace`'s agent: the session a start, stop or send names
/// by its workspace.
///
/// # Errors
///
/// [`Error::Invalid`] for a workspace with no agent.
pub fn agent_of(workspace: &Workspace) -> Result<&str> {
    workspace.agent.as_ref().map(|a| a.name.as_str()).ok_or_else(|| Error::Invalid {
        message: format!("workspace {:?} has no agent", workspace.name),
        hint: Some("name one under [agent] in its slinger-workspace.toml".into()),
    })
}

/// The open workspace whose canvas is `canvas`: the one a sling on that
/// canvas goes to. A canvas has at most one (docs/workspace.md).
///
/// # Errors
///
/// [`Error::NotFound`] when no open workspace has that canvas.
pub fn on_canvas<'a>(
    workspaces: impl IntoIterator<Item = &'a Workspace>,
    canvas: &str,
) -> Result<&'a Workspace> {
    workspaces.into_iter().find(|ws| ws.canvas == canvas).ok_or_else(|| Error::NotFound {
        what: format!("an open workspace with canvas {canvas:?}"),
        hint: Some(format!(
            "a sling goes to the agent of the canvas's workspace: open one with canvas = \"{canvas}\""
        )),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Agent;

    fn with_agent(name: &str, agent: Option<&str>) -> Workspace {
        let mut ws = Workspace::named(name);
        ws.agent = agent.map(|a| Agent::new(a, false));
        ws
    }

    #[test]
    fn a_status_parses_from_its_name_and_nothing_else() {
        for status in Status::ALL {
            assert_eq!(Status::parse(status.name()).unwrap(), status);
        }
        let err = Status::parse("idle").unwrap_err();
        assert_eq!((err.code(), err.hint()), ("invalid", Some("pass working, blocked or done")));
    }

    #[test]
    fn blocked_and_done_round_trip_through_their_events_and_working_has_none() {
        assert_eq!(Status::Working.body(Some("x")), None);
        for (status, note) in [
            (Status::Blocked, Some("need a key")),
            (Status::Blocked, None),
            (Status::Done, Some("wrote it")),
            (Status::Done, None),
        ] {
            let body = status.body(note).unwrap();
            let report = Report::of(&body).unwrap();
            assert_eq!(report, Report { status, note: note.map(str::to_string) });
        }
        let blocked = Status::Blocked.body(None).unwrap();
        assert_eq!(blocked.kind(), "agent_blocked");
        assert_eq!(Status::Done.body(None).unwrap().kind(), "agent_done");
        let other = Body::new(
            "pane_moved",
            serde_json::json!({"pane":"p","x":0,"y":0,"width":1,"height":1}),
        );
        assert_eq!(Report::of(&other.unwrap()), None);
    }

    #[test]
    fn one_session_per_agent_at_home_in_its_first_workspace() {
        let dirs: Vec<PathBuf> = ["/a", "/b", "/c", "/d"].map(PathBuf::from).into();
        let wss = [
            with_agent("a", Some("scout")),
            with_agent("b", None),
            with_agent("c", Some("ada")),
            with_agent("d", Some("scout")),
        ];
        let done = Report { status: Status::Done, note: None };
        let sessions = list(dirs.iter().zip(&wss), |agent| (agent == "ada").then(|| done.clone()));
        let got: Vec<_> =
            sessions.iter().map(|s| (s.agent.as_str(), s.home.to_str().unwrap())).collect();
        assert_eq!(got, [("ada", "/c"), ("scout", "/a")]);
        assert_eq!(sessions[0].status, Some(done));
        assert_eq!((sessions[1].status.as_ref(), sessions[1].deliver.name()), (None, "process"));
    }

    #[test]
    fn a_workspace_names_its_agent_and_one_with_none_is_invalid() {
        assert_eq!(agent_of(&with_agent("a", Some("scout"))).unwrap(), "scout");
        let err = agent_of(&with_agent("a", None)).unwrap_err();
        assert_eq!(err.code(), "invalid");
        assert!(err.hint().unwrap().contains("[agent]"));
    }

    #[test]
    fn a_canvas_leads_to_its_workspace_and_one_with_none_is_not_found() {
        let wss = [with_agent("a", Some("scout")), with_agent("b", None)];
        assert_eq!(on_canvas(&wss, &wss[1].canvas).unwrap().name, "b");
        let err = on_canvas(&wss, "nowhere").unwrap_err();
        assert_eq!(err.code(), "not_found");
        assert!(err.hint().unwrap().contains("canvas = \"nowhere\""));
    }

    #[test]
    fn a_blank_agent_name_is_invalid_with_a_hint() {
        assert!(check_agent("scout").is_ok());
        let err = check_agent(" ").unwrap_err();
        assert_eq!(err.code(), "invalid");
        assert!(err.hint().unwrap().contains("SLINGER_AGENT"));
    }
}
