//! `slinger canvas ...`: canvases the daemon keeps. One arm per command, each
//! one request; the daemon owns the files and every rule.

use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use clap::Subcommand;
use serde_json::{Value, json};

use slinger_control as control;

use crate::env::Env;
use crate::output::Output;
use crate::session::AGENT_OR_USER_HELP;

/// Help for `canvas apply`'s operations: a string, because a doc comment wants the JSON bare.
const OPS_HELP: &str = "One operation as a JSON object, or a list of them; - reads standard \
     input. Example: '{\"op\":\"add\",\"item\":{\"page\":\"p1\",\"kind\":\"box\",\"x\":0,\"y\":0,\
     \"w\":100,\"h\":50}}' (lengths in tenths of a unit). 'slinger canvas schema' lists the \
     item kinds and operations";

#[derive(Subcommand)]
pub enum Canvas {
    /// Make a new canvas with one page: empty, or made from a file of operations
    Create {
        /// The canvas id: a-z, 0-9, '-', '_' and '.'
        canvas: String,
        /// A file holding a JSON list of operations, as 'canvas apply' takes, made in one
        /// step as the agent's (else the user's): the whole canvas or none
        #[arg(long, value_name = "FILE")]
        ops: Option<PathBuf>,
        #[arg(long, value_name = "NAME", help = AGENT_OR_USER_HELP)]
        agent: Option<String>,
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
    #[command(after_long_help = crate::schema::SCHEMA)]
    Apply {
        canvas: String,
        #[arg(help = OPS_HELP)]
        ops: String,
        #[arg(long, value_name = "NAME", help = AGENT_OR_USER_HELP)]
        agent: Option<String>,
    },
    /// Print the item kinds and operations 'canvas apply' takes: section, name and fields per line
    Schema,
    /// Undo your last step on the canvas
    Undo {
        canvas: String,
        #[arg(long, value_name = "NAME", help = AGENT_OR_USER_HELP)]
        agent: Option<String>,
    },
    /// Redo the last step you undid on the canvas
    Redo {
        canvas: String,
        #[arg(long, value_name = "NAME", help = AGENT_OR_USER_HELP)]
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
    /// Print a page's packet: what an agent reads instead of a screenshot
    Read {
        canvas: String,
        /// The page, by id (p1) or name [default: the first page]
        #[arg(long)]
        page: Option<String>,
        /// Operations numbered above this are new [default: the canvas's last sling]
        #[arg(long, value_name = "NUMBER")]
        after: Option<u64>,
        /// Keep only these items in the page outline: ids such as c42,t12
        #[arg(long, value_name = "IDS", value_delimiter = ',')]
        select: Vec<String>,
        /// Write the packet and its images into this directory [default: the daemon's]
        #[arg(long, value_name = "DIR")]
        out: Option<PathBuf>,
    },
    /// Print the operations after a number, one JSON line each, as the canvas file holds them
    Changes {
        canvas: String,
        /// Only operations numbered above this ('canvas show --json' gives the next one)
        #[arg(long, value_name = "NUMBER", default_value_t = 0)]
        after: u64,
        /// When there are none, wait up to this many milliseconds (at most 60000) for one
        #[arg(long, value_name = "MS", default_value_t = 0)]
        wait: u64,
    },
    Crop(crate::look::Crop),
    Strokes(crate::look::Strokes),
    Put(crate::put::Put),
    Edit(crate::edit::Edit),
    Remove(crate::edit::Remove),
}

impl Canvas {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Create { .. } => "canvas create",
            Self::List => "canvas list",
            Self::Show { .. } => "canvas show",
            Self::Apply { .. } => "canvas apply",
            Self::Schema => "canvas schema",
            Self::Undo { .. } => "canvas undo",
            Self::Redo { .. } => "canvas redo",
            Self::Import { .. } => "canvas import",
            Self::Read { .. } => "canvas read",
            Self::Changes { .. } => "canvas changes",
            Self::Crop(_) => "canvas crop",
            Self::Strokes(_) => "canvas strokes",
            Self::Put(_) => "canvas put",
            Self::Edit(_) => "canvas edit",
            Self::Remove(_) => "canvas remove",
        }
    }
}

pub fn execute(
    command: &Canvas,
    daemon: &control::Client,
    input: &mut dyn Read,
    env: &Env,
) -> anyhow::Result<Output> {
    match command {
        Canvas::Create { canvas, ops, agent } => {
            let by = by(env, agent.as_deref());
            match ops {
                Some(file) => daemon.canvas_create_with(canvas, file, Some(&by))?,
                None => daemon.canvas_create(canvas)?,
            }
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
            let request = control::CanvasOps {
                canvas: canvas.clone(),
                by: by(env, agent.as_deref()),
                ops,
                once: None,
            };
            change(&daemon.canvas_apply(request)?)
        }
        Canvas::Schema => Ok(crate::schema::schema()),
        Canvas::Undo { canvas, agent } => {
            change(&daemon.canvas_undo(step(canvas, env, agent.as_deref()))?)
        }
        Canvas::Redo { canvas, agent } => {
            change(&daemon.canvas_redo(step(canvas, env, agent.as_deref()))?)
        }
        Canvas::Import { file, name } => {
            let imported = daemon.canvas_import(file, name.as_deref())?;
            Ok(Output::new(serde_json::to_value(&imported)?, imported_lines(&imported)))
        }
        Canvas::Read { canvas, page, after, select, out } => {
            let ask = control::PacketAsk {
                canvas: canvas.clone(),
                page: page.clone(),
                after: *after,
                select: select.clone(),
                out: out.clone(),
            };
            let packet = daemon.canvas_read(ask)?;
            let text = packet.text.trim_end().to_string();
            Ok(Output::new(serde_json::to_value(&packet)?, text))
        }
        Canvas::Changes { canvas, after, wait } => {
            let feed = daemon.canvas_changes(canvas, *after, Duration::from_millis(*wait))?;
            let human = feed.ops.iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
            Ok(Output::new(serde_json::to_value(feed)?, human))
        }
        Canvas::Crop(args) => crate::look::crop(args, daemon),
        Canvas::Strokes(args) => crate::look::strokes(args, daemon),
        Canvas::Put(args) => crate::put::put(args, daemon, env),
        Canvas::Edit(args) => crate::edit::edit(args, daemon, env),
        Canvas::Remove(args) => crate::edit::remove(args, daemon, env),
    }
}

/// Who asks: the agent named by `--agent` or `$SLINGER_AGENT`, else the user,
/// so an agent's process acts as that agent and its steps stay off the user's
/// undo history.
fn by(env: &Env, agent: Option<&str>) -> String {
    env.named_agent(agent).map_or_else(|| "user".into(), |name| format!("agent:{name}"))
}

fn step(canvas: &str, env: &Env, agent: Option<&str>) -> control::CanvasStep {
    control::CanvasStep { canvas: canvas.to_string(), by: by(env, agent) }
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
