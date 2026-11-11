//! How the parts of a line are written and read: ids, units, colours, items.
//!
//! Coordinates and lengths are stored as integers in tenths of a canvas unit.
//! [`tenths`] rounds to the nearest tenth, halves away from zero, and
//! saturates at the ends of `i64` (a canvas never holds such values). A value
//! that is already `k / 10.0` for an integer `k` comes back unchanged, so a
//! snapped value survives any number of round trips.

use serde_json::{Map, Value};

use super::time;
use crate::canvas::{Author, Colour, End, Item, ItemId, PageId, Point, Rect, Shape};

/// A JSON object, as read.
pub(super) type Fields = Map<String, Value>;

/// Why a line does not fit its operation. It becomes `canvas_corrupt`.
pub(super) type Bad = String;

/// A value in canvas units, as stored.
#[expect(clippy::cast_possible_truncation, reason = "saturates by design, see the module doc")]
pub(super) fn tenths(units: f64) -> i64 {
    (units * 10.0).round() as i64
}

/// A stored value, in canvas units.
#[expect(clippy::cast_precision_loss, reason = "exact below 2^53 tenths, far past any canvas")]
pub(super) fn units(tenths: i64) -> f64 {
    tenths as f64 / 10.0
}

/// One JSON object, written with its keys in the order they are added and no
/// spaces, so the same value is always the same bytes.
pub(super) struct Obj(String);

impl Obj {
    pub(super) fn new() -> Self {
        Self(String::from("{"))
    }

    /// Adds a key whose value is already JSON.
    pub(super) fn raw(mut self, key: &str, json: &str) -> Self {
        if self.0.len() > 1 {
            self.0.push(',');
        }
        self.0.push('"');
        self.0.push_str(key);
        self.0.push_str("\":");
        self.0.push_str(json);
        self
    }

    pub(super) fn str(self, key: &str, value: &str) -> Self {
        let json = Value::from(value).to_string();
        self.raw(key, &json)
    }

    pub(super) fn int(self, key: &str, value: i64) -> Self {
        self.raw(key, &value.to_string())
    }

    pub(super) fn end(mut self) -> String {
        self.0.push('}');
        self.0
    }
}

pub(super) fn item_id(id: ItemId) -> String {
    format!("i{}", id.get())
}

pub(super) fn page_id(id: PageId) -> String {
    format!("p{}", id.get())
}

fn number(s: &str, prefix: char) -> Option<u64> {
    let digits = s.strip_prefix(prefix)?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

pub(super) fn parse_item(s: &str) -> Result<ItemId, Bad> {
    number(s, 'i').map(ItemId::new).ok_or_else(|| format!("{s:?} is not an item id"))
}

pub(super) fn parse_page(s: &str) -> Result<PageId, Bad> {
    number(s, 'p').map(PageId::new).ok_or_else(|| format!("{s:?} is not a page id"))
}

/// `#rrggbb`, or `#rrggbbaa` when the colour is not opaque.
fn colour(c: Colour) -> String {
    let rgb = format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b);
    if c.a == 255 { rgb } else { format!("{rgb}{:02x}", c.a) }
}

/// A stored colour. A palette name, or anything else that is not `#rrggbb`
/// or `#rrggbbaa`, reads as black: the model has no palette yet, and a newer
/// writer's colour must not make the item unreadable.
fn parse_colour(s: &str) -> Colour {
    let Some(hex) = s.strip_prefix('#') else { return Colour::BLACK };
    if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Colour::BLACK;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
    let a = if hex.len() == 8 { byte(6) } else { 255 };
    Colour { r: byte(0), g: byte(2), b: byte(4), a }
}

/// A point list as one flat array: the first pair absolute, each later pair
/// the change from the one before. The deltas are taken between the rounded
/// values, so rounding never accumulates along a stroke.
fn pts(points: impl IntoIterator<Item = Point>) -> String {
    let mut out = Vec::new();
    let (mut px, mut py) = (0_i64, 0_i64);
    for p in points {
        let (x, y) = (tenths(p.x), tenths(p.y));
        out.push(x.wrapping_sub(px));
        out.push(y.wrapping_sub(py));
        (px, py) = (x, y);
    }
    Value::from(out).to_string()
}

fn parse_pts(m: &Fields) -> Result<Vec<Point>, Bad> {
    let bad = || "`pts` is not a list of integer pairs".to_string();
    let list = field(m, "pts")?.as_array().ok_or_else(bad)?;
    if list.is_empty() || list.len() % 2 != 0 {
        return Err(bad());
    }
    let (mut x, mut y) = (0_i64, 0_i64);
    let mut out = Vec::with_capacity(list.len() / 2);
    for [dx, dy] in list.as_chunks::<2>().0 {
        let (Some(dx), Some(dy)) = (dx.as_i64(), dy.as_i64()) else { return Err(bad()) };
        (x, y) = (x.wrapping_add(dx), y.wrapping_add(dy));
        out.push(Point::new(units(x), units(y)));
    }
    Ok(out)
}

fn rect(o: Obj, r: Rect) -> Obj {
    o.int("x", tenths(r.x)).int("y", tenths(r.y)).int("w", tenths(r.w)).int("h", tenths(r.h))
}

/// Writes a shape's fields into `o`: `kind`, `color`, then the kind's own.
/// A stroke's width is at least one tenth, so a thin stroke stays drawable.
/// A live item and a button have no colour: they draw themselves.
pub(super) fn shape(o: Obj, s: &Shape) -> Obj {
    match s {
        Shape::Stroke { points, colour: c, width } => o
            .str("kind", "stroke")
            .str("color", &colour(*c))
            .int("width", tenths(*width).max(1))
            .raw("pts", &pts(points.iter().copied())),
        Shape::Text { text, frame, colour: c } => {
            rect(o.str("kind", "text").str("color", &colour(*c)), *frame).str("text", text)
        }
        Shape::Box { frame, colour: c } => {
            rect(o.str("kind", "box").str("color", &colour(*c)), *frame)
        }
        Shape::Arrow { from, to, colour: c } => {
            let mut o =
                o.str("kind", "arrow").str("color", &colour(*c)).raw("pts", &pts([from.at, to.at]));
            if let Some(i) = from.item {
                o = o.str("from", &item_id(i));
            }
            if let Some(i) = to.item {
                o = o.str("to", &item_id(i));
            }
            o
        }
        Shape::Live { frame, ui, hash, open } => {
            rect(o.str("kind", "live"), *frame).str("ui", ui).str("hash", hash).str("open", open)
        }
        Shape::Button { frame } => rect(o.str("kind", "button"), *frame),
    }
}

/// An `add` operation's item object.
pub(super) fn item(id: ItemId, page: PageId, s: &Shape) -> String {
    shape(Obj::new().str("id", &item_id(id)).str("page", &page_id(page)), s).end()
}

/// An item whole, as `restore` and `page.restore` carry it: its id, page,
/// maker and time, then its shape.
pub(super) fn whole_item(i: &Item) -> String {
    let o = Obj::new().str("id", &item_id(i.id)).str("page", &page_id(i.page));
    shape(o.str("by", &author(&i.author)).str("t", &time::format(i.time)), &i.shape).end()
}

/// Reads [`whole_item`]. `None` for an item kind or maker this reader does
/// not know.
pub(super) fn read_whole_item(m: &Fields) -> Result<Option<Item>, Bad> {
    let Some(shape) = read_shape(m)? else { return Ok(None) };
    let Some(author) = parse_author(text(m, "by")?) else { return Ok(None) };
    Ok(Some(Item {
        id: parse_item(text(m, "id")?)?,
        page: parse_page(text(m, "page")?)?,
        author,
        time: time::parse(text(m, "t")?)?,
        shape,
    }))
}

pub(super) fn author(a: &Author) -> String {
    match a {
        Author::User => "user".into(),
        Author::Agent(name) => format!("agent:{name}"),
    }
}

/// `None` for an author kind this reader does not know.
pub(super) fn parse_author(s: &str) -> Option<Author> {
    match s {
        "user" => Some(Author::User),
        _ => s.strip_prefix("agent:").map(|name| Author::Agent(name.to_owned())),
    }
}

/// A shape's fields as an object, as a `set` carries them. An optional field
/// the shape lacks is `null`, so laying the fields over another shape's
/// clears it.
pub(super) fn fields(s: &Shape) -> Fields {
    let mut m = match serde_json::from_str(&shape(Obj::new(), s).end()) {
        Ok(Value::Object(m)) => m,
        _ => Fields::new(),
    };
    if matches!(s, Shape::Arrow { .. }) {
        for key in ["from", "to"] {
            m.entry(key).or_insert(Value::Null);
        }
    }
    m
}

/// A requested `set`'s `fields` laid over `current`'s. New text without a
/// `w` or `h` leaves that one at 0, so [`Shape::fitted`] sizes it to the new
/// words and the frame is never the old text's. The line written for it
/// carries the whole frame, so replay lays a line's fields over plainly.
pub(super) fn laid_over(current: &Shape, fields: &Fields) -> Fields {
    let mut merged = self::fields(current);
    merged.extend(fields.iter().map(|(k, v)| (k.clone(), v.clone())));
    if fields.contains_key("text") && merged.get("kind").and_then(Value::as_str) == Some("text") {
        for key in ["w", "h"] {
            if !fields.contains_key(key) {
                merged.insert(key.into(), 0.into());
            }
        }
    }
    merged
}

/// Reads a shape from an item object or merged `set` fields. `None` for a
/// kind this reader does not know.
pub(super) fn read_shape(m: &Fields) -> Result<Option<Shape>, Bad> {
    let kind = text(m, "kind")?;
    let at = |key| int(m, key).map(units);
    if kind == "live" {
        let frame = Rect::new(at("x")?, at("y")?, at("w")?, at("h")?);
        let [ui, hash, open] = ["ui", "hash", "open"].map(|k| text(m, k).map(str::to_owned));
        return Ok(Some(Shape::Live { frame, ui: ui?, hash: hash?, open: open? }));
    }
    if kind == "button" {
        return Ok(Some(Shape::Button {
            frame: Rect::new(at("x")?, at("y")?, at("w")?, at("h")?),
        }));
    }
    if !matches!(kind, "stroke" | "text" | "box" | "arrow") {
        return Ok(None);
    }
    let colour = parse_colour(text(m, "color")?);
    let shape = match kind {
        "stroke" => Shape::Stroke { points: parse_pts(m)?, colour, width: units(int(m, "width")?) },
        "text" => {
            let size = |k| opt_int(m, k).map(|v| units(v.unwrap_or(0)));
            let frame = Rect::new(at("x")?, at("y")?, size("w")?, size("h")?);
            Shape::Text { text: text(m, "text")?.to_owned(), frame, colour }
        }
        "box" => {
            let frame = Rect::new(at("x")?, at("y")?, at("w")?, at("h")?);
            Shape::Box { frame, colour }
        }
        _ => {
            let points = parse_pts(m)?;
            let (Some(first), Some(last)) = (points.first(), points.last()) else {
                return Err("an arrow has no points".into());
            };
            if points.len() < 2 {
                return Err("an arrow needs two points".into());
            }
            let end = |at: Point, key| -> Result<End, Bad> {
                Ok(End { at, item: opt_text(m, key)?.map(parse_item).transpose()? })
            };
            Shape::Arrow { from: end(*first, "from")?, to: end(*last, "to")?, colour }
        }
    };
    Ok(Some(shape))
}

pub(super) fn field<'a>(m: &'a Fields, key: &str) -> Result<&'a Value, Bad> {
    m.get(key).ok_or_else(|| format!("`{key}` is missing"))
}

pub(super) fn text<'a>(m: &'a Fields, key: &str) -> Result<&'a str, Bad> {
    field(m, key)?.as_str().ok_or_else(|| format!("`{key}` is not a string"))
}

fn opt_text<'a>(m: &'a Fields, key: &str) -> Result<Option<&'a str>, Bad> {
    match m.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => text(m, key).map(Some),
    }
}

pub(super) fn int(m: &Fields, key: &str) -> Result<i64, Bad> {
    field(m, key)?.as_i64().ok_or_else(|| format!("`{key}` is not an integer"))
}

fn opt_int(m: &Fields, key: &str) -> Result<Option<i64>, Bad> {
    match m.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => int(m, key).map(Some),
    }
}

pub(super) fn ids(m: &Fields, key: &str) -> Result<Vec<ItemId>, Bad> {
    let bad = || format!("`{key}` is not a list of item ids");
    let list = field(m, key)?.as_array().ok_or_else(bad)?;
    list.iter().map(|v| v.as_str().ok_or_else(bad).and_then(parse_item)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_read_back_and_names_read_as_black() {
        for c in [Colour::BLACK, Colour::rgb(1, 128, 255), Colour { r: 9, g: 8, b: 7, a: 6 }] {
            assert_eq!(parse_colour(&colour(c)), c);
        }
        assert_eq!(colour(Colour { r: 255, g: 0, b: 16, a: 128 }), "#ff001080");
        for name in ["blue", "#12345", "#gggggg", "#ééé", ""] {
            assert_eq!(parse_colour(name), Colour::BLACK, "{name}");
        }
    }

    #[test]
    fn ids_need_their_letter_and_digits() {
        assert_eq!(parse_item("i42"), Ok(ItemId::new(42)));
        assert_eq!(parse_page("p7"), Ok(PageId::new(7)));
        for bad in ["42", "i", "i+4", "p4", "i4x", "i99999999999999999999"] {
            assert!(parse_item(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn tenths_round_half_away_from_zero_and_saturate() {
        assert_eq!(tenths(1.25), 13);
        assert_eq!(tenths(-1.25), -13);
        assert_eq!(tenths(0.04), 0);
        assert_eq!(tenths(f64::MAX), i64::MAX);
        assert_eq!(tenths(units(-12_345)), -12_345);
    }
}
