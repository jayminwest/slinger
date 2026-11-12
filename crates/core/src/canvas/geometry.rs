//! Geometry queries on a page: the item under a point, the items inside a
//! lasso, the items an eraser touches, the items a box touches, the box
//! around a set of items, and free space beside an item. Pure: no drawing and no input. The app asks them as
//! the pen moves; the packet asks them for what the user selected.
//!
//! Every query takes canvas units, answers in drawing order (first drawn
//! first), and has no errors: an empty page, a missing page, a lasso of fewer
//! than three points or an empty eraser path finds nothing, and a negative or
//! not-a-number tolerance counts as zero.
//!
//! How each shape is hit:
//! - A stroke is its line, thickened by half its width on each side. A stroke
//!   of one point is a dot.
//! - An arrow is the line between its ends.
//! - Text and a box are solid: their whole frame counts, not only its edge.
//!
//! The eraser is the exception: it takes a box by its outline only, so
//! erasing the ink inside a box keeps the box. It takes strokes, text,
//! boxes and arrows, never a live item or the button: those are not ink.

use std::borrow::Cow;

use super::{Canvas, Item, ItemId, PageId, Point, Rect, Shape};

/// The space, in canvas units, between an item and one put beside it.
pub const GAP: f64 = 24.0;

impl Canvas {
    /// The topmost item on `page` (the last drawn) within `tolerance` canvas
    /// units of `at`, if any.
    #[must_use]
    pub fn item_at(&self, page: PageId, at: Point, tolerance: f64) -> Option<ItemId> {
        let tolerance = tolerance.max(0.0);
        let near = grown(Rect::new(at.x, at.y, 0.0, 0.0), tolerance);
        self.items(page)
            .filter(|i| overlaps(i.bounds(), near))
            .filter(|i| Geometry::of(i.shape()).distance_to(at, at) <= tolerance)
            .last()
            .map(Item::id)
    }

    /// The items on `page` that lie wholly inside `lasso`, a closed polygon
    /// (the last point joins the first). A point is inside when the lasso
    /// winds around it, so a lasso that crosses itself keeps every loop it
    /// drew, and a loop drawn twice still counts. An item is inside when every
    /// point that defines it is: a stroke's points, an arrow's ends, a frame's
    /// corners.
    #[must_use]
    pub fn items_in_lasso(&self, page: PageId, lasso: &[Point]) -> Vec<ItemId> {
        let Some(outer) = Rect::around(lasso.iter().copied()).filter(|_| lasso.len() >= 3) else {
            return Vec::new();
        };
        self.items(page)
            .filter(|i| {
                let shape = Geometry::of(i.shape());
                shape.vertices().all(|p| contains(outer, p) && winding(lasso, p) != 0)
            })
            .map(Item::id)
            .collect()
    }

    /// The items on `page` that an eraser of `radius` touches as it moves
    /// along `path`: strokes, text, boxes (by their outline) and arrows. A
    /// path of one point is a dab.
    #[must_use]
    pub fn items_erased(&self, page: PageId, path: &[Point], radius: f64) -> Vec<ItemId> {
        let radius = radius.max(0.0);
        let Some(swept) = Rect::around(path.iter().copied()).map(|r| grown(r, radius)) else {
            return Vec::new();
        };
        self.items(page)
            .filter(|i| overlaps(i.bounds(), swept))
            .filter(|i| {
                Geometry::erased(i.shape()).is_some_and(|shape| {
                    segments(path).any(|(a, b)| shape.distance_to(a, b) <= radius)
                })
            })
            .map(Item::id)
            .collect()
    }

    /// The items on `page` that touch `area`, its edges included.
    #[must_use]
    pub fn items_in_box(&self, page: PageId, area: Rect) -> Vec<ItemId> {
        self.items(page)
            .filter(|i| overlaps(i.bounds(), area))
            .filter(|i| Geometry::of(i.shape()).touches(area))
            .map(Item::id)
            .collect()
    }

    /// Where a new `w` by `h` item goes beside `near` on `page` so that it
    /// covers nothing: [`GAP`] from `near`'s right side, top-aligned; else
    /// below it, left of it or above it; else further right, past whatever
    /// is in the way. There is always such a place: the canvas is endless.
    /// An item counts as in the way when it comes within half a [`GAP`]. A
    /// size below zero or not a finite number counts as zero.
    #[must_use]
    pub fn free_beside(&self, page: PageId, near: Rect, w: f64, h: f64) -> Rect {
        let size = |v: f64| if v.is_finite() { v.max(0.0) } else { 0.0 };
        let (w, h) = (size(w), size(h));
        let right = Rect::new(near.x + near.w + GAP, near.y, w, h);
        let sides = [
            right,
            Rect::new(near.x, near.y + near.h + GAP, w, h),
            Rect::new(near.x - GAP - w, near.y, w, h),
            Rect::new(near.x, near.y - GAP - h, w, h),
        ];
        let blockers = |r: Rect| self.items_in_box(page, grown(r, GAP / 2.0));
        if let Some(free) = sides.into_iter().find(|&r| blockers(r).is_empty()) {
            return free;
        }
        // Each step moves past at least one item, so this ends.
        let mut at = right;
        loop {
            let Some(past) = self.bounds_of(blockers(at)) else { return at };
            at.x = at.x.max(past.x + past.w + GAP);
        }
    }

    /// The smallest rectangle that holds every one of `items` on the canvas,
    /// or `None` when none of them is. Ids not on the canvas are skipped.
    #[must_use]
    pub fn bounds_of(&self, items: impl IntoIterator<Item = ItemId>) -> Option<Rect> {
        items.into_iter().filter_map(|id| self.item(id)).map(Item::bounds).reduce(|a, b| {
            Rect::around([corner(a, false), corner(a, true), corner(b, false), corner(b, true)])
                .unwrap_or(a)
        })
    }
}

/// A shape as the queries see it.
enum Geometry<'a> {
    /// A polyline thickened by `radius` on each side.
    Line { points: Cow<'a, [Point]>, radius: f64 },
    /// A solid rectangle.
    Area(Rect),
}

impl<'a> Geometry<'a> {
    fn of(shape: &'a Shape) -> Self {
        match shape {
            Shape::Stroke { points, width, .. } => {
                Geometry::Line { points: Cow::Borrowed(points), radius: width / 2.0 }
            }
            Shape::Arrow { from, to, .. } => {
                Geometry::Line { points: Cow::Owned(vec![from.at, to.at]), radius: 0.0 }
            }
            Shape::Text { frame, .. }
            | Shape::Box { frame, .. }
            | Shape::Live { frame, .. }
            | Shape::Button { frame } => Geometry::Area(*frame),
        }
    }

    /// The shape as the eraser sees it: a box is its outline; a live item
    /// and the button are not erased (`None`).
    fn erased(shape: &'a Shape) -> Option<Self> {
        match shape {
            Shape::Box { frame, .. } => {
                let [tl, tr, br, bl] = corners(*frame);
                Some(Geometry::Line { points: Cow::Owned(vec![tl, tr, br, bl, tl]), radius: 0.0 })
            }
            Shape::Live { .. } | Shape::Button { .. } => None,
            Shape::Stroke { .. } | Shape::Text { .. } | Shape::Arrow { .. } => {
                Some(Self::of(shape))
            }
        }
    }

    /// How far the shape is from the segment `a`-`b` (a point when `a == b`);
    /// zero when they touch.
    fn distance_to(&self, a: Point, b: Point) -> f64 {
        match self {
            Geometry::Line { points, radius } => {
                let d = segments(points).map(|(p, q)| segment_distance(p, q, a, b));
                (d.fold(f64::INFINITY, f64::min) - radius).max(0.0)
            }
            Geometry::Area(r) => segment_rect_distance(a, b, *r),
        }
    }

    fn touches(&self, area: Rect) -> bool {
        match self {
            Geometry::Line { points, radius } => {
                segments(points).any(|(p, q)| segment_rect_distance(p, q, area) <= *radius)
            }
            Geometry::Area(r) => overlaps(*r, area),
        }
    }

    /// The points that define the shape: a line's points, an area's corners.
    fn vertices(&self) -> Box<dyn Iterator<Item = Point> + '_> {
        match self {
            Geometry::Line { points, .. } => Box::new(points.iter().copied()),
            Geometry::Area(r) => Box::new(corners(*r).into_iter()),
        }
    }
}

/// The segments of a polyline. One point is one segment of zero length.
fn segments(points: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    let dot = points.first().filter(|_| points.len() == 1).map(|&p| (p, p));
    dot.into_iter().chain(points.windows(2).map(|w| (w[0], w[1])))
}

fn sub(a: Point, b: Point) -> Point {
    Point::new(a.x - b.x, a.y - b.y)
}

fn dot(a: Point, b: Point) -> f64 {
    a.x * b.x + a.y * b.y
}

/// Positive when `p` is left of the line from `a` to `b`, negative when
/// right, zero when on it.
fn side(a: Point, b: Point, p: Point) -> f64 {
    let (ab, ap) = (sub(b, a), sub(p, a));
    ab.x * ap.y - ab.y * ap.x
}

fn point_segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let ab = sub(b, a);
    let len2 = dot(ab, ab);
    let t = if len2 > 0.0 { (dot(sub(p, a), ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let off = sub(p, Point::new(a.x + t * ab.x, a.y + t * ab.y));
    dot(off, off).sqrt()
}

/// The distance between two segments; zero when they cross or touch.
fn segment_distance(a: Point, b: Point, c: Point, d: Point) -> f64 {
    let opposite = |s: f64, t: f64| (s > 0.0 && t < 0.0) || (s < 0.0 && t > 0.0);
    if opposite(side(c, d, a), side(c, d, b)) && opposite(side(a, b, c), side(a, b, d)) {
        return 0.0;
    }
    // Otherwise the nearest pair includes an end of one segment. Touching and
    // overlapping segments land here with a distance of zero.
    [
        point_segment_distance(a, c, d),
        point_segment_distance(b, c, d),
        point_segment_distance(c, a, b),
        point_segment_distance(d, a, b),
    ]
    .into_iter()
    .fold(f64::INFINITY, f64::min)
}

/// The distance from a segment to a solid rectangle; zero when it touches.
fn segment_rect_distance(a: Point, b: Point, r: Rect) -> f64 {
    if contains(r, a) || contains(r, b) {
        return 0.0;
    }
    let [tl, tr, br, bl] = corners(r);
    [(tl, tr), (tr, br), (br, bl), (bl, tl)]
        .into_iter()
        .map(|(c, d)| segment_distance(a, b, c, d))
        .fold(f64::INFINITY, f64::min)
}

/// How many times the closed polygon winds around `p` (nonzero rule).
fn winding(polygon: &[Point], p: Point) -> i32 {
    let next = polygon.iter().cycle().skip(1);
    polygon.iter().zip(next).fold(0, |w, (&a, &b)| {
        if a.y <= p.y && b.y > p.y && side(a, b, p) > 0.0 {
            w + 1
        } else if a.y > p.y && b.y <= p.y && side(a, b, p) < 0.0 {
            w - 1
        } else {
            w
        }
    })
}

fn corners(r: Rect) -> [Point; 4] {
    let (x2, y2) = (r.x + r.w, r.y + r.h);
    [Point::new(r.x, r.y), Point::new(x2, r.y), Point::new(x2, y2), Point::new(r.x, y2)]
}

/// The top-left corner, or the bottom-right one when `far`.
fn corner(r: Rect, far: bool) -> Point {
    if far { Point::new(r.x + r.w, r.y + r.h) } else { Point::new(r.x, r.y) }
}

fn grown(r: Rect, by: f64) -> Rect {
    Rect::new(r.x - by, r.y - by, r.w + 2.0 * by, r.h + 2.0 * by)
}

fn contains(r: Rect, p: Point) -> bool {
    p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h
}

/// Whether two rectangles share any point, edges included.
fn overlaps(a: Rect, b: Rect) -> bool {
    a.x <= b.x + b.w && b.x <= a.x + a.w && a.y <= b.y + b.h && b.y <= a.y + a.h
}
