//! The pen's changes as the canvas file's operations ([`ops`]), and the
//! item fields they write: the writing half of the mirror.

use serde_json::{Map, Value};

use super::{LINE_HEIGHT, TEXT_SIZE, item_ref};
use crate::config::Rgb;
use crate::layout::{Point, Rect};
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::glyphs;
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
            item.insert("color".into(), hex(*colour).into());
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

/// `item` as the canvas file's object, under its id: it reads back as
/// `item` ([`super::item`]), to the tenth. Chrome (a panel, a caret) has none.
pub fn object(item: &Item) -> Option<Value> {
    let mut o = match &item.shape {
        Shape::Stroke { points, width } => {
            let mut o = Map::new();
            o.insert("kind".into(), "stroke".into());
            o.insert("width".into(), tenths(*width).max(1).into());
            o.insert("pts".into(), deltas(points).into());
            o
        }
        Shape::Live { frame, name, open } => {
            let mut o = Map::new();
            o.insert("kind".into(), "live".into());
            for (key, v) in
                [("x", frame.x), ("y", frame.y), ("w", frame.width), ("h", frame.height)]
            {
                o.insert(key.into(), tenths(v).into());
            }
            o.insert("ui".into(), name.as_str().into());
            o.insert("open".into(), open.as_str().into());
            o
        }
        Shape::Panel { .. } | Shape::Caret { .. } => return None,
        shape => fields(shape),
    };
    o.insert("id".into(), item_ref(item.id).into());
    // A `pdf` item has no colour (docs/design/pdf.md).
    if !matches!(item.shape, Shape::Pdf { .. }) {
        o.insert("color".into(), hex(item.colour).into());
    }
    Some(Value::Object(o))
}

/// The item objects the `add`s of `change` write, in order, each under
/// the id in `ids` at its place (the id the ink shows it under), and an
/// end bound to another of its adds (`$n`) named by that one's id: as the
/// canvas file will hold them once written.
pub(super) fn added(change: &Change, ids: &[ItemId]) -> Vec<(ItemId, Map<String, Value>)> {
    let ops = match ops(change) {
        Value::Array(ops) => ops,
        op => vec![op],
    };
    let adds = ops.into_iter().filter(|op| op["op"] == "add");
    let objects = adds.filter_map(|op| match op.get("item") {
        Some(Value::Object(o)) => Some(o.clone()),
        _ => None,
    });
    let place = |end: &Value| {
        let n = end.as_str()?.strip_prefix('$')?.parse::<usize>().ok()?;
        ids.get(n).copied()
    };
    let mut out = Vec::new();
    for (id, mut object) in ids.iter().copied().zip(objects) {
        for key in ["from", "to"] {
            if let Some(end) = object.get(key).and_then(place) {
                object.insert(key.into(), item_ref(end).into());
            }
        }
        object.insert("id".into(), item_ref(id).into());
        out.push((id, object));
    }
    out
}

/// `object` changed to draw as `item` does, as a resize writes it
/// ([`reshape`]): a field set to `null` is cleared.
pub(super) fn draw(object: &mut Map<String, Value>, item: &Item) {
    let set = reshape(item);
    let Some(fields) = set.get("fields").and_then(Value::as_object) else { return };
    for (key, value) in fields {
        if value.is_null() {
            object.remove(key);
        } else {
            object.insert(key.clone(), value.clone());
        }
    }
}

/// A `set` that makes item `id` the shape `shape`, every field of it given
/// (an arrow end bound to nothing as `null`, which clears it, and a text of
/// the usual size with a `null` size).
fn set(id: ItemId, shape: &Shape) -> Value {
    let mut fields = fields(shape);
    fields.remove("kind");
    let optional: &[&str] = match shape {
        Shape::Arrow { .. } => &["from", "to"],
        Shape::Text { .. } => &["size"],
        Shape::Pdf { .. } => &["crop", "name"],
        _ => &[],
    };
    for &key in optional {
        fields.entry(key).or_insert(Value::Null);
    }
    serde_json::json!({"op": "set", "id": format!("i{}", id.0), "fields": fields})
}

/// The `set` that draws `item` as it is: a stroke by its points, an agent
/// UI by its frame, anything else as [`set`]; a stroke, a text, a box and
/// an arrow with its colour too (a recolour, [`crate::scene::select`]).
fn reshape(item: &Item) -> Value {
    let mut fields = match &item.shape {
        Shape::Stroke { points, .. } => serde_json::json!({"pts": deltas(points)}),
        Shape::Live { frame, .. } => serde_json::json!({
            "x": tenths(frame.x), "y": tenths(frame.y),
            "w": tenths(frame.width), "h": tenths(frame.height),
        }),
        shape => set(item.id, shape)["fields"].take(),
    };
    if matches!(
        item.shape,
        Shape::Stroke { .. } | Shape::Text { .. } | Shape::Box { .. } | Shape::Arrow { .. }
    ) {
        fields["color"] = hex(item.colour).into();
    }
    serde_json::json!({"op": "set", "id": item_ref(item.id), "fields": fields})
}

/// The item fields of a text, a box, a button, a `pdf` item or an arrow,
/// with its kind, in tenths:
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
        Shape::Text { frame: r, text, size, column } => {
            o.insert("kind".into(), "text".into());
            let unwrapped = text_frame_at(Point { x: r.x, y: r.y }, text, *size);
            // A column as wide as its longest line is one that does not wrap.
            let wraps = |w: &f64| tenths(*w) != tenths(unwrapped.width);
            match column.and_then(|c| stored_column(c, *size)).filter(wraps) {
                Some(width) => {
                    let rows = glyphs::rows(text, *size, width).split('\n').count();
                    let rows = u32::try_from(rows).map_or(f64::from(u32::MAX), f64::from);
                    frame(&mut o, &Rect { width, height: rows * size, ..unwrapped });
                }
                None => frame(&mut o, &unwrapped),
            }
            o.insert("text".into(), text.as_str().into());
            // The usual size is left out, as the core writes it.
            if tenths(*size) != tenths(TEXT_SIZE) {
                o.insert("size".into(), tenths(*size).into());
            }
        }
        Shape::Box { frame: r, .. } => {
            o.insert("kind".into(), "box".into());
            frame(&mut o, r);
        }
        Shape::Button { frame: r, .. } => {
            o.insert("kind".into(), "button".into());
            frame(&mut o, r);
        }
        Shape::Pdf { frame: r, hash, pdf_page, page_size: (w, h), crop, name } => {
            o.insert("kind".into(), "pdf".into());
            frame(&mut o, r);
            o.insert("hash".into(), hash.as_str().into());
            o.insert("pdfpage".into(), (*pdf_page).into());
            o.insert("pagesize".into(), serde_json::json!([tenths(*w), tenths(*h)]));
            if let Some(c) = crop {
                let crop = [c.x, c.y, c.width, c.height].map(tenths);
                o.insert("crop".into(), serde_json::json!(crop));
            }
            if let Some(name) = name {
                o.insert("name".into(), name.as_str().into());
            }
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
        // Only an agent makes a live item; a resize sets its frame ([`reshape`]).
        // Chrome is the app's alone and never written.
        Shape::Stroke { .. } | Shape::Live { .. } | Shape::Panel { .. } | Shape::Caret { .. } => {}
    }
    o
}

/// The `add` of a `pdf` item `shape` to `page` (docs/design/pdf.md): its
/// fields, without a colour.
pub fn add_pdf(page: &str, shape: &Shape) -> Value {
    let mut item = fields(shape);
    item.insert("page".into(), page.into());
    serde_json::json!({"op": "add", "item": item})
}

/// The frame of `text` of `size` set with its top left at `at`, as the
/// canvas file stores it: a character cell of `size` a character of its
/// longest line (the core's render cell, so the text never wraps there), and
/// a line of [`LINE_HEIGHT`] a line, on whole tenths. Empty text is one cell.
pub fn text_frame_at(at: Point, text: &str, size: f64) -> Rect {
    let count = |n: usize| u32::try_from(n).map_or(f64::from(u32::MAX), f64::from);
    let longest = text.split('\n').map(|l| l.chars().count()).max().unwrap_or(0).max(1);
    let lines = count(text.split('\n').count());
    Rect {
        x: at.x,
        y: at.y,
        width: count(longest) * size,
        height: (lines * size * LINE_HEIGHT * 10.0).round() / 10.0,
    }
}

/// The width a text of `size` wrapped in `column` is stored at: the column
/// on whole tenths, or, when rounding it there would change how many whole
/// cells of the stored size it holds, the middle of its last cell. `None`
/// for a text that does not wrap: a typed one ([`glyphs::UNWRAPPED`]), or
/// a column narrower than a cell (docs/design/canvas-file.md). So an agent's
/// paragraph resized, carried or edited reads back in the same rows.
#[expect(clippy::cast_precision_loss, reason = "canvas units are far inside 2^52 tenths")]
fn stored_column(column: f64, size: f64) -> Option<f64> {
    let cells = |w: f64, size: f64| (w >= size).then(|| (w / size).floor());
    let n = cells(column, size).filter(|_| column < glyphs::UNWRAPPED)?;
    let (w, cell) = (tenths(column), tenths(size));
    let w = if cells(w as f64 / 10.0, cell as f64 / 10.0) == Some(n) {
        w
    } else {
        // n is a whole count of cells far below 2^52.
        #[expect(clippy::cast_possible_truncation, reason = "a whole count of cells")]
        let n = n as i64;
        n * cell + cell / 2
    };
    Some(w as f64 / 10.0)
}

/// `colour` as the canvas file writes it: `#rrggbb`.
fn hex(Rgb { r, g, b }: Rgb) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
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
    serde_json::json!({
        "page": stroke.page,
        "kind": "stroke",
        "color": hex(stroke.colour),
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
