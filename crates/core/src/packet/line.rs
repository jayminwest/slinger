//! One line of a packet's text, and the order lines are read in. Owns the
//! line grammar of `docs/design/packet.md`: the box syntax, field syntax,
//! quoting, colour names and reading order.

use std::cmp::Ordering;
use std::fmt::Write as _;

use crate::canvas::{Colour, Rect};

/// How many characters of a text item the `page` section keeps.
const PAGE_TEXT_CHARS: usize = 80;

/// One line: `id kind x,y wxh field... "text"`.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Line {
    /// The item number the id is built from. Unique within a packet, so it
    /// breaks ties in reading order.
    pub n: u64,
    pub id: String,
    pub kind: &'static str,
    pub frame: Rect,
    /// `key=value` pairs, in the order the format lists them.
    pub fields: Vec<(&'static str, String)>,
    /// The full text of a `text` line.
    pub text: Option<String>,
}

impl Line {
    /// The line, with `img` names appended as the last field, and the text
    /// cut to its first 80 characters when `short`.
    pub fn write(&self, out: &mut String, images: &[String], short: bool) {
        let f = self.frame;
        let img = images.join(",");
        let mut fields: Vec<(&str, &str)> =
            self.fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
        if !images.is_empty() {
            fields.push(("img", &img));
        }
        let frame = [units(f.x), units(f.y), units(f.w), units(f.h)];
        write_line(out, (&self.id, self.kind), frame, &fields, self.text.as_deref(), short);
    }
}

/// One line of a packet as read back: an item, or a cluster of ink.
///
/// Kinds and fields are kept as read, known or not; a reader skips what it
/// does not know (B5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// `c42`, `t12`, ...
    pub id: String,
    /// `ink`, `text`, `box` or `arrow`.
    pub kind: String,
    /// The box, in whole canvas units.
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    /// `key=value` pairs, in the order written.
    pub fields: Vec<(String, String)>,
    /// The quoted text of a `text` line.
    pub text: Option<String>,
}

impl Entry {
    /// The value of the first field named `key`.
    #[must_use]
    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub(super) fn write(&self, out: &mut String) {
        let fields: Vec<(&str, &str)> =
            self.fields.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let frame = [self.x, self.y, self.w, self.h];
        write_line(out, (&self.id, &self.kind), frame, &fields, self.text.as_deref(), false);
    }

    /// Reads one line, without its `\n`. `None` when it breaks the grammar.
    pub(super) fn parse(line: &str) -> Option<Self> {
        let (head, text) = match line.find('"') {
            Some(at) => (line[..at].strip_suffix(' ')?, Some(unquote(&line[at..])?)),
            None => (line, None),
        };
        let mut tokens = head.split(' ');
        let mut next = || tokens.next().filter(|t| !t.is_empty());
        let (id, kind) = (next()?.to_string(), next()?.to_string());
        let (x, y) = next()?.split_once(',')?;
        let (w, h) = next()?.split_once('x')?;
        let (x, y, w, h) = (x.parse().ok()?, y.parse().ok()?, w.parse().ok()?, h.parse().ok()?);
        let mut fields = Vec::new();
        while let Some(token) = next() {
            let (key, value) = token.split_once('=').filter(|(k, _)| !k.is_empty())?;
            fields.push((key.to_string(), unvalue(value)?));
        }
        if tokens.next().is_some() {
            return None;
        }
        Some(Self { id, kind, x, y, w, h, fields, text })
    }
}

/// Writes `id kind x,y wxh key=value... "text"` and the line end.
fn write_line(
    out: &mut String,
    (id, kind): (&str, &str),
    [x, y, w, h]: [i64; 4],
    fields: &[(&str, &str)],
    text: Option<&str>,
    short: bool,
) {
    let _ = write!(out, "{id} {kind} {x},{y} {w}x{h}");
    for (key, value) in fields {
        let _ = write!(out, " {key}=");
        push_value(out, value);
    }
    if let Some(text) = text {
        out.push_str(" \"");
        quote(out, text, short);
        out.push('"');
    }
    out.push('\n');
}

/// Appends a field value. A space, `"`, `%` or control character in it is
/// written as `%XX` for each of its UTF-8 bytes, so a value is one token.
pub(super) fn push_value(out: &mut String, value: &str) {
    for c in value.chars() {
        if c == ' ' || c == '"' || c == '%' || c.is_control() {
            for byte in c.to_string().bytes() {
                let _ = write!(out, "%{byte:02X}");
            }
        } else {
            out.push(c);
        }
    }
}

/// A field value as written by [`push_value`], decoded. `None` when a `%` is
/// not followed by two hex digits or the bytes are not UTF-8.
pub(super) fn unvalue(value: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(value.len());
    let mut rest = value.as_bytes();
    while let Some((&b, tail)) = rest.split_first() {
        if b == b'%' {
            let hex = std::str::from_utf8(tail.get(..2)?).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            rest = &tail[2..];
        } else {
            bytes.push(b);
            rest = tail;
        }
    }
    String::from_utf8(bytes).ok()
}

/// The text inside `"..."`, unescaped. `None` when the quotes are not whole
/// or an escape is unknown.
fn unquote(quoted: &str) -> Option<String> {
    let inner = quoted.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push(match chars.next()? {
                '"' => '"',
                '\\' => '\\',
                'n' => '\n',
                _ => return None,
            }),
            '"' => return None,
            c => out.push(c),
        }
    }
    Some(out)
}

/// A coordinate as the format writes it: an integer, rounded to nearest.
fn units(v: f64) -> i64 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "canvas coordinates are far inside the i64 range; `as` saturates beyond it"
    )]
    let n = v.round() as i64;
    n
}

/// Appends `text` with `"`, `\` and newline escaped, cut to 80 characters
/// plus `…` when `short` and longer than that.
fn quote(out: &mut String, text: &str, short: bool) {
    let cut = short && text.chars().count() > PAGE_TEXT_CHARS;
    let kept = if cut { PAGE_TEXT_CHARS } else { usize::MAX };
    for c in text.chars().take(kept) {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    if cut {
        out.push('…');
    }
}

/// The name a line gives a colour: a common name for an exact match, else
/// `#rrggbb`, with `aa` added when it is not opaque.
pub(super) fn colour_name(c: Colour) -> String {
    let named = match (c.r, c.g, c.b, c.a) {
        (0, 0, 0, 255) => "black",
        (255, 255, 255, 255) => "white",
        (255, 0, 0, 255) => "red",
        (0, 128, 0, 255) => "green",
        (0, 0, 255, 255) => "blue",
        (255, 255, 0, 255) => "yellow",
        (255, 165, 0, 255) => "orange",
        (128, 0, 128, 255) => "purple",
        (128, 128, 128, 255) => "grey",
        _ if c.a == 255 => return format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b),
        _ => return format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a),
    };
    named.to_string()
}

/// Puts lines in reading order: by row, then left to right. The topmost line
/// left starts a row, and every other line whose box overlaps it vertically
/// by at least half the shorter height joins that row.
pub(super) fn reading_order(lines: Vec<Line>) -> Vec<Line> {
    let mut rest = lines;
    rest.sort_by(|a, b| by_y(a, b).then_with(|| by_x(a, b)));
    let mut out = Vec::with_capacity(rest.len());
    while !rest.is_empty() {
        let anchor = rest[0].frame;
        let (mut row, others): (Vec<Line>, Vec<Line>) =
            rest.into_iter().partition(|l| same_row(anchor, l.frame));
        row.sort_by(|a, b| by_x(a, b).then_with(|| by_y(a, b)));
        out.append(&mut row);
        rest = others;
    }
    out
}

fn by_y(a: &Line, b: &Line) -> Ordering {
    a.frame.y.total_cmp(&b.frame.y).then(a.n.cmp(&b.n))
}

fn by_x(a: &Line, b: &Line) -> Ordering {
    a.frame.x.total_cmp(&b.frame.x).then(a.n.cmp(&b.n))
}

fn same_row(a: Rect, b: Rect) -> bool {
    let overlap = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
    overlap >= 0.0 && overlap >= a.h.min(b.h) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(n: u64, frame: Rect) -> Line {
        Line { n, id: format!("t{n}"), kind: "text", frame, fields: vec![], text: None }
    }

    fn order(lines: Vec<Line>) -> Vec<u64> {
        reading_order(lines).iter().map(|l| l.n).collect()
    }

    #[test]
    fn rows_go_top_down_and_each_row_left_to_right() {
        let lines = vec![
            line(1, Rect::new(500.0, 10.0, 100.0, 40.0)),
            line(2, Rect::new(0.0, 25.0, 100.0, 40.0)),
            line(3, Rect::new(0.0, 200.0, 100.0, 40.0)),
        ];
        assert_eq!(order(lines), [2, 1, 3]);
    }

    #[test]
    fn a_small_vertical_overlap_starts_a_new_row() {
        // Overlap 10 is under half of the shorter height 40.
        let lines = vec![
            line(1, Rect::new(500.0, 0.0, 100.0, 40.0)),
            line(2, Rect::new(0.0, 30.0, 100.0, 40.0)),
        ];
        assert_eq!(order(lines), [1, 2]);
    }

    #[test]
    fn a_flat_line_inside_a_row_joins_it() {
        let lines = vec![
            line(1, Rect::new(500.0, 0.0, 100.0, 40.0)),
            line(2, Rect::new(0.0, 20.0, 100.0, 0.0)),
            line(3, Rect::new(0.0, 41.0, 100.0, 0.0)),
        ];
        assert_eq!(order(lines), [2, 1, 3]);
    }

    #[test]
    fn colours_have_names_or_hex() {
        assert_eq!(colour_name(Colour::BLACK), "black");
        assert_eq!(colour_name(Colour::rgb(255, 0, 0)), "red");
        assert_eq!(colour_name(Colour::rgb(1, 2, 255)), "#0102ff");
        assert_eq!(colour_name(Colour { r: 0, g: 0, b: 0, a: 128 }), "#00000080");
    }

    #[test]
    fn text_is_escaped_and_cut_only_when_short() {
        let mut l = line(7, Rect::new(-0.4, 1.5, 10.2, 0.0));
        l.text = Some(format!("a \"b\" \\ c\n{}", "x".repeat(80)));
        let mut full = String::new();
        l.write(&mut full, &[], false);
        assert_eq!(full, format!("t7 text 0,2 10x0 \"a \\\"b\\\" \\\\ c\\n{}\"\n", "x".repeat(80)));
        let mut short = String::new();
        l.write(&mut short, &[], true);
        let kept = format!("a \\\"b\\\" \\\\ c\\n{}", "x".repeat(70));
        assert_eq!(short, format!("t7 text 0,2 10x0 \"{kept}…\"\n"));
    }
}
