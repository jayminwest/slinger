//! How a pen line is drawn smooth. The pen's points carry its jitter: a
//! wobble of a point or so, and at a slow pen many points that close. The
//! wider the line, the more each wobble of its middle shows on its edges, as
//! a bump. So the points are thinned to a step that grows with the width,
//! eased toward their neighbours, then drawn as a cubic B-spline: a curve
//! with no corners and no jump in its bend. The first and the last points
//! stay where the pen put them.
//!
//! Each step looks only a few points back, so a point added at the end of a
//! line changes only its last few curves (`canvas::grown`).

use crate::layout::Point;

/// The least distance, in canvas points, between the points kept of a line,
/// as a share of its width, and the bounds of that distance.
const STEP_PER_WIDTH: f64 = 0.4;
const MIN_STEP: f64 = 1.0;
const MAX_STEP: f64 = 6.0;
/// How many times each inner point moves halfway to the middle of its two
/// neighbours.
const EASES: usize = 2;

/// The line through `points`, `width` wide, as cubic curves from its first
/// point: each curve is two control points, then its end. One point is one
/// curve of no length (a dot); no points is no curves. The line stays inside
/// the points' box.
pub(super) fn smooth(points: &[Point], width: f64) -> Vec<[Point; 3]> {
    let step = (width * STEP_PER_WIDTH).clamp(MIN_STEP, MAX_STEP);
    let mut kept = thinned(points, step);
    for _ in 0..EASES {
        eased(&mut kept);
    }
    spline(&kept)
}

/// The points at least `step` apart, from the first, and the last point. A
/// last point nearer than half a step to the one kept before it takes that
/// one's place, so the line does not end in a hook.
fn thinned(points: &[Point], step: f64) -> Vec<Point> {
    let (Some(&first), Some(&last)) = (points.first(), points.last()) else { return Vec::new() };
    let mut kept = vec![first];
    for &p in points {
        if distance(kept[kept.len() - 1], p) >= step {
            kept.push(p);
        }
    }
    let end = kept[kept.len() - 1];
    if end != last {
        if kept.len() > 1 && distance(end, last) < step / 2.0 {
            kept.pop();
        }
        kept.push(last);
    }
    kept
}

/// Each inner point moved halfway to the middle of its two neighbours (a
/// 1-2-1 average); the ends stay.
fn eased(points: &mut [Point]) {
    let Some(mut before) = points.first().copied() else { return };
    let inner = points.len().saturating_sub(1);
    for i in 1..inner {
        let (here, after) = (points[i], points[i + 1]);
        points[i] = Point {
            x: (before.x + 2.0 * here.x + after.x) / 4.0,
            y: (before.y + 2.0 * here.y + after.y) / 4.0,
        };
        before = here;
    }
}

/// The uniform cubic B-spline with `points` as its controls, the first and
/// last tripled so the curve starts and ends on them, as Bézier curves.
fn spline(points: &[Point]) -> Vec<[Point; 3]> {
    let (Some(&first), Some(&last)) = (points.first(), points.last()) else { return Vec::new() };
    if points.len() == 1 {
        return vec![[first; 3]];
    }
    let controls: Vec<Point> =
        [first, first].into_iter().chain(points.iter().copied()).chain([last, last]).collect();
    let toward =
        |a: Point, b: Point, t: f64| Point { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t };
    // Each run of four controls is one curve; its start is the last one's end.
    controls
        .windows(4)
        .map(|w| {
            let (b, c, d) = (w[1], w[2], w[3]);
            // (b + 4c + d) / 6, put so the last curve ends on the last point
            // exactly.
            let end = Point {
                x: c.x + (b.x - c.x + d.x - c.x) / 6.0,
                y: c.y + (b.y - c.y + d.y - c.y) / 6.0,
            };
            [toward(b, c, 1.0 / 3.0), toward(b, c, 2.0 / 3.0), end]
        })
        .collect()
}

fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

#[cfg(test)]
mod tests;
