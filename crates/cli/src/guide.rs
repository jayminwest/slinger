//! `slinger guide` and `slinger skill install`: what an agent reads to use
//! slinger. The guide is the text in `guide.md` and then one entry per
//! command, made from the commands themselves, so the reference is never
//! behind the binary that prints it. The skill (`plugin/skills/slinger`) only
//! says when to use slinger and to run `slinger guide`. Neither needs a daemon.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use clap::{Arg, Command, Subcommand};
use serde_json::{Value, json};

use slinger_control as control;

use crate::output::Output;

/// The skill file, the same bytes the Claude Code plugin ships.
pub const SKILL: &str = include_str!("../../../plugin/skills/slinger/SKILL.md");

/// The part of the guide a person wrote: the ideas and the common tasks.
pub const GUIDE: &str = include_str!("guide.md");

/// Help for `--dir`: a string, because a doc comment may not hold `$CLAUDE_CONFIG_DIR` bare.
const DIR_HELP: &str =
    "The skills directory [default: $CLAUDE_CONFIG_DIR/skills, else ~/.claude/skills]";

#[derive(Subcommand)]
pub enum Skill {
    /// Write the slinger skill where an agent harness finds it; prints the file's path
    Install {
        #[arg(long, value_name = "DIR", help = DIR_HELP)]
        dir: Option<PathBuf>,
    },
}

impl Skill {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Install { .. } => "skill install",
        }
    }

    pub fn execute(&self) -> anyhow::Result<Output> {
        let Self::Install { dir } = self;
        let config = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from);
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let Some(dir) = dir.clone().or_else(|| default_dir(config, home)) else {
            let hint = "pass --dir <DIR>, such as --dir ~/.claude/skills".to_string();
            let message = "no home directory to install the skill under";
            return Err(control::RemoteError::new("usage", message, Some(hint)).into());
        };
        install(&dir)
    }
}

/// Where Claude Code looks for a user's skills: under its config directory
/// when one is set, else under `.claude` in the home directory.
fn default_dir(config: Option<PathBuf>, home: Option<PathBuf>) -> Option<PathBuf> {
    let config = config.filter(|c| !c.as_os_str().is_empty());
    let home = home.filter(|h| !h.as_os_str().is_empty());
    config.or_else(|| home.map(|h| h.join(".claude"))).map(|c| c.join("skills"))
}

/// Writes the skill to `<dir>/slinger/SKILL.md`, unless it holds these bytes.
fn install(dir: &Path) -> anyhow::Result<Output> {
    let folder = dir.join("slinger");
    let path = folder.join("SKILL.md");
    let new = fs::read_to_string(&path).map_or(true, |old| old != SKILL);
    if new {
        fs::create_dir_all(&folder).with_context(|| format!("cannot make {}", folder.display()))?;
        fs::write(&path, SKILL).with_context(|| format!("cannot write {}", path.display()))?;
    }
    let shown = path.display().to_string();
    Ok(Output::new(json!({ "path": shown, "new": new }), shown))
}

/// One command an agent can run: its words, its arguments and what it does.
pub struct Entry {
    pub name: String,
    pub usage: String,
    pub about: String,
}

/// The guide: the written part, then every command of `root`.
pub fn guide(root: &Command) -> Output {
    let entries = entries(root);
    let mut text = format!("{}\n\n## Commands\n", GUIDE.trim_end());
    let mut list: Vec<Value> = Vec::new();
    for Entry { name, usage, about } in &entries {
        let _ = write!(text, "\n{usage}\n    {about}");
        list.push(json!({ "name": name, "usage": usage, "about": about }));
    }
    Output::new(json!({ "guide": text, "commands": list }), text)
}

/// Every command of `root` that runs something, in help order.
pub fn entries(root: &Command) -> Vec<Entry> {
    let mut out = Vec::new();
    collect(root, root.get_name(), &mut out);
    out
}

fn collect(command: &Command, path: &str, out: &mut Vec<Entry>) {
    for sub in command.get_subcommands() {
        let name = format!("{path} {}", sub.get_name());
        if sub.has_subcommands() {
            collect(sub, &name, out);
            continue;
        }
        // Positionals first, as the user types them; then the flags.
        let (places, flags): (Vec<&Arg>, Vec<&Arg>) =
            sub.get_arguments().partition(|a| a.is_positional());
        let args: Vec<String> = places.into_iter().chain(flags).map(arg_usage).collect();
        let usage = [name.clone()].into_iter().chain(args).collect::<Vec<_>>().join(" ");
        let about = sub.get_about().map(ToString::to_string).unwrap_or_default();
        out.push(Entry { name, usage, about });
    }
}

/// One argument as a usage line shows it: `<ID>`, `--flag`, `--name <VALUE>`,
/// in brackets when the command runs without it.
fn arg_usage(arg: &Arg) -> String {
    let id = arg.get_id().as_str().to_uppercase();
    let values = arg.get_value_names().map_or_else(
        || format!("<{id}>"),
        |names| names.iter().map(|n| format!("<{n}>")).collect::<Vec<_>>().join(" "),
    );
    let body = match arg.get_long() {
        Some(long) if arg.get_action().takes_values() => format!("--{long} {values}"),
        Some(long) => format!("--{long}"),
        None => values,
    };
    if arg.is_required_set() { body } else { format!("[{body}]") }
}
