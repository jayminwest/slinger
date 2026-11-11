//! Claude Code's stream format (SPEC D10, Spike C): how a session's process is
//! started, what a message to it looks like, and what its output says.
//!
//! The process is `claude --print` with stream input and stream output: one
//! JSON object per line each way. It stays open between messages, so each
//! message lands in the same conversation. A session id names that
//! conversation; Slinger picks it for a fresh start and passes it back to
//! resume. No IO: the server spawns the process and moves the lines.
//!
//! A session has stated limits ([`Limits`]): what it may read, which tools
//! it may use and which configuration it loads. Anything else a tool would
//! ask permission for is denied, since nobody is there to ask.

use std::fmt::Write;

use serde_json::{Value, json};

use crate::workspace::Workspace;
use crate::{Error, Result};

/// The built-in tools every session has: reading and running commands.
const TOOLS: [&str; 4] = ["Read", "Glob", "Grep", "Bash"];

/// The permission rule every session has: running the `slinger` CLI.
const SLINGER: &str = "Bash(slinger *)";

/// How a session's process starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start<'a> {
    /// A new conversation under this id, which [`session_id`] made.
    Fresh(&'a str),
    /// The conversation this id names, as an earlier process left it.
    Resume(&'a str),
}

/// What a session may read, run and load, as its workspace's agent sets it.
///
/// It reads its home (the working directory) and [`Limits::dirs`]; it uses
/// the built-in tools that read and run commands, but runs only what
/// [`Limits::tools`] allows (the `slinger` CLI, at least, and read-only
/// commands in those directories); it loads the settings and instructions of
/// its home, and the owner's own only with [`Limits::user_config`]. No MCP
/// server loads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Limits {
    /// More directories it may read, absolute.
    pub dirs: Vec<String>,
    /// Claude Code permission rules it may use without asking.
    pub tools: Vec<String>,
    /// Whether it loads the owner's own (user scope) settings and instructions.
    pub user_config: bool,
}

impl Limits {
    /// The limits of `workspace`'s agent on `machine`: its roots on this
    /// machine and the agent's `dirs`, the `slinger` CLI and the agent's
    /// `tools`, and its `user_config`. A workspace with no agent gets the
    /// roots and the CLI.
    #[must_use]
    pub fn of(workspace: &Workspace, machine: &str) -> Self {
        let agent = workspace.agent.as_ref();
        let roots = workspace.roots.iter().filter(|r| !r.primary && r.machine == machine);
        let more = agent.map(|a| a.dirs.as_slice()).unwrap_or_default();
        let rules = agent.map(|a| a.tools.as_slice()).unwrap_or_default();
        Self {
            dirs: unique(roots.map(|r| r.path.as_str()).chain(more.iter().map(String::as_str))),
            tools: unique([SLINGER].into_iter().chain(rules.iter().map(String::as_str))),
            user_config: agent.is_some_and(|a| a.user_config),
        }
    }

    /// The built-in tools the session has: [`TOOLS`] and the tool each of
    /// its rules names (`Edit` for `Edit(src/**)`).
    fn builtin(&self) -> String {
        let named = self.tools.iter().map(|rule| rule.split('(').next().unwrap_or(rule).trim());
        unique(TOOLS.into_iter().chain(named)).join(",")
    }
}

/// The arguments of the process, after the program's path.
#[must_use]
pub fn args(start: Start<'_>, limits: &Limits) -> Vec<String> {
    let (flag, id) = match start {
        Start::Fresh(id) => ("--session-id", id),
        Start::Resume(id) => ("--resume", id),
    };
    // Stream output in print mode needs --verbose.
    let fixed = ["--print", "--input-format", "stream-json", "--output-format", "stream-json"];
    let sources = if limits.user_config { "user,project,local" } else { "project,local" };
    let mut args: Vec<String> = fixed.into_iter().map(str::to_string).collect();
    args.extend(["--setting-sources", sources, "--strict-mcp-config"].map(str::to_string));
    // dontAsk denies whatever no rule allows, as nobody answers a prompt.
    args.extend(["--permission-mode", "dontAsk", "--tools"].map(str::to_string));
    args.push(limits.builtin());
    args.push("--allowedTools".into());
    args.extend(limits.tools.iter().cloned());
    if !limits.dirs.is_empty() {
        args.push("--add-dir".into());
        args.extend(limits.dirs.iter().cloned());
    }
    args.extend(["--verbose", flag, id].map(str::to_string));
    args
}

/// `items` in order, each once, blank ones dropped.
fn unique<'a>(items: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in items.into_iter().filter(|i| !i.is_empty()) {
        if !out.iter().any(|o| o == item) {
            out.push(item.to_string());
        }
    }
    out
}

/// A new session id, a UUID (version 4) made from `random`.
#[must_use]
pub fn session_id(mut random: [u8; 16]) -> String {
    random[6] = (random[6] & 0x0f) | 0x40;
    random[8] = (random[8] & 0x3f) | 0x80;
    let hex: String = random.iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    });
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

/// The line that delivers `text` to the process as one user message, newline
/// included.
///
/// # Errors
///
/// [`Error::Invalid`] for blank text: there is nothing to deliver.
pub fn message(text: &str) -> Result<String> {
    if text.trim().is_empty() {
        return Err(Error::Invalid {
            message: "the message is empty".into(),
            hint: Some("pass the text to send, or - to read it from standard input".into()),
        });
    }
    let line = json!({ "type": "user", "message": { "role": "user", "content": text } });
    Ok(format!("{line}\n"))
}

/// What one line of the process's output says, as far as the server cares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Said {
    /// The conversation's id, when the line names one.
    pub session_id: Option<String>,
    /// The answer to one message, when the line ends a turn: its text,
    /// empty when the turn ended with none (an error).
    pub answer: Option<String>,
}

/// Reads one line of output. A line that is not a JSON object says nothing.
#[must_use]
pub fn read(line: &str) -> Said {
    let Ok(Value::Object(object)) = serde_json::from_str::<Value>(line) else {
        return Said::default();
    };
    let session_id = object.get("session_id").and_then(Value::as_str);
    let session_id = session_id.filter(|id| !id.is_empty()).map(str::to_string);
    let answer = (object.get("type").and_then(Value::as_str) == Some("result"))
        .then(|| object.get("result").and_then(Value::as_str).unwrap_or_default().to_string());
    Said { session_id, answer }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_start_names_its_id_and_a_resume_names_the_one_to_resume() {
        let fresh = args(Start::Fresh("abc"), &Limits::default());
        let tail: Vec<&str> = fresh.iter().rev().take(2).map(String::as_str).collect();
        assert_eq!(tail, ["abc", "--session-id"]);
        let resume = args(Start::Resume("abc"), &Limits::default());
        assert_eq!(resume[resume.len() - 2..], ["--resume", "abc"]);
        for flag in ["--print", "stream-json", "--verbose"] {
            assert!(fresh.iter().any(|a| a == flag), "{flag}");
        }
    }

    fn workspace(text: &str) -> Workspace {
        Workspace::parse(&format!("version = 1\nname = \"w\"\n{text}")).unwrap()
    }

    #[test]
    fn a_default_session_reads_its_local_roots_and_runs_only_the_cli() {
        let ws = workspace(
            "[agent]\nname = \"a\"\n\
             [[root]]\nmachine = \"here\"\npath = \"/home/ws\"\nprimary = true\n\
             [[root]]\nmachine = \"here\"\npath = \"/data\"\n\
             [[root]]\nmachine = \"there\"\npath = \"/remote\"\n",
        );
        let limits = Limits::of(&ws, "here");
        let want =
            Limits { dirs: vec!["/data".into()], tools: vec![SLINGER.into()], ..Limits::default() };
        assert_eq!(limits, want);
        assert_eq!(
            args(Start::Fresh("id"), &limits).join(" "),
            [
                "--print --input-format stream-json --output-format stream-json",
                "--setting-sources project,local --strict-mcp-config",
                "--permission-mode dontAsk --tools Read,Glob,Grep,Bash",
                "--allowedTools Bash(slinger *) --add-dir /data --verbose --session-id id",
            ]
            .join(" ")
        );
    }

    #[test]
    fn an_agent_widens_its_limits_with_dirs_tools_and_its_user_config() {
        let ws = workspace(
            "[agent]\nname = \"a\"\nuser_config = true\ndirs = [\"/docs\", \"/docs\"]\n\
             tools = [\"Edit(/docs/**)\", \"Bash(git *)\", \"WebFetch\"]\n",
        );
        let limits = Limits::of(&ws, "here");
        assert_eq!(limits.dirs, ["/docs"], "each once");
        let args = args(Start::Resume("id"), &limits);
        let after = |flag: &str| {
            let at = args.iter().position(|a| a == flag).unwrap();
            args[at + 1..].iter().take_while(|a| !a.starts_with("--")).cloned().collect::<Vec<_>>()
        };
        assert_eq!(after("--setting-sources"), ["user,project,local"]);
        assert_eq!(after("--tools"), ["Read,Glob,Grep,Bash,Edit,WebFetch"]);
        assert_eq!(after("--allowedTools"), [SLINGER, "Edit(/docs/**)", "Bash(git *)", "WebFetch"]);
        assert_eq!(after("--add-dir"), ["/docs"]);
        assert_eq!(args[args.len() - 2..], ["--resume", "id"]);
    }

    #[test]
    fn a_session_id_is_a_version_4_uuid() {
        assert_eq!(session_id([0; 16]), "00000000-0000-4000-8000-000000000000");
        assert_eq!(session_id([0xff; 16]), "ffffffff-ffff-4fff-bfff-ffffffffffff");
    }

    #[test]
    fn a_message_is_one_user_line_and_blank_text_is_invalid() {
        let line = message("say \"hi\"\nthen stop").unwrap();
        assert_eq!(line.matches('\n').count(), 1);
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["type"], "user");
        assert_eq!(value["message"], json!({ "role": "user", "content": "say \"hi\"\nthen stop" }));
        assert_eq!(message(" \n").unwrap_err().code(), "invalid");
    }

    #[test]
    fn output_names_the_session_and_a_result_ends_a_turn_with_its_answer() {
        let init = read(r#"{"type":"system","subtype":"init","session_id":"s1"}"#);
        assert_eq!(init, Said { session_id: Some("s1".into()), answer: None });
        let result =
            read(r#"{"type":"result","subtype":"success","session_id":"s1","result":"hi"}"#);
        assert_eq!(result, Said { session_id: Some("s1".into()), answer: Some("hi".into()) });
        let failed = read(r#"{"type":"result","subtype":"error_during_execution"}"#);
        assert_eq!(failed.answer.as_deref(), Some(""));
        for nothing in ["", "not json", "[1]", r#"{"type":"assistant","session_id":""}"#] {
            assert_eq!(read(nothing), Said::default(), "{nothing}");
        }
    }
}
