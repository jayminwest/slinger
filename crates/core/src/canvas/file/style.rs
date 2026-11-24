//! The style and figure keys of an item object (docs/design/canvas-file.md,
//! "Item objects"): `fill`, `dash`, `heads` and `closed`, then `locked`,
//! `fig` and `part`, written after the kind's own fields and left out at
//! their default, so a file without them reads and writes the same bytes. A
//! reader from before them ignores them and draws the item plain, in no
//! figure (docs/design/figures.md, "What an old reader does with new
//! fields").
//!
//! Reading a file never fails on them: a value of the wrong type or an
//! unknown word reads as the default, and so does a field the item's kind
//! does not take, a name that breaks the rules or a `part` without `fig`.
//! Applying a request is strict ([`check`], [`check_member`]).

use serde_json::Value;

use super::json::{self, Fields, Obj};
use crate::canvas::{Colour, Dash, Heads, Member, Shape, Style, figure_name, part_name};
use crate::{Error, Result};

/// The style keys, in the order they are written.
const KEYS: [&str; 4] = ["fill", "dash", "heads", "closed"];

/// The figure keys, in the order they are written, after the style's.
const MEMBER_KEYS: [&str; 3] = ["locked", "fig", "part"];

/// Writes `style`'s keys, then `member`'s, into `o`, each only when not
/// at its default.
pub(super) fn write(o: Obj, style: Style, member: &Member) -> Obj {
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
    let o = if style.closed { o.raw("closed", "true") } else { o };
    let o = if member.locked { o.raw("locked", "true") } else { o };
    let o = match &member.fig {
        Some(fig) => o.str("fig", fig),
        None => o,
    };
    match &member.part {
        Some(part) => o.str("part", part),
        None => o,
    }
}

/// Every key of `style`, `null` at its default, as a `set` carries them:
/// laid over an item, they replace its whole style.
pub(super) fn fields(style: Style) -> Fields {
    every(write(Obj::new(), style, &Member::default()), &KEYS)
}

/// Every key of `member`, `null` at its default, as [`fields`] for a style.
pub(super) fn member_fields(member: &Member) -> Fields {
    every(write(Obj::new(), Style::default(), member), &MEMBER_KEYS)
}

/// The object `o` with each of `keys` it lacks as `null`.
fn every(o: Obj, keys: &[&str]) -> Fields {
    let Ok(Value::Object(mut m)) = serde_json::from_str(&o.end()) else {
        return Fields::new();
    };
    for key in keys {
        m.entry(*key).or_insert(Value::Null);
    }
    m
}

/// Whether `m` names any style key, `null` included.
pub(super) fn named(m: &Fields) -> bool {
    KEYS.iter().any(|k| m.contains_key(*k))
}

/// Whether `m` names any figure key, `null` included.
pub(super) fn member_named(m: &Fields) -> bool {
    MEMBER_KEYS.iter().any(|k| m.contains_key(*k))
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

/// Reads the figure keys of an item object or merged `set` fields,
/// leniently: a name that breaks the rules is none, and so is a `part`
/// without `fig`.
pub(super) fn read_member(m: &Fields) -> Member {
    let word = |key| m.get(key).and_then(Value::as_str).map(str::to_owned);
    let fig = word("fig").filter(|f| figure_name(f));
    let part = word("part").filter(|p| fig.is_some() && part_name(p));
    Member { locked: m.get("locked").and_then(Value::as_bool) == Some(true), fig, part }
}

/// A value of the wrong type for `key`.
fn wrong(key: &str, v: &Value, takes: &str) -> Error {
    Error::Invalid {
        message: format!("`{key}` is {v}, which is not a {key}"),
        hint: Some(format!("`{key}` takes {takes}, or null for none")),
    }
}

/// The value of `key` in `m`, unless it is absent or `null`.
fn given<'a>(m: &'a Fields, key: &str) -> Option<&'a Value> {
    m.get(key).filter(|v| !v.is_null())
}

/// Reads the style of a requested item strictly: a value of the wrong
/// type, an unknown word, a colour that does not parse or a field `shape`
/// does not take is [`Error::Invalid`], with a hint naming what it takes.
/// `null` is the default.
pub(super) fn check(m: &Fields, shape: &Shape) -> Result<Style> {
    let read = |key: &str, takes: &str, parse: &dyn Fn(&Value) -> bool| match given(m, key) {
        Some(v) if !parse(v) => Err(wrong(key, v, takes)),
        _ => Ok(()),
    };
    let text = |v: &Value| v.as_str().map(str::to_owned).unwrap_or_default();
    read("fill", "a colour, \"#rrggbb\" or \"#rrggbbaa\"", &|v| Colour::parse(&text(v)).is_some())?;
    read("dash", "\"dashed\" or \"dotted\"", &|v| Dash::from_word(&text(v)).is_some())?;
    read("heads", "\"none\", \"end\" or \"both\"", &|v| Heads::from_word(&text(v)).is_some())?;
    read("closed", "true or false", &|v| v.is_boolean())?;
    let style = Style {
        fill: given(m, "fill").and_then(|v| Colour::parse(&text(v))),
        dash: given(m, "dash").and_then(|v| Dash::from_word(&text(v))).unwrap_or_default(),
        heads: given(m, "heads").and_then(|v| Heads::from_word(&text(v))).unwrap_or_default(),
        closed: given(m, "closed").and_then(Value::as_bool) == Some(true),
    };
    style.check(shape)?;
    Ok(style)
}

/// Reads the figure keys of a requested item strictly: `locked` not a
/// boolean, `fig` or `part` not a string, a name that breaks the rules or
/// a `part` without `fig` is [`Error::Invalid`] with a hint. `null` is
/// the default.
pub(super) fn check_member(m: &Fields) -> Result<Member> {
    let locked = match given(m, "locked") {
        Some(v) => v.as_bool().ok_or_else(|| wrong("locked", v, "true or false"))?,
        None => false,
    };
    let name = |key: &str, takes: &str| match given(m, key) {
        Some(v) => v.as_str().map(|s| Some(s.to_owned())).ok_or_else(|| wrong(key, v, takes)),
        None => Ok(None),
    };
    let fig = name("fig", "a figure name, as \"worksheet\"")?;
    let part = name("part", "a part name, as \"1a-answer\"")?;
    let member = Member { locked, fig, part };
    member.check()?;
    Ok(member)
}
