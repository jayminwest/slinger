//! What the CLI takes from its process's environment: the calling agent and
//! the daemon's socket, which slingerd names to the agents' processes it
//! starts (docs/cli.md). `main` reads them once; the rest of the CLI asks
//! [`Env`], so a run in-process (the tests) sees only the environment it is
//! given, never the one its caller happens to have (slinger-09c5b5).

use std::path::PathBuf;

use slinger_control as control;

/// Where an agent verb finds the calling agent's name when `--agent` is not given.
pub const AGENT_ENV: &str = "SLINGER_AGENT";

/// Where the CLI finds the socket when `--socket` is not given.
pub const SOCKET_ENV: &str = "SLINGER_SOCKET";

/// The CLI's environment. The default is an empty one: no agent, the default socket.
#[derive(Debug, Clone, Default)]
pub struct Env {
    /// `$SLINGER_AGENT`, when set and not blank.
    pub agent: Option<String>,
    /// `$SLINGER_SOCKET`, when set.
    pub socket: Option<PathBuf>,
}

impl Env {
    /// The process's own environment.
    pub fn of_process() -> Self {
        Self {
            agent: std::env::var(AGENT_ENV).ok(),
            socket: std::env::var_os(SOCKET_ENV).map(PathBuf::from),
        }
    }

    /// The agent a command acts as, if any: `--agent`, else `$SLINGER_AGENT`.
    pub fn named_agent(&self, flag: Option<&str>) -> Option<String> {
        let agent = flag.or(self.agent.as_deref()).map(str::to_string);
        agent.filter(|a| !a.trim().is_empty())
    }

    /// The agent an agent verb acts as: `--agent`, else `$SLINGER_AGENT`.
    pub fn calling_agent(&self, flag: Option<&str>) -> Result<String, control::RemoteError> {
        self.named_agent(flag).ok_or_else(|| {
            let hint = format!("pass --agent <NAME>, or set {AGENT_ENV}");
            control::RemoteError::new("usage", "no agent name", Some(hint))
        })
    }
}
