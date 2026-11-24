//! Reading the lines of a canvas file: the header, then each operation
//! line into the change it states (see the module doc of `mod.rs`).

use serde_json::Value;

use super::json::{self, Bad, Fields};
use super::{Change, Header, Line, VERSION, corrupt, style, time};
use crate::canvas::{Page, Point, Step};
use crate::{Error, Result};

/// Why a line could not be read.
pub(super) enum Damage {
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

pub(super) fn header(row: &[u8]) -> Result<Header> {
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

pub(super) fn line(row: &[u8], expected: u64) -> std::result::Result<Line, Damage> {
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
    Ok(Line::Op { n: expected, by, at, change, step: read_step(&m) })
}

/// Where a line stands in the undo history. A mark that is not a number
/// above 0 is read as none: the line is then no undo step, and nothing else
/// of the file reads differently.
fn read_step(m: &Fields) -> Step {
    let mark = |key| m.get(key).and_then(Value::as_u64).filter(|mark| *mark > 0);
    (mark("step").map(Step::New))
        .or_else(|| mark("undoes").map(Step::Undo))
        .or_else(|| mark("redoes").map(Step::Redo))
        .unwrap_or_default()
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
            let (style, member) = (style::read(item, &shape), style::read_member(item));
            Change::Add { item: id, page, style, member, shape }
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
