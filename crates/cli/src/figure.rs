//! `slinger canvas figure`: the figures on a canvas, or one figure's
//! parts, script or data (docs/design/figures.md, "`canvas figure`").

use clap::{ArgGroup, Args};

use slinger_control as control;

use crate::cli::Scratch;
use crate::output::Output;

/// List the canvas's figures, one per line (name, page, x,y wxh in canvas units, maker, parts); with a name, that figure's parts, its script or its data
#[derive(Args)]
#[command(group(ArgGroup::new("print").args(["script", "data"]).requires("name")))]
pub struct Figure {
    canvas: String,
    /// A figure's name: print its parts, one per line (name, x,y wxh, items)
    name: Option<String>,
    /// Print the figure's script
    #[arg(long)]
    script: bool,
    /// Print the data the figure's script last ran on, as JSON
    #[arg(long)]
    data: bool,
}

pub fn figure(args: &Figure, daemon: &control::Client) -> anyhow::Result<Output> {
    let shown = daemon.canvas_figure(&args.canvas, args.name.as_deref())?;
    let value = serde_json::to_value(&shown)?;
    let Some(one) = args.name.as_ref().and(shown.figures.first()) else {
        return Ok(Output::new(
            value,
            shown.figures.iter().map(line).collect::<Vec<_>>().join("\n"),
        ));
    };
    let drawn = |what: &str| {
        let message = format!("figure {} has no {what}: no script drew it", one.name);
        let hint = "only a figure 'slinger canvas draw' drew has a script and data";
        control::RemoteError::new("not_found", message, Some(hint.into()))
    };
    let human = if args.script {
        let id = one.script.as_deref().ok_or_else(|| drawn("script"))?;
        let (scratch, _) = Scratch::new()?;
        daemon.blob_get(id, &scratch.0)?;
        let text = std::fs::read_to_string(&scratch.0)?;
        text.trim_end_matches('\n').to_string()
    } else if args.data {
        one.data.as_ref().ok_or_else(|| drawn("data"))?.to_string()
    } else {
        one.parts
            .iter()
            .map(|p| format!("{} {} items={}", p.name, place(p.frame), p.items))
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(Output::new(value, human))
}

/// A figure for people: `name page x,y wxh by=<maker> parts=<n>`.
fn line(figure: &control::FigureShown) -> String {
    let by = figure.by.strip_prefix("agent:").unwrap_or(&figure.by);
    let (name, page, parts) = (&figure.name, &figure.page, figure.parts.len());
    format!("{name} {page} {} by={by} parts={parts}", place(figure.frame))
}

/// `x,y wxh` in canvas units, the packet's, from tenths.
fn place([x, y, w, h]: [i64; 4]) -> String {
    let units = |t: i64| f64::from(i32::try_from(t).unwrap_or(i32::MAX)) / 10.0;
    format!("{},{} {}x{}", units(x), units(y), units(w), units(h))
}
