//! `slinger sling send`: a canvas's new ink and a prompt, to the agent of
//! the canvas's workspace. One request; the daemon builds the packet,
//! delivers it and keeps the record.

use std::io::Read;

use clap::Subcommand;

use slinger_control as control;

use crate::output::Output;

#[derive(Subcommand)]
pub enum Sling {
    /// Send the ink drawn since the last sling and a prompt to the canvas's running agent
    Send {
        /// The canvas; its open workspace's agent gets the sling
        canvas: String,
        /// The page, by id (p1) or name [default: the first page]
        #[arg(long)]
        page: Option<String>,
        /// The selected items: ids such as c42,t12 or i7
        #[arg(long, value_name = "IDS", value_delimiter = ',')]
        select: Vec<String>,
        /// What to ask, or - to read standard input [default: none: the selection is the question]
        #[arg(long, value_name = "TEXT")]
        prompt: Option<String>,
        /// Send only if the agent runs under this profile
        #[arg(long, value_name = "NAME")]
        profile: Option<String>,
    },
}

impl Sling {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Send { .. } => "sling send",
        }
    }
}

pub fn execute(
    command: &Sling,
    daemon: &control::Client,
    input: &mut dyn Read,
) -> anyhow::Result<Output> {
    let Sling::Send { canvas, page, select, prompt, profile } = command;
    let mut prompt = prompt.clone().unwrap_or_default();
    if prompt == "-" {
        prompt.clear();
        input.read_to_string(&mut prompt)?;
    }
    let ask = control::SlingAsk {
        canvas: canvas.clone(),
        page: page.clone(),
        select: select.clone(),
        prompt,
        profile: profile.clone(),
        // Only the app can render a PDF: ink on white, `over=` names the pages.
        backdrops: Vec::new(),
    };
    let slung = daemon.sling_send(ask)?;
    Ok(Output::new(serde_json::to_value(slung)?, String::new()))
}
