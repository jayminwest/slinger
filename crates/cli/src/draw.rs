//! `slinger canvas draw` and `slinger canvas redraw`: an agent draws with
//! code. One request each: the daemon reads the script, runs it in its
//! bounded engine and places what it draws as one figure
//! (docs/design/figures.md, "The verbs"). A redraw patches the figure's data.

use std::io::Write;
use std::path::PathBuf;

use clap::{ArgGroup, Args};

use slinger_control as control;

use crate::cli::Scratch;
use crate::env::Env;
use crate::output::Output;
use crate::session::AGENT_OR_USER_HELP;

/// Run a drawing script (JavaScript; 'slinger guide' has its API) and place what it draws as one named figure, as one undo step; prints its warnings
#[derive(Args)]
#[command(group(ArgGroup::new("place").required(true).args(["beside", "over", "inside", "at"])))]
pub struct Draw {
    canvas: String,
    /// The script's file
    file: PathBuf,
    /// The figure's name: a-z, 0-9 and '-'. Drawing a name again replaces that figure in place
    #[arg(long)]
    name: String,
    /// Put the figure beside this item (i12, c12, b4, figure/part), where it covers nothing
    #[arg(long, value_name = "ID")]
    beside: Option<String>,
    /// Put the figure's 0,0 on this item's top left, to draw over it: the script sees its size as fig.view.target
    #[arg(long, value_name = "ID")]
    over: Option<String>,
    /// Put the figure in this box's room, where it covers nothing the box holds
    #[arg(long = "in", value_name = "BOX")]
    inside: Option<String>,
    /// Put the figure's 0,0 here, in tenths of a unit like 'canvas put --at', on the first page or on PAGE:X,Y
    #[arg(long, value_name = "X,Y", value_parser = at, allow_hyphen_values = true)]
    at: Option<At>,
    /// The script's data, as fig.data: JSON, or @FILE to read it from a file [default: {}]
    #[arg(long, value_name = "JSON|@FILE")]
    data: Option<String>,
    /// Write nothing: print what the draw would do (with --json: the frame and the parts)
    #[arg(long)]
    dry_run: bool,
    /// Write a PNG of the figure alone to this file
    #[arg(long, value_name = "PATH")]
    png: Option<PathBuf>,
    #[arg(long, value_name = "NAME", help = AGENT_OR_USER_HELP)]
    agent: Option<String>,
}

/// Draw a figure again from its script on its last data patched (a JSON merge patch: objects merge key by key, null takes a key away, anything else replaces), where it now is; only what changed is written, and the same data again writes nothing; prints the script's warnings
#[derive(Args)]
#[command(group(ArgGroup::new("change").required(true).multiple(true).args(["data", "script"])))]
pub struct Redraw {
    canvas: String,
    /// The figure's name
    name: String,
    /// The patch on the figure's data: JSON, or @FILE to read it from a file
    #[arg(long, value_name = "JSON|@FILE")]
    data: Option<String>,
    /// Swap the figure's script for this file
    #[arg(long, value_name = "FILE")]
    script: Option<PathBuf>,
    #[arg(long, value_name = "NAME", help = AGENT_OR_USER_HELP)]
    agent: Option<String>,
}

/// An `--at` value: a page, maybe, and a point in tenths.
#[derive(Clone)]
struct At {
    page: Option<String>,
    tenths: [i64; 2],
}

pub fn draw(args: &Draw, daemon: &control::Client, env: &Env) -> anyhow::Result<Output> {
    // clap makes sure exactly one of `--beside`, `--over`, `--in` and `--at` is here.
    let (place, page) = match (&args.beside, &args.over, &args.inside, &args.at) {
        (Some(id), ..) => (control::Place::Beside(id.clone()), None),
        (None, Some(id), ..) => (control::Place::Over(id.clone()), None),
        (None, None, Some(id), _) => (control::Place::In(id.clone()), None),
        (None, None, None, Some(at)) => (control::Place::At(at.tenths), at.page.clone()),
        (None, None, None, None) => (control::Place::At([0, 0]), None),
    };
    let (data, _scratch) = data(args.data.as_deref())?;
    let drawn = daemon.canvas_draw(control::CanvasDraw {
        canvas: args.canvas.clone(),
        by: crate::canvas::by(env, args.agent.as_deref()),
        name: args.name.clone(),
        path: args.file.clone(),
        data,
        place,
        page,
        dry_run: args.dry_run,
        png: args.png.clone(),
        once: None,
    })?;
    drawn_lines(&drawn)
}

pub fn redraw(args: &Redraw, daemon: &control::Client, env: &Env) -> anyhow::Result<Output> {
    let (data, _scratch) = data(args.data.as_deref())?;
    let drawn = daemon.canvas_redraw(control::CanvasRedraw {
        canvas: args.canvas.clone(),
        by: crate::canvas::by(env, args.agent.as_deref()),
        name: args.name.clone(),
        data,
        path: args.script.clone(),
        once: None,
    })?;
    drawn_lines(&drawn)
}

/// A draw for people: each image's path, then each warning.
fn drawn_lines(drawn: &control::CanvasDrawn) -> anyhow::Result<Output> {
    let mut lines: Vec<String> =
        drawn.images.iter().map(|i| i.path.display().to_string()).collect();
    lines.extend(drawn.warnings.iter().map(|w| format!("warning: {w}")));
    Ok(Output::new(serde_json::to_value(drawn)?, lines.join("\n")))
}

/// The file `--data` names: `@FILE`'s, or a scratch file holding the JSON
/// given, kept until the reply. Data crosses as a file, as a script does.
fn data(arg: Option<&str>) -> anyhow::Result<(Option<PathBuf>, Option<Scratch>)> {
    let Some(text) = arg else { return Ok((None, None)) };
    if let Some(path) = text.strip_prefix('@') {
        return Ok((Some(PathBuf::from(path)), None));
    }
    serde_json::from_str::<serde_json::Value>(text).map_err(|e| {
        let hint = "pass JSON, such as '{\"n\": 3}', or @FILE";
        control::RemoteError::new("usage", format!("--data is not JSON: {e}"), Some(hint.into()))
    })?;
    let (scratch, mut file) = Scratch::new()?;
    file.write_all(text.as_bytes())?;
    Ok((Some(scratch.0.clone()), Some(scratch)))
}

/// `X,Y` or `PAGE:X,Y` in tenths of a unit, as `canvas put --at` takes.
fn at(text: &str) -> Result<At, String> {
    let (page, point) = match text.rsplit_once(':') {
        Some((page, point)) if !page.is_empty() => (Some(page.to_string()), point),
        Some(_) => return Err(format!("{text:?} is not X,Y or PAGE:X,Y in tenths of a unit")),
        None => (None, text),
    };
    Ok(At { page, tenths: crate::put::point(point)? })
}
