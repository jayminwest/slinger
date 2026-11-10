//! How an agent answers on a canvas (SPEC section 7): text, a box or an
//! arrow, put beside an item it names, where it covers nothing.
//!
//! The item named takes ids as `crop` does (`i12`, or a packet's `c12`,
//! `t12`): `c<n>` on the user's ink is the whole cluster. The new item goes
//! on that item's page, in the free space [`Canvas::free_beside`] finds. An
//! arrow joins two items on that page instead, from the edge of one to the
//! edge of the other, and points at both. Lengths are in tenths of a unit,
//! as everywhere in a request. The put is one undo step of its author, and
//! the item carries the author and the time, as every item does.
//!
//! [`Canvas::free_beside`]: crate::canvas::Canvas::free_beside

use super::{Log, Written, json};
use crate::canvas::{Action, Author, Colour, End, Item, ItemId, PageId, Point, Rect, Shape, Time};
use crate::render;
use crate::{Error, Result};

/// The widest a put text runs before it wraps, in characters.
pub const TEXT_COLUMNS: usize = 40;

/// What a put adds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Put {
    /// Text, wrapped to at most [`TEXT_COLUMNS`] characters a line.
    Text(String),
    /// An empty box, `w` by `h` tenths of a unit.
    Box { w: i64, h: i64 },
    /// An arrow from one item to another.
    Arrow { from: String, to: String },
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
    /// Puts `what` beside the item `beside` names, as `by` at `at`.
    ///
    /// # Errors
    ///
    /// `not_found` for an item that is not on the canvas, and `invalid` for
    /// an id that is not one, blank text, a box of a negative size, an arrow
    /// to an item on another page, or an author that may not add.
    pub fn put(&mut self, by: &Author, at: Time, beside: &str, what: &Put) -> Result<Placed> {
        let (_, near, page) = self.named(beside)?;
        let shape = match what {
            Put::Text(text) => {
                if text.trim().is_empty() {
                    return Err(invalid("the text is blank", "pass the words to put"));
                }
                let (w, h) = render::text_size(text, TEXT_COLUMNS);
                let frame = self.canvas().free_beside(page, near, w, h);
                Shape::Text { text: text.clone(), frame, colour: Colour::BLACK }
            }
            Put::Box { w, h } => {
                let (w, h) = (json::units(*w), json::units(*h));
                if w < 0.0 || h < 0.0 {
                    return Err(invalid("the box has a size below zero", "give w,h of 0 or more"));
                }
                let frame = self.canvas().free_beside(page, near, w, h);
                Shape::Box { frame, colour: Colour::BLACK }
            }
            Put::Arrow { from, to } => {
                let ((from, a, on_a), (to, b, on_b)) = (self.named(from)?, self.named(to)?);
                if on_a != page || on_b != page {
                    let hint = "join two items on the page of the item it goes beside";
                    return Err(invalid("the arrow's items are on another page", hint));
                }
                let (ca, cb) = (centre(a), centre(b));
                let (from, to) = (End::on(edge(a, cb), from), End::on(edge(b, ca), to));
                Shape::Arrow { from, to, colour: Colour::BLACK }
            }
        };
        let written = self.step(by, at, vec![Action::Add { page, shape }])?;
        let item = written.made.first().cloned().unwrap_or_default();
        let made = json::parse_item(&item).ok().and_then(|id| self.canvas().item(id));
        let frame = made.map(Item::bounds).unwrap_or_default();
        let frame = [frame.x, frame.y, frame.w, frame.h].map(json::tenths);
        Ok(Placed { written, item, page: json::page_id(page), frame })
    }

    /// The item an id names, the box around what it names (a whole cluster
    /// for `c<n>` on ink) and its page.
    fn named(&self, id: &str) -> Result<(ItemId, Rect, PageId)> {
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
