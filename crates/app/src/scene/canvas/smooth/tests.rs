use super::*;

fn pt(x: f64, y: f64) -> Point {
    Point { x, y }
}

/// A wobble from -1 to 1, the same every run.
struct Shake(u64);

impl Shake {
    fn next(&mut self) -> f64 {
        self.0 =
            self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        #[expect(clippy::cast_precision_loss, reason = "a wobble needs no more than 53 bits")]
        let unit = (self.0 >> 11) as f64 / (1u64 << 53) as f64;
        unit * 2.0 - 1.0
    }
}

/// A slow, shaky pen along `path` (t from 0 to 1): `count` points, each off
/// by up to `by` points in x and in y.
fn shaky(count: u32, by: f64, path: impl Fn(f64) -> Point) -> Vec<Point> {
    let mut shake = Shake(7);
    (0..=count)
        .map(|i| {
            let p = path(f64::from(i) / f64::from(count));
            pt(p.x + by * shake.next(), p.y + by * shake.next())
        })
        .collect()
}

/// Points along the curves from `start`, with the line's bend at each: one
/// over the radius of the circle it follows there. Points where a curve
/// does not move have no bend.
fn sampled(start: Point, curves: &[[Point; 3]]) -> Vec<(Point, f64)> {
    let mut from = start;
    let mut out = Vec::new();
    for &[c1, c2, end] in curves {
        let [p0, p1, p2, p3] = [from, c1, c2, end].map(|p| (p.x, p.y));
        for i in 0..=32 {
            let t = f64::from(i) / 32.0;
            let u = 1.0 - t;
            let at = |a: f64, b: f64, c: f64, d: f64| {
                u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
            };
            let d1 = |a: f64, b: f64, c: f64, d: f64| {
                3.0 * u * u * (b - a) + 6.0 * u * t * (c - b) + 3.0 * t * t * (d - c)
            };
            let d2 = |a: f64, b: f64, c: f64, d: f64| {
                6.0 * u * (c - 2.0 * b + a) + 6.0 * t * (d - 2.0 * c + b)
            };
            let (dx, dy) = (d1(p0.0, p1.0, p2.0, p3.0), d1(p0.1, p1.1, p2.1, p3.1));
            let (ddx, ddy) = (d2(p0.0, p1.0, p2.0, p3.0), d2(p0.1, p1.1, p2.1, p3.1));
            let speed = dx.hypot(dy);
            let bend = if speed < 1e-6 { 0.0 } else { (dx * ddy - dy * ddx).abs() / speed.powi(3) };
            out.push((pt(at(p0.0, p1.0, p2.0, p3.0), at(p0.1, p1.1, p2.1, p3.1)), bend));
        }
        from = end;
    }
    out
}

/// How far the line turns in all, left and right counted alike, in radians.
fn turning(samples: &[(Point, f64)]) -> f64 {
    let heading = |a: Point, b: Point| (b.y - a.y).atan2(b.x - a.x);
    let points: Vec<Point> = samples.iter().map(|s| s.0).collect();
    let mut steps = points.windows(2).filter(|w| distance(w[0], w[1]) > 1e-6);
    let Some(first) = steps.next() else { return 0.0 };
    let mut last = heading(first[0], first[1]);
    let mut total = 0.0;
    for w in steps {
        let now = heading(w[0], w[1]);
        let mut turn = now - last;
        while turn > std::f64::consts::PI {
            turn -= std::f64::consts::TAU;
        }
        while turn < -std::f64::consts::PI {
            turn += std::f64::consts::TAU;
        }
        total += turn.abs();
        last = now;
    }
    total
}

fn sharpest(samples: &[(Point, f64)]) -> f64 {
    samples.iter().map(|s| s.1).fold(0.0, f64::max)
}

#[test]
fn a_slow_shaky_line_drawn_wide_has_no_bumps() {
    // A marker pulled slowly along a ruler: a point every half point, each
    // off by up to half a point.
    let points = shaky(400, 0.5, |t| pt(200.0 * t, 0.0));
    let width = 16.0;
    let line = sampled(points[0], &smooth(&points, width));
    // An edge, half the width out, never folds or kinks: the middle never
    // bends tighter than a quarter of what would fold it.
    assert!(sharpest(&line) * width / 2.0 < 0.25, "bend {}", sharpest(&line));
    // A straight line turns hardly at all, and stays on the ruler.
    assert!(turning(&line) < 0.5, "turned {}", turning(&line));
    assert!(line.iter().all(|(p, _)| p.y.abs() < 0.6));
}

#[test]
fn a_shaky_circle_keeps_its_round_without_bumps() {
    let (r, width) = (40.0, 8.0);
    let around =
        |t: f64| pt(r * (t * std::f64::consts::TAU).cos(), r * (t * std::f64::consts::TAU).sin());
    let points = shaky(500, 0.5, around);
    let line = sampled(points[0], &smooth(&points, width));
    // Its bend stays near the circle's: no bump bends it much tighter.
    assert!(sharpest(&line) < 2.0 / r, "bend {}", sharpest(&line));
    // Once round, and no more: wobbles would add turns both ways.
    let turned = turning(&line);
    assert!((turned - std::f64::consts::TAU).abs() < 0.6, "turned {turned}");
    assert!(line.iter().all(|(p, _)| (p.x.hypot(p.y) - r).abs() < 1.5));
}

#[test]
fn a_fine_line_keeps_its_corners_sharp_enough_to_read() {
    // The point of a small "v", 10 points high, in the finest pen.
    let points: Vec<Point> = (0..=20)
        .map(|i| {
            let x = f64::from(i) * 0.5;
            pt(x, 10.0 - 2.0 * (x - 5.0).abs())
        })
        .collect();
    let line = sampled(points[0], &smooth(&points, 2.0));
    let tip = line.iter().map(|(p, _)| distance(*p, pt(5.0, 10.0))).fold(f64::INFINITY, f64::min);
    assert!(tip < 1.5, "the tip is {tip} away");
}

#[test]
fn the_ends_stay_where_the_pen_put_them() {
    for width in [2.0, 4.0, 8.0, 16.0] {
        for count in [2, 3, 5, 40] {
            let points = shaky(count, 1.0, |t| pt(50.0 * t, 20.0 * t * t));
            let curves = smooth(&points, width);
            let last = curves.last().map(|c| c[2]);
            assert_eq!(last, points.last().copied(), "width {width}, {count} points");
            assert_eq!(sampled(points[0], &curves)[0].0, points[0]);
        }
    }
}

#[test]
fn the_line_stays_inside_its_points_box() {
    let points = shaky(60, 3.0, |t| pt(30.0 * (t * 9.0).sin(), 80.0 * t));
    let (lo, hi) = points.iter().fold(
        (pt(f64::INFINITY, f64::INFINITY), pt(f64::NEG_INFINITY, f64::NEG_INFINITY)),
        |(lo, hi), p| (pt(lo.x.min(p.x), lo.y.min(p.y)), pt(hi.x.max(p.x), hi.y.max(p.y))),
    );
    let inside = |p: &Point| p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y;
    assert!(smooth(&points, 16.0).iter().flatten().all(inside));
}

#[test]
fn a_dot_is_one_curve_of_no_length_and_no_points_is_none() {
    assert_eq!(smooth(&[pt(4.0, 5.0)], 2.0), [[pt(4.0, 5.0); 3]]);
    // Points closer than a step are one dot, still ending where the pen did.
    let close = smooth(&[pt(4.0, 5.0), pt(4.1, 5.0), pt(4.2, 5.0)], 16.0);
    assert_eq!(close.last().map(|c| c[2]), Some(pt(4.2, 5.0)));
    assert_eq!(smooth(&[], 2.0), Vec::<[Point; 3]>::new());
}

#[test]
fn a_point_more_changes_only_the_last_few_curves() {
    let points = shaky(200, 0.5, |t| pt(100.0 * t, 30.0 * t));
    for n in 20..points.len() {
        let (was, now) = (smooth(&points[..n], 8.0), smooth(&points[..=n], 8.0));
        let shared = was.iter().zip(&now).take_while(|(a, b)| a == b).count();
        assert!(was.len() - shared <= EASES + 4, "{} of {} changed", was.len() - shared, was.len());
    }
}
