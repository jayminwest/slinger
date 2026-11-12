//! The pen's changes as the canvas file's operations ([`ops`]), and the
//! item fields they write: the writing half of the mirror.

use serde_json::{Map, Value};

use super::{LINE_HEIGHT, TEXT_SIZE, item_ref};
use crate::config::Rgb;
use crate::layout::{Point, Rect};
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::ink::{Carried, Change, Stroke};
use crate::scene::pages::PageOp;

/// The operations of the one `canvas_apply` that makes `change`: an `add`
/// (a stroke, a text, a box or an arrow), a `remove`, a `move` by whole
/// tenths with a `set` of each arrow that follows it, the `set` of an edit,
/// a `set` of each item a resize changes, a carry's `add`s or a page's
/// operation. An undo or a redo has none.
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
        Change::Reshape { items, .. } => Value::Array(items.iter().map(reshape).collect()),
        Change::Carry { items, gone, .. } => {
            let add = |c: &Carried| serde_json::json!({"op": "add", "item": c.object});
            let mut ops: Vec<Value> = items.iter().map(add).collect();
            if !gone.is_empty() {
                ops.push(serde_json::json!({"op": "remove", "ids": ids(gone)}));
            }
            Value::Array(ops)
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

/// The `set` that draws `item` as it is: a stroke by its points, an agent
/// UI by its place (only an agent sizes it), anything else as [`set`].
fn reshape(item: &Item) -> Value {
    let fields = match &item.shape {
        Shape::Stroke { points, .. } => serde_json::json!({"pts": deltas(points)}),
        Shape::Live { frame, .. } => {
            serde_json::json!({"x": tenths(frame.x), "y": tenths(frame.y)})
        }
        shape => return set(item.id, shape),
    };
    serde_json::json!({"op": "set", "id": item_ref(item.id), "fields": fields})
}

/// The item fields of a text, a box, a button or an arrow, with its kind, in tenths:
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
        // The file keeps the core's cells, whatever the glyphs fill.
        Shape::Text { frame: r, text, .. } => {
            o.insert("kind".into(), "text".into());
            frame(&mut o, &text_frame_at(Point { x: r.x, y: r.y }, text));
            o.insert("text".into(), text.as_str().into());
        }
        Shape::Box { frame: r, .. } => {
            o.insert("kind".into(), "box".into());
            frame(&mut o, r);
        }
        Shape::Button { frame: r, .. } => {
            o.insert("kind".into(), "button".into());
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
        // Chrome is the app's alone and never written.
        Shape::Stroke { .. } | Shape::Live { .. } | Shape::Panel { .. } | Shape::Caret { .. } => {}
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
pub(super) fn tenths(v: f64) -> i64 {
    (v * 10.0).round() as i64
}

/// A pen stroke as the item object an `add` takes: its page, `#rrggbb`, its
/// width and its points as a delta array, each value rounded to the tenth
/// before the deltas are taken (so error does not build up along it). A
/// width is at least one tenth.
pub(super) fn stroke_object(stroke: &Stroke) -> Value {
    let Rgb { r, g, b } = stroke.colour;
    serde_json::json!({
        "page": stroke.page,
        "kind": "stroke",
        "color": format!("#{r:02x}{g:02x}{b:02x}"),
        "width": tenths(stroke.width).max(1),
        "pts": deltas(&stroke.points),
    })
}

/// `points` as a delta array, `[x0, y0, dx1, dy1, ...]` in tenths, each
/// value rounded to the tenth before the deltas are taken (so error does
/// not build up along it).
fn deltas(points: &[Point]) -> Vec<i64> {
    let mut pts = Vec::with_capacity(points.len() * 2);
    let mut last = (0, 0);
    for p in points {
        let at = (tenths(p.x), tenths(p.y));
        pts.extend([at.0 - last.0, at.1 - last.1]);
        last = at;
    }
    pts
}
