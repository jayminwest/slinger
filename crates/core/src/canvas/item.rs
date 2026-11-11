//! What a canvas holds: items, their shapes, who made them and when. Values
//! only; the rules for changing a canvas live in `mod.rs`.

use std::fmt;

use crate::{Error, Result};

/// Names one item on a canvas. A canvas never reuses an id, even after the
/// item is removed, so an id in an old operation or packet never means a
/// different item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemId(u64);

/// Names one page on a canvas. Never reused, like [`ItemId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageId(u64);

macro_rules! id {
    ($t:ident, $prefix:literal) => {
        impl $t {
            /// The id with this number. An id a canvas did not hand out
            /// names nothing, and lookups with it find nothing.
            #[must_use]
            pub const fn new(n: u64) -> Self {
                Self(n)
            }

            /// The number inside the id.
            #[must_use]
            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $t {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }
    };
}

id!(ItemId, "item ");
id!(PageId, "page ");

/// A moment, in milliseconds since the Unix epoch. The caller supplies it: the
/// core reads no clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Time(pub u64);

/// Who made an item or asked for a change.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Author {
    /// The person at the window. The user may change anything.
    User,
    /// An agent, by name. An agent may change only items it made.
    Agent(String),
}

impl Author {
    /// Whether this author may edit, move or remove an item `maker` made.
    #[must_use]
    pub fn may_change(&self, maker: &Author) -> bool {
        matches!(self, Author::User) || self == maker
    }
}

impl fmt::Display for Author {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Author::User => f.write_str("the user"),
            Author::Agent(name) => write!(f, "agent {name}"),
        }
    }
}

/// A point in canvas units. The canvas is endless, so any finite value is a
/// place on it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    fn shifted(self, by: Point) -> Self {
        Self::new(self.x + by.x, self.y + by.y)
    }

    fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// An axis-aligned rectangle: its top-left corner and a size that is never
/// negative.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    #[must_use]
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    /// The smallest rectangle that holds every point, or `None` for no points.
    #[must_use]
    pub fn around(points: impl IntoIterator<Item = Point>) -> Option<Self> {
        let mut points = points.into_iter();
        let first = points.next()?;
        let (mut lo, mut hi) = (first, first);
        for p in points {
            lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
        }
        Some(Self::new(lo.x, lo.y, hi.x - lo.x, hi.y - lo.y))
    }

    fn grown(self, by: f64) -> Self {
        Self::new(self.x - by, self.y - by, self.w + 2.0 * by, self.h + 2.0 * by)
    }

    fn shifted(self, by: Point) -> Self {
        Self::new(self.x + by.x, self.y + by.y, self.w, self.h)
    }

    fn check(self, what: &str) -> Result<()> {
        let finite = [self.x, self.y, self.w, self.h].iter().all(|v| v.is_finite());
        if !finite || self.w < 0.0 || self.h < 0.0 {
            return Err(invalid(
                format!("{what} has a size below zero or a value that is not a number"),
                "give a finite position and a size of zero or more",
            ));
        }
        Ok(())
    }
}

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
}

/// One end of an arrow: where it is, and the item it points at, if any.
///
/// The point is always kept, so an arrow still draws after the item it points
/// at is removed. A canvas checks `item` when the arrow is added or edited;
/// after that the item may go away.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct End {
    pub at: Point,
    pub item: Option<ItemId>,
}

impl End {
    /// An end at a point that points at no item.
    #[must_use]
    pub const fn free(at: Point) -> Self {
        Self { at, item: None }
    }

    /// An end at a point that points at `item`.
    #[must_use]
    pub const fn on(at: Point, item: ItemId) -> Self {
        Self { at, item: Some(item) }
    }
}

/// What an item looks like. This is the part of an item a caller supplies; the
/// canvas adds the id, the page, the author and the time.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Pen ink at a constant width (D6: no pressure). `width` is in canvas
    /// units and is above zero.
    Stroke { points: Vec<Point>, colour: Colour, width: f64 },
    /// Text laid out inside `frame`.
    Text { text: String, frame: Rect, colour: Colour },
    /// A rectangle outline.
    Box { frame: Rect, colour: Colour },
    /// A straight arrow from one end to the other.
    Arrow { from: End, to: End, colour: Colour },
    /// A live item (docs/design/live-items.md): an agent UI drawn in
    /// `frame`, one CSS pixel a canvas unit. `ui` is the UI's name, `hash`
    /// the blob id of the exact code it shows (D19) and `open` the showing
    /// it belongs to (`<name>#<n>`), whose data and agent its context gives.
    Live { frame: Rect, ui: String, hash: String, open: String },
}

impl Shape {
    /// The rectangle the shape covers. A stroke's box includes half its width
    /// on every side.
    #[must_use]
    pub fn bounds(&self) -> Rect {
        match self {
            Shape::Stroke { points, width, .. } => {
                Rect::around(points.iter().copied()).unwrap_or_default().grown(width / 2.0)
            }
            Shape::Text { frame, .. } | Shape::Box { frame, .. } | Shape::Live { frame, .. } => {
                *frame
            }
            Shape::Arrow { from, to, .. } => Rect::around([from.at, to.at]).unwrap_or_default(),
        }
    }

    /// The items this shape points at.
    pub(super) fn targets(&self) -> impl Iterator<Item = ItemId> {
        let ends = match self {
            Shape::Arrow { from, to, .. } => [from.item, to.item],
            _ => [None, None],
        };
        ends.into_iter().flatten()
    }

    /// The same shape, shifted by `by`.
    pub(super) fn shifted(&self, by: Point) -> Self {
        let mut s = self.clone();
        match &mut s {
            Shape::Stroke { points, .. } => points.iter_mut().for_each(|p| *p = p.shifted(by)),
            Shape::Text { frame, .. } | Shape::Box { frame, .. } | Shape::Live { frame, .. } => {
                *frame = frame.shifted(by);
            }
            Shape::Arrow { from, to, .. } => {
                from.at = from.at.shifted(by);
                to.at = to.at.shifted(by);
            }
        }
        s
    }

    /// Rejects a shape that cannot be drawn.
    pub(super) fn check(&self) -> Result<()> {
        match self {
            Shape::Stroke { points, width, .. } => {
                if points.is_empty() {
                    return Err(invalid("a stroke has no points", "give at least one point"));
                }
                if !points.iter().all(|p| p.is_finite()) {
                    return Err(invalid("a stroke point is not a number", "give finite points"));
                }
                if !(width.is_finite() && *width > 0.0) {
                    return Err(invalid(
                        "a stroke width is not above zero",
                        "give a width above 0",
                    ));
                }
                Ok(())
            }
            Shape::Text { frame, .. } => frame.check("the text frame"),
            Shape::Box { frame, .. } => frame.check("the box"),
            Shape::Live { frame, ui, hash, open } => {
                frame.check("the live item")?;
                crate::ui::check_name(ui)?;
                hash.parse::<crate::record::BlobId>()?;
                if crate::ui::showing::number(open).is_none() {
                    return Err(invalid(
                        format!("`{open}` is not a showing's id"),
                        "a showing's id is <name>#<number>, as 'slinger ui open' prints it",
                    ));
                }
                Ok(())
            }
            Shape::Arrow { from, to, .. } => {
                if from.at.is_finite() && to.at.is_finite() {
                    Ok(())
                } else {
                    Err(invalid("an arrow end is not a number", "give finite ends"))
                }
            }
        }
    }
}

/// One thing on a canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub(super) id: ItemId,
    pub(super) page: PageId,
    pub(super) author: Author,
    pub(super) time: Time,
    pub(super) shape: Shape,
}

impl Item {
    #[must_use]
    pub fn id(&self) -> ItemId {
        self.id
    }

    /// The page the item is on.
    #[must_use]
    pub fn page(&self) -> PageId {
        self.page
    }

    /// Who made the item. An edit or a move does not change it.
    #[must_use]
    pub fn author(&self) -> &Author {
        &self.author
    }

    /// When the item was made. An edit or a move does not change it.
    #[must_use]
    pub fn time(&self) -> Time {
        self.time
    }

    #[must_use]
    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    /// The rectangle the item covers.
    #[must_use]
    pub fn bounds(&self) -> Rect {
        self.shape.bounds()
    }
}

/// A named page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub(super) id: PageId,
    pub(super) name: String,
}

impl Page {
    #[must_use]
    pub fn id(&self) -> PageId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

pub(super) fn invalid(message: impl Into<String>, hint: &str) -> Error {
    Error::Invalid { message: message.into(), hint: Some(hint.into()) }
}
