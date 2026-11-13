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
//! it may use without asking and which configuration it loads. Anything
//! else a tool would ask permission for is denied, since nobody is there to
//! ask.

use std::fmt::Write;

use serde_json::{Value, json};

use crate::sling::{Block, Message};
use crate::workspace::{Workspace, profile};
use crate::{Error, Result};

/// The built-in tools a session under a profile has: reading and running
/// commands. One under no profile has all of Claude Code's.
const TOOLS: [&str; 4] = ["Read", "Glob", "Grep", "Bash"];

/// The permission rule every session has: running the `slinger` CLI.
const SLINGER: &str = "Bash(slinger *)";

/// The permission rules a session under no profile has too: writing files.
const EDITS: [&str; 2] = ["Edit", "Write"];

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
/// It reads its home (the working directory) and [`Limits::dirs`]; it has
/// every built-in tool of Claude Code and the MCP servers its settings name
/// (slinger-29bb9f), but uses without asking only what [`Limits::tools`]
/// allows (the `slinger` CLI, at least, and read-only commands in those
/// directories); under no profile it writes files too ([`EDITS`]); it loads
/// the settings and instructions of its home, and the owner's own with
/// [`Limits::user_config`] (as the agent's is by default). Under a profile it runs in the profile's
/// directory ([`Limits::dir`]), with the profile's rules in place of the
/// agent's and of [`EDITS`], and only the built-in tools that read, run
/// commands and its rules name ([`Limits::narrow`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Limits {
    /// The working directory, when it is not the agent's home: a profile's.
    pub dir: Option<String>,
    /// More directories it may read, absolute.
    pub dirs: Vec<String>,
    /// Claude Code permission rules it may use without asking.
    pub tools: Vec<String>,
    /// Whether it loads the owner's own (user scope) settings and instructions.
    pub user_config: bool,
    /// Whether it has only [`TOOLS`] and the built-in tools its rules name,
    /// as under a profile, rather than all of them.
    pub narrow: bool,
}

impl Limits {
    /// The limits of `workspace`'s agent on `machine`: its roots on this
    /// machine and the agent's `dirs`, the `slinger` CLI, the rules that
    /// write files ([`EDITS`]) and the agent's `tools`, and its
    /// `user_config`. A workspace with no agent gets the roots, the CLI,
    /// those rules and the owner's own config.
    #[must_use]
    pub fn of(workspace: &Workspace, machine: &str) -> Self {
        let agent = workspace.agent.as_ref();
        let roots = workspace.roots.iter().filter(|r| !r.primary && r.machine == machine);
        let more = agent.map(|a| a.dirs.as_slice()).unwrap_or_default();
        let rules = agent.map(|a| a.tools.as_slice()).unwrap_or_default();
        Self {
            dirs: unique(roots.map(|r| r.path.as_str()).chain(more.iter().map(String::as_str))),
            tools: unique(
                [SLINGER].into_iter().chain(EDITS).chain(rules.iter().map(String::as_str)),
            ),
            user_config: agent.is_none_or(|a| a.user_config),
            dir: None,
            narrow: false,
        }
    }

    /// The limits of `workspace`'s agent on `machine` under the profile
    /// `profile` names, or [`Limits::of`] with none. A profile's rules take
    /// the place of the agent's `tools` and of [`EDITS`] (the `slinger` CLI
    /// stays), so a profile with no rules cannot write, and it has only the
    /// built-in tools they name ([`Limits::narrow`]). A profile
    /// with a directory runs there, and still reads the home (the primary
    /// root, when it is on `machine`) and the other directories.
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] when the agent has no such profile.
    pub fn under(workspace: &Workspace, machine: &str, profile: Option<&str>) -> Result<Self> {
        let mut limits = Self::of(workspace, machine);
        let Some(name) = profile else { return Ok(limits) };
        let profile = workspace.profile(name)?;
        let rules = profile.tools.iter().map(String::as_str);
        limits.tools = unique([SLINGER].into_iter().chain(rules));
        limits.narrow = true;
        if let Some(dir) = &profile.dir {
            let home = workspace.primary_root().filter(|r| r.machine == machine);
            let all = home
                .map(|r| r.path.as_str())
                .into_iter()
                .chain(limits.dirs.iter().map(String::as_str));
            limits.dirs = unique(all.filter(|d| d != dir));
            limits.dir = Some(dir.clone());
        }
        Ok(limits)
    }

    /// The built-in tools the session has: [`TOOLS`] and the built-in tool
    /// each of its rules names (`Edit` for `Edit(src/**)`; none for an MCP
    /// rule).
    fn builtin(&self) -> String {
        let named = self.tools.iter().filter_map(|rule| profile::builtin(rule));
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
    args.extend(["--setting-sources", sources].map(str::to_string));
    // dontAsk denies whatever no rule allows, as nobody answers a prompt.
    args.extend(["--permission-mode", "dontAsk"].map(str::to_string));
    if limits.narrow {
        args.extend(["--tools".into(), limits.builtin()]);
    }
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

/// The line that delivers a sling's message to the process as one user
/// message, newline included: its blocks in order, text as text blocks and
/// each image as a base64 PNG image block, as the model's API takes them.
/// A sling message is never blank, so there is nothing to reject.
#[must_use]
pub fn sling(message: &Message) -> String {
    let content: Vec<Value> = message
        .blocks
        .iter()
        .map(|block| match block {
            Block::Text(text) => json!({ "type": "text", "text": text }),
            Block::Image(image) => json!({
                "type": "image",
                "source": { "type": "base64", "media_type": "image/png", "data": base64(&image.png) },
            }),
        })
        .collect();
    let line = json!({ "type": "user", "message": { "role": "user", "content": content } });
    format!("{line}\n")
}

/// Standard base64, with padding.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            let pad = i > chunk.len();
            out.push(if pad {
                '='
            } else {
                char::from(ALPHABET[(n >> (18 - 6 * i) & 63) as usize])
            });
        }
    }
    out
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
    fn a_default_session_reads_its_local_roots_has_every_tool_and_runs_only_the_cli_and_edits() {
        let ws = workspace(
            "[agent]\nname = \"a\"\n\
             [[root]]\nmachine = \"here\"\npath = \"/home/ws\"\nprimary = true\n\
             [[root]]\nmachine = \"here\"\npath = \"/data\"\n\
             [[root]]\nmachine = \"there\"\npath = \"/remote\"\n",
        );
        let limits = Limits::of(&ws, "here");
        let tools = vec![SLINGER.into(), "Edit".into(), "Write".into()];
        let want =
            Limits { dirs: vec!["/data".into()], tools, user_config: true, ..Limits::default() };
        assert_eq!(limits, want);
        assert_eq!(
            args(Start::Fresh("id"), &limits).join(" "),
            [
                "--print --input-format stream-json --output-format stream-json",
                "--setting-sources user,project,local --permission-mode dontAsk",
                "--allowedTools Bash(slinger *) Edit Write --add-dir /data --verbose --session-id id",
            ]
            .join(" ")
        );
    }

    #[test]
    fn an_agent_widens_its_limits_with_dirs_and_tools_and_can_leave_out_its_user_config() {
        let ws = workspace(
            "[agent]\nname = \"a\"\nuser_config = false\ndirs = [\"/docs\", \"/docs\"]\n\
             tools = [\"Edit(/docs/**)\", \"Bash(git *)\", \"WebFetch\", \"mcp__gh\"]\n",
        );
        let limits = Limits::of(&ws, "here");
        assert_eq!(limits.dirs, ["/docs"], "each once");
        let args = args(Start::Resume("id"), &limits);
        let after = |flag: &str| {
            let at = args.iter().position(|a| a == flag).unwrap();
            args[at + 1..].iter().take_while(|a| !a.starts_with("--")).cloned().collect::<Vec<_>>()
        };
        assert_eq!(after("--setting-sources"), ["project,local"]);
        assert!(!args.iter().any(|a| a == "--tools"), "every built-in tool");
        assert!(!args.iter().any(|a| a == "--strict-mcp-config"), "its MCP servers load");
        let rules =
            [SLINGER, "Edit", "Write", "Edit(/docs/**)", "Bash(git *)", "WebFetch", "mcp__gh"];
        assert_eq!(after("--allowedTools"), rules);
        assert_eq!(after("--add-dir"), ["/docs"]);
        assert_eq!(args[args.len() - 2..], ["--resume", "id"]);
    }

    #[test]
    fn a_profile_replaces_the_agents_rules_and_moves_the_working_directory() {
        let ws = workspace(
            "[agent]\nname = \"a\"\ndirs = [\"/docs\"]\ntools = [\"WebFetch\"]\n\
             [[agent.profile]]\nname = \"dev\"\ndir = \"/src\"\ntools = [\"Edit\", \"Bash(git *)\"]\n\
             [[agent.profile]]\nname = \"look\"\n\
             [[root]]\nmachine = \"here\"\npath = \"/home/ws\"\nprimary = true\n",
        );
        assert_eq!(Limits::under(&ws, "here", None).unwrap(), Limits::of(&ws, "here"));
        let dev = Limits::under(&ws, "here", Some("dev")).unwrap();
        let want = Limits {
            dir: Some("/src".into()),
            dirs: vec!["/home/ws".into(), "/docs".into()],
            tools: vec![SLINGER.into(), "Edit".into(), "Bash(git *)".into()],
            user_config: true,
            narrow: true,
        };
        assert_eq!(dev, want);
        let args = args(Start::Fresh("id"), &dev);
        let at = args.iter().position(|a| a == "--tools").unwrap();
        assert_eq!(args[at + 1], "Read,Glob,Grep,Bash,Edit", "only the tools its rules name");
        assert_eq!(Limits::under(&ws, "elsewhere", Some("dev")).unwrap().dirs, ["/docs"]);
        let look = Limits::under(&ws, "here", Some("look")).unwrap();
        assert_eq!(
            (look.dir, look.tools),
            (None, vec![SLINGER.to_string()]),
            "no rules: it cannot write"
        );
        assert_eq!(Limits::under(&ws, "here", Some("nope")).unwrap_err().code(), "not_found");
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
    fn base64_matches_the_standard_vectors() {
        let cases =
            [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foobar", "Zm9vYmFy")];
        for (plain, coded) in cases {
            assert_eq!(base64(plain.as_bytes()), coded, "{plain}");
        }
        assert_eq!(base64(&[0xfb, 0xff, 0xbf]), "+/+/");
    }

    #[test]
    fn a_sling_is_one_user_line_of_text_and_image_blocks_in_order() {
        let image = crate::render::Image {
            name: "c1.png".into(),
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            png: b"png".to_vec(),
        };
        let message = Message {
            blocks: vec![
                Block::Text("c1 ink\n".into()),
                Block::Image(image),
                Block::Text("# prompt\nwhy?".into()),
            ],
        };
        let line = sling(&message);
        assert_eq!(line.matches('\n').count(), 1);
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["type"], "user");
        assert_eq!(
            value["message"]["content"],
            json!([
                { "type": "text", "text": "c1 ink\n" },
                { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "cG5n" } },
                { "type": "text", "text": "# prompt\nwhy?" },
            ])
        );
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
