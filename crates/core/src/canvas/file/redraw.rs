//! A figure drawn again (docs/design/figures.md, "`canvas redraw`"): where
//! the redraw puts it and the data it runs on. The script runs in the
//! daemon, from the figure's last script, data, origin and frame, which its
//! canvas file keeps ([`Source`], `source.rs`); [`Log::place_figure`]
//! then replaces the figure by match, so the user's ink is never touched.
//!
//! The placement follows the user: the new origin is the last one moved as
//! far as the figure's frame has moved since it was drawn, so a figure the
//! user moved as one unit is drawn again where it now is.

use serde_json::{Map, Value};

use super::{Log, Placement, Source, json as file_json};
use crate::canvas::{Author, Item, not_yours};
use crate::{Error, Result};

/// A figure as a redraw finds it: where to draw it again, and how it stands
/// now, which is the reply to a redraw that changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Again {
    /// Where to draw it: its origin, on its page.
    pub at: Placement,
    /// Its page's id (`p1`).
    pub page: String,
    /// Where its (0,0) goes, `[x, y]` in tenths.
    pub origin: [i64; 2],
    /// The joint frame of its items now, `[x, y, w, h]` in tenths.
    pub frame: [i64; 4],
    /// Its part names, in reading order of their frames.
    pub parts: Vec<String>,
    /// How many items it has.
    pub items: usize,
    /// Its last draw: the script and data a redraw starts from.
    pub source: Source,
}

impl Log {
    /// Where `by` draws the figure `name` again, from its last draw.
    ///
    /// # Errors
    ///
    /// `not_found` for a name no item on the canvas has, or a figure no
    /// script drew; `invalid` (the author rule) for a figure `by` may not
    /// change.
    pub fn again(&self, by: &Author, name: &str) -> Result<Again> {
        let canvas = self.canvas();
        let Some(figure) = canvas.figure(name) else {
            let hint = "draw it again with 'slinger canvas draw'";
            return Err(Error::NotFound {
                what: format!("figure {name}"),
                hint: Some(hint.into()),
            });
        };
        let Some(source) = self.source(name) else {
            let hint = "only a figure a script drew is drawn again; 'slinger canvas figure' lists the figures";
            return Err(Error::NotFound {
                what: format!("figure {name}, drawn by a script"),
                hint: Some(hint.into()),
            });
        };
        if !by.may_change(&figure.maker) {
            return Err(not_yours(by, &format!("figure {name}"), &figure.maker));
        }
        let (origin, frame) = (source.origin, source.frame);
        // The frame as a draw measures it: every item, shown or not.
        let now = figure
            .items
            .iter()
            .filter_map(|id| canvas.item(*id))
            .map(Item::bounds)
            .reduce(crate::render::union)
            .unwrap_or_default();
        let now = [now.x, now.y, now.w, now.h].map(file_json::tenths);
        let origin = [origin[0] + now[0] - frame[0], origin[1] + now[1] - frame[1]];
        let page = file_json::page_id(figure.page);
        let at = Placement::At { page: Some(page.clone()), x: origin[0], y: origin[1] };
        let parts = figure.parts.into_iter().map(|p| p.name).collect();
        let (items, source) = (figure.items.len(), source.clone());
        Ok(Again { at, page, origin, frame: now, parts, items, source })
    }
}

/// Applies `patch` to `data` as a JSON merge patch (RFC 7396): an object
/// merges key by key, `null` takes a key away, and anything else (an
/// array too) replaces what was there.
pub fn merge_patch(data: &mut Value, patch: &Value) {
    let Value::Object(fields) = patch else {
        data.clone_from(patch);
        return;
    };
    if !data.is_object() {
        *data = Value::Object(Map::new());
    }
    if let Value::Object(map) = data {
        for (key, value) in fields {
            if value.is_null() {
                map.remove(key);
            } else {
                merge_patch(map.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::merge_patch;

    /// RFC 7396, Appendix A: each target, patch and result.
    #[test]
    fn a_merge_patch_does_what_rfc_7396_says() {
        for (data, patch, want) in [
            (json!({"a": "b"}), json!({"a": "c"}), json!({"a": "c"})),
            (json!({"a": "b"}), json!({"b": "c"}), json!({"a": "b", "b": "c"})),
            (json!({"a": "b"}), json!({"a": null}), json!({})),
            (json!({"a": "b", "b": "c"}), json!({"a": null}), json!({"b": "c"})),
            (json!({"a": ["b"]}), json!({"a": "c"}), json!({"a": "c"})),
            (json!({"a": "c"}), json!({"a": ["b"]}), json!({"a": ["b"]})),
            (
                json!({"a": {"b": "c"}}),
                json!({"a": {"b": "d", "c": null}}),
                json!({"a": {"b": "d"}}),
            ),
            (json!({"a": [{"b": "c"}]}), json!({"a": [1]}), json!({"a": [1]})),
            (json!(["a", "b"]), json!(["c", "d"]), json!(["c", "d"])),
            (json!({"a": "b"}), json!(["c"]), json!(["c"])),
            (json!({"a": "foo"}), json!(null), json!(null)),
            (json!({"a": "foo"}), json!("bar"), json!("bar")),
            (json!({"e": null}), json!({"a": 1}), json!({"e": null, "a": 1})),
            (json!([1, 2]), json!({"a": "b", "c": null}), json!({"a": "b"})),
            (json!({}), json!({"a": {"bb": {"ccc": null}}}), json!({"a": {"bb": {}}})),
        ] {
            let mut out = data.clone();
            merge_patch(&mut out, &patch);
            assert_eq!(out, want, "{data} patched with {patch}");
        }
    }
}
