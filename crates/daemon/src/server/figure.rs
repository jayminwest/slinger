//! The figure verbs as they cross the socket (docs/design/figures.md, "The
//! verbs"): `canvas_draw` and `canvas_redraw` run a script in
//! `slinger_script` (outside the canvas's lock) and the core places what it
//! drew as one figure; `canvas_figure` lists the figures. A draw that wrote
//! appends `figure_drawn` (the script as a blob, the data whole) and, for
//! an agent slung something on the canvas, `reply_landed`. The canvas file
//! keeps each figure's last script and data (its `figure` line): a redraw
//! starts from it, so a canvas file moved elsewhere still redraws, and the
//! record's events are its history. A dry run writes nothing at all.

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use slinger_control::{
    CanvasDraw, CanvasDrawn, CanvasFigureAsk, CanvasFigures, CanvasRedraw, FigureShown, PartShown,
    Place, RemoteError,
};
use slinger_core::canvas::file::{
    Draft, Placement, Script, merge_patch, parse_author, rect_tenths,
};
use slinger_core::canvas::{Author, Figure};
use slinger_core::record::{Body, Event, FigureDrawn, Tags};
use slinger_core::render;
use slinger_script::{AGENT, View};

use super::{absolute, agent, drawn, look, remote, stored, unreadable};
use crate::store::Store;

pub(super) fn draw(store: &Store, ask: CanvasDraw) -> Result<CanvasDrawn, RemoteError> {
    let CanvasDraw { canvas, by: who, name, path, data, place, page, dry_run, png: out, once } =
        ask;
    parse_author(&who).map_err(|e| remote(&e))?;
    let at = placement(place, page);
    let script = read_script(&path)?;
    let data = data.as_deref().map_or_else(|| Ok(json!({})), read_data)?;
    let canvases = store.canvases();
    let (page, target) = canvases.figure_view(&canvas, &at).map_err(|e| drawn(&e))?;
    let shapes = run(&script, &data, &name, page, target)?;
    let warnings: Vec<String> = shapes.warnings.iter().map(ToString::to_string).collect();
    if dry_run {
        let draft = canvases
            .draft_figure(&canvas, &who, &name, &shapes.items, &at)
            .map_err(|e| drawn(&e))?;
        let images = png(out.as_deref(), &draft)?;
        return Ok(reply(draft, Vec::new(), warnings, images, None));
    }
    let figure = Drawing { canvas: &canvas, who: &who, name: &name, script: &script, data, target };
    let (mut reply, draft) = land(store, &figure, &shapes.items, &at, once.as_deref())?;
    reply.warnings = warnings;
    reply.images = png(out.as_deref(), &draft)?;
    Ok(reply)
}

pub(super) fn redraw(store: &Store, ask: CanvasRedraw) -> Result<CanvasDrawn, RemoteError> {
    let CanvasRedraw { canvas, by: who, name, data: patch, path: swap, once } = ask;
    parse_author(&who).map_err(|e| remote(&e))?;
    let canvases = store.canvases();
    let again = canvases.again(&canvas, &who, &name).map_err(|e| drawn(&e))?;
    let last = again.source.script.clone();
    let mut data = last.data.clone();
    if let Some(patch) = patch.as_deref() {
        merge_patch(&mut data, &read_data(patch)?);
    }
    let script = match swap.as_deref() {
        Some(file) => read_script(file)?,
        None => last.text.clone(),
    };
    if script == last.text && data == last.data {
        let (page, origin, frame, parts, items) =
            (again.page, again.origin, again.frame, again.parts, again.items);
        let (seq, warnings, images, event) = (Vec::new(), Vec::new(), Vec::new(), None);
        return Ok(CanvasDrawn {
            name,
            page,
            origin,
            frame,
            parts,
            items,
            seq,
            warnings,
            images,
            event,
        });
    }
    let (page, _) = canvases.figure_view(&canvas, &again.at).map_err(|e| drawn(&e))?;
    let shapes = run(&script, &data, &name, page, last.target)?;
    let target = last.target;
    let figure = Drawing { canvas: &canvas, who: &who, name: &name, script: &script, data, target };
    let (mut reply, _) = land(store, &figure, &shapes.items, &again.at, once.as_deref())?;
    reply.warnings = shapes.warnings.iter().map(ToString::to_string).collect();
    Ok(reply)
}

pub(super) fn figures(store: &Store, ask: CanvasFigureAsk) -> Result<CanvasFigures, RemoteError> {
    let CanvasFigureAsk { canvas, name } = ask;
    let all = store.canvases().figures(&canvas).map_err(|e| drawn(&e))?;
    let Some(name) = name else {
        return Ok(CanvasFigures { figures: all.iter().map(shown).collect() });
    };
    let figure = all.iter().find(|f| f.name == name).ok_or_else(|| {
        let hint = format!("list the figures with 'slinger canvas figure {canvas}'");
        RemoteError::new("not_found", format!("figure {name} is not on {canvas}"), Some(hint))
    })?;
    let mut one = shown(figure);
    if let Some(last) = store.canvases().source(&canvas, &name).map_err(|e| drawn(&e))? {
        // The wire names a script by its blob: store it, for a file from elsewhere.
        let blob = store.put_blob(last.script.text.as_bytes()).map_err(|e| stored(&e))?;
        (one.script, one.data) = (Some(blob.to_string()), Some(last.script.data));
    }
    Ok(CanvasFigures { figures: vec![one] })
}

/// A figure a script draws: on which canvas, by whom, from what.
struct Drawing<'a> {
    canvas: &'a str,
    who: &'a str,
    name: &'a str,
    script: &'a str,
    data: Value,
    /// The frame it is drawn over or in, in tenths.
    target: Option<[i64; 4]>,
}

/// Places `items`, what `figure`'s script drew, as `at` says, under the
/// key `once`; when that wrote, appends `figure_drawn` and, for an
/// agent's figure, `reply_landed`. The reply has no warnings or images
/// yet; the draft is for those.
fn land(
    store: &Store,
    figure: &Drawing<'_>,
    items: &[Value],
    at: &Placement,
    once: Option<&str>,
) -> Result<(CanvasDrawn, Draft), RemoteError> {
    let Drawing { canvas, who, name, script, .. } = *figure;
    let blob = store.put_blob(script.as_bytes()).map_err(|e| stored(&e))?;
    let kept = Script { text: script.into(), data: figure.data.clone(), target: figure.target };
    let done =
        store.canvases().draw(canvas, who, (name, items, kept), at, once).map_err(|e| drawn(&e))?;
    let draft = done.draft;
    if draft.unchanged() {
        return Ok((reply(draft.clone(), Vec::new(), Vec::new(), Vec::new(), None), draft));
    }
    let maker = match parse_author(who).map_err(|e| remote(&e))? {
        Author::Agent(name) => Some(name),
        Author::User => None,
    };
    let body = Body::FigureDrawn(FigureDrawn {
        canvas: canvas.into(),
        figure: draft.name.clone(),
        page: draft.page.clone(),
        script: blob,
        data: figure.data.clone(),
        origin: draft.origin,
        frame: draft.frame,
        target: figure.target,
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
    Ok((reply(draft.clone(), done.written.seq, Vec::new(), Vec::new(), event), draft))
}

/// Runs `script` on `data` as the figure `name` on the page called
/// `page`, drawn over or in `target` (tenths) if any.
fn run(
    script: &str,
    data: &Value,
    name: &str,
    page: String,
    target: Option<[i64; 4]>,
) -> Result<slinger_script::Drawn, RemoteError> {
    let view =
        View { name: name.into(), target: target.map(|t| t.map(units)), agent: AGENT.into(), page };
    slinger_script::run(script, data, &view).map_err(|e| {
        let hint =
            "fix the script there and try it with --dry-run; 'slinger guide' has the drawing API";
        RemoteError::new("invalid", e.to_string(), Some(hint.into()))
    })
}

/// A figure as `canvas_figure` gives it, without its script and data.
fn shown(figure: &Figure) -> FigureShown {
    let parts = figure
        .parts
        .iter()
        .map(|p| PartShown {
            name: p.name.clone(),
            frame: rect_tenths(p.frame),
            items: p.items.len(),
        })
        .collect();
    FigureShown {
        name: figure.name.clone(),
        page: format!("p{}", figure.page.get()),
        frame: rect_tenths(figure.frame),
        by: match &figure.maker {
            Author::User => "user".into(),
            Author::Agent(name) => format!("agent:{name}"),
        },
        locked: figure.locked,
        items: figure.items.len(),
        parts,
        script: None,
        data: None,
    }
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
    utf8(bytes, &format!("script {}", path.display()))
}

/// `bytes` as text: `what`, a script, must be UTF-8.
fn utf8(bytes: Vec<u8>, what: &str) -> Result<String, RemoteError> {
    String::from_utf8(bytes).map_err(|_| {
        let what = format!("{what} is not UTF-8 text");
        RemoteError::new("invalid", what, Some("save the script as UTF-8".into()))
    })
}

/// The JSON in the file at `path`: a script's data, or a patch of it.
fn read_data(path: &Path) -> Result<Value, RemoteError> {
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
