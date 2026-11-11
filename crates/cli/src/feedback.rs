//! `slinger feedback`: notes about slinger, kept by slingerd in its
//! feedback file (docs/service.md, "Feedback"). Argv in, control requests out.

use clap::Subcommand;
use serde_json::json;

use slinger_control as control;

use crate::output::Output;

#[derive(Subcommand)]
pub enum Feedback {
    /// Save a note about slinger; prints its ts, which names it
    Add {
        /// What you noticed; the words are joined by spaces
        #[arg(required = true, num_args = 1..)]
        text: Vec<String>,
    },
    /// List the open notes, newest first: ts and text, separated by a tab
    Ls {
        /// Resolved notes too, with `resolved` and the note in a third column
        #[arg(long)]
        all: bool,
        /// At most this many
        #[arg(long, value_name = "N", default_value_t = 20)]
        limit: u64,
    },
    /// Mark a note resolved
    Resolve {
        /// The note's ts, as `ls` prints it
        ts: String,
        /// How it was resolved, such as the issue that tracks it
        note: Vec<String>,
    },
    /// Print the path of the feedback file
    Path,
}

impl Feedback {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Add { .. } => "feedback add",
            Self::Ls { .. } => "feedback ls",
            Self::Resolve { .. } => "feedback resolve",
            Self::Path => "feedback path",
        }
    }

    pub fn execute(&self, daemon: &control::Client) -> anyhow::Result<Output> {
        match self {
            Self::Add { text } => {
                let note = control::FeedbackNote { text: text.join(" "), ..Default::default() };
                let added = daemon.feedback_add(note)?;
                let human = added.entry.ts.clone();
                Ok(Output::new(json!({ "path": added.path, "entry": added.entry }), human))
            }
            Self::Ls { all, limit } => {
                let listed = daemon.feedback_list(*all, Some(*limit))?;
                let human = listed.entries.iter().map(line).collect::<Vec<_>>().join("\n");
                let data = json!({ "path": listed.path, "entries": listed.entries });
                Ok(Output::new(data, human))
            }
            Self::Resolve { ts, note } => {
                let note = Some(note.join(" ")).filter(|n| !n.trim().is_empty());
                let done = daemon.feedback_resolve(ts, note)?;
                Ok(Output::new(serde_json::to_value(done)?, String::new()))
            }
            Self::Path => {
                let path = daemon.feedback_path()?;
                Ok(Output::new(json!({ "path": path }), path.display().to_string()))
            }
        }
    }
}

/// One note for people: ts and text on one line (its line breaks as
/// spaces), then `resolved` and its note when it is resolved.
fn line(entry: &control::FeedbackEntry) -> String {
    let mut line = format!("{}\t{}", entry.ts, entry.text.replace('\n', " "));
    if let Some(resolved) = &entry.resolved {
        line.push_str("\tresolved");
        if let Some(note) = &resolved.note {
            line.push(' ');
            line.push_str(note);
        }
    }
    line
}
