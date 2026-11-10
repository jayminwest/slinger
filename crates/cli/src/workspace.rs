//! `slinger workspace ...`: the workspaces the daemon serves. One arm per
//! command, each one request; the daemon owns the files and every rule.

use std::path::{Path, PathBuf};

use clap::Subcommand;
use serde_json::json;

use slinger_control as control;

use crate::output::Output;

#[derive(Subcommand)]
pub enum Workspace {
    /// Open a directory as a workspace, making its file and canvas if needed; prints its name
    Open {
        /// The directory; leave it out for a workspace with no root
        dir: Option<PathBuf>,
        /// The name: of a new workspace, or of the one with no root to open
        #[arg(long)]
        name: Option<String>,
    },
    /// List the open workspaces: name, canvas and directory per line
    List,
    /// Print one open workspace: a field and its value per line
    Show {
        /// Its name, or the directory of its file
        workspace: String,
    },
    /// Close a workspace; its file, canvas and agent stay
    Close {
        /// Its name, or the directory of its file
        workspace: String,
    },
}

impl Workspace {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Open { .. } => "workspace open",
            Self::List => "workspace list",
            Self::Show { .. } => "workspace show",
            Self::Close { .. } => "workspace close",
        }
    }
}

pub fn execute(command: &Workspace, daemon: &control::Client) -> anyhow::Result<Output> {
    match command {
        Workspace::Open { dir, name } => {
            let opened = daemon.workspace_open(dir.as_deref(), name.as_deref())?;
            let human = opened.workspace.name.clone();
            Ok(Output::new(serde_json::to_value(opened)?, human))
        }
        Workspace::List => {
            let workspaces = daemon.workspace_list()?;
            let human = workspaces.iter().map(line).collect::<Vec<_>>().join("\n");
            Ok(Output::new(json!({ "workspaces": workspaces }), human))
        }
        Workspace::Show { workspace } => {
            let ws = daemon.workspace_show(&which(workspace)?)?;
            let human = fields(&ws);
            Ok(Output::new(json!({ "workspace": ws }), human))
        }
        Workspace::Close { workspace } => {
            let dir = daemon.workspace_close(&which(workspace)?)?;
            Ok(Output::new(json!({ "dir": dir }), String::new()))
        }
    }
}

/// A name as it is, a directory (`.`, `..` or anything with a `/`) from the root.
fn which(workspace: &str) -> anyhow::Result<String> {
    if workspace != "." && workspace != ".." && !workspace.contains('/') {
        return Ok(workspace.to_string());
    }
    Ok(std::path::absolute(Path::new(workspace))?.to_string_lossy().into_owned())
}

/// One workspace for `list`: name, canvas and directory, by tabs.
fn line(ws: &control::Workspace) -> String {
    format!("{}\t{}\t{}", ws.name, ws.canvas, ws.dir.display())
}

/// One workspace for `show`: a field and its value per line, by a tab.
fn fields(ws: &control::Workspace) -> String {
    let mut lines = vec![
        format!("name\t{}", ws.name),
        format!("tag\t{}", ws.tag),
        format!("canvas\t{}", ws.canvas),
        format!("dir\t{}", ws.dir.display()),
    ];
    if let Some(agent) = &ws.agent {
        let durable = if agent.durable { " (durable)" } else { "" };
        lines.push(format!("agent\t{}{durable}", agent.name));
    }
    for root in &ws.roots {
        let primary = if root.primary { " (primary)" } else { "" };
        lines.push(format!("root\t{}:{}{primary}", root.machine, root.path));
    }
    lines.join("\n")
}
