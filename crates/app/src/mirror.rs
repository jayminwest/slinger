//! A copy of one canvas page as the server holds it (docs/service.md): made
//! from `canvas_show`, then kept up to date by applying the change feed's
//! operations in order. Pure. It owns the canvas file's item objects
//! (docs/design/canvas-file.md): lengths in tenths of a unit, stroke and
//! arrow points as a delta array, colours as `#rrggbb`. It reads them, and
//! writes the pen's changes as the operations of one `canvas_apply` ([`ops`]),
//! and its items as the clipboard holds them ([`clip`]).
//!
//! What it cannot draw it leaves out (an item of a kind it does not know, an
//! operation it does not know); the server keeps those, so nothing is lost.
//! Items keep the server's drawing order, which is the order of their ids.

use serde_json::{Map, Value};
use slinger_control::{CanvasFeed, CanvasPage};

use crate::config::Rgb;
use crate::layout::{Point, Rect};
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::glyphs;
use crate::scene::ink::Carried;
use crate::scene::pages::Page;

pub mod clip;
mod write;

pub use write::ops;
use write::tenths;
#[cfg(test)]
pub use write::text_frame_at;

/// The height of text, in canvas units: the packet's text size, so the pane
/// and the images an agent reads agree.
pub const TEXT_SIZE: f64 = 24.0;
/// The line width of boxes and arrows, in canvas units, as the packet draws them.
pub const LINE_WIDTH: f64 = 2.0;
/// The smallest and the largest text size, in canvas units, as the core
/// takes them (`slinger_core::canvas::TEXT_SIZES`).
pub const TEXT_SIZES: std::ops::RangeInclusive<f64> = 4.0..=400.0;
/// How tall a line of text is in a stored frame, as a part of its size.
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

    /// Who made the item at index `at` of [`Mirror::items`]: `user`, or
    /// `agent:<name>`.
    pub fn by(&self, at: usize) -> Option<&str> {
        self.objects.get(at)?.get("by")?.as_str()
    }

    /// The canvas's pages, in order.
    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    /// Items `ids` of this page as they would be carried to `page` (of this
    /// canvas or another) and moved by `by` canvas units: each item, and
    /// its object for an `add` there, without what the server gives an item
    /// (its id, maker and time). An arrow's end bound to an item carried
    /// with it stays bound to that item there, named by its place in the
    /// adds (`$n`, docs/service.md), so such an arrow comes after every
    /// other item; an end bound to an item left here is let go. Ids not on
    /// this page are left out.
    pub fn carried(&self, ids: &[ItemId], page: &str, by: Point) -> Vec<Carried> {
        let (dx, dy) = (Value::from(tenths(by.x)), Value::from(tenths(by.y)));
        let found = self.items.iter().zip(&self.objects).filter(|(i, _)| ids.contains(&i.id));
        let bound_here = |i: &Item| match i.shape {
            Shape::Arrow { bound, .. } => bound.iter().flatten().any(|end| ids.contains(end)),
            _ => false,
        };
        let (last, first): (Vec<_>, Vec<_>) = found.partition(|(i, _)| bound_here(i));
        let mut out = Vec::new();
        let mut places = Vec::new();
        for (old, object) in first.into_iter().chain(last) {
            let mut object = object.clone();
            shift(&mut object, &dx, &dy);
            let ends = ["from", "to"].map(|key| object.remove(key));
            let Some(item) = read(&object) else { continue };
            for (key, end) in ["from", "to"].into_iter().zip(ends) {
                let end = end.as_ref().and_then(Value::as_str).and_then(item_id);
                if let Some(n) = places.iter().position(|id| Some(*id) == end) {
                    object.insert(key.into(), format!("${n}").into());
                }
            }
            for key in ["id", "by", "t"] {
                object.remove(key);
            }
            object.insert("page".into(), page.into());
            places.push(old.id);
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
            // The feed says who added an item on the operation, `canvas_show` on the item.
            "add" | "restore" => op.get("item").is_some_and(|item| {
                let mut item = item.clone();
                if let (Some(o), Some(by)) = (item.as_object_mut(), op.get("by")) {
                    o.entry("by").or_insert_with(|| by.clone());
                }
                self.put(&item)
            }),
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
pub fn item_id(text: &str) -> Option<ItemId> {
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
            let size = units("size").filter(|s| TEXT_SIZES.contains(s)).unwrap_or(TEXT_SIZE);
            Shape::Text { frame: drawn_frame(frame()?, &text, size), text, size }
        }
        "box" => Shape::Box { frame: frame()?, width: LINE_WIDTH },
        "button" => Shape::Button { frame: frame()?, status: None },
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

/// The frame a stored text of `size` is drawn in, and hit in: the box its
/// glyphs fill ([`glyphs::frame`]), wrapped to its stored width, never the
/// stored width itself. A stored frame narrower than one character would
/// wrap the text a character a line; such text is drawn unwrapped.
fn drawn_frame(frame: Rect, text: &str, size: f64) -> Rect {
    let column = if frame.width >= size { frame.width } else { glyphs::UNWRAPPED };
    glyphs::frame(Point { x: frame.x, y: frame.y }, text, size, column)
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
