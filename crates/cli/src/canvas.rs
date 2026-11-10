//! `slinger canvas ...`: canvases the daemon keeps. One arm per command, each
//! one request; the daemon owns the files and every rule.

use std::io::Read;
use std::path::PathBuf;

use clap::Subcommand;
use serde_json::{Value, json};

use slinger_control as control;

use crate::output::Output;

/// Help for `canvas apply`'s operations: a string, because a doc comment wants the JSON bare.
const OPS_HELP: &str = "One operation as a JSON object, or a list of them; - reads standard \
     input. Example: '{\"op\":\"add\",\"item\":{\"page\":\"p1\",\"kind\":\"box\",\"x\":0,\"y\":0,\
     \"w\":100,\"h\":50}}' (lengths in tenths of a unit)";

#[derive(Subcommand)]
pub enum Canvas {
    /// Make a new, empty canvas with one page
    Create {
        /// The canvas id: a-z, 0-9, '-', '_' and '.'
        canvas: String,
    },
    /// List the canvases, one id per line
    List,
    /// Print one page's items: id, kind, maker and the item as JSON per line
    Show {
        canvas: String,
        /// The page, by id (p1) or name [default: the first page]
        #[arg(long)]
        page: Option<String>,
    },
    /// Apply operations as one undo step; prints the ids of what they made
    Apply {
        canvas: String,
        #[arg(help = OPS_HELP)]
        ops: String,
        /// Apply as this agent, which may change only what it made
        #[arg(long, value_name = "NAME")]
        agent: Option<String>,
    },
    /// Undo your last step on the canvas
    Undo {
        canvas: String,
        /// Undo this agent's last step instead of the user's
        #[arg(long, value_name = "NAME")]
        agent: Option<String>,
    },
    /// Redo the last step you undid on the canvas
    Redo {
        canvas: String,
        /// Redo this agent's step instead of the user's
        #[arg(long, value_name = "NAME")]
        agent: Option<String>,
    },
    /// Make a new canvas from a tldraw file; prints its id and what it holds
    Import {
        /// A .tldraw or .tldr file
        file: PathBuf,
        /// The new canvas's id [default: made from the file's name]
        #[arg(long)]
        name: Option<String>,
    },
}

impl Canvas {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Create { .. } => "canvas create",
            Self::List => "canvas list",
            Self::Show { .. } => "canvas show",
            Self::Apply { .. } => "canvas apply",
            Self::Undo { .. } => "canvas undo",
            Self::Redo { .. } => "canvas redo",
            Self::Import { .. } => "canvas import",
        }
    }
}

pub fn execute(
    command: &Canvas,
    daemon: &control::Client,
    input: &mut dyn Read,
) -> anyhow::Result<Output> {
    match command {
        Canvas::Create { canvas } => {
            daemon.canvas_create(canvas)?;
            Ok(Output::new(json!({ "canvas": canvas }), String::new()))
        }
        Canvas::List => {
            let canvases = daemon.canvas_list()?;
            let human = canvases.join("\n");
            Ok(Output::new(json!({ "canvases": canvases }), human))
        }
        Canvas::Show { canvas, page } => {
            let shown = daemon.canvas_show(canvas, page.as_deref())?;
            let human = shown.items.iter().map(item_line).collect::<Vec<_>>().join("\n");
            Ok(Output::new(serde_json::to_value(shown)?, human))
        }
        Canvas::Apply { canvas, ops, agent } => {
            let mut text = ops.clone();
            if ops == "-" {
                text.clear();
                input.read_to_string(&mut text)?;
            }
            let ops = serde_json::from_str(&text).map_err(|e| {
                let hint = "pass a JSON object or list; see 'slinger canvas apply --help'";
                control::RemoteError::new(
                    "invalid",
                    format!("operations are not JSON: {e}"),
                    Some(hint.into()),
                )
            })?;
            let request =
                control::CanvasOps { canvas: canvas.clone(), by: by(agent.as_deref()), ops };
            change(&daemon.canvas_apply(request)?)
        }
        Canvas::Undo { canvas, agent } => {
            change(&daemon.canvas_undo(step(canvas, agent.as_deref()))?)
        }
        Canvas::Redo { canvas, agent } => {
            change(&daemon.canvas_redo(step(canvas, agent.as_deref()))?)
        }
        Canvas::Import { file, name } => {
            let imported = daemon.canvas_import(file, name.as_deref())?;
            Ok(Output::new(serde_json::to_value(&imported)?, imported_lines(&imported)))
        }
    }
}

/// Who asks: the user, or the agent named.
fn by(agent: Option<&str>) -> String {
    agent.map_or_else(|| "user".into(), |name| format!("agent:{name}"))
}

fn step(canvas: &str, agent: Option<&str>) -> control::CanvasStep {
    control::CanvasStep { canvas: canvas.to_string(), by: by(agent) }
}

/// What a change wrote: for people, the ids it made, one per line.
fn change(change: &control::CanvasChange) -> anyhow::Result<Output> {
    Ok(Output::new(serde_json::to_value(change)?, change.made.join("\n")))
}

/// One item for people: id, kind and maker, then the item as JSON, by tabs.
fn item_line(item: &Value) -> String {
    let field = |key: &str| item[key].as_str().unwrap_or_default().to_string();
    format!("{}\t{}\t{}\t{item}", field("id"), field("kind"), field("by"))
}

/// An import for people: the canvas id, then one count per line, tab
/// separated: `pages`, each item kind, then `skipped` and each shape type.
fn imported_lines(imported: &control::CanvasImported) -> String {
    let mut lines = vec![imported.canvas.clone(), format!("pages\t{}", imported.pages)];
    lines.extend(imported.items.iter().map(|(kind, n)| format!("{kind}\t{n}")));
    lines.extend(imported.skipped.iter().map(|(kind, n)| format!("skipped\t{kind}\t{n}")));
    lines.join("\n")
}
