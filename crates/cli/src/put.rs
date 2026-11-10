//! `slinger canvas put`: an agent answers on the canvas, beside an item. One
//! request; the daemon finds the free space and keeps the record.

use clap::{ArgGroup, Args};

use slinger_control as control;

use crate::output::Output;
use crate::session::{AGENT_HELP, calling_agent};

/// Put text, a box or an arrow beside an item, where it covers nothing; prints its id and box
#[derive(Args)]
#[command(group(ArgGroup::new("what").required(true).args(["text", "size", "arrow"])))]
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
    /// Put an arrow from one item to another, on the page of the item it goes beside
    #[arg(long, num_args = 2, value_names = ["FROM", "TO"])]
    arrow: Option<Vec<String>>,
    #[arg(long, value_name = "NAME", help = AGENT_HELP)]
    agent: Option<String>,
}

pub fn put(args: &Put, daemon: &control::Client) -> anyhow::Result<Output> {
    let agent = calling_agent(args.agent.as_deref())?;
    // clap makes sure exactly one of `--text`, `--box` and `--arrow` is here.
    let what = match (&args.text, args.size, args.arrow.as_deref()) {
        (Some(text), ..) => control::Put::Text(text.clone()),
        (None, Some(size), _) => control::Put::Box(size),
        (None, None, Some([from, to])) => control::Put::Arrow([from.clone(), to.clone()]),
        (None, None, _) => control::Put::Text(String::new()),
    };
    let placed = daemon.canvas_put(&args.canvas, &agent, &args.beside, what)?;
    let human = format!("{}\t{},{},{},{}", placed.item, placed.x, placed.y, placed.w, placed.h);
    Ok(Output::new(serde_json::to_value(placed)?, human))
}

/// `w,h` as two integers.
fn size(text: &str) -> Result<[i64; 2], String> {
    let parts: Vec<i64> = text.split(',').map_while(|n| n.trim().parse().ok()).collect();
    <[i64; 2]>::try_from(parts).map_err(|_| format!("{text:?} is not w,h in tenths of a unit"))
}
