//! `slinger canvas put`: an agent answers on the canvas, beside an item. One
//! request; the daemon finds the free space and keeps the record.

use clap::{ArgGroup, Args};

use slinger_control as control;

use crate::env::Env;
use crate::output::Output;
use crate::session::AGENT_HELP;

/// Put text, a box, ink or an arrow beside an item, where it covers nothing; prints its id and box
#[derive(Args)]
#[command(group(ArgGroup::new("what").required(true).args(["text", "size", "ink", "arrow"])))]
pub struct Put {
    canvas: String,
    /// The item to put it beside: an id from 'canvas show' (i12) or from a packet (c12, t12)
    #[arg(long, value_name = "ID")]
    beside: String,
    /// Put this text
    #[arg(long, value_name = "TEXT")]
    text: Option<String>,
    /// Put an empty box, w by h in tenths of a unit
    #[arg(long = "box", value_name = "W,H", value_parser = size)]
    size: Option<[i64; 2]>,
    /// Put a pen stroke: x,y pairs in tenths, the first where it starts, each next the step from the one before
    #[arg(long, value_name = "PTS", value_parser = pts, allow_hyphen_values = true)]
    ink: Option<Pts>,
    /// Put an arrow from one item to another, on the page of the item it goes beside
    #[arg(long, num_args = 2, value_names = ["FROM", "TO"])]
    arrow: Option<Vec<String>>,
    /// Draw it in this colour: #rrggbb, or #rrggbbaa with alpha (default: the colour of the user's writing beside it, else black)
    #[arg(long, value_name = "COLOR")]
    color: Option<String>,
    /// Put the top-left corner of its box here, in tenths, not beside the item; not for an arrow
    #[arg(long, value_name = "X,Y", value_parser = point, conflicts_with = "arrow", allow_hyphen_values = true)]
    at: Option<[i64; 2]>,
    /// Size the text's characters, in tenths like --at and --box: 40 to 4000 (default: the size of the user's writing beside it, else 240); only with --text
    #[arg(long = "size", value_name = "TENTHS")]
    text_size: Option<i64>,
    #[arg(long, value_name = "NAME", help = AGENT_HELP)]
    agent: Option<String>,
}

pub fn put(args: &Put, daemon: &control::Client, env: &Env) -> anyhow::Result<Output> {
    let agent = env.calling_agent(args.agent.as_deref())?;
    // clap makes sure exactly one of `--text`, `--box`, `--ink` and `--arrow` is here.
    let what = match (&args.text, args.size, &args.ink, args.arrow.as_deref()) {
        (Some(text), ..) => control::Put::Text(text.clone()),
        (None, Some(size), ..) => control::Put::Box(size),
        (None, None, Some(Pts(pts)), _) => control::Put::Ink(pts.clone()),
        (None, None, None, Some([from, to])) => control::Put::Arrow([from.clone(), to.clone()]),
        (None, None, None, _) => control::Put::Text(String::new()),
    };
    let placed = daemon.canvas_put_as(control::CanvasPut {
        canvas: args.canvas.clone(),
        agent,
        beside: args.beside.clone(),
        what,
        color: args.color.clone(),
        at: args.at,
        text_size: args.text_size,
    })?;
    let human = format!("{}\t{},{},{},{}", placed.item, placed.x, placed.y, placed.w, placed.h);
    Ok(Output::new(serde_json::to_value(placed)?, human))
}

/// `w,h` as two integers.
pub fn size(text: &str) -> Result<[i64; 2], String> {
    let parts: Vec<i64> = text.split(',').map_while(|n| n.trim().parse().ok()).collect();
    <[i64; 2]>::try_from(parts).map_err(|_| format!("{text:?} is not w,h in tenths of a unit"))
}

/// `x,y` as two integers.
pub fn point(text: &str) -> Result<[i64; 2], String> {
    size(text).map_err(|_| format!("{text:?} is not x,y in tenths of a unit"))
}

/// A `--ink` value: one argument, many numbers.
#[derive(Clone)]
struct Pts(Vec<i64>);

/// `x,y,x,y,…` as pairs of integers.
fn pts(text: &str) -> Result<Pts, String> {
    let bad = || format!("{text:?} is not x,y pairs of integers, such as 0,0,400,0");
    let parts: Vec<i64> =
        text.split(',').map(|n| n.trim().parse()).collect::<Result<_, _>>().map_err(|_| bad())?;
    if parts.is_empty() || !parts.len().is_multiple_of(2) {
        return Err(bad());
    }
    Ok(Pts(parts))
}
