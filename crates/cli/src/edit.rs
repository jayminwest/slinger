//! `slinger canvas edit` and `slinger canvas remove`: an agent changes or
//! takes back what it put. The daemon decides what the agent may change.

use clap::{ArgGroup, Args};

use slinger_control as control;

use crate::output::Output;
use crate::session::{AGENT_HELP, calling_agent};

/// Change an item the agent made: its text, its text size, its size or its place; prints its id and box
#[derive(Args)]
#[command(group(ArgGroup::new("change").required(true).multiple(true).args(["text", "text_size", "size", "to"])))]
pub struct Edit {
    canvas: String,
    /// The item to change: an id from 'canvas show' (i12) or from a packet (t12)
    #[arg(long, value_name = "ID")]
    item: String,
    /// New words for a text item
    #[arg(long, value_name = "TEXT")]
    text: Option<String>,
    /// A new size for a text item's characters, in tenths like --box and --move: 40 to 4000 (default 240); the frame refits the words
    #[arg(long = "size", value_name = "TENTHS")]
    text_size: Option<i64>,
    /// A new size for a text or a box, w by h in tenths of a unit
    #[arg(long = "box", value_name = "W,H", value_parser = crate::put::size)]
    size: Option<[i64; 2]>,
    /// Move the item's top-left corner to x,y in tenths of a unit
    #[arg(long = "move", value_name = "X,Y", value_parser = crate::put::point)]
    to: Option<[i64; 2]>,
    #[arg(long, value_name = "NAME", help = AGENT_HELP)]
    agent: Option<String>,
}

/// Remove an item the agent made; one already gone is fine
#[derive(Args)]
pub struct Remove {
    canvas: String,
    /// The item to remove: an id from 'canvas show' (i12) or from a packet (t12)
    #[arg(long, value_name = "ID")]
    item: String,
    #[arg(long, value_name = "NAME", help = AGENT_HELP)]
    agent: Option<String>,
}

pub fn edit(args: &Edit, daemon: &control::Client) -> anyhow::Result<Output> {
    let edited = daemon.canvas_edit(control::CanvasEdit {
        canvas: args.canvas.clone(),
        agent: calling_agent(args.agent.as_deref())?,
        item: args.item.clone(),
        text: args.text.clone(),
        text_size: args.text_size,
        size: args.size,
        to: args.to,
    })?;
    let human = format!("{}\t{},{},{},{}", edited.item, edited.x, edited.y, edited.w, edited.h);
    Ok(Output::new(serde_json::to_value(edited)?, human))
}

pub fn remove(args: &Remove, daemon: &control::Client) -> anyhow::Result<Output> {
    let agent = calling_agent(args.agent.as_deref())?;
    let removed = daemon.canvas_remove(&args.canvas, &agent, &args.item)?;
    Ok(Output::new(serde_json::to_value(removed)?, String::new()))
}
