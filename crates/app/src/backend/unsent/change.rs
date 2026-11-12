//! One kept change as the file holds it ([`write`], [`read`]): its keys
//! (`key`, `after`, `once`), its `canvas`, what it is (`change`) and that
//! kind's fields. Items are the canvas file's objects
//! ([`crate::mirror::object`]), ids as `i<n>`; a stroke's points and a
//! move's distance are canvas units, as the ink holds them.
//!
//! ```json
//! {"key": 4, "after": null, "once": "4021-1762790400.250000000.4",
//!  "canvas": "notes", "change": "remove", "page": "p1", "ids": ["i7"]}
//! ```
//!
//! An entry without `change` is a stroke: the first files kept only those.

use serde_json::{Value, json};

use crate::config::Rgb;
use crate::layout::Point;
use crate::mirror::{item, item_id, item_ref, object};
use crate::scene::canvas::{Item, ItemId};
use crate::scene::ink::{Carried, Change, Kept, Stroke};
use crate::scene::pages::PageOp;

pub(super) fn write(kept: &Kept) -> Value {
    let mut o = match &kept.change {
        Change::Add(stroke) => {
            let Rgb { r, g, b } = stroke.colour;
            json!({"change": "add", "page": stroke.page, "colour": [r, g, b],
                "width": stroke.width, "points": flat(&stroke.points)})
        }
        Change::Remove { page, ids: gone, .. } => {
            json!({"change": "remove", "page": page, "ids": ids(gone)})
        }
        Change::Move { page, ids: moved, by, follow, .. } => json!({"change": "move", "page": page,
            "ids": ids(moved), "by": [by.x, by.y], "follow": objects(follow)}),
        Change::Put { page, colour, shape, .. } => {
            let put = Item { id: ItemId(0), colour: *colour, shape: shape.clone() };
            json!({"change": "put", "page": page, "item": object(&put)})
        }
        Change::Set { page, id, shape, .. } => {
            let set = Item { id: *id, colour: Rgb { r: 0, g: 0, b: 0 }, shape: shape.clone() };
            json!({"change": "set", "page": page, "item": object(&set)})
        }
        Change::Reshape { page, items, .. } => {
            json!({"change": "reshape", "page": page, "items": objects(items)})
        }
        Change::Carry { page, items, gone, .. } => {
            let carried = |c: &Carried| json!({"item": object(&c.item), "object": c.object});
            let items: Vec<Value> = items.iter().map(carried).collect();
            json!({"change": "carry", "page": page, "items": items, "gone": ids(gone)})
        }
        Change::Page { op, .. } => match op {
            PageOp::Add { name } => json!({"change": "page", "op": "add", "name": name}),
            PageOp::Rename { page, name } => {
                json!({"change": "page", "op": "rename", "page": page, "name": name})
            }
            PageOp::Remove { page } => json!({"change": "page", "op": "remove", "page": page}),
        },
        // Never kept (`Ink::unsent`): an undo has no fields.
        Change::Step { redo, .. } => json!({"change": "step", "redo": redo}),
    };
    if let Some(o) = o.as_object_mut() {
        o.insert("canvas".into(), kept.change.canvas().into());
        o.insert("key".into(), kept.key.into());
        o.insert("after".into(), kept.after.into());
        o.insert("once".into(), kept.once.clone().into());
    }
    o
}

/// The kept change `o` writes; `None` for one this app cannot read.
pub(super) fn read(o: &Value) -> Option<Kept> {
    let key = |name: &str| o.get(name).and_then(Value::as_u64);
    let once = o.get("once").and_then(Value::as_str).map(str::to_string);
    Some(Kept { key: key("key"), after: key("after"), once, change: change(o)? })
}

fn change(o: &Value) -> Option<Change> {
    let text = |key: &str| o.get(key)?.as_str().map(str::to_string);
    let canvas = text("canvas")?;
    let kind = o.get("change").map_or(Some("add"), Value::as_str)?;
    if kind == "page" {
        return Some(Change::Page { canvas, op: page_op(o)? });
    }
    let page = text("page")?;
    let ids = |key: &str| -> Option<Vec<ItemId>> {
        o.get(key)?.as_array()?.iter().map(|id| item_id(id.as_str()?)).collect()
    };
    let items =
        |key: &str| -> Option<Vec<Item>> { o.get(key)?.as_array()?.iter().map(item).collect() };
    Some(match kind {
        "add" => Change::Add(stroke(o, canvas, page)?),
        "remove" => Change::Remove { canvas, page, ids: ids("ids")? },
        "move" => {
            let [x, y] = o.get("by")?.as_array()?.as_slice() else { return None };
            let by = Point { x: x.as_f64()?, y: y.as_f64()? };
            Change::Move { canvas, page, ids: ids("ids")?, by, follow: items("follow")? }
        }
        "put" => {
            // Under id 0: it has none until the daemon makes it.
            let Item { colour, shape, .. } = item(o.get("item")?)?;
            Change::Put { canvas, page, colour, shape }
        }
        "set" => {
            let Item { id, shape, .. } = item(o.get("item")?)?;
            Change::Set { canvas, page, id, shape }
        }
        "reshape" => Change::Reshape { canvas, page, items: items("items")? },
        "carry" => Change::Carry { canvas, page, items: carried(o)?, gone: ids("gone")? },
        _ => return None,
    })
}

fn carried(o: &Value) -> Option<Vec<Carried>> {
    let carried =
        |c: &Value| Some(Carried { item: item(c.get("item")?)?, object: c.get("object")?.clone() });
    o.get("items")?.as_array()?.iter().map(carried).collect()
}

fn page_op(o: &Value) -> Option<PageOp> {
    let text = |key: &str| o.get(key)?.as_str().map(str::to_string);
    Some(match text("op")?.as_str() {
        "add" => PageOp::Add { name: text("name")? },
        "rename" => PageOp::Rename { page: text("page")?, name: text("name")? },
        "remove" => PageOp::Remove { page: text("page")? },
        _ => return None,
    })
}

fn stroke(o: &Value, canvas: String, page: String) -> Option<Stroke> {
    let byte = |v: &Value| u8::try_from(v.as_u64()?).ok();
    let [r, g, b] = o.get("colour")?.as_array()?.as_slice() else { return None };
    let numbers: Vec<f64> =
        o.get("points")?.as_array()?.iter().map(Value::as_f64).collect::<Option<_>>()?;
    if numbers.is_empty() || !numbers.len().is_multiple_of(2) {
        return None;
    }
    Some(Stroke {
        canvas,
        page,
        colour: Rgb { r: byte(r)?, g: byte(g)?, b: byte(b)? },
        width: o.get("width")?.as_f64()?,
        points: numbers.as_chunks::<2>().0.iter().map(|[x, y]| Point { x: *x, y: *y }).collect(),
    })
}

fn flat(points: &[Point]) -> Vec<f64> {
    points.iter().flat_map(|p| [p.x, p.y]).collect()
}

fn ids(ids: &[ItemId]) -> Vec<String> {
    ids.iter().copied().map(item_ref).collect()
}

fn objects(items: &[Item]) -> Vec<Value> {
    items.iter().filter_map(object).collect()
}
