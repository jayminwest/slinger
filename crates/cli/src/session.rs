//! `slinger session ...` and `slinger status set`: the agents the daemon can
//! reach and the status each reports. One request per command; the daemon
//! owns the sessions and the record.

use std::io::Read;

use clap::Subcommand;
use serde_json::json;

use slinger_control as control;

use crate::output::Output;

/// Where an agent verb finds the calling agent's name when `--agent` is not
/// given (docs/cli.md).
pub const AGENT_ENV: &str = "SLINGER_AGENT";

/// Help for `--agent`: a string, because a doc comment may not hold `$SLINGER_AGENT` bare.
pub const AGENT_HELP: &str = "The calling agent [default: $SLINGER_AGENT]";

#[derive(Subcommand)]
pub enum Session {
    /// List the sessions, by agent: agent, status, home and note per line
    List,
    /// Print one agent's session: a field and its value per line
    Show {
        /// The agent's name
        agent: String,
    },
    /// Start a workspace's agent: a Claude Code process in its home, resuming its last session
    Start {
        /// The open workspace: its name, or the directory of its file
        workspace: String,
    },
    /// Stop a workspace's agent: end its process
    Stop {
        /// The open workspace: its name, or the directory of its file
        workspace: String,
    },
    /// Send one message to a workspace's running agent
    Send {
        /// The open workspace: its name, or the directory of its file
        workspace: String,
        /// The message, or - to read standard input
        text: String,
    },
}

#[derive(Subcommand)]
pub enum Status {
    /// Set the calling agent's status; blocked and done go into the record
    Set {
        /// What the agent is doing
        #[arg(value_parser = ["working", "blocked", "done"])]
        status: String,
        /// What the agent says with it: what it needs, or what it did
        #[arg(long, value_name = "TEXT")]
        note: Option<String>,
        #[arg(long, value_name = "NAME", help = AGENT_HELP)]
        agent: Option<String>,
    },
}

impl Session {
    pub fn name(&self) -> &'static str {
        match self {
            Self::List => "session list",
            Self::Show { .. } => "session show",
            Self::Start { .. } => "session start",
            Self::Stop { .. } => "session stop",
            Self::Send { .. } => "session send",
        }
    }
}

impl Status {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Set { .. } => "status set",
        }
    }
}

pub fn execute(
    command: &Session,
    daemon: &control::Client,
    input: &mut dyn Read,
) -> anyhow::Result<Output> {
    match command {
        Session::List => {
            let sessions = daemon.session_list()?;
            let human = sessions.iter().map(line).collect::<Vec<_>>().join("\n");
            Ok(Output::new(json!({ "sessions": sessions }), human))
        }
        Session::Show { agent } => {
            let session = daemon.session_show(agent)?;
            let human = fields(&session);
            Ok(Output::new(json!({ "session": session }), human))
        }
        Session::Start { workspace } => Ok(quiet(&daemon.session_start(workspace)?)),
        Session::Stop { workspace } => Ok(quiet(&daemon.session_stop(workspace)?)),
        Session::Send { workspace, text } => {
            let mut read = String::new();
            let text = if text == "-" {
                input.read_to_string(&mut read)?;
                &read
            } else {
                text
            };
            Ok(quiet(&daemon.session_send(workspace, text)?))
        }
    }
}

/// A verb that prints nothing on success; `--json` gets the session after it.
fn quiet(session: &control::Session) -> Output {
    Output::new(json!({ "session": session }), String::new())
}

pub fn set(command: &Status, daemon: &control::Client) -> anyhow::Result<Output> {
    let Status::Set { status, note, agent } = command;
    let agent = calling_agent(agent.as_deref())?;
    let set = daemon.status_set(&agent, status, note.as_deref())?;
    Ok(Output::new(serde_json::to_value(set)?, String::new()))
}

/// The agent an agent verb acts as: `--agent`, else `$SLINGER_AGENT`.
pub fn calling_agent(flag: Option<&str>) -> Result<String, control::RemoteError> {
    let agent = flag.map(str::to_string).or_else(|| std::env::var(AGENT_ENV).ok());
    agent.filter(|a| !a.trim().is_empty()).ok_or_else(|| {
        let hint = format!("pass --agent <NAME>, or set {AGENT_ENV}");
        control::RemoteError::new("usage", "no agent name", Some(hint))
    })
}

/// One session for `list`: agent, status (`-` for none) and home, by tabs,
/// then the note when there is one.
fn line(s: &control::Session) -> String {
    let status = s.status.as_deref().unwrap_or("-");
    let mut line = format!("{}\t{status}\t{}", s.agent, s.home.display());
    if let Some(note) = &s.note {
        line.push('\t');
        line.push_str(note);
    }
    line
}

/// One session for `show`: a field and its value per line, by a tab.
fn fields(s: &control::Session) -> String {
    let mut lines = vec![
        format!("agent\t{}", s.agent),
        format!("home\t{}", s.home.display()),
        format!("deliver\t{}", s.deliver),
        format!("status\t{}", s.status.as_deref().unwrap_or("-")),
    ];
    if let Some(note) = &s.note {
        lines.push(format!("note\t{note}"));
    }
    let process = [
        ("process", s.process.clone()),
        ("pid", s.pid.map(|p| p.to_string())),
        ("pending", s.pending.map(|p| p.to_string())),
        ("session_id", s.session_id.clone()),
        // One line per field: an answer's own line breaks are escaped.
        ("answer", s.answer.as_ref().map(|a| a.replace('\n', "\\n"))),
    ];
    lines.extend(process.into_iter().filter_map(|(k, v)| Some(format!("{k}\t{}", v?))));
    lines.join("\n")
}
