//! The figure verbs: `canvas_draw`, `canvas_redraw` and `canvas_figure`
//! (docs/service.md, "Drawing a figure"; docs/design/figures.md, "The
//! verbs"). An agent's drawing script runs in the daemon and its shapes land
//! as one figure: items that share a name, with named parts. A figure is its
//! script plus its data, so a redraw is a patch of its data.
//!
//! The script and its data cross as files, as a blob's bytes do: a request
//! line is at most [`crate::MAX_LINE`], and data may be larger.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::absolute;
use crate::{CanvasImage, Client, RemoteError};

/// `canvas_draw`: run a script and place what it draws as the figure
/// `name`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "DrawLine")]
pub struct CanvasDraw {
    pub canvas: String,
    /// Who draws: `user` or `agent:<name>`, the figure's maker.
    pub by: String,
    /// The figure's name: kebab case, unique on the canvas.
    pub name: String,
    /// The script's file, which the daemon reads.
    pub path: PathBuf,
    /// A file of the JSON the script reads as `fig.data`; absent is `{}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<PathBuf>,
    /// Where the figure goes: one of `beside`, `over`, `in` and `at` on
    /// the wire.
    #[serde(flatten)]
    pub place: Place,
    /// The page `at` is on, by id or name; absent is the first. Only with `at`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// Draft only: write nothing and reply as the draw would.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dry_run: bool,
    /// Write a PNG of the figure alone here; tiles go beside it as
    /// `<stem>-<n>.png`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub png: Option<PathBuf>,
    /// A key the draw is written under once, as `canvas_apply`'s `once`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub once: Option<String>,
}

/// Where a figure goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Place {
    /// Its frame beside the item (`i4`, `b4`, a figure address), where it
    /// covers nothing.
    Beside(String),
    /// Its origin on the item's top left; the script sees the item's size.
    Over(String),
    /// Its frame in the box's room, where it covers nothing the box holds.
    #[serde(rename = "in")]
    In(String),
    /// Its origin here, `[x, y]` in tenths of a unit.
    At([i64; 2]),
}

/// A draw as the line holds it: exactly one place.
#[derive(Deserialize)]
struct DrawLine {
    canvas: String,
    #[serde(default = "crate::canvas::user")]
    by: String,
    name: String,
    path: PathBuf,
    data: Option<PathBuf>,
    beside: Option<String>,
    over: Option<String>,
    #[serde(rename = "in")]
    inside: Option<String>,
    at: Option<[i64; 2]>,
    page: Option<String>,
    #[serde(default)]
    dry_run: bool,
    png: Option<PathBuf>,
    once: Option<String>,
}

impl TryFrom<DrawLine> for CanvasDraw {
    type Error = &'static str;

    fn try_from(line: DrawLine) -> Result<Self, Self::Error> {
        let place = match (line.beside, line.over, line.inside, line.at) {
            (Some(id), None, None, None) => Place::Beside(id),
            (None, Some(id), None, None) => Place::Over(id),
            (None, None, Some(id), None) => Place::In(id),
            (None, None, None, Some(at)) => Place::At(at),
            _ => return Err("a draw takes exactly one of `beside`, `over`, `in` and `at`"),
        };
        if line.page.is_some() && !matches!(place, Place::At(_)) {
            return Err("`page` goes only with `at`");
        }
        let DrawLine { canvas, by, name, path, data, page, dry_run, png, once, .. } = line;
        Ok(Self { canvas, by, name, path, data, place, page, dry_run, png, once })
    }
}

/// The reply to `canvas_draw`: the figure as placed (or as it would be, on
/// a dry run).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasDrawn {
    /// The figure's name.
    pub name: String,
    /// Its page's id (`p1`).
    pub page: String,
    /// Where its (0,0) went, `[x, y]` in tenths.
    pub origin: [i64; 2],
    /// The joint frame of its items, `[x, y, w, h]` in tenths.
    pub frame: [i64; 4],
    /// Its part names, in the order the script first drew in them.
    pub parts: Vec<String>,
    /// How many items it has.
    pub items: usize,
    /// The numbers written: empty on a dry run, and for a draw that changed
    /// nothing.
    pub seq: Vec<u64>,
    /// What the script should know, each `script:LINE:COL: message`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// The images `png` asked for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<CanvasImage>,
    /// The `reply_landed` appended, when the draw answered a sling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<u64>,
}

/// `canvas_redraw`: run the figure `name`'s script again on its last data
/// patched, and replace the figure in place. The reply is a [`CanvasDrawn`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasRedraw {
    pub canvas: String,
    /// Who redraws: `user` or `agent:<name>`. Only the maker (or the user).
    #[serde(default = "crate::canvas::user")]
    pub by: String,
    /// The figure's name.
    pub name: String,
    /// A file holding a JSON merge patch (RFC 7396) on the figure's last
    /// data; absent keeps the data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<PathBuf>,
    /// A script's file to swap in for the figure's; absent keeps its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// A key the redraw is written under once, as `canvas_apply`'s `once`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub once: Option<String>,
}

/// `canvas_figure`: the canvas's figures, or the one named, with its
/// script and data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasFigureAsk {
    pub canvas: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// The reply to `canvas_figure`: page by page, in reading order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasFigures {
    pub figures: Vec<FigureShown>,
}

/// One figure on a canvas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FigureShown {
    pub name: String,
    /// Its page's id (`p1`).
    pub page: String,
    /// The joint frame of its items the page shows, `[x, y, w, h]` in tenths.
    pub frame: [i64; 4],
    /// Its maker: `user` or `agent:<name>`.
    pub by: String,
    /// Whether any of its items is locked against the user's pen.
    pub locked: bool,
    /// How many items it has.
    pub items: usize,
    /// Its parts, in reading order.
    pub parts: Vec<PartShown>,
    /// The blob id of its script, for one figure asked by name that a
    /// script drew (`blob_get` reads it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    /// The data its script last ran on, for one figure asked by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// A named part of a figure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartShown {
    pub name: String,
    /// The joint frame of its items, `[x, y, w, h]` in tenths.
    pub frame: [i64; 4],
    /// How many items it has.
    pub items: usize,
}

impl Client {
    /// Runs the script `draw.path` names in the daemon and places what it
    /// draws as the figure `draw.name`, as one undo step; or, on a dry run,
    /// only says how it would go. Relative paths are taken from this
    /// process's working directory.
    ///
    /// # Errors
    ///
    /// `invalid` for a script that fails (the message keeps its
    /// `script:LINE:COL`), data that is not JSON, a name that is not a
    /// figure name and shapes the canvas does not take; `not_found` for a
    /// canvas, item, page or file that is not there; `conflict` for a name
    /// another maker drew or a box with no room; `too_long` for an agent's
    /// text over its rows; and `unavailable` when the daemon does not answer.
    pub fn canvas_draw(&self, mut draw: CanvasDraw) -> Result<CanvasDrawn, RemoteError> {
        draw.path = absolute(&draw.path)?;
        draw.data = draw.data.as_deref().map(absolute).transpose()?;
        draw.png = draw.png.as_deref().map(absolute).transpose()?;
        self.call(draw)
    }

    /// Runs the figure `redraw.name`'s script again (or the one
    /// `redraw.path` names) on its last data patched by `redraw.data`, and
    /// replaces the figure in place where it now is, as one undo step. The
    /// same script on the same data runs nothing and writes nothing.
    /// Relative paths are taken from this process's working directory.
    ///
    /// # Errors
    ///
    /// Those of [`Client::canvas_draw`]; `not_found` for a figure that is
    /// not on the canvas or that no script drew, and `invalid` for one
    /// another agent made.
    pub fn canvas_redraw(&self, mut redraw: CanvasRedraw) -> Result<CanvasDrawn, RemoteError> {
        redraw.data = redraw.data.as_deref().map(absolute).transpose()?;
        redraw.path = redraw.path.as_deref().map(absolute).transpose()?;
        self.call(redraw)
    }

    /// The figures on `canvas`, or the one called `name` with its script
    /// and data.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas, or a figure `name`, that is not there, and
    /// `unavailable` when the daemon does not answer.
    pub fn canvas_figure(
        &self,
        canvas: &str,
        name: Option<&str>,
    ) -> Result<CanvasFigures, RemoteError> {
        self.call(CanvasFigureAsk { canvas: canvas.into(), name: name.map(Into::into) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;
    use crate::wire::request_line;

    fn draw(place: Place) -> CanvasDraw {
        CanvasDraw {
            canvas: "main".into(),
            by: "agent:scout".into(),
            name: "worksheet".into(),
            path: "/tmp/w.js".into(),
            data: None,
            place,
            page: None,
            dry_run: false,
            png: None,
            once: None,
        }
    }

    #[test]
    fn a_draw_reads_back_as_itself_for_each_place() {
        let mut full = draw(Place::At([10, -20]));
        (full.data, full.page) = (Some("/tmp/d.json".into()), Some("p2".into()));
        (full.dry_run, full.png, full.once) = (true, Some("/tmp/w.png".into()), Some("k1".into()));
        for (ask, line) in [
            (
                draw(Place::Beside("i4".into())),
                r#"{"v":1,"op":"canvas_draw","canvas":"main","by":"agent:scout","name":"worksheet","path":"/tmp/w.js","beside":"i4"}"#,
            ),
            (
                draw(Place::Over("b4".into())),
                r#"{"v":1,"op":"canvas_draw","canvas":"main","by":"agent:scout","name":"worksheet","path":"/tmp/w.js","over":"b4"}"#,
            ),
            (
                draw(Place::In("b4".into())),
                r#"{"v":1,"op":"canvas_draw","canvas":"main","by":"agent:scout","name":"worksheet","path":"/tmp/w.js","in":"b4"}"#,
            ),
            (
                full,
                r#"{"v":1,"op":"canvas_draw","canvas":"main","by":"agent:scout","name":"worksheet","path":"/tmp/w.js","data":"/tmp/d.json","at":[10,-20],"page":"p2","dry_run":true,"png":"/tmp/w.png","once":"k1"}"#,
            ),
        ] {
            let ask = Request::CanvasDraw(ask);
            assert_eq!(request_line(&ask).unwrap(), format!("{line}\n"));
            let read = crate::read_request(&mut line.as_bytes()).unwrap();
            assert_eq!(read, Some(ask), "{line}");
        }
    }

    #[test]
    fn a_draw_takes_exactly_one_place_and_a_page_only_with_at() {
        for line in [
            r#"{"v":1,"op":"canvas_draw","canvas":"main","name":"f","path":"/a.js"}"#,
            r#"{"v":1,"op":"canvas_draw","canvas":"main","name":"f","path":"/a.js","beside":"i1","at":[0,0]}"#,
            r#"{"v":1,"op":"canvas_draw","canvas":"main","name":"f","path":"/a.js","beside":"i1","page":"p2"}"#,
        ] {
            let read = crate::read_request(&mut line.as_bytes());
            assert!(read.is_err(), "{line}: {read:?}");
        }
        let line =
            r#"{"v":1,"op":"canvas_draw","canvas":"main","name":"f","path":"/a.js","at":[0,0]}"#;
        let read = crate::read_request(&mut line.as_bytes()).unwrap();
        let Some(Request::CanvasDraw(ask)) = read else { unreachable!("{read:?}") };
        assert_eq!((ask.by.as_str(), ask.dry_run), ("user", false), "the user's, not dry");
    }
}
