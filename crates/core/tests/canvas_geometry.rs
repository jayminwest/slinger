//! The canvas's geometry queries, through its public interface only.
#![cfg(test)]

use std::time::{Duration, Instant};

use slinger_core::canvas::{
    Action, Applied, Author, Canvas, Colour, End, ItemId, Op, PageId, Point, Rect, Shape, Time,
};

/// A point as `(x, y)`, to keep the tables short.
type Xy = (f64, f64);

fn pts(points: &[Xy]) -> Vec<Point> {
    points.iter().map(|&(x, y)| Point::new(x, y)).collect()
}

fn stroke(points: &[(f64, f64)], width: f64) -> Shape {
    Shape::Stroke { points: pts(points), colour: Colour::BLACK, width }
}

fn frame(x: f64, y: f64, w: f64, h: f64) -> Shape {
    Shape::Box { frame: Rect::new(x, y, w, h), colour: Colour::BLACK }
}

fn text(x: f64, y: f64, w: f64, h: f64) -> Shape {
    Shape::Text { text: "hi".into(), frame: Rect::new(x, y, w, h), colour: Colour::BLACK }
}

fn arrow(from: (f64, f64), to: (f64, f64)) -> Shape {
    Shape::Arrow {
        from: End::free(Point::new(from.0, from.1)),
        to: End::free(Point::new(to.0, to.1)),
        colour: Colour::BLACK,
    }
}

/// A canvas whose first page holds `shapes`, in order, and their ids.
fn page_of(shapes: Vec<Shape>) -> (Canvas, PageId, Vec<ItemId>) {
    let mut c = Canvas::new();
    let page = c.pages().next().unwrap().id();
    let ids = shapes
        .into_iter()
        .map(|shape| {
            let op = Op { by: Author::User, at: Time(0), action: Action::Add { page, shape } };
            match c.apply(op).unwrap() {
                Applied::Added(id) => id,
                other => panic!("expected Added, got {other:?}"),
            }
        })
        .collect();
    (c, page, ids)
}

/// Which of `ids` (by index) are in `found`.
fn which(ids: &[ItemId], found: &[ItemId]) -> Vec<usize> {
    found.iter().map(|f| ids.iter().position(|i| i == f).unwrap()).collect()
}

/// The shapes most tests share:
/// 0: a horizontal stroke from (0,0) to (100,0), width 4
/// 1: a stroke of one point at (200,200), width 10
/// 2: a box at (50,50) size 40x40
/// 3: text at (300,0) size 60x20
/// 4: an arrow from (0,100) to (100,200)
/// 5: a stroke over the box, from (40,70) to (100,70), width 2
fn sample() -> (Canvas, PageId, Vec<ItemId>) {
    page_of(vec![
        stroke(&[(0.0, 0.0), (100.0, 0.0)], 4.0),
        stroke(&[(200.0, 200.0)], 10.0),
        frame(50.0, 50.0, 40.0, 40.0),
        text(300.0, 0.0, 60.0, 20.0),
        arrow((0.0, 100.0), (100.0, 200.0)),
        stroke(&[(40.0, 70.0), (100.0, 70.0)], 2.0),
    ])
}

#[test]
fn item_at_finds_the_topmost_item_within_the_tolerance() {
    let (c, page, ids) = sample();
    let cases: &[(&str, Xy, f64, Option<usize>)] = &[
        ("on a stroke's line", (50.0, 0.0), 0.0, Some(0)),
        ("inside a stroke's width", (50.0, 2.0), 0.0, Some(0)),
        ("just past a stroke's width", (50.0, 2.5), 0.0, None),
        ("past the width, inside the tolerance", (50.0, 2.5), 1.0, Some(0)),
        ("past a stroke's end", (103.0, 0.0), 0.0, None),
        ("on a one-point stroke", (204.0, 200.0), 0.0, Some(1)),
        ("beside a one-point stroke", (206.0, 200.0), 0.0, None),
        ("inside a box", (60.0, 60.0), 0.0, Some(2)),
        ("on a box's edge", (50.0, 60.0), 0.0, Some(2)),
        ("near a box", (47.0, 60.0), 3.0, Some(2)),
        ("where a stroke crosses a box, the later one wins", (70.0, 70.0), 0.0, Some(5)),
        ("inside text", (310.0, 10.0), 0.0, Some(3)),
        ("on an arrow", (50.0, 150.0), 0.0, Some(4)),
        ("beside an arrow", (60.0, 140.0), 1.0, None),
        ("in empty space", (500.0, 500.0), 10.0, None),
        ("a negative tolerance counts as zero", (50.0, 0.0), -5.0, Some(0)),
        ("a tolerance that is not a number counts as zero", (50.0, 2.5), f64::NAN, None),
        ("a point that is not a number hits nothing", (f64::NAN, 0.0), 10.0, None),
    ];
    for &(name, (x, y), tol, want) in cases {
        let got = c.item_at(page, Point::new(x, y), tol);
        assert_eq!(got, want.map(|i| ids[i]), "{name}");
    }
}

#[test]
fn queries_on_an_empty_or_missing_page_find_nothing() {
    let (c, page, _) = page_of(vec![]);
    let missing = PageId::new(99);
    let lasso = pts(&[(-1e6, -1e6), (1e6, -1e6), (0.0, 1e6)]);
    for p in [page, missing] {
        assert_eq!(c.item_at(p, Point::new(0.0, 0.0), 1e9), None);
        assert_eq!(c.items_in_lasso(p, &lasso), []);
        assert_eq!(c.items_erased(p, &lasso, 1e9), []);
        assert_eq!(c.items_in_box(p, Rect::new(-1e6, -1e6, 2e6, 2e6)), []);
    }
    let (full, _, _) = sample();
    assert_eq!(full.item_at(missing, Point::new(50.0, 0.0), 10.0), None);
    assert_eq!(c.bounds_of([]), None);
}

#[test]
fn items_in_lasso_finds_items_wholly_inside() {
    let (c, page, ids) = sample();
    // A figure eight: one loop around the box and its stroke, one around the
    // one-point stroke, crossing near (150,135).
    let eight = [(30.0, 40.0), (110.0, 40.0), (190.0, 230.0), (230.0, 230.0), (230.0, 170.0)];
    let eight = [&eight[..], &[(190.0, 170.0), (110.0, 100.0), (30.0, 100.0)]].concat();
    // A loop drawn twice around the box.
    let twice = [(40.0, 40.0), (110.0, 40.0), (110.0, 100.0), (40.0, 100.0)].repeat(2);
    let cases: &[(&str, Vec<Xy>, Vec<usize>)] = &[
        (
            "around everything",
            vec![(-10.0, -10.0), (400.0, -10.0), (400.0, 300.0), (-10.0, 300.0)],
            vec![0, 1, 2, 3, 4, 5],
        ),
        (
            "around the box and its stroke",
            vec![(30.0, 40.0), (110.0, 40.0), (110.0, 100.0), (30.0, 100.0)],
            vec![2, 5],
        ),
        (
            "around the box, cutting its stroke",
            vec![(45.0, 45.0), (95.0, 45.0), (95.0, 95.0), (45.0, 95.0)],
            vec![2],
        ),
        (
            "around half a stroke",
            vec![(-10.0, -10.0), (50.0, -10.0), (50.0, 10.0), (-10.0, 10.0)],
            vec![],
        ),
        (
            "a triangle around the arrow",
            vec![(-10.0, 80.0), (120.0, 210.0), (-10.0, 210.0)],
            vec![4],
        ),
        (
            "around a one-point stroke",
            vec![(190.0, 190.0), (210.0, 190.0), (200.0, 210.0)],
            vec![1],
        ),
        ("a lasso that crosses itself", eight, vec![1, 2, 5]),
        ("a loop drawn twice", twice, vec![2, 5]),
        ("two points", vec![(-1e6, -1e6), (1e6, 1e6)], vec![]),
        ("no points", vec![], vec![]),
    ];
    for (name, lasso, want) in cases {
        let found = c.items_in_lasso(page, &pts(lasso));
        assert_eq!(&which(&ids, &found), want, "{name}");
    }
}

#[test]
fn items_erased_finds_what_the_eraser_touches() {
    let (c, page, ids) = sample();
    let cases: &[(&str, Vec<Xy>, f64, Vec<usize>)] = &[
        ("a swipe across a stroke", vec![(50.0, -20.0), (50.0, 20.0)], 0.0, vec![0]),
        ("a swipe that stops short", vec![(50.0, -20.0), (50.0, -5.0)], 2.0, vec![]),
        ("a swipe that reaches with its radius", vec![(50.0, -20.0), (50.0, -5.0)], 3.0, vec![0]),
        ("a dab on a stroke", vec![(10.0, 1.0)], 0.0, vec![0]),
        ("a dab on a one-point stroke", vec![(207.0, 200.0)], 2.0, vec![1]),
        ("a dab inside a box keeps it", vec![(60.0, 60.0)], 1.0, vec![]),
        ("a dab on a box's edge", vec![(50.0, 60.0)], 1.0, vec![2]),
        ("a dab on text", vec![(310.0, 10.0)], 1.0, vec![3]),
        ("a dab on an arrow", vec![(50.0, 150.0)], 1.0, vec![4]),
        ("a dab beside an arrow", vec![(60.0, 150.0)], 1.0, vec![]),
        (
            "a swipe across a box erases the box and the stroke on it",
            vec![(70.0, 45.0), (70.0, 95.0)],
            0.0,
            vec![2, 5],
        ),
        (
            "a swipe inside a box erases only the stroke",
            vec![(70.0, 60.0), (70.0, 80.0)],
            0.0,
            vec![5],
        ),
        (
            "a path along both strokes, across the box's sides",
            vec![(-5.0, 0.0), (120.0, 0.0), (120.0, 70.0), (30.0, 70.0)],
            0.0,
            vec![0, 2, 5],
        ),
        ("a negative radius counts as zero", vec![(10.0, 1.0)], -3.0, vec![0]),
        ("no path", vec![], 100.0, vec![]),
    ];
    for (name, path, radius, want) in cases {
        let found = c.items_erased(page, &pts(path), *radius);
        assert_eq!(&which(&ids, &found), want, "{name}");
    }
}

#[test]
fn the_eraser_keeps_live_items_and_the_button() {
    let live = Shape::Live {
        frame: Rect::new(0.0, 0.0, 100.0, 100.0),
        ui: "pick".into(),
        hash: format!("sha256:{}", "0".repeat(64)),
        open: "pick#1".into(),
    };
    let button = Shape::Button { frame: Rect::new(200.0, 0.0, 40.0, 20.0) };
    let (c, page, _) = page_of(vec![live, button]);
    let path = pts(&[(-10.0, 10.0), (300.0, 10.0)]);
    assert_eq!(c.items_erased(page, &path, 5.0), []);
}

#[test]
fn items_in_box_finds_items_that_touch_it() {
    let (c, page, ids) = sample();
    let cases: &[(&str, Rect, Vec<usize>)] = &[
        ("around everything", Rect::new(-10.0, -10.0, 500.0, 500.0), vec![0, 1, 2, 3, 4, 5]),
        ("across a stroke's middle", Rect::new(40.0, -5.0, 10.0, 10.0), vec![0]),
        ("touching a stroke's width only", Rect::new(40.0, 1.5, 10.0, 10.0), vec![0]),
        ("inside a box", Rect::new(55.0, 55.0, 5.0, 5.0), vec![2]),
        ("touching a box's edge", Rect::new(20.0, 50.0, 30.0, 5.0), vec![2]),
        ("a zero-size box on a stroke", Rect::new(50.0, 0.0, 0.0, 0.0), vec![0]),
        ("inside an arrow's bounds but off its line", Rect::new(60.0, 110.0, 30.0, 30.0), vec![]),
        ("across an arrow's line", Rect::new(40.0, 130.0, 30.0, 30.0), vec![4]),
        ("near a one-point stroke, past its width", Rect::new(206.0, 190.0, 10.0, 20.0), vec![]),
        ("in empty space", Rect::new(500.0, 500.0, 10.0, 10.0), vec![]),
    ];
    for (name, area, want) in cases {
        let found = c.items_in_box(page, *area);
        assert_eq!(&which(&ids, &found), want, "{name}");
    }
}

#[test]
fn bounds_of_holds_every_item_and_skips_missing_ones() {
    let (c, _, ids) = sample();
    let cases: &[(&str, Vec<ItemId>, Option<Rect>)] = &[
        ("one stroke, with its width", vec![ids[0]], Some(Rect::new(-2.0, -2.0, 104.0, 4.0))),
        ("a one-point stroke", vec![ids[1]], Some(Rect::new(195.0, 195.0, 10.0, 10.0))),
        ("a box and text", vec![ids[2], ids[3]], Some(Rect::new(50.0, 0.0, 310.0, 90.0))),
        (
            "a missing id is skipped",
            vec![ids[2], ItemId::new(99)],
            Some(Rect::new(50.0, 50.0, 40.0, 40.0)),
        ),
        ("only missing ids", vec![ItemId::new(99)], None),
        ("everything", ids.clone(), Some(Rect::new(-2.0, -2.0, 362.0, 207.0))),
    ];
    for (name, set, want) in cases {
        assert_eq!(c.bounds_of(set.iter().copied()), *want, "{name}");
    }
}

/// 7500 strokes of 32 points each, as many as the real board's busiest page.
/// The bound is loose so the test holds in debug and coverage builds; it
/// catches a query that has gone quadratic, not a slow machine.
#[test]
fn queries_on_a_page_of_7500_strokes_keep_up_with_the_pen() {
    let shapes = (0..7500)
        .map(|n| {
            let (ox, oy) = (f64::from(n % 100) * 50.0, f64::from(n / 100) * 50.0);
            let points: Vec<(f64, f64)> =
                (0..32).map(|k| (ox + f64::from(k), oy + f64::from(k % 4))).collect();
            stroke(&points, 2.0)
        })
        .collect();
    let (c, page, ids) = page_of(shapes);
    let lasso = pts(&[(-10.0, -10.0), (2000.0, -10.0), (2000.0, 1000.0), (-10.0, 1000.0)]);
    let path = pts(&[(0.0, 2.0), (5000.0, 2.0)]);

    let start = Instant::now();
    let hit = c.item_at(page, Point::new(2510.0, 1802.0), 2.0);
    let lassoed = c.items_in_lasso(page, &lasso);
    let erased = c.items_erased(page, &path, 1.0);
    let boxed = c.items_in_box(page, Rect::new(0.0, 0.0, 1000.0, 1000.0));
    let all = c.bounds_of(ids.iter().copied());
    let took = start.elapsed();

    assert_eq!(hit, Some(ids[3650]));
    assert_eq!(lassoed.len(), 40 * 20);
    assert_eq!(erased.len(), 100);
    assert_eq!(boxed.len(), 21 * 21);
    assert!(all.is_some());
    assert!(took < Duration::from_secs(2), "five queries took {took:?}");
}
