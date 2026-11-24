//! How an item is drawn past its shape (docs/design/figures.md, "The
//! model"): its colour, a fill, a dash, an arrow's heads and a stroke
//! closed into a loop. Values and the rule for which kind takes which; the
//! file keys are in `file/style.rs`.

use super::item::invalid;
use super::{Item, Shape};
use crate::Result;

/// A colour, 8 bits a channel, alpha last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Colour {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Colour {
    /// Opaque black, the default ink.
    pub const BLACK: Self = Self::rgb(0, 0, 0);

    /// An opaque colour.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// `#rrggbb`, or `#rrggbbaa` with alpha; `None` for anything else.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let hex = s.strip_prefix('#')?;
        if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
        let a = if hex.len() == 8 { byte(6) } else { 255 };
        Some(Self { r: byte(0), g: byte(2), b: byte(4), a })
    }
}

/// The agent colour: a figure's shapes take it unless they name another.
/// A script resolves its colour names to hex; this is `agent`'s.
pub const AGENT: Colour = Colour::rgb(0x8a, 0x5c, 0xf5);

/// The most a fill covers what lies under it: a fill is a tint, drawn at
/// its own alpha up to this (40%), so ink over it always reads. The app's
/// fill pass caps at the same value.
pub const FILL_MAX_ALPHA: u8 = 0x66;

/// How a line is drawn: whole, in dashes or in dots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Dash {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

/// Which ends of an arrow have a head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Heads {
    /// At `to` only: the arrow as it always was.
    #[default]
    End,
    /// No head: a plain line whose ends still bind to items.
    None,
    /// At both ends.
    Both,
}

impl Dash {
    /// The word the file writes, `None` for solid (left out).
    #[must_use]
    pub const fn word(self) -> Option<&'static str> {
        match self {
            Dash::Solid => None,
            Dash::Dashed => Some("dashed"),
            Dash::Dotted => Some("dotted"),
        }
    }

    /// Reads [`Dash::word`]: `dashed` or `dotted`.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        match word {
            "dashed" => Some(Dash::Dashed),
            "dotted" => Some(Dash::Dotted),
            _ => None,
        }
    }
}

impl Heads {
    /// The word the file writes, `None` for `end` (left out).
    #[must_use]
    pub const fn word(self) -> Option<&'static str> {
        match self {
            Heads::End => None,
            Heads::None => Some("none"),
            Heads::Both => Some("both"),
        }
    }

    /// Reads `none`, `end` or `both`.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        match word {
            "end" => Some(Heads::End),
            "none" => Some(Heads::None),
            "both" => Some(Heads::Both),
            _ => None,
        }
    }
}

/// An item's style. The default is how every item drew before styles:
/// no fill, solid, a head at an arrow's `to`, a stroke open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Style {
    /// The inside's tint: a box's, or a closed stroke's. Drawn under all
    /// ink, at most [`FILL_MAX_ALPHA`].
    pub fill: Option<Colour>,
    /// A box's outline, a stroke or an arrow's shaft.
    pub dash: Dash,
    /// An arrow's heads.
    pub heads: Heads,
    /// A stroke whose last point joins its first.
    pub closed: bool,
}

/// One style field, for the rule of which kind takes it.
#[derive(Debug, Clone, Copy)]
enum Field {
    Fill,
    Dash,
    Heads,
    Closed,
}

impl Style {
    /// The fill as drawn: its colour at its alpha, at most [`FILL_MAX_ALPHA`].
    #[must_use]
    pub fn tint(&self) -> Option<Colour> {
        self.fill.map(|c| Colour { a: c.a.min(FILL_MAX_ALPHA), ..c })
    }

    /// The same style with each field `shape` does not take at its
    /// default: what a style kept over a change of shape becomes.
    #[must_use]
    pub fn fitted(self, shape: &Shape) -> Self {
        let closed = self.closed && takes(Field::Closed, shape, self);
        let mut out = Self { closed, ..self };
        if !takes(Field::Fill, shape, out) {
            out.fill = None;
        }
        if !takes(Field::Dash, shape, out) {
            out.dash = Dash::Solid;
        }
        if !takes(Field::Heads, shape, out) {
            out.heads = Heads::End;
        }
        out
    }

    /// Rejects a field `shape` does not take: a fill on anything but a
    /// box or a closed stroke, a dash on anything but a box, a stroke or an
    /// arrow, heads on anything but an arrow, `closed` on anything but a
    /// stroke.
    pub(super) fn check(self, shape: &Shape) -> Result<()> {
        let set = [
            (Field::Closed, self.closed),
            (Field::Fill, self.fill.is_some()),
            (Field::Dash, self.dash != Dash::Solid),
            (Field::Heads, self.heads != Heads::End),
        ];
        for (field, set) in set {
            if set && !takes(field, shape, self) {
                let (name, on) = match field {
                    Field::Fill => ("fill", "a box or a stroke with `closed`"),
                    Field::Dash => ("dash", "a box, a stroke or an arrow"),
                    Field::Heads => ("heads", "an arrow"),
                    Field::Closed => ("closed", "a stroke"),
                };
                let hint = format!("`{name}` is for {on}; leave it out here");
                return Err(invalid(format!("this item cannot take `{name}`"), &hint));
            }
        }
        Ok(())
    }
}

/// Whether `shape` takes `field`, given the rest of its `style` (a
/// stroke takes a fill only when closed).
fn takes(field: Field, shape: &Shape, style: Style) -> bool {
    match (field, shape) {
        (Field::Fill, Shape::Stroke { .. }) => style.closed,
        (Field::Fill | Field::Dash, Shape::Box { .. })
        | (Field::Dash, Shape::Stroke { .. } | Shape::Arrow { .. })
        | (Field::Heads, Shape::Arrow { .. })
        | (Field::Closed, Shape::Stroke { .. }) => true,
        _ => false,
    }
}

impl Item {
    /// How the item is drawn past its shape.
    #[must_use]
    pub fn style(&self) -> &Style {
        &self.style
    }
}
