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
        let _ = write!(
            out,
            "{} {} {},{} {}x{}",
            self.id,
            self.kind,
            units(f.x),
            units(f.y),
            units(f.w),
            units(f.h)
        );
        for (key, value) in &self.fields {
            let _ = write!(out, " {key}={value}");
        }
        if !images.is_empty() {
            let _ = write!(out, " img={}", images.join(","));
        }
        if let Some(text) = &self.text {
            out.push_str(" \"");
            quote(out, text, short);
            out.push('"');
        }
        out.push('\n');
    }
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
