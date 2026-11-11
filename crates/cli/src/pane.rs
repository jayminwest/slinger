//! `slinger pane ...`: a workspace's panes, as the server keeps them. One
//! arm per command, each one request; the daemon owns the layout.

use clap::Subcommand;

use slinger_control::{self as control, PaneKind, PaneLayout};

use crate::output::Output;
use crate::workspace::which;

#[derive(Subcommand)]
pub enum Pane {
    /// List a workspace's panes, left to right and top to bottom: number, kind and target per line
    List {
        /// Its name, or the directory of its file
        workspace: String,
    },
    /// Open a pane that shows a canvas or an agent UI; prints its number
    Open {
        /// Its name, or the directory of its file
        workspace: String,
        /// What the pane shows
        #[arg(long = "type", value_name = "TYPE", value_parser = ["canvas", "ui"])]
        kind: String,
        /// The canvas id or the UI name
        #[arg(long, value_name = "ID")]
        target: String,
        /// Open it beside this pane, by number; else beside the whole window
        #[arg(long, value_name = "PANE")]
        beside: Option<u32>,
        /// Put it below instead of to the right
        #[arg(long)]
        below: bool,
    },
    /// Close a pane; the pane beside it takes its space. A pane already gone, or the last, stays as it is
    Close {
        /// Its name, or the directory of its file
        workspace: String,
        /// The pane's number, as `pane list` prints it
        pane: u32,
    },
    /// Move the border after a pane (on its right, or below it): its split gives its first side RATIO. The last pane has none, and stays as it is
    Resize {
        /// Its name, or the directory of its file
        workspace: String,
        /// The number of the pane before the border, as `pane list` prints it
        pane: u32,
        /// The first side's share of the split, held in 0.1 to 0.9
        ratio: f64,
    },
}

impl Pane {
    pub fn name(&self) -> &'static str {
        match self {
            Self::List { .. } => "pane list",
            Self::Open { .. } => "pane open",
            Self::Close { .. } => "pane close",
            Self::Resize { .. } => "pane resize",
        }
    }
}

pub fn execute(command: &Pane, daemon: &control::Client) -> anyhow::Result<Output> {
    let (layout, human) = match command {
        Pane::List { workspace } => {
            let layout = daemon.pane_list(&which(workspace)?)?;
            let lines: Vec<String> = layout.layout.panes().iter().map(|p| line(p)).collect();
            let human = lines.join("\n");
            (layout, human)
        }
        Pane::Open { workspace, kind, target, beside, below } => {
            let kind = if kind == "ui" { PaneKind::Ui } else { PaneKind::Canvas };
            let layout = daemon.pane_open(&which(workspace)?, kind, target, *beside, *below)?;
            let human = layout.pane.map(|n| n.to_string()).unwrap_or_default();
            (layout, human)
        }
        Pane::Close { workspace, pane } => {
            (daemon.pane_close(&which(workspace)?, *pane)?, String::new())
        }
        Pane::Resize { workspace, pane, ratio } => {
            (daemon.pane_resize(&which(workspace)?, *pane, *ratio)?, String::new())
        }
    };
    Ok(Output::new(json(&layout)?, human))
}

/// The reply, with the panes listed in order beside the tree.
fn json(layout: &PaneLayout) -> anyhow::Result<serde_json::Value> {
    let mut doc = serde_json::to_value(layout)?;
    doc["panes"] = serde_json::to_value(layout.layout.panes())?;
    Ok(doc)
}

/// One pane for `list`: number, kind and target, by tabs.
fn line(pane: &control::Pane) -> String {
    let kind = match pane.kind {
        PaneKind::Canvas => "canvas",
        PaneKind::Ui => "ui",
    };
    format!("{}\t{kind}\t{}", pane.pane, pane.target)
}
