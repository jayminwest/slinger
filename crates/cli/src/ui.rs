//! `slinger ui`: agent UIs, named and versioned in the record
//! (docs/design/live-items.md). Argv in, control requests out.

use std::io::{self, Read};
use std::path::PathBuf;

use clap::Subcommand;
use serde_json::json;

use slinger_control as control;

use crate::cli::{NAME, Scratch};
use crate::output::Output;
use crate::session::{AGENT_HELP, calling_agent};
use crate::workspace::which;

#[derive(Subcommand)]
pub enum Ui {
    /// Store an HTML file as the next version of a UI; prints <name>@<version>
    Put {
        /// The UI's name: a-z, 0-9 and -, starting with a letter
        name: String,
        /// The HTML file, or - to read standard input
        file: PathBuf,
        /// The program or agent that wrote the UI
        #[arg(long, value_name = "NAME", default_value = NAME)]
        source: String,
    },
    /// List the latest version of every UI, one line each
    List,
    /// List every version of one UI, oldest first
    Show {
        /// The UI's name
        name: String,
    },
    /// Write one version out as <name>.v<version>.html; prints its path
    Export {
        /// The UI: <name> (latest), <name>@<version> or a blob id
        ui: String,
        /// The directory to write into, made if missing
        #[arg(long, value_name = "DIR", default_value = ".")]
        out: PathBuf,
    },
    /// Show a UI, with data, for the calling agent, in a new pane or as a live item on a canvas; prints the showing's id and the pane's number or the item's id
    Open {
        /// The UI: <name> (latest), <name>@<version> or a blob id
        ui: String,
        /// The workspace whose window shows it as a pane: its name, or the directory of its file
        #[arg(long, value_name = "WORKSPACE", required_if_eq("host", "pane"))]
        workspace: Option<String>,
        /// Where it shows: a pane of its own, or an item on a canvas
        #[arg(long = "as", value_name = "HOST", default_value = "pane", value_parser = ["pane", "item"])]
        host: String,
        /// The JSON the UI's context gives it, or - to read standard input
        #[arg(long, value_name = "JSON")]
        data: Option<String>,
        /// A pane: open it beside this pane, by number (else beside the whole window). An item: put it beside this item (i12, c12)
        #[arg(long, value_name = "PANE|ITEM", required_if_eq("host", "item"))]
        beside: Option<String>,
        /// A pane: put it below instead of to the right
        #[arg(long)]
        below: bool,
        /// An item: the canvas it goes on
        #[arg(long, value_name = "CANVAS", required_if_eq("host", "item"))]
        canvas: Option<String>,
        /// An item: its size, w,h in tenths of a unit (default 3200,2400)
        #[arg(long, value_name = "W,H", value_parser = crate::put::size)]
        size: Option<[i64; 2]>,
        #[arg(long, value_name = "NAME", help = AGENT_HELP)]
        agent: Option<String>,
    },
}

impl Ui {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Put { .. } => "ui put",
            Self::List => "ui list",
            Self::Show { .. } => "ui show",
            Self::Export { .. } => "ui export",
            Self::Open { .. } => "ui open",
        }
    }

    pub fn execute(
        &self,
        daemon: &control::Client,
        input: &mut dyn Read,
    ) -> anyhow::Result<Output> {
        match self {
            Self::Put { name, file, source } => {
                let put = if file.as_os_str() == "-" {
                    let (scratch, mut copy) = Scratch::new()?;
                    io::copy(input, &mut copy)?;
                    daemon.ui_put(name, &scratch.0, source)?
                } else {
                    daemon.ui_put(name, file, source)?
                };
                let human = format!("{}@{}", put.ui.name, put.ui.version);
                Ok(Output::new(json!({ "ui": put.ui, "new": put.new }), human))
            }
            Self::List => Ok(list(&daemon.ui_list(None)?)),
            Self::Show { name } => Ok(list(&daemon.ui_list(Some(name))?)),
            Self::Export { ui, out } => {
                let exported = daemon.ui_export(ui, out)?;
                let human = exported.path.display().to_string();
                Ok(Output::new(json!({ "ui": exported.ui, "path": exported.path }), human))
            }
            Self::Open { ui, workspace, host, data, beside, below, canvas, size, agent } => {
                let data = match data.as_deref() {
                    None => serde_json::Value::Null,
                    Some("-") => json_data(&io::read_to_string(input)?)?,
                    Some(text) => json_data(text)?,
                };
                let mut ask = control::UiOpen {
                    ui: ui.clone(),
                    agent: calling_agent(agent.as_deref())?,
                    data,
                    ..control::UiOpen::default()
                };
                if host == "item" {
                    ask.host = "canvas".into();
                    (ask.canvas, ask.item, ask.size) = (canvas.clone(), beside.clone(), *size);
                } else {
                    ask.host = "pane".into();
                    ask.workspace = which(workspace.as_deref().unwrap_or_default())?;
                    ask.beside = beside.as_deref().map(pane_number).transpose()?;
                    ask.below = *below;
                }
                let opened = daemon.ui_open(ask)?;
                let place = match (&opened.item, opened.pane) {
                    (Some(item), _) => item.item.clone(),
                    (None, pane) => pane.unwrap_or_default().to_string(),
                };
                let human = format!("{}\t{place}", opened.open);
                Ok(Output::new(serde_json::to_value(opened)?, human))
            }
        }
    }
}

/// A pane's number, as `--beside` names it for a pane.
fn pane_number(text: &str) -> Result<u32, control::RemoteError> {
    text.parse().map_err(|_| {
        let hint = "beside a pane, --beside takes its number, as 'slinger pane list' prints it";
        control::RemoteError::new(
            "invalid",
            format!("`{text}` is not a pane's number"),
            Some(hint.into()),
        )
    })
}

/// The data a UI is given, as JSON.
fn json_data(text: &str) -> Result<serde_json::Value, control::RemoteError> {
    serde_json::from_str(text).map_err(|e| {
        let hint = "pass JSON, such as '{\"slots\":[\"Tue 10:00\"]}'".to_string();
        control::RemoteError::new("invalid", format!("the data is not JSON: {e}"), Some(hint))
    })
}

/// One line per version: `<name>@<version>`, hash, bytes, source and time,
/// separated by tabs.
fn list(uis: &[control::Ui]) -> Output {
    let human = uis
        .iter()
        .map(|v| {
            format!("{}@{}\t{}\t{}\t{}\t{}", v.name, v.version, v.hash, v.bytes, v.source, v.time)
        })
        .collect::<Vec<_>>()
        .join("\n");
    Output::new(json!({ "uis": uis }), human)
}
