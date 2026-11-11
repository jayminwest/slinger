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
    /// Show a UI in a new pane, with data, for the calling agent; prints the showing's id and the pane's number
    Open {
        /// The UI: <name> (latest), <name>@<version> or a blob id
        ui: String,
        /// The workspace whose window shows it: its name, or the directory of its file
        #[arg(long, value_name = "WORKSPACE")]
        workspace: String,
        /// Where it shows
        #[arg(long = "as", value_name = "HOST", default_value = "pane", value_parser = ["pane"])]
        host: String,
        /// The JSON the UI's context gives it, or - to read standard input
        #[arg(long, value_name = "JSON")]
        data: Option<String>,
        /// Open it beside this pane, by number; else beside the whole window
        #[arg(long, value_name = "PANE")]
        beside: Option<u32>,
        /// Put it below instead of to the right
        #[arg(long)]
        below: bool,
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
            Self::Open { ui, workspace, host, data, beside, below, agent } => {
                let data = match data.as_deref() {
                    None => serde_json::Value::Null,
                    Some("-") => json_data(&io::read_to_string(input)?)?,
                    Some(text) => json_data(text)?,
                };
                let ask = control::UiOpen {
                    ui: ui.clone(),
                    workspace: which(workspace)?,
                    agent: calling_agent(agent.as_deref())?,
                    host: host.clone(),
                    data,
                    beside: *beside,
                    below: *below,
                };
                let opened = daemon.ui_open(ask)?;
                let human = format!("{}\t{}", opened.open, opened.pane);
                Ok(Output::new(serde_json::to_value(opened)?, human))
            }
        }
    }
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
