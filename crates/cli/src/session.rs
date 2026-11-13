//! `slinger session ...`, `slinger status set` and `slinger file touched`:
//! the agents the daemon can reach and what each reports. One request per command; the daemon
//! owns the sessions and the record.

use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::Subcommand;
use serde_json::json;

use slinger_control as control;

use crate::env::Env;
use crate::output::Output;

/// Help for `--agent`: a string, because a doc comment may not hold `$SLINGER_AGENT` bare.
pub const AGENT_HELP: &str = "The calling agent [default: $SLINGER_AGENT]";

/// Help for `--agent` where the user acts when no agent is named.
pub const AGENT_OR_USER_HELP: &str =
    "Act as this agent, with its own undo history [default: $SLINGER_AGENT, else the user]";

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
        /// Start a new session instead of resuming the last one, ending a running process
        #[arg(long)]
        fresh: bool,
        /// Run under this profile of the agent's: its directory and rules, its own session
        #[arg(long, value_name = "NAME")]
        profile: Option<String>,
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
    /// Wait for the next sling to a workspace's agent and print it: a session of your own takes the agent's slings
    Listen {
        /// The open workspace: its name, or the directory of its file
        workspace: String,
        /// Give up after this long and print nothing [default: wait until a sling comes]
        #[arg(long, value_name = "SECONDS")]
        wait: Option<u64>,
    },
}

/// How long one ask of `session listen` is held open: the daemon knows
/// within this long that a listener is gone.
const ASK: Duration = Duration::from_secs(20);

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

#[derive(Subcommand)]
pub enum Touch {
    /// Record that the calling agent wrote a file: its path and hash, never its contents
    Touched {
        /// The file
        #[arg(required_unless_present = "hook", conflicts_with = "hook")]
        path: Option<PathBuf>,
        /// Read a Claude Code `PostToolUse` hook's payload on stdin and take the file from it
        #[arg(long)]
        hook: bool,
        #[arg(long, value_name = "NAME", help = AGENT_HELP)]
        agent: Option<String>,
    },
}

impl Touch {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Touched { .. } => "file touched",
        }
    }
}

impl Session {
    pub fn name(&self) -> &'static str {
        match self {
            Self::List => "session list",
            Self::Show { .. } => "session show",
            Self::Start { .. } => "session start",
            Self::Stop { .. } => "session stop",
            Self::Send { .. } => "session send",
            Self::Listen { .. } => "session listen",
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
        Session::Start { workspace, fresh, profile } => {
            Ok(quiet(&daemon.session_start_with(workspace, profile.as_deref(), *fresh)?))
        }
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
        Session::Listen { workspace, wait } => {
            let until = wait.map(|seconds| Instant::now() + Duration::from_secs(seconds));
            loop {
                let left = until.map(|u| u.saturating_duration_since(Instant::now()));
                let heard = daemon.session_listen(workspace, left.map_or(ASK, |l| l.min(ASK)))?;
                if heard.sling.is_some() || left.is_some_and(|l| l <= ASK) {
                    let text = heard.sling.as_ref().map(|s| s.text.clone()).unwrap_or_default();
                    return Ok(Output::new(serde_json::to_value(heard)?, text));
                }
            }
        }
    }
}

/// A verb that prints nothing on success; `--json` gets the session after it.
fn quiet(session: &control::Session) -> Output {
    Output::new(json!({ "session": session }), String::new())
}

pub fn set(command: &Status, daemon: &control::Client, env: &Env) -> anyhow::Result<Output> {
    let Status::Set { status, note, agent } = command;
    let agent = env.calling_agent(agent.as_deref())?;
    let set = daemon.status_set(&agent, status, note.as_deref())?;
    Ok(Output::new(serde_json::to_value(set)?, String::new()))
}

pub fn touched(
    command: &Touch,
    daemon: &control::Client,
    input: &mut dyn Read,
    env: &Env,
) -> anyhow::Result<Output> {
    let Touch::Touched { path, hook, agent } = command;
    let agent = env.calling_agent(agent.as_deref())?;
    let mut payload = String::new();
    let absolute;
    let what = if let Some(path) = path {
        // The daemon does not share this directory: it gets the whole path.
        absolute = std::path::absolute(path)?;
        control::Touched::Path(&absolute)
    } else {
        debug_assert!(hook, "clap requires one of the two");
        input.read_to_string(&mut payload)?;
        control::Touched::Hook(&payload)
    };
    let touched = daemon.file_touched(&agent, what)?;
    Ok(Output::new(serde_json::to_value(touched)?, String::new()))
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
    if let Some(listening) = s.listening {
        lines.push(format!("listening\t{listening}"));
    }
    if let Some(note) = &s.note {
        lines.push(format!("note\t{note}"));
    }
    let process = [
        ("process", s.process.clone()),
        ("pid", s.pid.map(|p| p.to_string())),
        ("profile", s.profile.clone()),
        ("pending", s.pending.map(|p| p.to_string())),
        ("session_id", s.session_id.clone()),
        // One line per field: an answer's own line breaks are escaped.
        ("answer", s.answer.as_ref().map(|a| a.replace('\n', "\\n"))),
    ];
    lines.extend(process.into_iter().filter_map(|(k, v)| Some(format!("{k}\t{}", v?))));
    lines.join("\n")
}
