//! `slinger ui`: agent UIs, named and versioned in the record
//! (docs/design/live-items.md). Argv in, control requests out.

use std::io::{self, Read};
use std::path::PathBuf;

use clap::Subcommand;
use serde_json::json;

use slinger_control as control;

use crate::cli::{NAME, Scratch};
use crate::output::Output;

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
}

impl Ui {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Put { .. } => "ui put",
            Self::List => "ui list",
            Self::Show { .. } => "ui show",
            Self::Export { .. } => "ui export",
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
        }
    }
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
