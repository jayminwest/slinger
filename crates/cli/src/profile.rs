//! `slinger profile ...`: the profiles of a workspace's agent, each a
//! working directory and the permission rules a session under it may use.
//! One request per command; the daemon checks them and keeps the file.

use std::path::PathBuf;

use clap::Subcommand;

use slinger_control as control;

use crate::output::Output;
use crate::workspace::which;

#[derive(Subcommand)]
pub enum Profile {
    /// List a workspace agent's profiles: name, directory and rules per line
    List {
        /// Its name, or the directory of its file
        workspace: String,
    },
    /// Add a profile, or replace the one of its name whole
    Set {
        /// Its name, or the directory of its file
        workspace: String,
        /// The profile's name: a-z, 0-9, - and _
        name: String,
        /// The session's working directory [default: the agent's home]
        #[arg(long)]
        dir: Option<PathBuf>,
        /// A Claude Code permission rule the session may use, beyond the slinger CLI: Edit, 'Bash(git *)'. Repeat it for more
        #[arg(long = "tool", value_name = "RULE")]
        tools: Vec<String>,
    },
    /// Remove a profile
    Remove {
        /// Its name, or the directory of its file
        workspace: String,
        /// The profile's name
        name: String,
    },
}

impl Profile {
    pub fn name(&self) -> &'static str {
        match self {
            Self::List { .. } => "profile list",
            Self::Set { .. } => "profile set",
            Self::Remove { .. } => "profile remove",
        }
    }
}

pub fn execute(command: &Profile, daemon: &control::Client) -> anyhow::Result<Output> {
    let (profiles, human) = match command {
        Profile::List { workspace } => {
            let profiles = daemon.profile_list(&which(workspace)?)?;
            let human = profiles.profiles.iter().map(line).collect::<Vec<_>>().join("\n");
            (profiles, human)
        }
        Profile::Set { workspace, name, dir, tools } => {
            // The daemon does not share this directory: it gets the whole path.
            let dir = dir.as_deref().map(std::path::absolute).transpose()?;
            let dir = dir.map(|d| d.to_string_lossy().into_owned());
            let profile = control::Profile { name: name.clone(), dir, tools: tools.clone() };
            (daemon.profile_set(&which(workspace)?, profile)?, String::new())
        }
        Profile::Remove { workspace, name } => {
            (daemon.profile_remove(&which(workspace)?, name)?, String::new())
        }
    };
    Ok(Output::new(serde_json::to_value(&profiles)?, human))
}

/// One profile for `list`: name, directory (`-` for the agent's home) and
/// rules joined by `, ` (`-` for none), by tabs.
fn line(p: &control::Profile) -> String {
    let dir = p.dir.as_deref().unwrap_or("-");
    let tools = if p.tools.is_empty() { "-".to_string() } else { p.tools.join(", ") };
    format!("{}\t{dir}\t{tools}", p.name)
}
