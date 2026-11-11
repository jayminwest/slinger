use super::*;

const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };

fn pt(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn gone() -> Problem {
    Problem { message: "no daemon".into(), hint: None }
}

/// A finished stroke on page p1 of `main` through `points`.
fn draw(ink: &mut Ink, points: &[(f64, f64)]) {
    let (&(x, y), rest) = points.split_first().unwrap();
    ink.down("main", "p1", RED, pt(x, y));
    for &(x, y) in rest {
        assert!(ink.drag(pt(x, y)));
    }
    assert!(ink.up(None));
}

fn first_x(ink: &Ink) -> Vec<f64> {
    let items = ink.items("main", "p1");
    items
        .iter()
        .map(|i| match &i.shape {
            Shape::Stroke { points, .. } => points[0].x,
            _ => panic!("not a stroke"),
        })
        .collect()
}

#[test]
fn the_stroke_under_the_pen_shows_at_once_point_by_point() {
    let mut ink = Ink::default();
    assert!(!ink.drag(pt(1.0, 1.0)), "no pen down");
    assert!(!ink.up(Some(pt(1.0, 1.0))));
    ink.down("main", "p1", RED, pt(1.0, 2.0));
    assert!(ink.drag(pt(3.0, 4.0)));
    assert!(ink.drag(pt(3.0, 4.0)), "the same point again");
    let items = ink.items("main", "p1");
    let [item] = items.as_slice() else { panic!("{items:?}") };
    assert_eq!(item.colour, RED);
    assert!(item.id.0 >= LOCAL, "ink never takes a server id");
    let Shape::Stroke { points, width } = &item.shape else { panic!() };
    assert_eq!((points.as_slice(), *width), ([pt(1.0, 2.0), pt(3.0, 4.0)].as_slice(), PEN_WIDTH));
    assert!(ink.items("main", "p2").is_empty() && ink.items("other", "p1").is_empty());
    assert_eq!(ink.next(), None, "nothing goes while the pen is down");
}

#[test]
fn a_lifted_stroke_keeps_its_id_and_waits_to_be_sent() {
    let mut ink = Ink::default();
    ink.down("main", "p1", RED, pt(0.0, 0.0));
    let under = ink.items("main", "p1")[0].id;
    assert!(ink.up(Some(pt(5.0, 5.0))));
    assert!(!ink.drag(pt(6.0, 6.0)), "the pen is up");
    let items = ink.items("main", "p1");
    assert_eq!(items[0].id, under, "lifting the pen repaints nothing");
    let (key, stroke) = ink.next().unwrap();
    assert_eq!(stroke.points, [pt(0.0, 0.0), pt(5.0, 5.0)]);
    assert_eq!((stroke.canvas.as_str(), stroke.page.as_str()), ("main", "p1"));
    assert_eq!(ink.next(), None, "one at a time");
    ink.answered(key, Answer::Applied(7));
    assert_eq!(first_x(&ink), [0.0], "still drawn until the feed brings it");
}

#[test]
fn a_stroke_leaves_once_the_feed_passes_its_line_whichever_comes_first() {
    let mut ink = Ink::default();
    draw(&mut ink, &[(1.0, 0.0)]);
    draw(&mut ink, &[(2.0, 0.0)]);
    let (one, _) = ink.next().unwrap();
    ink.heard("main", 9);
    assert_eq!(first_x(&ink), [1.0, 2.0], "a stroke not yet written stays");
    ink.answered(one, Answer::Applied(4));
    ink.heard("other", 9);
    assert_eq!(first_x(&ink), [1.0, 2.0], "another canvas's feed says nothing");
    ink.heard("main", 3);
    assert_eq!(first_x(&ink), [1.0, 2.0]);
    ink.heard("main", 4);
    assert_eq!(first_x(&ink), [2.0]);
    let (two, _) = ink.next().unwrap();
    ink.answered(two, Answer::Applied(5));
    ink.heard("main", 9);
    assert_eq!(ink.items("main", "p1"), []);
    assert_eq!(ink.next(), None);
}

#[test]
fn a_stroke_not_delivered_is_sent_again_before_any_after_it() {
    let mut ink = Ink::default();
    for x in [1.0, 2.0, 3.0] {
        draw(&mut ink, &[(x, 0.0)]);
    }
    let mut order = Vec::new();
    let mut send = |ink: &mut Ink, answer: Answer| {
        let (key, stroke) = ink.next().unwrap();
        order.push(stroke.points[0].x);
        ink.answered(key, answer);
    };
    send(&mut ink, Answer::Applied(1));
    send(&mut ink, Answer::Unreachable(gone()));
    assert_eq!(ink.problem(), Some(&gone()));
    draw(&mut ink, &[(4.0, 0.0)]);
    send(&mut ink, Answer::Unreachable(gone()));
    send(&mut ink, Answer::Applied(2));
    assert_eq!(ink.problem(), None, "a stroke that goes clears the problem");
    send(&mut ink, Answer::Applied(3));
    send(&mut ink, Answer::Applied(4));
    assert_eq!(order, [1.0, 2.0, 2.0, 2.0, 3.0, 4.0]);
    assert_eq!(first_x(&ink), [1.0, 2.0, 3.0, 4.0], "every stroke stayed on screen");
    assert_eq!(ink.next(), None);
}

#[test]
fn a_refused_stroke_is_dropped_and_the_next_goes() {
    let mut ink = Ink::default();
    draw(&mut ink, &[(1.0, 0.0)]);
    draw(&mut ink, &[(2.0, 0.0)]);
    let (key, _) = ink.next().unwrap();
    ink.answered(key, Answer::Refused(gone()));
    ink.answered(key, Answer::Applied(1));
    assert_eq!(ink.problem(), Some(&gone()));
    assert_eq!(first_x(&ink), [2.0]);
    assert_eq!(ink.next().unwrap().1.points[0].x, 2.0);
}

#[test]
fn a_new_stroke_finishes_one_whose_lift_was_lost() {
    let mut ink = Ink::default();
    ink.down("main", "p1", RED, pt(1.0, 0.0));
    ink.down("main", "p1", RED, pt(2.0, 0.0));
    assert_eq!(first_x(&ink), [1.0, 2.0]);
    assert_eq!(ink.next().unwrap().1.points[0].x, 1.0);
}

#[test]
fn the_delay_log_has_the_stroke_s_numbers() {
    let mut ink = Ink::default();
    ink.painted(0.5);
    assert_eq!(ink.delay_log(), None, "no stroke, no log");
    ink.down("main", "p1", RED, pt(0.0, 0.0));
    for ms in [4.0, 2.0, 3.0, 9.0, f64::NAN] {
        ink.painted(ms / 1000.0);
    }
    ink.up(None);
    ink.painted(0.5);
    assert_eq!(
        ink.delay_log().unwrap(),
        "ink: pen to paint over 4 paints, ms: median 4.0, 95th 9.0, most 9.0"
    );
    assert_eq!(ink.delay_log(), None);
}
