//! The style keys of an item object (docs/design/canvas-file.md, "Item
//! objects"): `fill`, `dash`, `heads` and `closed`, written after the
//! kind's own fields and left out at their default, so a file without them
//! reads and writes the same bytes. A reader from before them ignores them
//! and draws the item plain (docs/design/figures.md, "What an old reader
//! does with new fields").
//!
//! Reading a file never fails on them: a value of the wrong type or an
//! unknown word reads as the default, and so does a field the item's kind
//! does not take. Applying a request is strict ([`check`]).

use serde_json::Value;

use super::json::{self, Fields, Obj};
use crate::canvas::{Colour, Dash, Heads, Shape, Style};
use crate::{Error, Result};

/// The keys, in the order they are written.
const KEYS: [&str; 4] = ["fill", "dash", "heads", "closed"];

/// Writes `style`'s keys into `o`, each only when not at its default.
pub(super) fn write(o: Obj, style: Style) -> Obj {
    let o = match style.fill {
        Some(c) => o.str("fill", &json::colour(c)),
        None => o,
    };
    let o = match style.dash.word() {
        Some(word) => o.str("dash", word),
        None => o,
    };
    let o = match style.heads.word() {
        Some(word) => o.str("heads", word),
        None => o,
    };
    if style.closed { o.raw("closed", "true") } else { o }
}

/// Every key of `style`, `null` at its default, as a `set` carries them:
/// laid over an item, they replace its whole style.
pub(super) fn fields(style: Style) -> Fields {
    let Ok(Value::Object(mut m)) = serde_json::from_str(&write(Obj::new(), style).end()) else {
        return Fields::new();
    };
    for key in KEYS {
        m.entry(key).or_insert(Value::Null);
    }
    m
}

/// Whether `m` names any style key, `null` included.
pub(super) fn named(m: &Fields) -> bool {
    KEYS.iter().any(|k| m.contains_key(*k))
}

/// Reads the style of an item object or merged `set` fields, leniently:
/// anything that does not read is the default, and so is a field `shape`
/// does not take. A fill that is not a colour is no fill, never black.
pub(super) fn read(m: &Fields, shape: &Shape) -> Style {
    let word = |key| m.get(key).and_then(Value::as_str);
    Style {
        fill: word("fill").and_then(Colour::parse),
        dash: word("dash").and_then(Dash::from_word).unwrap_or_default(),
        heads: word("heads").and_then(Heads::from_word).unwrap_or_default(),
        closed: m.get("closed").and_then(Value::as_bool) == Some(true),
    }
    .fitted(shape)
}

/// Reads the style of a requested item strictly: a value of the wrong
/// type, an unknown word, a colour that does not parse or a field `shape`
/// does not take is [`Error::Invalid`], with a hint naming what it takes.
/// `null` is the default.
pub(super) fn check(m: &Fields, shape: &Shape) -> Result<Style> {
    let given = |key: &str| m.get(key).filter(|v| !v.is_null());
    let read = |key: &str, takes: &str, parse: &dyn Fn(&Value) -> bool| match given(key) {
        Some(v) if !parse(v) => Err(Error::Invalid {
            message: format!("`{key}` is {v}, which is not a {key}"),
            hint: Some(format!("`{key}` takes {takes}, or null for none")),
        }),
        _ => Ok(()),
    };
    let text = |v: &Value| v.as_str().map(str::to_owned).unwrap_or_default();
    read("fill", "a colour, \"#rrggbb\" or \"#rrggbbaa\"", &|v| Colour::parse(&text(v)).is_some())?;
    read("dash", "\"dashed\" or \"dotted\"", &|v| Dash::from_word(&text(v)).is_some())?;
    read("heads", "\"none\", \"end\" or \"both\"", &|v| Heads::from_word(&text(v)).is_some())?;
    read("closed", "true or false", &|v| v.is_boolean())?;
    let style = Style {
        fill: given("fill").and_then(|v| Colour::parse(&text(v))),
        dash: given("dash").and_then(|v| Dash::from_word(&text(v))).unwrap_or_default(),
        heads: given("heads").and_then(|v| Heads::from_word(&text(v))).unwrap_or_default(),
        closed: given("closed").and_then(Value::as_bool) == Some(true),
    };
    style.check(shape)?;
    Ok(style)
}
