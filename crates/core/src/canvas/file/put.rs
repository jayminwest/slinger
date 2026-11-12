//! How an agent answers on a canvas (SPEC section 7): text, a box, ink,
//! an arrow or a live item (an agent UI), put beside an item it names,
//! where it covers nothing.
//!
//! The item named takes ids as `crop` does (`i12`, or a packet's `c12`,
//! `t12`): `c<n>` on the user's ink is the whole cluster. The new item goes
//! on that item's page, in the free space [`Canvas::free_beside`] finds. An
//! arrow joins two items on that page instead, from the edge of one to the
//! edge of the other, and points at both. [`How`] gives the item a colour
//! (black by default) and, for all but an arrow, a place of the agent's
//! choosing: the top-left corner of its box. Lengths are in tenths of a
//! unit, as everywhere in a request. The put is one undo step of its
//! author, and the item carries the author and the time, as every item does.
//!
//! [`Canvas::free_beside`]: crate::canvas::Canvas::free_beside

use super::{Log, Written, json};
use crate::canvas::{Action, Author, Colour, End, Item, ItemId, PageId, Point, Rect, Shape, Time};
use crate::render;
use crate::{Error, Result};

/// The widest a put text runs before it wraps, in characters.
pub const TEXT_COLUMNS: usize = 40;

/// The width of put ink, in canvas units: the app's thinnest pen.
pub const INK_WIDTH: f64 = 2.0;

/// What a put adds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Put {
    /// Text, wrapped to at most [`TEXT_COLUMNS`] characters a line.
    Text(String),
    /// An empty box, `w` by `h` tenths of a unit.
    Box { w: i64, h: i64 },
    /// A pen stroke at [`INK_WIDTH`] through `pts`, as the file holds them:
    /// x,y pairs, the first where it starts, each next the step from the one
    /// before. Only its shape counts: it goes where any put goes.
    Ink(Vec<i64>),
    /// An arrow from one item to another.
    Arrow { from: String, to: String },
    /// A live item: the showing `open` of the UI `ui`, whose code is the blob
    /// `hash`, drawn in a box `w` by `h` tenths of a unit.
    Live { ui: String, hash: String, open: String, w: i64, h: i64 },
}

/// How a put draws what it adds; what it leaves out is the default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct How {
    /// `#rrggbb` or `#rrggbbaa`; black when absent. A live item has none.
    pub colour: Option<String>,
    /// Where the top-left corner of the item's box goes, `[x, y]` in tenths
    /// of a unit, in place of the free space beside the item. Not for an
    /// arrow, whose ends are its items'.
    pub at: Option<[i64; 2]>,
}

/// What a put made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    /// The line it wrote.
    pub written: Written,
    /// The new item's id (`i12`).
    pub item: String,
    /// Its page's id (`p1`).
    pub page: String,
    /// The box it covers, `[x, y, w, h]` in tenths of a unit.
    pub frame: [i64; 4],
}

impl Log {
    /// Puts `what` beside the item `beside` names, as `by` at `at`, drawn
    /// the default way. [`Log::check_put`] says first whether it would go.
    ///
    /// # Errors
    ///
    /// As [`Log::put_how`]'s.
    pub fn put(&mut self, by: &Author, at: Time, beside: &str, what: &Put) -> Result<Placed> {
        self.put_how(by, at, beside, what, &How::default())
    }

    /// Puts `what` beside the item `beside` names, as `by` at `at`, drawn as
    /// `how` says.
    ///
    /// # Errors
    ///
    /// `not_found` for an item that is not on the canvas, and `invalid` for
    /// an id that is not one, blank text, a box of a negative size, ink that
    /// is not x,y pairs, a live item of no area, a colour that is not one, a
    /// place for an arrow, an arrow to an item on another page, a live item
    /// that is not one, or an author that may not add.
    pub fn put_how(
        &mut self,
        by: &Author,
        at: Time,
        beside: &str,
        what: &Put,
        how: &How,
    ) -> Result<Placed> {
        let colour = self.check_put_how(beside, what, how)?;
        let (_, near, page) = self.named(beside)?;
        let origin = |w, h| Rect::new(0.0, 0.0, w, h);
        let shape = match what {
            Put::Text(text) => {
                let (w, h) = render::text_size(text, TEXT_COLUMNS);
                Shape::Text { text: text.clone(), frame: origin(w, h), colour }
            }
            Put::Box { w, h } => {
                Shape::Box { frame: origin(json::units(*w), json::units(*h)), colour }
            }
            Put::Ink(pts) => Shape::Stroke { points: points(pts), colour, width: INK_WIDTH },
            Put::Live { ui, hash, open, w, h } => Shape::Live {
                frame: origin(json::units(*w), json::units(*h)),
                ui: ui.clone(),
                hash: hash.clone(),
                open: open.clone(),
            },
            Put::Arrow { from, to } => {
                let ((from, a, on_a), (to, b, on_b)) = (self.named(from)?, self.named(to)?);
                if on_a != page || on_b != page {
                    let hint = "join two items on the page of the item it goes beside";
                    return Err(invalid("the arrow's items are on another page", hint));
                }
                let (ca, cb) = (centre(a), centre(b));
                let (from, to) = (End::on(edge(a, cb), from), End::on(edge(b, ca), to));
                Shape::Arrow { from, to, colour }
            }
        };
        let shape = match (what, how.at) {
            (Put::Arrow { .. }, _) => shape,
            (_, Some([x, y])) => placed(&shape, Point::new(json::units(x), json::units(y))),
            (_, None) => {
                let b = shape.bounds();
                let free = self.canvas().free_beside(page, near, b.w, b.h);
                placed(&shape, Point::new(free.x, free.y))
            }
        };
        let written = self.step(by, at, vec![Action::Add { page, shape }])?;
        let item = written.made.first().cloned().unwrap_or_default();
        let made = json::parse_item(&item).ok().and_then(|id| self.canvas().item(id));
        let frame = made.map(Item::bounds).unwrap_or_default();
        let frame = [frame.x, frame.y, frame.w, frame.h].map(json::tenths);
        Ok(Placed { written, item, page: json::page_id(page), frame })
    }

    /// Whether a put of `what` beside the item `beside` names would go, as
    /// far as can be told before what it needs is made (the showing a live
    /// item names): the item is there, the text is not blank, a box's size
    /// is not below zero, ink is x,y pairs and a live item has an area.
    ///
    /// # Errors
    ///
    /// As [`Log::put`]'s for those.
    pub fn check_put(&self, beside: &str, what: &Put) -> Result<()> {
        self.check_put_how(beside, what, &How::default()).map(|_| ())
    }

    /// [`Log::check_put`], and that `how` fits `what`; the colour to draw in.
    fn check_put_how(&self, beside: &str, what: &Put, how: &How) -> Result<Colour> {
        self.named(beside)?;
        match what {
            Put::Text(text) if text.trim().is_empty() => {
                return Err(invalid("the text is blank", "pass the words to put"));
            }
            Put::Box { w, h } if *w < 0 || *h < 0 => {
                return Err(invalid("the box has a size below zero", "give w,h of 0 or more"));
            }
            Put::Ink(pts) if pts.is_empty() || !pts.len().is_multiple_of(2) => {
                let hint = "give x,y pairs: where it starts, then each step from the one before";
                return Err(invalid("the ink is not x,y pairs", hint));
            }
            Put::Live { w, h, .. } if *w <= 0 || *h <= 0 => {
                return Err(invalid(
                    "a live item has no area",
                    "give a size above 0, such as --size 3200,2400",
                ));
            }
            Put::Arrow { .. } if how.at.is_some() => {
                let hint = "an arrow goes from its item to its item; leave out the place";
                return Err(invalid("an arrow takes no place", hint));
            }
            _ => {}
        }
        match &how.colour {
            None => Ok(Colour::BLACK),
            Some(c) => Colour::parse(c).ok_or_else(|| {
                invalid(&format!("{c:?} is not a colour"), "give #rrggbb or #rrggbbaa")
            }),
        }
    }

    /// The item an id names, the box around what it names (a whole cluster
    /// for `c<n>` on ink) and its page.
    pub(super) fn named(&self, id: &str) -> Result<(ItemId, Rect, PageId)> {
        let items = self.resolve(id)?;
        let canvas = self.canvas();
        let number = id.get(1..).and_then(|n| n.parse().ok()).map(ItemId::new);
        let found = number.and_then(|n| canvas.item(n));
        match (found, canvas.bounds_of(items)) {
            (Some(item), Some(frame)) => Ok((item.id(), frame, item.page())),
            _ => Err(Error::NotFound { what: format!("item {id}"), hint: None }),
        }
    }
}

/// `shape` moved so the top-left corner of its box is at `to`.
fn placed(shape: &Shape, to: Point) -> Shape {
    let b = shape.bounds();
    shape.shifted(Point::new(to.x - b.x, to.y - b.y))
}

/// The points of a flat pts list (see [`Put::Ink`]), in canvas units.
fn points(pts: &[i64]) -> Vec<Point> {
    let (mut x, mut y) = (0_i64, 0_i64);
    let pairs = pts.as_chunks::<2>().0;
    pairs
        .iter()
        .map(|&[dx, dy]| {
            (x, y) = (x.saturating_add(dx), y.saturating_add(dy));
            Point::new(json::units(x), json::units(y))
        })
        .collect()
}

fn centre(r: Rect) -> Point {
    Point::new(r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// Where the line from `r`'s centre toward `toward` leaves `r`, or `toward`
/// when that is inside it.
fn edge(r: Rect, toward: Point) -> Point {
    let c = centre(r);
    let (dx, dy) = (toward.x - c.x, toward.y - c.y);
    let reach = |half: f64, d: f64| if d == 0.0 { f64::INFINITY } else { half / d.abs() };
    let t = reach(r.w / 2.0, dx).min(reach(r.h / 2.0, dy)).min(1.0);
    Point::new(c.x + dx * t, c.y + dy * t)
}

fn invalid(message: &str, hint: &str) -> Error {
    Error::Invalid { message: message.into(), hint: Some(hint.into()) }
}
