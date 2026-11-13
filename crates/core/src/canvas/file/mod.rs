//! The canvas file (`docs/design/canvas-file.md`): an append-only log of
//! operations as JSON Lines, after a header that carries the format version.
//!
//! Bytes in, bytes out; no IO. The interface:
//! - [`Header::new`] and [`Header::encode`]: the first line of a new file.
//! - [`Line::of`] turns an operation the canvas applied into a line, and
//!   [`Line::encode`] writes it. Append the bytes; that is the only write.
//! - [`read`] takes a whole file and returns its [`File`]: the header, the
//!   lines and how many bytes of a torn last line it cut. [`File::canvas`]
//!   replays the lines into the canvas they build.
//! - [`Log`] holds a file open for a server: each change returns the bytes
//!   to append, numbered, and undo lives beside it. [`Log::after`] finds the
//!   stored lines after a point, and [`lines`] reads them as they are. It
//!   also crops images of an item or a box and gives a stroke's points, for
//!   an agent (`look`), puts an agent's answer beside an item (`put`) and
//!   lets the agent change or remove it (`edit`).
//! - [`snap`] rounds an operation to what the file can hold. Apply the
//!   snapped op, so the canvas in memory is the one the file rebuilds.
//!
//! **Units.** The model holds `f64` canvas units; the file holds integers in
//! tenths of a unit. Encoding rounds to the nearest tenth (halves away from
//! zero); decoding gives `k / 10.0`. A value already on that grid is kept
//! exactly, so `snap` is idempotent and a snapped canvas round-trips with no
//! loss. (A move adds two grid values in `f64`, which can land a hair off the
//! grid in memory; a later `restore` or `set` of that item writes the grid
//! value, so the canvas from the file agrees with memory at tenths.) A
//! stroke's width is at least one tenth. Stroke points are rounded
//! one by one before their deltas are taken, so error never builds up.
//!
//! **Ids.** Items are written `i<n>` and pages `p<n>`, from the model's ids. A
//! new canvas starts with page `p1`, named [`FIRST_PAGE`](super::FIRST_PAGE),
//! before any line. Replay honours the ids in the file, so an `add` this
//! reader skips (an item kind it does not know) leaves no gap in meaning.
//!
//! **Newer writers (B5).** A line with an operation kind, an item kind or an
//! author this reader does not know comes back as [`Line::Unknown`], whole,
//! and replay skips it. Fields this reader does not know are ignored.

mod edit;
mod json;
mod log;
mod look;
mod once;
mod packet;
mod put;
mod replay;
mod time;
mod whole;

pub use edit::Edit;
pub use log::{Log, Shown, Span, Written, check_id, id_from_name, lines, parse_author};
pub use look::MAX_CROP;
pub use once::ONCES;
pub use packet::{backdrop, rect_tenths, size_tenths};
pub use put::{How, INK_WIDTH, Placed, Put, TEXT_COLUMNS};
use serde_json::{Map, Value};

use super::{Action, Applied, Author, Item, ItemId, Op, Page, PageId, Point, Shape, Time};
use crate::{Error, Result};
use json::{Bad, Fields, Obj};

/// The format's major version. It goes up only when an old reader would
/// misread a new file; new kinds and fields do not change it.
pub const VERSION: u64 = 1;

/// Line 1 of a canvas file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// The canvas id.
    pub id: String,
    /// The `n` of the first operation line.
    pub first: u64,
}

impl Header {
    /// The header of a new canvas file: its operations count from 1.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into(), first: 1 }
    }

    /// The header line, with its `\n`, at the current [`VERSION`].
    #[must_use]
    pub fn encode(&self) -> String {
        let line = Obj::new()
            .str("slinger", "canvas")
            .raw("v", &VERSION.to_string())
            .str("id", &self.id)
            .raw("first", &self.first.to_string())
            .end();
        line + "\n"
    }
}

/// One operation line.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    /// An operation this reader knows.
    Op { n: u64, by: Author, at: Time, change: Change },
    /// A line from a newer writer: an operation kind, item kind or author this
    /// reader does not know. Kept as its text, without the `\n`.
    Unknown { n: u64, text: String },
}

/// What an operation line changes, as the file states it. Positions and
/// lengths are already on the tenths grid.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    PageAdd {
        page: PageId,
        name: String,
    },
    PageRename {
        page: PageId,
        name: String,
    },
    PageRemove {
        page: PageId,
    },
    Add {
        item: ItemId,
        page: PageId,
        shape: Shape,
    },
    Remove {
        items: Vec<ItemId>,
    },
    Move {
        items: Vec<ItemId>,
        by: Point,
    },
    /// Replace these fields of an item (`kind`, `color`, `x`, ...) whole.
    /// Replay lays them over the item's current fields.
    Set {
        item: ItemId,
        fields: Map<String, Value>,
    },
    /// Put a removed item back whole, under its own id, maker and time.
    /// Undo and redo write it.
    Restore {
        item: Item,
    },
    /// Put a removed page back at `index`, with its items. Undo writes it.
    PageRestore {
        page: Page,
        index: usize,
        items: Vec<Item>,
    },
}

impl Line {
    /// The line for an operation the canvas applied, numbered `n`, where
    /// `applied` is what [`Canvas::apply`](super::Canvas::apply) returned for
    /// it. `None` when there is nothing to write: the op changed nothing, or
    /// `applied` does not belong to it.
    #[must_use]
    pub fn of(n: u64, op: &Op, applied: Applied) -> Option<Self> {
        let change = match (&op.action, applied) {
            (Action::Add { page, shape }, Applied::Added(item)) => {
                Change::Add { item, page: *page, shape: snap_shape(shape) }
            }
            (Action::PageAdd { name }, Applied::PageAdded(page)) => {
                Change::PageAdd { page, name: name.trim().into() }
            }
            (Action::Edit { item, shape }, Applied::Changed) => {
                Change::Set { item: *item, fields: json::fields(shape) }
            }
            (Action::Move { item, by }, Applied::Changed) => {
                Change::Move { items: vec![*item], by: snap_point(*by) }
            }
            (Action::Remove { item }, Applied::Changed) => Change::Remove { items: vec![*item] },
            (Action::PageRename { page, name }, Applied::Changed) => {
                Change::PageRename { page: *page, name: name.trim().into() }
            }
            (Action::PageRemove { page }, Applied::Changed) => Change::PageRemove { page: *page },
            (Action::Restore { item }, Applied::Changed) => {
                Change::Restore { item: snap_item(item.clone()) }
            }
            (Action::PageRestore { page, index, items }, Applied::Changed) => Change::PageRestore {
                page: page.clone(),
                index: *index,
                items: items.iter().cloned().map(snap_item).collect(),
            },
            _ => return None,
        };
        Some(Line::Op { n, by: op.by.clone(), at: op.at, change })
    }

    /// The line's sequence number.
    #[must_use]
    pub fn n(&self) -> u64 {
        match self {
            Line::Op { n, .. } | Line::Unknown { n, .. } => *n,
        }
    }

    /// The line, with its `\n`. A known line is always the same bytes; an
    /// unknown one is written back as it was read.
    #[must_use]
    pub fn encode(&self) -> String {
        let (n, by, at, change) = match self {
            Line::Unknown { text, .. } => return format!("{text}\n"),
            Line::Op { n, by, at, change } => (n, by, at, change),
        };
        let o = Obj::new()
            .raw("n", &n.to_string())
            .str("t", &time::format(*at))
            .str("by", &json::author(by));
        let ids = |items: &[ItemId]| Value::from_iter(items.iter().map(|i| json::item_id(*i)));
        let o = match change {
            Change::PageAdd { page, name } => {
                o.str("op", "page.add").str("page", &json::page_id(*page)).str("name", name)
            }
            Change::PageRename { page, name } => {
                o.str("op", "page.rename").str("page", &json::page_id(*page)).str("name", name)
            }
            Change::PageRemove { page } => {
                o.str("op", "page.remove").str("page", &json::page_id(*page))
            }
            Change::Add { item, page, shape } => {
                o.str("op", "add").raw("item", &json::item(*item, *page, shape))
            }
            Change::Remove { items } => o.str("op", "remove").raw("ids", &ids(items).to_string()),
            Change::Move { items, by } => o
                .str("op", "move")
                .raw("ids", &ids(items).to_string())
                .int("dx", json::tenths(by.x))
                .int("dy", json::tenths(by.y)),
            Change::Set { item, fields } => o
                .str("op", "set")
                .str("id", &json::item_id(*item))
                .raw("fields", &Value::Object(fields.clone()).to_string()),
            Change::Restore { item } => o.str("op", "restore").raw("item", &json::whole_item(item)),
            Change::PageRestore { page, index, items } => {
                let items: Vec<String> = items.iter().map(json::whole_item).collect();
                o.str("op", "page.restore")
                    .str("page", &json::page_id(page.id))
                    .str("name", &page.name)
                    .raw("index", &index.to_string())
                    .raw("items", &format!("[{}]", items.join(",")))
            }
        };
        o.end() + "\n"
    }
}

/// A whole canvas file, as read.
#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub header: Header,
    pub lines: Vec<Line>,
    /// Bytes of a torn last line that were cut off. The caller truncates the
    /// file to `len - torn` before it appends, and may say so.
    pub torn: usize,
}

impl File {
    /// The `n` of the next line to append.
    #[must_use]
    pub fn next(&self) -> u64 {
        self.header.first + self.lines.len() as u64
    }
}

/// Reads a whole canvas file.
///
/// A last line without its `\n`, or one that is not JSON, is a torn append:
/// it is cut and counted in [`File::torn`]. Nothing before it is lost.
///
/// # Errors
///
/// - [`Error::CanvasTooNew`] when the header's version is above [`VERSION`].
///   Do not write to that file.
/// - [`Error::CanvasCorrupt`] with the line number when the header is missing
///   or not a canvas header, an earlier line is not JSON, a line's `n` is not
///   the one expected, or a known operation's fields do not fit it.
pub fn read(bytes: &[u8]) -> Result<File> {
    let whole = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let mut torn = bytes.len() - whole;
    let mut rows: Vec<&[u8]> = bytes[..whole].split(|&b| b == b'\n').collect();
    rows.pop();
    let header = match rows.first() {
        Some(row) => read_header(row)?,
        None => return Err(corrupt(1, "there is no whole header line".into())),
    };
    let mut lines = Vec::with_capacity(rows.len().saturating_sub(1));
    for (i, row) in rows.iter().enumerate().skip(1) {
        let expected = header.first + lines.len() as u64;
        match read_line(row, expected) {
            Ok(line) => lines.push(line),
            Err(Damage::NotJson(_)) if i == rows.len() - 1 => torn += row.len() + 1,
            Err(Damage::NotJson(why) | Damage::Bad(why)) => return Err(corrupt(i + 1, why)),
        }
    }
    Ok(File { header, lines, torn })
}

/// `op` rounded to what the file holds (see the module doc), with each text
/// frame [`Action::fitted`], so the line holds the box the canvas keeps. An
/// op with a value that is not a number is left alone, so the canvas still
/// rejects it.
#[must_use]
pub fn snap(op: Op) -> Op {
    let action = match op.action.fitted() {
        Action::Add { page, shape } => Action::Add { page, shape: snap_shape(&shape) },
        Action::Edit { item, shape } => Action::Edit { item, shape: snap_shape(&shape) },
        Action::Move { item, by } => Action::Move { item, by: snap_point(by) },
        Action::Restore { item } => Action::Restore { item: snap_item(item) },
        Action::PageRestore { page, index, items } => {
            Action::PageRestore { page, index, items: items.into_iter().map(snap_item).collect() }
        }
        other => other,
    };
    Op { action, ..op }
}

fn snap_shape(shape: &Shape) -> Shape {
    if shape.check().is_err() {
        return shape.clone();
    }
    json::read_shape(&json::fields(shape)).ok().flatten().unwrap_or_else(|| shape.clone())
}

fn snap_item(item: Item) -> Item {
    Item { shape: snap_shape(&item.shape), ..item }
}

fn snap_point(p: Point) -> Point {
    if !(p.x.is_finite() && p.y.is_finite()) {
        return p;
    }
    Point::new(json::units(json::tenths(p.x)), json::units(json::tenths(p.y)))
}

fn corrupt(line: usize, message: String) -> Error {
    Error::CanvasCorrupt { line, message }
}

/// Why a line could not be read.
enum Damage {
    /// Not JSON at all: a torn write, if it is the last line.
    NotJson(String),
    /// JSON that does not fit the format.
    Bad(Bad),
}

fn object(row: &[u8]) -> std::result::Result<(&str, Fields), Damage> {
    let text = std::str::from_utf8(row).map_err(|_| Damage::NotJson("not UTF-8".into()))?;
    match serde_json::from_str(text) {
        Ok(Value::Object(m)) => Ok((text, m)),
        Ok(_) => Err(Damage::Bad("not a JSON object".into())),
        Err(e) => Err(Damage::NotJson(format!("not JSON: {e}"))),
    }
}

fn read_header(row: &[u8]) -> Result<Header> {
    let (_, m) =
        object(row).map_err(|(Damage::NotJson(why) | Damage::Bad(why))| corrupt(1, why))?;
    if m.get("slinger").and_then(Value::as_str) != Some("canvas") {
        return Err(corrupt(1, "not a canvas header".into()));
    }
    let version = m.get("v").and_then(Value::as_u64).unwrap_or(0);
    if version > VERSION {
        return Err(Error::CanvasTooNew { version });
    }
    let first = m.get("first").and_then(Value::as_u64).unwrap_or(0);
    match (version, json::text(&m, "id"), first) {
        (0, ..) => Err(corrupt(1, "`v` is missing or not a version".into())),
        (_, Err(why), _) => Err(corrupt(1, why)),
        (_, _, 0) => Err(corrupt(1, "`first` is missing or not above 0".into())),
        (_, Ok(id), first) => Ok(Header { id: id.to_owned(), first }),
    }
}

fn read_line(row: &[u8], expected: u64) -> std::result::Result<Line, Damage> {
    let (text, m) = object(row)?;
    let n = m.get("n").and_then(Value::as_u64);
    if n != Some(expected) {
        return Err(Damage::Bad(format!("expected operation {expected}, found `n` {n:?}")));
    }
    let unknown = Line::Unknown { n: expected, text: text.to_owned() };
    let Some(change) = read_change(&m).map_err(Damage::Bad)? else { return Ok(unknown) };
    let Some(by) = json::parse_author(json::text(&m, "by").map_err(Damage::Bad)?) else {
        return Ok(unknown);
    };
    let at = time::parse(json::text(&m, "t").map_err(Damage::Bad)?).map_err(Damage::Bad)?;
    Ok(Line::Op { n: expected, by, at, change })
}

/// The change a line states, or `None` for one this reader does not know.
fn read_change(m: &Fields) -> std::result::Result<Option<Change>, Bad> {
    let page = || json::text(m, "page").and_then(json::parse_page);
    let name = || json::text(m, "name").map(str::to_owned);
    let change = match json::text(m, "op")? {
        "page.add" => Change::PageAdd { page: page()?, name: name()? },
        "page.rename" => Change::PageRename { page: page()?, name: name()? },
        "page.remove" => Change::PageRemove { page: page()? },
        "add" => {
            let item = json::field(m, "item")?.as_object().ok_or("`item` is not an object")?;
            let Some(shape) = json::read_shape(item)? else { return Ok(None) };
            let id = json::text(item, "id").and_then(json::parse_item)?;
            let page = json::text(item, "page").and_then(json::parse_page)?;
            Change::Add { item: id, page, shape }
        }
        "remove" => Change::Remove { items: json::ids(m, "ids")? },
        "move" => {
            let by = Point::new(json::units(json::int(m, "dx")?), json::units(json::int(m, "dy")?));
            Change::Move { items: json::ids(m, "ids")?, by }
        }
        "set" => {
            let fields =
                json::field(m, "fields")?.as_object().ok_or("`fields` is not an object")?;
            let item = json::text(m, "id").and_then(json::parse_item)?;
            Change::Set { item, fields: fields.clone() }
        }
        "restore" => {
            let item = json::field(m, "item")?.as_object().ok_or("`item` is not an object")?;
            let Some(item) = json::read_whole_item(item)? else { return Ok(None) };
            Change::Restore { item }
        }
        "page.restore" => {
            let index = json::field(m, "index")?.as_u64().and_then(|i| usize::try_from(i).ok());
            let list = json::field(m, "items")?.as_array().ok_or("`items` is not a list")?;
            let mut items = Vec::with_capacity(list.len());
            for item in list {
                let item = item.as_object().ok_or("an item is not an object")?;
                // An item of a kind this reader does not know stays off the canvas.
                items.extend(json::read_whole_item(item)?);
            }
            let page = Page { id: page()?, name: name()? };
            Change::PageRestore { page, index: index.ok_or("`index` is not a position")?, items }
        }
        // `page.move` is in the format, but the model cannot reorder pages yet.
        _ => return Ok(None),
    };
    Ok(Some(change))
}
