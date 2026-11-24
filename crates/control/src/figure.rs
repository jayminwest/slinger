//! The figure verbs: `canvas_draw` (docs/service.md, "Drawing a figure";
//! docs/design/figures.md, "The verbs"). An agent's drawing script runs in
//! the daemon and its shapes land as one figure: items that share a name,
//! with named parts.
//!
//! The script and its data cross as files, as a blob's bytes do: a request
//! line is at most [`crate::MAX_LINE`], and data may be larger.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
        let DrawLine { canvas, by, name, path, data, page, dry_run, png, .. } = line;
        Ok(Self { canvas, by, name, path, data, place, page, dry_run, png })
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
        }
    }

    #[test]
    fn a_draw_reads_back_as_itself_for_each_place() {
        let mut full = draw(Place::At([10, -20]));
        (full.data, full.page) = (Some("/tmp/d.json".into()), Some("p2".into()));
        (full.dry_run, full.png) = (true, Some("/tmp/w.png".into()));
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
                r#"{"v":1,"op":"canvas_draw","canvas":"main","by":"agent:scout","name":"worksheet","path":"/tmp/w.js","data":"/tmp/d.json","at":[10,-20],"page":"p2","dry_run":true,"png":"/tmp/w.png"}"#,
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
