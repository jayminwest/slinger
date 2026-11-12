//! A canvas's items on the clipboard: Edit > Copy of the selection puts
//! their file objects there as a JSON array, as a carry writes them
//! ([`Mirror::carried`]: without the server's id, maker and time, or the
//! page; an arrow's end bound to an item copied with it names that item by
//! its place, `$n`), and the plain text of its texts, one a line, for other
//! apps. Edit > Paste reads the objects back as items to add to a page, an
//! arrow still bound to the items pasted with it; plain text alone, from
//! another app, comes as one new text item ([`text_items`]). Pure.

use serde_json::{Map, Value};

use super::write::tenths;
use super::{Mirror, read, shift};
use crate::config::Rgb;
use crate::layout::{Point, Rect};
use crate::scene::canvas::{ItemId, Shape};
use crate::scene::ink::{Carried, Ink};
use crate::scene::paste::{self, Clip};

impl Mirror {
    /// Items `ids` of this page as the user sees it, `canvas`'s `ink` laid
    /// over it ([`Mirror::carried`]), as a copy puts them on the clipboard,
    /// in drawing order; `None` when none of them is here.
    pub fn copied(&self, ink: (&str, &Ink), ids: &[ItemId]) -> Option<Clip> {
        let carried = self.carried(ink, ids, "", Point { x: 0.0, y: 0.0 });
        let texts: Vec<&str> = carried
            .iter()
            .filter_map(|c| match &c.item.shape {
                Shape::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        let text = texts.join("\n");
        let mut objects = Vec::new();
        for Carried { mut object, .. } in carried {
            if let Some(o) = object.as_object_mut() {
                o.remove("page");
            }
            objects.push(object);
        }
        let items = (!objects.is_empty()).then(|| Value::Array(objects).to_string());
        items.map(|items| Clip::new(Some(text), Some(items)))
    }
}

/// The items a clip's `items` holds as a paste adds them to `page`: moved
/// so the box around them is centred on `centre`, in canvas units (to the
/// tenth, as the file keeps lengths). An arrow's end `$n` stays bound to
/// the clip's object `n`, renumbered for its place in the adds, when that
/// object is pasted before the arrow; any other end is let go. Nothing for
/// text that is not a JSON array; an object that does not read as an item
/// is left out.
pub fn pasted(items: &str, page: &str, centre: Point) -> Vec<Carried> {
    let Ok(Value::Array(objects)) = serde_json::from_str::<Value>(items) else {
        return Vec::new();
    };
    let objects: Vec<Map<String, Value>> = objects
        .into_iter()
        .filter_map(|o| match o {
            Value::Object(o) => Some(o),
            _ => None,
        })
        .collect();
    // An object reads only with an id: each its place in the clip, which
    // the ink shows under one of its own.
    let item = |n: usize, o: &Map<String, Value>| {
        let mut o = o.clone();
        o.insert("id".into(), format!("i{n}").into());
        read(&o)
    };
    let around = objects.iter().enumerate().filter_map(|(n, o)| item(n, o)?.shape.bounds());
    let Some(around) = around.reduce(Rect::union) else { return Vec::new() };
    let by = |c: f64, from: f64, size: f64| Value::from(tenths(c - from - size / 2.0));
    let (dx, dy) = (by(centre.x, around.x, around.width), by(centre.y, around.y, around.height));
    let mut out = Vec::new();
    // The place in `out` of each object of the clip pasted so far.
    let mut places: Vec<Option<usize>> = Vec::new();
    for (n, mut object) in objects.into_iter().enumerate() {
        shift(&mut object, &dx, &dy);
        let Some(item) = item(n, &object) else {
            places.push(None);
            continue;
        };
        for key in ["from", "to"] {
            let end = object.remove(key);
            let end = end.as_ref().and_then(Value::as_str).and_then(|e| e.strip_prefix('$'));
            let place = end.and_then(|e| e.parse::<usize>().ok()).and_then(|m| *places.get(m)?);
            if let Some(place) = place {
                object.insert(key.into(), format!("${place}").into());
            }
        }
        for key in ["id", "by", "t"] {
            object.remove(key);
        }
        object.insert("page".into(), page.into());
        places.push(Some(out.len()));
        out.push(Carried { item, object: Value::Object(object) });
    }
    out
}

/// Plain `text` (from another app) as the items a clip holds: one text
/// item of `colour`, its lines as [`paste::plain`] makes them; `None` for
/// blank text.
pub fn text_items(text: &str, colour: Rgb) -> Option<String> {
    let text = paste::plain(text);
    if text.trim().is_empty() {
        return None;
    }
    let Rgb { r, g, b } = colour;
    let hex = format!("#{r:02x}{g:02x}{b:02x}");
    let object = serde_json::json!({"kind": "text", "color": hex, "x": 0, "y": 0, "text": text});
    Some(Value::Array(vec![object]).to_string())
}

#[cfg(test)]
mod tests;
