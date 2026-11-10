//! `slinger canvas crop` and `slinger canvas strokes`: what an agent asks a
//! canvas for when the packet is not enough. One request each.

use std::path::PathBuf;

use clap::{ArgGroup, Args};
use serde_json::json;

use slinger_control as control;

use crate::output::Output;

/// Write an image of one item or one box, at the packet's scale; prints the path of each image
#[derive(Args)]
#[command(group(ArgGroup::new("of").required(true).args(["item", "area"])))]
pub struct Crop {
    canvas: String,
    /// The item: an id from 'canvas show' (i12) or from a packet (c12 is a whole cluster)
    #[arg(long, value_name = "ID")]
    item: Option<String>,
    /// The box: x,y,w,h in tenths of a unit
    #[arg(long = "box", value_name = "X,Y,W,H", allow_hyphen_values = true)]
    area: Option<String>,
    /// The box's page, by id (p1) or name [default: the first page]
    #[arg(long, conflicts_with = "item")]
    page: Option<String>,
    /// The image file; tiles go beside it as <stem>-<n>.png [default: <ID>.png or crop.png]
    #[arg(long, value_name = "FILE")]
    out: Option<PathBuf>,
}

/// Print the raw points of a stroke, or of every stroke of a cluster: one stroke as JSON per line
#[derive(Args)]
pub struct Strokes {
    canvas: String,
    /// A stroke (i12) or a cluster (c12)
    #[arg(long, value_name = "ID")]
    item: String,
}

pub fn crop(args: &Crop, daemon: &control::Client) -> anyhow::Result<Output> {
    // clap makes sure exactly one of `--item` and `--box` is here.
    let area = args.area.clone().unwrap_or_default();
    let of = match (&args.item, &args.page) {
        (Some(item), _) => item.clone(),
        (None, Some(page)) => format!("{page}:{area}"),
        (None, None) => area,
    };
    let name = args.item.as_deref().unwrap_or("crop");
    let file = args.out.clone().unwrap_or_else(|| PathBuf::from(format!("{name}.png")));
    let images = daemon.canvas_crop(&args.canvas, &of, &file)?;
    let human = images.iter().map(|i| i.path.display().to_string()).collect::<Vec<_>>();
    Ok(Output::new(json!({ "images": images }), human.join("\n")))
}

pub fn strokes(args: &Strokes, daemon: &control::Client) -> anyhow::Result<Output> {
    let strokes = daemon.canvas_strokes(&args.canvas, &args.item)?;
    let human = strokes.iter().map(ToString::to_string).collect::<Vec<_>>();
    Ok(Output::new(json!({ "strokes": strokes }), human.join("\n")))
}
