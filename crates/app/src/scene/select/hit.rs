//! Which strokes an eraser's path touches, and which items a lasso holds.
//! These are the core's rules (`canvas/geometry.rs`: `strokes_erased` and
//! `items_in_lasso`), kept here as well because the eraser and the lasso
//! answer under the pen at every pen event, from the page the app already
//! holds. Asking the daemon each time would make the main thread wait on
//! the socket, or the eraser trail behind the pen (P7). Keep the two alike:
//!
//! - An eraser of `radius` takes a stroke when the stroke's line, thickened
//!   by half its width, comes within `radius` of the path. Only strokes are
//!   erased; a stroke of one point is a dot.
//! - A lasso, a closed polygon, holds an item when it winds around every
//!   point that defines the item (the nonzero rule): a stroke's points, an
//!   arrow's ends, a frame's corners. A lasso of fewer than three points
//!   holds nothing.

use crate::layout::{Point, Rect};
use crate::scene::canvas::Shape;

/// Whether an eraser of `radius` moving from `a` to `b` touches `shape`.
pub fn erases(shape: &Shape, a: Point, b: Point, radius: f64) -> bool {
    let Shape::Stroke { points, width } = shape else { return false };
    let reach = radius.max(0.0) + width / 2.0;
    segments(points).any(|(p, q)| segment_distance(p, q, a, b) <= reach)
}

/// Whether `lasso` holds the whole of `shape`.
pub fn holds(lasso: &[Point], shape: &Shape) -> bool {
    if lasso.len() < 3 {
        return false;
    }
    let corners = |r: &Rect| {
        let (x2, y2) = (r.x + r.width, r.y + r.height);
        vec![
            Point { x: r.x, y: r.y },
            Point { x: x2, y: r.y },
            Point { x: x2, y: y2 },
            Point { x: r.x, y: y2 },
        ]
    };
    let vertices = match shape {
        Shape::Stroke { points, .. } => points.clone(),
        Shape::Arrow { from, to, .. } => vec![*from, *to],
        Shape::Text { frame, .. }
        | Shape::Box { frame, .. }
        | Shape::Live { frame, .. }
        | Shape::Button { frame, .. }
        | Shape::Panel { frame, .. }
        | Shape::Caret { frame, .. } => corners(frame),
    };
    !vertices.is_empty() && vertices.into_iter().all(|p| winding(lasso, p) != 0)
}

/// The segments of a polyline. One point is one segment of no length.
fn segments(points: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    let dot = points.first().filter(|_| points.len() == 1).map(|&p| (p, p));
    dot.into_iter().chain(points.windows(2).map(|w| (w[0], w[1])))
}

fn sub(a: Point, b: Point) -> Point {
    Point { x: a.x - b.x, y: a.y - b.y }
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
    let off = sub(p, Point { x: a.x + t * ab.x, y: a.y + t * ab.y });
    dot(off, off).sqrt()
}

/// The distance between two segments; zero when they cross or touch.
fn segment_distance(a: Point, b: Point, c: Point, d: Point) -> f64 {
    let opposite = |s: f64, t: f64| (s > 0.0 && t < 0.0) || (s < 0.0 && t > 0.0);
    if opposite(side(c, d, a), side(c, d, b)) && opposite(side(a, b, c), side(a, b, d)) {
        return 0.0;
    }
    // Otherwise the nearest pair includes an end of one segment.
    [
        point_segment_distance(a, c, d),
        point_segment_distance(b, c, d),
        point_segment_distance(c, a, b),
        point_segment_distance(d, a, b),
    ]
    .into_iter()
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
