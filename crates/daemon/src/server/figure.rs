//! `canvas_draw` as it crosses the socket (docs/design/figures.md, "The
//! verbs"): the daemon reads the script and its data, runs the script in
//! `slinger_script` (outside the canvas's lock), and the core places what
//! it drew as one figure. A draw that wrote appends `figure_drawn` (the
//! script as a blob, the data whole) and, for an agent slung something on
//! the canvas, `reply_landed`. A dry run writes nothing at all.

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use slinger_control::{CanvasDraw, CanvasDrawn, Place, RemoteError};
use slinger_core::canvas::Author;
use slinger_core::canvas::file::{Draft, Placement, parse_author};
use slinger_core::record::{Body, Event, FigureDrawn, Tags};
use slinger_core::render;
use slinger_script::{AGENT, View};

use super::{absolute, agent, drawn, look, remote, stored, unreadable};
use crate::store::Store;

pub(super) fn draw(store: &Store, ask: CanvasDraw) -> Result<CanvasDrawn, RemoteError> {
    let CanvasDraw { canvas, by: who, name, path, data, place, page, dry_run, png: out } = ask;
    let by = parse_author(&who).map_err(|e| remote(&e))?;
    let at = placement(place, page);
    let script = read_script(&path)?;
    let data = read_data(data.as_deref())?;
    let canvases = store.canvases();
    let (page, target) = canvases.figure_view(&canvas, &at).map_err(|e| drawn(&e))?;
    let view = View {
        name: name.clone(),
        target: target.map(|t| t.map(units)),
        agent: AGENT.into(),
        page,
    };
    let shapes = slinger_script::run(&script, &data, &view).map_err(|e| {
        let hint =
            "fix the script there and try it with --dry-run; 'slinger guide' has the drawing API";
        RemoteError::new("invalid", e.to_string(), Some(hint.into()))
    })?;
    let warnings: Vec<String> = shapes.warnings.iter().map(ToString::to_string).collect();
    let (canvas, name, items) = (canvas.as_str(), name.as_str(), &shapes.items);
    if dry_run {
        let draft = canvases.draft_figure(canvas, &who, name, items, &at).map_err(|e| drawn(&e))?;
        let images = png(out.as_deref(), &draft)?;
        return Ok(reply(draft, Vec::new(), warnings, images, None));
    }
    let blob = store.put_blob(script.as_bytes()).map_err(|e| stored(&e))?;
    let done = canvases.draw(canvas, &who, name, items, &at).map_err(|e| drawn(&e))?;
    let images = png(out.as_deref(), &done.draft)?;
    let draft = done.draft;
    if draft.unchanged() {
        return Ok(reply(draft, Vec::new(), warnings, images, None));
    }
    let maker = match &by {
        Author::Agent(name) => Some(name.clone()),
        Author::User => None,
    };
    let body = Body::FigureDrawn(FigureDrawn {
        canvas: canvas.into(),
        figure: draft.name.clone(),
        page: draft.page.clone(),
        script: blob,
        data,
        origin: draft.origin,
        frame: draft.frame,
    });
    let tags = Tags { agent: maker.clone(), canvas: Some(canvas.into()), ..Tags::on("") };
    let source = maker.clone().unwrap_or_else(|| "user".into());
    store.append(Event::new(0, source, tags, body)).map_err(|e| stored(&e))?;
    let event = match &maker {
        Some(agent) => {
            let ids = draft.items.iter().map(|i| format!("i{}", i.id().get())).collect();
            agent::landed(store, canvas, agent, ids)?
        }
        None => None,
    };
    Ok(reply(draft, done.written.seq, warnings, images, event))
}

fn placement(place: Place, page: Option<String>) -> Placement {
    match place {
        Place::Beside(id) => Placement::Beside(id),
        Place::Over(id) => Placement::Over(id),
        Place::In(id) => Placement::In(id),
        Place::At([x, y]) => Placement::At { page, x, y },
    }
}

/// The script's text, read from its file.
fn read_script(path: &Path) -> Result<String, RemoteError> {
    let bytes = fs::read(absolute(path)?).map_err(|e| unreadable(path, &e))?;
    String::from_utf8(bytes).map_err(|_| {
        let what = format!("script {} is not UTF-8 text", path.display());
        RemoteError::new("invalid", what, Some("save the script as UTF-8".into()))
    })
}

/// The script's data, read from its file; none is `{}`.
fn read_data(path: Option<&Path>) -> Result<Value, RemoteError> {
    let Some(path) = path else { return Ok(json!({})) };
    let bytes = fs::read(absolute(path)?).map_err(|e| unreadable(path, &e))?;
    serde_json::from_slice(&bytes).map_err(|e| {
        let what = format!("data {} is not JSON: {e}", path.display());
        RemoteError::new("invalid", what, Some("pass a JSON object, such as {\"n\": 3}".into()))
    })
}

/// The images of the figure alone that `path` asks for, if it does.
fn png(
    path: Option<&Path>,
    draft: &Draft,
) -> Result<Vec<slinger_control::CanvasImage>, RemoteError> {
    path.map_or_else(
        || Ok(Vec::new()),
        |path| look::written(path, |stem| Ok(render::shapes(&draft.items, stem))),
    )
}

fn reply(
    draft: Draft,
    seq: Vec<u64>,
    warnings: Vec<String>,
    images: Vec<slinger_control::CanvasImage>,
    event: Option<u64>,
) -> CanvasDrawn {
    let Draft { name, page, origin, frame, parts, items, .. } = draft;
    let items = items.len();
    CanvasDrawn { name, page, origin, frame, parts, items, seq, warnings, images, event }
}

/// Tenths as canvas units, as a script sees them.
fn units(tenths: i64) -> f64 {
    f64::from(i32::try_from(tenths).unwrap_or(i32::MAX)) / 10.0
}
