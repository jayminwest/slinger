//! An agent as the sidebar shows it: its status, read against its
//! process ([`Status::of`]), what it said, and its profiles.

/// An agent's status as the window shows it: what it last set with
/// `slinger status set`, read against its process ([`Status::of`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Working,
    Blocked,
    Done,
    /// Its process is not running: a sling cannot reach it.
    Stopped,
}

/// An agent's process as slingerd holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Process {
    Running,
    /// It ended, or was stopped: slingerd started it before.
    Exited,
    /// slingerd never started it: the agent may run outside it, and its
    /// hooks say how it is.
    Absent,
}

impl Process {
    /// The process the daemon calls `name` (`running` or `exited`; `None`:
    /// it holds none) for an agent it `started` before (it holds a session
    /// id for it).
    pub fn named(name: Option<&str>, started: bool) -> Process {
        match name {
            Some("running") => Process::Running,
            None if !started => Process::Absent,
            _ => Process::Exited,
        }
    }
}

impl Status {
    /// The status to show for an agent that `said` it is at a status, whose
    /// `process` has `pending` messages it has not answered, and which has
    /// `answered` one: the daemon's own count wins over what the agent's
    /// hooks last said, which may be late or never come. A process that
    /// exited is [`Status::Stopped`]; for an agent slingerd never started,
    /// what its hooks said shows (stopped when they said nothing). A running one with a
    /// message pending is working, or blocked when it says so; with none
    /// it is done once it said anything or answered, and has no status
    /// before.
    pub fn of(
        said: Option<Status>,
        process: Process,
        pending: u32,
        answered: bool,
    ) -> Option<Status> {
        match process {
            Process::Running => {}
            Process::Exited => return Some(Status::Stopped),
            Process::Absent => return said.or(Some(Status::Stopped)),
        }
        if pending > 0 {
            let blocked = said == Some(Status::Blocked);
            return Some(if blocked { Status::Blocked } else { Status::Working });
        }
        (said.is_some() || answered).then_some(Status::Done)
    }

    /// The status the daemon calls `name`; `None` for any other word.
    pub fn parse(name: &str) -> Option<Status> {
        match name {
            "working" => Some(Status::Working),
            "blocked" => Some(Status::Blocked),
            "done" => Some(Status::Done),
            _ => None,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Status::Working => "working",
            Status::Blocked => "blocked",
            Status::Done => "done",
            Status::Stopped => "not running",
        }
    }
}

/// The agent of a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Agent {
    pub name: String,
    /// `None` until the agent sets one.
    pub status: Option<Status>,
    /// What the agent said with its status.
    pub note: Option<String>,
    /// The profile its running process runs under; `None` while it runs
    /// within its own limits or does not run.
    pub profile: Option<String>,
    /// The names of its profiles, in its workspace file's order.
    pub profiles: Vec<String>,
}

impl Agent {
    /// How the window names it: `scout`, or `scout (dev)` while it runs
    /// under the profile `dev`.
    pub fn label(&self) -> String {
        match &self.profile {
            Some(profile) => format!("{} ({profile})", self.name),
            None => self.name.clone(),
        }
    }
}
