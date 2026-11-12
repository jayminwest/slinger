//! A canvas file held open: the one place a stored canvas changes.
//!
//! [`Log`] keeps the canvas its lines build, the undo [`History`] beside it
//! and the number of the next line. Each change returns the bytes to append
//! ([`Written`]); the caller appends them, syncs, and only then answers. The
//! log in memory has already moved on, so a caller whose write fails drops
//! the log and opens the file again.
//!
//! The log also knows where each line ends, so a server can hand out the
//! stored lines after a point ([`Log::after`], [`lines`]) as the file holds
//! them, fields from newer writers included.
//!
//! Requests name things the way the file does: item ids `i<n>`, page ids
//! `p<n>`, items as the file's item objects (positions and lengths in tenths
//! of a unit). See "Canvas operations" in docs/service.md.

use std::ops::Range;

use serde_json::Value;

use super::json::{self, Fields};
use super::{File, Header, Line, read, snap};
use crate::canvas::{
    Action, Applied, Author, Canvas, History, ItemId, Op, Page, Point, Shape, Time,
};
use crate::{Error, Result};

/// The longest canvas id, in bytes.
const MAX_ID: usize = 64;

/// What requests may be shaped like.
const OPS_HINT: &str = "see \"Canvas operations\" in docs/service.md";

/// A canvas id is 1 to 64 of `a-z`, `0-9`, `-`, `_` and `.`, and does not
/// start with `.`. It names the canvas's file, so it is never a path, and it
/// is lowercase so two ids never name one file on a case-blind disk.
///
/// # Errors
///
/// [`Error::Invalid`] for any other string.
pub fn check_id(id: &str) -> Result<()> {
    let ok = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_.".contains(&b);
    if (1..=MAX_ID).contains(&id.len()) && !id.starts_with('.') && id.bytes().all(ok) {
        return Ok(());
    }
    Err(invalid(
        format!("{id:?} is not a canvas id"),
        "use 1 to 64 of a-z, 0-9, '-', '_' and '.', not starting with '.'",
    ))
}

/// A canvas id made from a name, such as an imported file's: lowercased,
/// each run of other characters made one `-`, trimmed of `-` and `.` at
/// the ends and cut to 64 bytes. A name with nothing left gives `canvas`.
#[must_use]
pub fn id_from_name(name: &str) -> String {
    let mut id = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        let keep = c.is_ascii_lowercase() || c.is_ascii_digit() || "_.".contains(c);
        let c = if keep { c } else { '-' };
        if !(c == '-' && id.ends_with('-')) {
            id.push(c);
        }
    }
    id.truncate(MAX_ID);
    let id = id.trim_matches(['-', '.']);
    if id.is_empty() { "canvas".into() } else { id.into() }
}

/// Who asks: `user` or `agent:<name>`.
///
/// # Errors
///
/// [`Error::Invalid`] for anything else.
pub fn parse_author(by: &str) -> Result<Author> {
    match json::parse_author(by) {
        Some(Author::Agent(name)) if name.trim().is_empty() => None,
        other => other,
    }
    .ok_or_else(|| invalid(format!("{by:?} is not an author"), "pass user or agent:<name>"))
}

/// What one change wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Written {
    /// The lines to append, each with its `\n`. Empty when nothing changed.
    pub bytes: String,
    /// The sequence number of each line, in order.
    pub seq: Vec<u64>,
    /// The ids of the items and pages the change made, in order.
    pub made: Vec<String>,
}

/// One page of a canvas, as `canvas show` gives it.
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    /// The id of the page shown.
    pub page: String,
    /// Every page, in order: `{"id": "p1", "name": "Page 1"}`.
    pub pages: Vec<Value>,
    /// The page's items in drawing order, as the file's whole item objects.
    pub items: Vec<Value>,
}

/// Where some whole operation lines lie in a canvas file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// The lines' bytes in the file. Empty when there are none.
    pub bytes: Range<u64>,
    /// The number of the first line after them.
    pub next: u64,
}

/// A canvas file held open. See the module doc.
#[derive(Debug, Clone)]
pub struct Log {
    canvas: Canvas,
    history: History,
    next: u64,
    /// Every op the canvas applied, in order: the log a packet is built from.
    ops: Vec<Op>,
    /// The `n` of the line each op in `ops` came from.
    lines: Vec<u64>,
    /// Where each whole line of the file ends: the header's, then each
    /// operation line's, in order.
    ends: Vec<u64>,
}

impl Log {
    /// A new canvas and the bytes of its file: the header alone.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] when `id` is not a canvas id ([`check_id`]).
    pub fn create(id: &str) -> Result<(Self, String)> {
        check_id(id)?;
        let header = Header::new(id);
        let (next, bytes) = (header.first, header.encode());
        let log = Self {
            canvas: Canvas::new(),
            history: History::new(),
            next,
            ops: Vec::new(),
            lines: Vec::new(),
            ends: vec![bytes.len() as u64],
        };
        Ok((log, bytes))
    }

    /// A new canvas file holding `canvas` whole (an import, say), and its
    /// bytes. The log holds the canvas as the file rebuilds it, so what the
    /// daemon serves is what it will read back. Page lines are the user's at
    /// `at`; item lines keep each item's maker and time.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] when `id` is not a canvas id ([`check_id`]).
    pub fn import(id: &str, canvas: &Canvas, at: Time) -> Result<(Self, String)> {
        check_id(id)?;
        let bytes = super::whole::encode(id, canvas, at);
        let (log, _) = Self::open(bytes.as_bytes())?;
        Ok((log, bytes))
    }

    /// Opens a whole file. Also returns how many bytes of a torn last line
    /// to cut off the end of the file before the next append.
    ///
    /// # Errors
    ///
    /// The errors of [`read`] and [`File::canvas`].
    pub fn open(bytes: &[u8]) -> Result<(Self, usize)> {
        let file: File = read(bytes)?;
        let (mut ops, mut lines) = (Vec::new(), Vec::new());
        let canvas = file.replay(|n, op| {
            lines.push(n);
            ops.push(op);
        })?;
        let whole = &bytes[..bytes.len() - file.torn];
        let ends = whole.iter().enumerate().filter(|(_, b)| **b == b'\n');
        let ends = ends.map(|(i, _)| i as u64 + 1).collect();
        let log = Self { canvas, history: History::new(), next: file.next(), ops, lines, ends };
        Ok((log, file.torn))
    }

    /// The canvas as the file builds it.
    #[must_use]
    pub fn canvas(&self) -> &Canvas {
        &self.canvas
    }

    /// The sequence number the next line gets.
    #[must_use]
    pub fn next(&self) -> u64 {
        self.next
    }

    /// Where the lines numbered above `after` lie in the file: whole lines,
    /// no more than `max` bytes of them, but at least one when there is one.
    /// `after` the newest number is an empty span.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] when `after` is not below [`Log::next`]: the caller
    /// holds a number this canvas never wrote.
    pub fn after(&self, after: u64, max: u64) -> Result<Span> {
        if after >= self.next {
            return Err(invalid(
                format!("the canvas has no operation {after}"),
                &format!("pass a number below {}, the next one 'canvas show' gives", self.next),
            ));
        }
        let written = self.ends.len() as u64 - 1;
        let first = self.next - written;
        // Line `first + k` starts where ends[k] says; `after < next` keeps
        // `skip` within the lines written.
        let from = (after + 1).max(first);
        let skip = usize::try_from(from - first).unwrap_or(usize::MAX);
        let (mut end, mut taken) = (self.ends.get(skip).copied().unwrap_or_default(), 0);
        let start = end;
        for &line_end in self.ends.iter().skip(skip + 1) {
            if line_end - start > max && taken > 0 {
                break;
            }
            (end, taken) = (line_end, taken + 1);
        }
        Ok(Span { bytes: start..end, next: from + taken })
    }

    /// Applies `ops` (one operation object or a list of them) as one undo
    /// step of `by`. Every operation applies or none does. The arrows bound
    /// to what the ops move or resize follow, as [`Log::followed`] says.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] for an operation that is not one, and the first
    /// error of [`Canvas::apply`]. The log is then as it was.
    pub fn apply(&mut self, by: &Author, at: Time, ops: &Value) -> Result<Written> {
        let list = match ops {
            Value::Array(list) => list.as_slice(),
            one => std::slice::from_ref(one),
        };
        let mut actions = Vec::new();
        for op in list {
            let op = op.as_object().ok_or_else(|| bad("an operation is not an object".into()))?;
            self.read_op(op, &mut actions)?;
        }
        let actions = self.followed(by, actions);
        self.step(by, at, actions)
    }

    /// Applies `actions` as one undo step of `by`, all or none, and writes
    /// their lines.
    pub(super) fn step(&mut self, by: &Author, at: Time, actions: Vec<Action>) -> Result<Written> {
        let ops: Vec<Op> =
            actions.into_iter().map(|action| snap(Op { by: by.clone(), at, action })).collect();
        let actions = ops.iter().map(|op| op.action.clone()).collect();
        let applied = self.history.apply(&mut self.canvas, by, at, actions)?;
        let mut written = Written::default();
        for (op, done) in ops.iter().zip(applied) {
            match done {
                Applied::Added(item) => written.made.push(json::item_id(item)),
                Applied::PageAdded(page) => written.made.push(json::page_id(page)),
                Applied::Changed | Applied::Unchanged => {}
            }
            self.write(op, done, &mut written);
        }
        Ok(written)
    }

    /// Takes back the last step of `by` (see [`History::undo`]). Nothing to
    /// undo writes nothing.
    pub fn undo(&mut self, by: &Author, at: Time) -> Written {
        let ops = self.history.undo(&mut self.canvas, by, at);
        self.write_all(&ops)
    }

    /// Puts back the last step `by` undid (see [`History::redo`]).
    pub fn redo(&mut self, by: &Author, at: Time) -> Written {
        let ops = self.history.redo(&mut self.canvas, by, at);
        self.write_all(&ops)
    }

    /// One page: the one with this id (`p<n>`) or name, or the first.
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] when no page has that id or name.
    pub fn show(&self, page: Option<&str>) -> Result<Shown> {
        let canvas = &self.canvas;
        let page = self.page(page)?;
        let pages = canvas
            .pages()
            .map(|p| serde_json::json!({ "id": json::page_id(p.id()), "name": p.name() }))
            .collect();
        let items = canvas
            .items(page.id())
            .filter_map(|item| serde_json::from_str(&json::whole_item(item)).ok())
            .collect();
        Ok(Shown { page: json::page_id(page.id()), pages, items })
    }

    /// Every op the canvas applied, in order, and the `n` of each op's line.
    pub(super) fn applied(&self) -> (&[Op], &[u64]) {
        (&self.ops, &self.lines)
    }

    /// The page with this id (`p<n>`) or name, or the first.
    pub(super) fn page(&self, page: Option<&str>) -> Result<&Page> {
        let canvas = &self.canvas;
        let found = match page {
            None => canvas.pages().next(),
            Some(p) => json::parse_page(p)
                .ok()
                .and_then(|id| canvas.page(id))
                .or_else(|| canvas.page_named(p)),
        };
        found.ok_or_else(|| Error::NotFound {
            what: format!("page {:?}", page.unwrap_or_default()),
            hint: Some("pass a page id (p1) or name from 'slinger canvas show'".into()),
        })
    }

    fn write_all(&mut self, ops: &[Op]) -> Written {
        let mut written = Written::default();
        for op in ops {
            self.write(op, Applied::Changed, &mut written);
        }
        written
    }

    fn write(&mut self, op: &Op, applied: Applied, written: &mut Written) {
        if let Some(line) = Line::of(self.next, op, applied) {
            self.ops.push(op.clone());
            self.lines.push(self.next);
            let line = line.encode();
            written.bytes.push_str(&line);
            let end = self.ends.last().copied().unwrap_or_default();
            self.ends.push(end + line.len() as u64);
            written.seq.push(self.next);
            self.next += 1;
        }
    }

    /// Reads one requested operation into the actions that do it.
    fn read_op(&self, m: &Fields, out: &mut Vec<Action>) -> Result<()> {
        let page = || json::text(m, "page").and_then(json::parse_page);
        let name = || json::text(m, "name").map(str::to_owned);
        match json::text(m, "op").map_err(bad)? {
            "add" => {
                let mut item = json::field(m, "item")
                    .and_then(|i| i.as_object().ok_or_else(|| "`item` is not an object".into()))
                    .map_err(bad)?
                    .clone();
                item.entry("color").or_insert_with(|| "#000000".into());
                for key in ["from", "to"] {
                    let Some(n) = item.get(key).and_then(Value::as_str) else { continue };
                    let Some(n) = n.strip_prefix('$') else { continue };
                    let made = self.made(out, n).ok_or_else(|| {
                        bad(format!("`{key}` \"${n}\" names no item an `add` before it made"))
                    })?;
                    item.insert(key.into(), json::item_id(made).into());
                }
                let page = json::text(&item, "page").and_then(json::parse_page).map_err(bad)?;
                let shape = read_shape(&item)?;
                out.push(Action::Add { page, shape });
            }
            "remove" => {
                let ids = json::ids(m, "ids").map_err(bad)?;
                out.extend(ids.into_iter().map(|item| Action::Remove { item }));
            }
            "move" => {
                let at = |key| json::int(m, key).map(json::units).map_err(bad);
                let by = Point::new(at("dx")?, at("dy")?);
                let ids = json::ids(m, "ids").map_err(bad)?;
                out.extend(ids.into_iter().map(|item| Action::Move { item, by }));
            }
            "set" => {
                let item = json::text(m, "id").and_then(json::parse_item).map_err(bad)?;
                let fields = json::field(m, "fields")
                    .and_then(|f| f.as_object().ok_or_else(|| "`fields` is not an object".into()))
                    .map_err(bad)?;
                let current = self.canvas.item(item).ok_or_else(|| Error::NotFound {
                    what: item.to_string(),
                    hint: Some("it may have been removed; show the canvas again".into()),
                })?;
                let merged = json::laid_over(current.shape(), fields);
                out.push(Action::Edit { item, shape: read_shape(&merged)? });
            }
            "page.add" => out.push(Action::PageAdd { name: name().map_err(bad)? }),
            "page.rename" => {
                let (page, name) = (page().map_err(bad)?, name().map_err(bad)?);
                out.push(Action::PageRename { page, name });
            }
            "page.remove" => out.push(Action::PageRemove { page: page().map_err(bad)? }),
            other => return Err(bad(format!("{other:?} is not an operation a client may send"))),
        }
        Ok(())
    }

    /// The item an arrow's end `$n` names: the one the `add` of `out` that
    /// is `made[n]` (counting from 0) will make. Adds take ids in order,
    /// from the canvas's next. `None` when `n` is not a number, or names a
    /// page or nothing made yet.
    fn made(&self, out: &[Action], n: &str) -> Option<ItemId> {
        let n: usize = n.parse().ok()?;
        let mut next = self.canvas.next_item().get();
        let mut made = out.iter().filter_map(|action| match action {
            Action::Add { .. } => {
                next += 1;
                Some(Some(ItemId::new(next - 1)))
            }
            Action::PageAdd { .. } => Some(None),
            _ => None,
        });
        made.nth(n)?
    }
}

/// The operation lines of a [`Span`] as read from the file, each as its
/// JSON object with every field it has.
///
/// # Errors
///
/// [`Error::CanvasCorrupt`] for a line that is not a JSON object; `line` is
/// then its place in `bytes`, from 1.
pub fn lines(bytes: &[u8]) -> Result<Vec<Value>> {
    let rows = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let read = |(i, row): (usize, &[u8])| match serde_json::from_slice(row) {
        Ok(Value::Object(m)) => Ok(Value::Object(m)),
        _ => Err(Error::CanvasCorrupt { line: i + 1, message: "not a JSON object".into() }),
    };
    rows.split(|&b| b == b'\n').enumerate().map(read).collect()
}

fn read_shape(m: &Fields) -> Result<Shape> {
    json::read_shape(m).map_err(bad)?.ok_or_else(|| bad("`kind` is not an item kind".into()))
}

/// A request that does not fit the operations.
fn bad(why: String) -> Error {
    invalid(why, OPS_HINT)
}

fn invalid(message: impl Into<String>, hint: &str) -> Error {
    Error::Invalid { message: message.into(), hint: Some(hint.into()) }
}
