//! A canvas's items on the clipboard: Edit > Copy of the selection puts
//! their file objects there as a JSON array, as a carry writes them
//! ([`Mirror::carried`]: without the server's id, maker and time, or the
//! page; an arrow's end bound to an item copied with it names that item by
//! its place, `$n`), and the plain text of its texts, one a line, for other
//! apps. Edit > Paste reads the objects back as items to add to a page, an
//! arrow still bound to the items pasted with it; plain text alone, from
//! another app, comes as one new text item ([`text_items`]). The crop
//! tool's snippet of a PDF goes there as one such object ([`snippet`]). Pure.
//!
//! A figure's items (docs/design/figures.md, "Paste and duplicate") keep
//! `fig`, `part` and `locked` only when the clip holds one whole figure,
//! and its paste is a new figure under a free name ([`free_name`]); a copy
//! of part of a figure, or of two, drops them, so the copies are the user's
//! free items. Fill, dash, heads and closed are the look, and always stay.

use std::collections::HashSet;

use serde_json::{Map, Value};

use super::write::tenths;
use super::{Mirror, read, shift};
use crate::config::Rgb;
use crate::layout::{Point, Rect};
use crate::scene::canvas::{Item, ItemId, Shape};
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
        let whole = self.whole(ink, ids);
        for Carried { mut object, item } in carried {
            if let Some(o) = object.as_object_mut() {
                o.remove("page");
                if item.member.fig.as_ref().is_some_and(|fig| !whole.contains(fig)) {
                    loose(o);
                }
            }
            objects.push(object);
        }
        let items = (!objects.is_empty()).then(|| Value::Array(objects).to_string());
        items.map(|items| Clip::new(Some(text), Some(items)))
    }
}

impl Mirror {
    /// The figures of this page as the user sees it, `canvas`'s `ink` laid
    /// over it, that `ids` hold whole: every item of each is among them.
    fn whole(&self, (canvas, ink): (&str, &Ink), ids: &[ItemId]) -> HashSet<String> {
        let shown = ink.shown(canvas, &self.page, &self.items);
        let figs = shown.iter().filter_map(|i| i.member.fig.as_ref().map(|f| (f, i.id)));
        let mut whole: HashSet<String> = figs.clone().map(|(f, _)| f.clone()).collect();
        for (fig, id) in figs {
            if !ids.contains(&id) {
                whole.remove(fig);
            }
        }
        whole
    }

    /// The names of the figures on the canvas, a figure's name being
    /// unique there across pages: this page's, folded away or not, and
    /// every other this mirror has seen (`canvas_show`'s pages and the feed). A paste does
    /// not take one of them ([`free_name`]).
    pub fn figures(&self) -> HashSet<String> {
        let items = self.items.iter().chain(&self.held);
        let here = items.filter_map(|i| i.member.fig.clone());
        here.chain(self.named.iter().cloned()).collect()
    }
}

/// The items a clip's `items` holds as a paste adds them to `page`: moved
/// so the box around them is centred on `centre`, in canvas units (to the
/// tenth, as the file keeps lengths). An arrow's end `$n` stays bound to
/// the clip's object `n`, renumbered for its place in the adds, when that
/// object is pasted before the arrow; any other end is let go. Nothing for
/// text that is not a JSON array; an object that does not read as an item
/// is left out. One whole figure is pasted as a new one, named apart from
/// `taken` (the canvas's figures, [`Mirror::figures`]) by [`free_name`];
/// any other figure's items come free.
pub fn pasted(items: &str, page: &str, centre: Point, taken: &HashSet<String>) -> Vec<Carried> {
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
    let objects = renamed(objects, taken);
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

/// `objects` as a paste adds them: of one figure, that figure under a free
/// name, its parts and lock kept; of two or more, every one let loose.
fn renamed(
    mut objects: Vec<Map<String, Value>>,
    taken: &HashSet<String>,
) -> Vec<Map<String, Value>> {
    let fig = |o: &Map<String, Value>| o.get("fig").and_then(Value::as_str).map(str::to_string);
    let figs: HashSet<String> = objects.iter().filter_map(fig).collect();
    let mut figs = figs.into_iter();
    let new = match (figs.next(), figs.next()) {
        (Some(one), None) => Some(free_name(&one, taken)),
        _ => None,
    };
    for o in objects.iter_mut().filter(|o| o.contains_key("fig")) {
        match &new {
            Some(name) => {
                o.insert("fig".into(), name.as_str().into());
            }
            None => loose(o),
        }
    }
    objects
}

/// Takes item object `o` out of its figure: no `fig`, `part` or `locked`.
fn loose(o: &mut Map<String, Value>) {
    for key in ["fig", "part", "locked"] {
        o.remove(key);
    }
}

/// The name a copy of figure `fig` takes (docs/design/figures.md, "A
/// clash"): `fig` with a trailing `-<n>` taken off, then `-2`, `-3`, ...,
/// the first not in `taken`, cut to the 40 characters a name may have.
pub fn free_name(fig: &str, taken: &HashSet<String>) -> String {
    let base = match fig.rsplit_once('-') {
        Some((base, n))
            if !base.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) =>
        {
            base
        }
        _ => fig,
    };
    // One of these is free: `taken` cannot hold them all.
    (2..=taken.len() + 2)
        .map(|n| {
            let tail = format!("-{n}");
            let room = NAME_MAX.saturating_sub(tail.len()).min(base.len());
            format!("{}{tail}", base[..room].trim_end_matches('-'))
        })
        .find(|name| !taken.contains(name))
        .unwrap_or_else(|| fig.to_string())
}

/// The longest figure name, in characters (the core's `figure::NAME_MAX`).
const NAME_MAX: usize = 40;

/// A snippet of a PDF ([`crate::scene::crop`]) as a copy puts it on the
/// clipboard: its file object, without an id, alone in the array, and no
/// text. `None` for a shape the file does not keep.
pub fn snippet(shape: &Shape) -> Option<Clip> {
    let item = Item::plain(ItemId(0), Rgb { r: 0, g: 0, b: 0 }, shape.clone());
    let mut object = super::object(&item)?;
    object.as_object_mut()?.remove("id");
    Some(Clip::new(None, Some(Value::Array(vec![object]).to_string())))
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
