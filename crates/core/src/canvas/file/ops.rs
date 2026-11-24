//! A request's operations read into the actions that do them (see
//! "Canvas operations" in docs/service.md): items as the file's item
//! objects, an arrow's end or a fold's list naming an item an `add` before
//! it makes (`$n`).

use serde_json::Value;

use super::Log;
use super::json::{self, Fields};
use super::log::bad;
use super::style;
use crate::canvas::{Action, ItemId, Point, Shape};
use crate::{Error, Result};

impl Log {
    /// Reads one requested operation into the actions that do it.
    pub(super) fn read_op(&self, m: &Fields, out: &mut Vec<Action>) -> Result<()> {
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
                let listed = item.get_mut("fold").and_then(|f| f.get_mut("items"));
                for v in listed.and_then(Value::as_array_mut).into_iter().flatten() {
                    let Some(n) = v.as_str().and_then(|s| s.strip_prefix('$')) else { continue };
                    let made = self.made(out, n).ok_or_else(|| {
                        bad(format!("`fold` lists \"${n}\", which no `add` before it made"))
                    })?;
                    *v = json::item_id(made).into();
                }
                let page = json::text(&item, "page").and_then(json::parse_page).map_err(bad)?;
                let shape = read_shape(&item)?;
                let style = style::check(&item, &shape)?;
                out.push(Action::Add { page, shape, style });
                if let Some(inside) = m.get("in") {
                    self.join(inside, out)?;
                }
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
                let current = self.canvas().item(item).ok_or_else(|| Error::NotFound {
                    what: item.to_string(),
                    hint: Some("it may have been removed; show the canvas again".into()),
                })?;
                let shape = read_shape(&json::laid_over(current.shape(), fields))?;
                // A set that names no style key keeps the item's style.
                let style = if style::named(fields) {
                    let mut styled = style::fields(*current.style());
                    styled.extend(fields.iter().map(|(k, v)| (k.clone(), v.clone())));
                    Some(style::check(&styled, &shape)?)
                } else {
                    None
                };
                out.push(Action::Edit { item, shape, style });
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
    pub(super) fn made(&self, out: &[Action], n: &str) -> Option<ItemId> {
        let n: usize = n.parse().ok()?;
        let mut next = self.canvas().next_item().get();
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

fn read_shape(m: &Fields) -> Result<Shape> {
    json::read_shape(m).map_err(bad)?.ok_or_else(|| bad("`kind` is not an item kind".into()))
}
