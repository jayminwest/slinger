//! The style keys of an item object (docs/design/canvas-file.md, "Item
//! objects"): `fill`, `dash`, `heads` and `closed`, and its figure's
//! (`locked`, `fig`, `part`), read as the core reads a file, leniently: a
//! value that does not read is the default, and so is a field the item's
//! kind does not take. A fill that is not a colour is no fill, never black.

use serde_json::{Map, Value};

use crate::config::Rgb;
use crate::scene::canvas::{Dash, Heads, Member, Shape, Style};

/// The style of item object `o`, whose shape is `shape`.
pub(super) fn read(o: &Map<String, Value>, shape: &Shape) -> Style {
    let word = |key: &str| o.get(key).and_then(Value::as_str);
    let closed =
        matches!(shape, Shape::Stroke { .. }) && o.get("closed") == Some(&Value::Bool(true));
    let fills = closed || matches!(shape, Shape::Box { .. });
    let dashes = matches!(shape, Shape::Box { .. } | Shape::Stroke { .. } | Shape::Arrow { .. });
    let dash = match word("dash") {
        Some("dashed") if dashes => Dash::Dashed,
        Some("dotted") if dashes => Dash::Dotted,
        _ => Dash::Solid,
    };
    let heads = match word("heads") {
        Some("none") if matches!(shape, Shape::Arrow { .. }) => Heads::None,
        Some("both") if matches!(shape, Shape::Arrow { .. }) => Heads::Both,
        _ => Heads::End,
    };
    let fill = word("fill").and_then(rgba).filter(|_| fills);
    Style { fill, dash, heads, closed }
}

/// Writes `style`'s keys into `o`, each only when not at its default, as
/// the core writes them.
pub(super) fn write(o: &mut Map<String, Value>, style: Style) {
    if let Some((Rgb { r, g, b }, a)) = style.fill {
        let alpha = if a == 255 { String::new() } else { format!("{a:02x}") };
        o.insert("fill".into(), format!("#{r:02x}{g:02x}{b:02x}{alpha}").into());
    }
    let dash = match style.dash {
        Dash::Solid => None,
        Dash::Dashed => Some("dashed"),
        Dash::Dotted => Some("dotted"),
    };
    if let Some(dash) = dash {
        o.insert("dash".into(), dash.into());
    }
    let heads = match style.heads {
        Heads::End => None,
        Heads::None => Some("none"),
        Heads::Both => Some("both"),
    };
    if let Some(heads) = heads {
        o.insert("heads".into(), heads.into());
    }
    if style.closed {
        o.insert("closed".into(), true.into());
    }
}

/// The figure of item object `o`: `locked` only when `true`, `part`
/// only with `fig`.
pub(super) fn member(o: &Map<String, Value>) -> Member {
    let name = |key: &str| o.get(key).and_then(Value::as_str).map(str::to_string);
    let fig = name("fig");
    let part = name("part").filter(|_| fig.is_some());
    Member { locked: o.get("locked") == Some(&Value::Bool(true)), fig, part }
}

/// Writes `member`'s keys into `o`, each only when not at its default.
pub(super) fn write_member(o: &mut Map<String, Value>, member: &Member) {
    if member.locked {
        o.insert("locked".into(), true.into());
    }
    for (key, name) in [("fig", &member.fig), ("part", &member.part)] {
        if let Some(name) = name {
            o.insert(key.into(), name.as_str().into());
        }
    }
}

#[cfg(test)]
mod tests;

/// `#rrggbb` (opaque) or `#rrggbbaa`; `None` for anything else.
fn rgba(text: &str) -> Option<(Rgb, u8)> {
    let colour = super::colour(text)?;
    let hex = text.strip_prefix('#')?;
    let alpha = hex.get(6..8).map_or(Some(255), |a| u8::from_str_radix(a, 16).ok())?;
    Some((colour, alpha))
}
