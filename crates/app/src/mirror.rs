//! A copy of one canvas page as the server holds it (docs/service.md): made
//! from `canvas_show`, then kept up to date by applying the change feed's
//! operations in order. Pure. It owns the canvas file's item objects
//! (docs/design/canvas-file.md): lengths in tenths of a unit, stroke and
//! arrow points as a delta array, colours as `#rrggbb`. It reads them, and
//! writes the pen's changes as the operations of one `canvas_apply` ([`ops`]).
//!
//! What it cannot draw it leaves out (an item of a kind it does not know, an
//! operation it does not know); the server keeps those, so nothing is lost.
//! Items keep the server's drawing order, which is the order of their ids.

use serde_json::{Map, Value};
use slinger_control::{CanvasFeed, CanvasPage};

use crate::config::Rgb;
use crate::layout::{Point, Rect};
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::ink::{Carried, Change, Stroke};
use crate::scene::pages::{Page, PageOp};

/// The height of text, in canvas units: the packet's text size, so the pane
/// and the images an agent reads agree.
pub const TEXT_SIZE: f64 = 24.0;
/// The line width of boxes and arrows, in canvas units, as the packet draws them.
pub const LINE_WIDTH: f64 = 2.0;
/// About how wide a character of text is, and how tall a line, as a part of
/// [`TEXT_SIZE`]: for text whose stored frame is too narrow to wrap in.
const CHAR_WIDTH: f64 = 0.6;
const LINE_HEIGHT: f64 = 1.3;

/// One page of a canvas, and where in the canvas's operations it stands.
#[derive(Debug, Clone, PartialEq)]
pub struct Mirror {
    page: String,
    /// The canvas's pages, in order.
    pages: Vec<Page>,
    /// The page's items that can be drawn, by id, and the object each was
    /// read from (to lay a `set` over). Both are in id order.
    items: Vec<Item>,
    objects: Vec<Map<String, Value>>,
}

/// What [`Mirror::apply`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// Nothing on the page changed.
    Same,
    /// The page's items changed: draw them again.
    Changed,
    /// The page was removed: show the canvas again, from its first page.
    Gone,
}

impl Mirror {
    /// The page `canvas_show` gave.
    pub fn new(shown: &CanvasPage) -> Self {
        let pages = shown.pages.iter().filter_map(page).collect();
        let mut mirror =
            Self { page: shown.page.clone(), pages, items: Vec::new(), objects: Vec::new() };
        for item in &shown.items {
            mirror.put(item);
        }
        mirror
    }

    /// The id of the page.
    pub fn page(&self) -> &str {
        &self.page
    }

    /// The page's items that can be drawn, bottom first.
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// The canvas's pages, in order.
    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    /// Items `ids` of this page as they would be carried to `page` (of this
    /// canvas or another) and moved by `by` canvas units: each item, and
    /// its object for an `add` there, without what the server gives an item
    /// (its id, maker and time) or what names items of this canvas (an
    /// arrow's ends). Ids not on this page are left out.
    pub fn carried(&self, ids: &[ItemId], page: &str, by: Point) -> Vec<Carried> {
        let (dx, dy) = (Value::from(tenths(by.x)), Value::from(tenths(by.y)));
        let found = self.items.iter().zip(&self.objects).filter(|(i, _)| ids.contains(&i.id));
        let mut out = Vec::new();
        for (_, object) in found {
            let mut object = object.clone();
            shift(&mut object, &dx, &dy);
            object.remove("from");
            object.remove("to");
            let Some(item) = read(&object) else { continue };
            for key in ["id", "by", "t"] {
                object.remove(key);
            }
            object.insert("page".into(), page.into());
            out.push(Carried { item, object: Value::Object(object) });
        }
        out
    }

    /// Applies the operations of one reply of the change feed, in order.
    pub fn apply(&mut self, feed: &CanvasFeed) -> Applied {
        let mut applied = Applied::Same;
        for op in &feed.ops {
            match self.apply_one(op) {
                Applied::Same => {}
                Applied::Changed => applied = Applied::Changed,
                Applied::Gone => return Applied::Gone,
            }
        }
        applied
    }

    fn apply_one(&mut self, op: &Value) -> Applied {
        let text = |key: &str| op.get(key).and_then(Value::as_str);
        let changed = match text("op").unwrap_or_default() {
            "add" | "restore" => op.get("item").is_some_and(|item| self.put(item)),
            "remove" => ids(op).fold(false, |any, id| self.remove(id) | any),
            "move" => {
                let zero = Value::from(0);
                let dx = op.get("dx").unwrap_or(&zero);
                let dy = op.get("dy").unwrap_or(&zero);
                ids(op).fold(false, |any, id| self.change(id, |o| shift(o, dx, dy)) | any)
            }
            "set" => {
                let Some(fields) = op.get("fields").and_then(Value::as_object) else {
                    return Applied::Same;
                };
                text("id").and_then(item_id).is_some_and(|id| {
                    self.change(id, |o| {
                        for (key, value) in fields {
                            if value.is_null() {
                                o.remove(key);
                            } else {
                                o.insert(key.clone(), value.clone());
                            }
                        }
                    })
                })
            }
            "page.add" => {
                self.pages.extend(page(op));
                false
            }
            "page.rename" => {
                let found = self.pages.iter_mut().find(|p| Some(p.id.as_str()) == text("page"));
                if let (Some(found), Some(name)) = (found, text("name")) {
                    found.name = name.to_string();
                }
                false
            }
            "page.restore" => {
                let index = op.get("index").and_then(Value::as_u64).unwrap_or(u64::MAX);
                let index = usize::try_from(index).unwrap_or(usize::MAX).min(self.pages.len());
                if let Some(restored) = page(op) {
                    self.pages.insert(index, restored);
                }
                let items = op.get("items").and_then(Value::as_array);
                items.into_iter().flatten().fold(false, |any, item| self.put(item) | any)
            }
            "page.remove" if text("page") == Some(self.page.as_str()) => return Applied::Gone,
            "page.remove" => {
                self.pages.retain(|p| Some(p.id.as_str()) != text("page"));
                false
            }
            // `page.move` and kinds
            // this app does not know are skipped, as the server's own reader does.
            _ => false,
        };
        if changed { Applied::Changed } else { Applied::Same }
    }

    /// Puts an item object in its place by id when it is on this page and
    /// can be drawn; returns whether it was put.
    fn put(&mut self, item: &Value) -> bool {
        let Some(object) = item.as_object() else { return false };
        if object.get("page").and_then(Value::as_str) != Some(&self.page) {
            return false;
        }
        let Some(read) = read(object) else { return false };
        match self.items.binary_search_by_key(&read.id, |i| i.id) {
            Ok(at) => (self.items[at], self.objects[at]) = (read, object.clone()),
            Err(at) => {
                self.items.insert(at, read);
                self.objects.insert(at, object.clone());
            }
        }
        true
    }

    fn remove(&mut self, id: ItemId) -> bool {
        let Ok(at) = self.items.binary_search_by_key(&id, |i| i.id) else { return false };
        self.items.remove(at);
        self.objects.remove(at);
        true
    }

    /// Changes the object of item `id` and reads it again; an item that no
    /// longer reads is dropped. Returns whether the item was here.
    fn change(&mut self, id: ItemId, edit: impl FnOnce(&mut Map<String, Value>)) -> bool {
        let Ok(at) = self.items.binary_search_by_key(&id, |i| i.id) else { return false };
        edit(&mut self.objects[at]);
        match read(&self.objects[at]) {
            Some(item) => self.items[at] = item,
            None => {
                self.remove(id);
            }
        }
        true
    }
}

/// The page an object names: `{"id", "name"}` as `canvas_show` lists it, or
/// an operation's `page` and `name`. A page without a name is named by its id.
fn page(o: &Value) -> Option<Page> {
    let id = o.get("id").or_else(|| o.get("page"))?.as_str()?.to_string();
    let name = o.get("name").and_then(Value::as_str).map_or_else(|| id.clone(), str::to_string);
    Some(Page { id, name })
}

/// The item ids an operation's `ids` names.
fn ids(op: &Value) -> impl Iterator<Item = ItemId> + '_ {
    let ids = op.get("ids").and_then(Value::as_array);
    ids.into_iter().flatten().filter_map(|id| item_id(id.as_str()?))
}

/// Item 12 is `i12`, as the canvas file and the daemon's verbs write it.
pub fn item_ref(id: ItemId) -> String {
    format!("i{}", id.0)
}

/// `i12` is item 12.
fn item_id(text: &str) -> Option<ItemId> {
    text.strip_prefix('i')?.parse().ok().map(ItemId)
}

/// Moves an item object by `dx`, `dy` tenths: its first point, or its frame.
fn shift(o: &mut Map<String, Value>, dx: &Value, dy: &Value) {
    if let Some(pts) = o.get_mut("pts").and_then(Value::as_array_mut) {
        if let [x, y, ..] = pts.as_mut_slice() {
            (*x, *y) = (sum(x, dx), sum(y, dy));
        }
        return;
    }
    for (key, by) in [("x", dx), ("y", dy)] {
        if let Some(v) = o.get_mut(key) {
            *v = sum(v, by);
        }
    }
}

/// Two stored numbers added: whole when both are.
fn sum(a: &Value, b: &Value) -> Value {
    match (a.as_i64(), b.as_i64()) {
        (Some(a), Some(b)) => Value::from(a.saturating_add(b)),
        _ => Value::from(a.as_f64().unwrap_or(0.0) + b.as_f64().unwrap_or(0.0)),
    }
}

/// An item object as an [`Item`]; `None` for a kind this app does not draw
/// or an object without the fields its kind needs.
fn read(o: &Map<String, Value>) -> Option<Item> {
    let id = item_id(o.get("id")?.as_str()?)?;
    let units = |key: &str| o.get(key).and_then(Value::as_f64).map(|v| v / 10.0);
    let frame = || {
        let (x, y) = (units("x")?, units("y")?);
        Some(Rect { x, y, width: units("w").unwrap_or(0.0), height: units("h").unwrap_or(0.0) })
    };
    let shape = match o.get("kind")?.as_str()? {
        "stroke" => Shape::Stroke { points: points(o.get("pts")?)?, width: units("width")? },
        "text" => {
            let text = o.get("text")?.as_str()?.to_string();
            Shape::Text { frame: text_frame(frame()?, &text), text, size: TEXT_SIZE }
        }
        "box" => Shape::Box { frame: frame()?, width: LINE_WIDTH },
        "live" => {
            let text = |key: &str| o.get(key).and_then(Value::as_str).map(str::to_string);
            Shape::Live { frame: frame()?, name: text("ui")?, open: text("open")? }
        }
        "arrow" => {
            let points = points(o.get("pts")?)?;
            let (&from, &to) = (points.first()?, points.last()?);
            let end = |key: &str| o.get(key).and_then(Value::as_str).and_then(item_id);
            let bound = [end("from"), end("to")];
            (points.len() >= 2).then_some(Shape::Arrow { from, to, width: LINE_WIDTH, bound })?
        }
        _ => return None,
    };
    let colour = o.get("color").and_then(Value::as_str).and_then(colour);
    Some(Item { id, colour: colour.unwrap_or(Rgb { r: 0, g: 0, b: 0 }), shape })
}

/// A stored delta array, `[x0, y0, dx1, dy1, ...]` in tenths, as points.
fn points(pts: &Value) -> Option<Vec<Point>> {
    let numbers: Vec<f64> = pts.as_array()?.iter().map(Value::as_f64).collect::<Option<_>>()?;
    if numbers.is_empty() || !numbers.len().is_multiple_of(2) {
        return None;
    }
    let mut at = Point { x: 0.0, y: 0.0 };
    let points = numbers.as_chunks::<2>().0.iter().map(|d| {
        (at.x, at.y) = (at.x + d[0], at.y + d[1]);
        Point { x: at.x / 10.0, y: at.y / 10.0 }
    });
    Some(points.collect())
}

/// The operations of the one `canvas_apply` that makes `change`: an `add`
/// (a stroke, a text, a box or an arrow), a `remove`, a `move` by whole
/// tenths with a `set` of each arrow that follows it, the `set` of an edit,
/// a carry's `add`s or a page's operation. An undo or a redo has none.
pub fn ops(change: &Change) -> Value {
    let ids = |ids: &[ItemId]| ids.iter().copied().map(item_ref).collect::<Vec<_>>();
    match change {
        Change::Add(stroke) => serde_json::json!({"op": "add", "item": stroke_object(stroke)}),
        Change::Remove { ids: gone, .. } => serde_json::json!({"op": "remove", "ids": ids(gone)}),
        Change::Move { ids: moved, by, follow, .. } => {
            let mut ops = vec![serde_json::json!({
                "op": "move",
                "ids": ids(moved),
                "dx": tenths(by.x),
                "dy": tenths(by.y),
            })];
            ops.extend(follow.iter().map(|arrow| set(arrow.id, &arrow.shape)));
            if ops.len() == 1 { ops.swap_remove(0) } else { Value::Array(ops) }
        }
        Change::Put { page, colour, shape, .. } => {
            let mut item = fields(shape);
            let Rgb { r, g, b } = *colour;
            item.insert("color".into(), format!("#{r:02x}{g:02x}{b:02x}").into());
            item.insert("page".into(), page.as_str().into());
            serde_json::json!({"op": "add", "item": item})
        }
        Change::Set { id, shape, .. } => set(*id, shape),
        Change::Carry { items, .. } => {
            let add = |c: &Carried| serde_json::json!({"op": "add", "item": c.object});
            Value::Array(items.iter().map(add).collect())
        }
        Change::Page { op, .. } => match op {
            PageOp::Add { name } => serde_json::json!({"op": "page.add", "name": name}),
            PageOp::Rename { page, name } => {
                serde_json::json!({"op": "page.rename", "page": page, "name": name})
            }
            PageOp::Remove { page } => serde_json::json!({"op": "page.remove", "page": page}),
        },
        // An undo or a redo is a verb of its own, with no operations.
        Change::Step { .. } => Value::Array(Vec::new()),
    }
}

/// A `set` that makes item `id` the shape `shape`, every field of it given
/// (an arrow end bound to nothing as `null`, which clears it).
fn set(id: ItemId, shape: &Shape) -> Value {
    let mut fields = fields(shape);
    fields.remove("kind");
    if matches!(shape, Shape::Arrow { .. }) {
        for end in ["from", "to"] {
            fields.entry(end).or_insert(Value::Null);
        }
    }
    serde_json::json!({"op": "set", "id": format!("i{}", id.0), "fields": fields})
}

/// The item fields of a text, a box or an arrow, with its kind, in tenths:
/// a frame as `x`, `y`, `w`, `h`; an arrow's ends as a delta array, and the
/// items they point at as `from` and `to`. A stroke is written whole by
/// [`stroke_object`], so it has none here.
fn fields(shape: &Shape) -> Map<String, Value> {
    let mut o = Map::new();
    let frame = |o: &mut Map<String, Value>, r: &Rect| {
        for (key, v) in [("x", r.x), ("y", r.y), ("w", r.width), ("h", r.height)] {
            o.insert(key.into(), tenths(v).into());
        }
    };
    match shape {
        Shape::Text { frame: r, text, .. } => {
            o.insert("kind".into(), "text".into());
            frame(&mut o, r);
            o.insert("text".into(), text.as_str().into());
        }
        Shape::Box { frame: r, .. } => {
            o.insert("kind".into(), "box".into());
            frame(&mut o, r);
        }
        Shape::Arrow { from, to, bound, .. } => {
            let (a, b) = ((tenths(from.x), tenths(from.y)), (tenths(to.x), tenths(to.y)));
            o.insert("kind".into(), "arrow".into());
            o.insert("pts".into(), serde_json::json!([a.0, a.1, b.0 - a.0, b.1 - a.1]));
            for (key, end) in [("from", bound[0]), ("to", bound[1])] {
                if let Some(id) = end {
                    o.insert(key.into(), format!("i{}", id.0).into());
                }
            }
        }
        // Only an agent makes or changes a live item.
        Shape::Stroke { .. } | Shape::Live { .. } => {}
    }
    o
}

/// The frame of `text` set with its top left at `at`, as the canvas file
/// stores it: a character cell of [`TEXT_SIZE`] a character of its longest
/// line (the core's render cell, so the text never wraps there), and a line
/// of [`LINE_HEIGHT`] a line, on whole tenths. Empty text is one cell.
pub fn text_frame_at(at: Point, text: &str) -> Rect {
    let count = |n: usize| u32::try_from(n).map_or(f64::from(u32::MAX), f64::from);
    let longest = text.split('\n').map(|l| l.chars().count()).max().unwrap_or(0).max(1);
    let lines = count(text.split('\n').count());
    Rect {
        x: at.x,
        y: at.y,
        width: count(longest) * TEXT_SIZE,
        height: (lines * TEXT_SIZE * LINE_HEIGHT * 10.0).round() / 10.0,
    }
}

/// A length in canvas units as the whole tenths the canvas file stores.
#[expect(clippy::cast_possible_truncation, reason = "canvas units are far inside i64")]
fn tenths(v: f64) -> i64 {
    (v * 10.0).round() as i64
}

/// A pen stroke as the item object an `add` takes: its page, `#rrggbb`, its
/// width and its points as a delta array, each value rounded to the tenth
/// before the deltas are taken (so error does not build up along it). A
/// width is at least one tenth.
fn stroke_object(stroke: &Stroke) -> Value {
    let mut pts = Vec::with_capacity(stroke.points.len() * 2);
    let mut last = (0, 0);
    for p in &stroke.points {
        let at = (tenths(p.x), tenths(p.y));
        pts.extend([at.0 - last.0, at.1 - last.1]);
        last = at;
    }
    let Rgb { r, g, b } = stroke.colour;
    serde_json::json!({
        "page": stroke.page,
        "kind": "stroke",
        "color": format!("#{r:02x}{g:02x}{b:02x}"),
        "width": tenths(stroke.width).max(1),
        "pts": pts,
    })
}

/// The frame text is drawn in. A stored frame narrower than one character
/// would wrap the text a character a line; such text is drawn unwrapped, in
/// a frame as big as it is likely to be.
fn text_frame(frame: Rect, text: &str) -> Rect {
    if frame.width >= TEXT_SIZE {
        return frame;
    }
    let count = |n: usize| u32::try_from(n).map_or(f64::from(u32::MAX), f64::from);
    let longest = text.split('\n').map(|l| l.chars().count()).max().unwrap_or(0);
    let lines = count(text.split('\n').count());
    Rect {
        width: count(longest) * TEXT_SIZE * CHAR_WIDTH,
        height: lines * TEXT_SIZE * LINE_HEIGHT,
        ..frame
    }
}

/// `#rrggbb`, or `#rrggbbaa` with its alpha dropped; `None` for anything else.
fn colour(text: &str) -> Option<Rgb> {
    let hex = text.strip_prefix('#')?;
    if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some(Rgb { r: byte(0)?, g: byte(2)?, b: byte(4)? })
}

#[cfg(test)]
mod tests;
