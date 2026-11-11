use super::*;
use std::borrow::Cow;

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

/// The ink of `page` of `canvas` over an empty page.
fn ink_of(ink: &Ink, canvas: &str, page: &str) -> Vec<Item> {
    ink.shown(canvas, page, &[]).into_iter().map(Cow::into_owned).collect()
}

/// The next change to send, which must be a stroke.
fn next(ink: &mut Ink) -> Option<(u64, Stroke)> {
    ink.next().map(|(key, change)| match change {
        Change::Add(stroke) => (key, stroke),
        other => panic!("not a stroke: {other:?}"),
    })
}

fn first_x(ink: &Ink) -> Vec<f64> {
    let items = ink_of(ink, "main", "p1");
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
    let items = ink_of(&ink, "main", "p1");
    let [item] = items.as_slice() else { panic!("{items:?}") };
    assert_eq!(item.colour, RED);
    assert!(item.id.0 >= LOCAL, "ink never takes a server id");
    let Shape::Stroke { points, width } = &item.shape else { panic!() };
    assert_eq!((points.as_slice(), *width), ([pt(1.0, 2.0), pt(3.0, 4.0)].as_slice(), PEN_WIDTH));
    assert!(ink_of(&ink, "main", "p2").is_empty() && ink_of(&ink, "other", "p1").is_empty());
    assert_eq!(ink.next(), None, "nothing goes while the pen is down");
}

#[test]
fn a_lifted_stroke_keeps_its_id_and_waits_to_be_sent() {
    let mut ink = Ink::default();
    ink.down("main", "p1", RED, pt(0.0, 0.0));
    let under = ink_of(&ink, "main", "p1")[0].id;
    assert!(ink.up(Some(pt(5.0, 5.0))));
    assert!(!ink.drag(pt(6.0, 6.0)), "the pen is up");
    let items = ink_of(&ink, "main", "p1");
    assert_eq!(items[0].id, under, "lifting the pen repaints nothing");
    let (key, stroke) = next(&mut ink).unwrap();
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
    let (one, _) = next(&mut ink).unwrap();
    ink.heard("main", 9);
    assert_eq!(first_x(&ink), [1.0, 2.0], "a stroke not yet written stays");
    ink.answered(one, Answer::Applied(4));
    ink.heard("other", 9);
    assert_eq!(first_x(&ink), [1.0, 2.0], "another canvas's feed says nothing");
    ink.heard("main", 3);
    assert_eq!(first_x(&ink), [1.0, 2.0]);
    ink.heard("main", 4);
    assert_eq!(first_x(&ink), [2.0]);
    let (two, _) = next(&mut ink).unwrap();
    ink.answered(two, Answer::Applied(5));
    ink.heard("main", 9);
    assert_eq!(ink_of(&ink, "main", "p1"), []);
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
        let (key, stroke) = next(ink).unwrap();
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
    let (key, _) = next(&mut ink).unwrap();
    ink.answered(key, Answer::Refused(gone()));
    ink.answered(key, Answer::Applied(1));
    assert_eq!(ink.problem(), Some(&gone()));
    assert_eq!(first_x(&ink), [2.0]);
    assert_eq!(next(&mut ink).unwrap().1.points[0].x, 2.0);
}

#[test]
fn a_new_stroke_finishes_one_whose_lift_was_lost() {
    let mut ink = Ink::default();
    ink.down("main", "p1", RED, pt(1.0, 0.0));
    ink.down("main", "p1", RED, pt(2.0, 0.0));
    assert_eq!(first_x(&ink), [1.0, 2.0]);
    assert_eq!(next(&mut ink).unwrap().1.points[0].x, 1.0);
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

/// A page of three server strokes, i1 to i3, each one point at x = its id.
fn page() -> Vec<Item> {
    (1..=3)
        .map(|n: u32| Item {
            id: ItemId(n.into()),
            colour: RED,
            shape: Shape::Stroke { points: vec![pt(f64::from(n), 0.0)], width: PEN_WIDTH },
        })
        .collect()
}

#[test]
fn a_removal_or_a_move_shows_over_the_page_at_once_and_goes_in_turn() {
    let mut ink = Ink::default();
    let page = page();
    draw(&mut ink, &[(9.0, 9.0)]);
    let remove = Change::Remove { canvas: "main".into(), page: "p1".into(), ids: vec![ItemId(1)] };
    ink.push(remove.clone());
    let by = pt(10.0, 5.0);
    let moved = Change::Move { canvas: "main".into(), page: "p1".into(), ids: vec![ItemId(3)], by };
    ink.push(moved.clone());
    let shown = ink.shown("main", "p1", &page);
    let ids: Vec<u64> = shown.iter().map(|i| i.id.0).collect();
    assert_eq!(ids[..2], [2, 3], "i1 is gone at once");
    assert!(ids[2] >= LOCAL, "then the stroke drawn");
    assert!(matches!(shown[0], Cow::Borrowed(_)), "an item no change touches is not copied");
    let Shape::Stroke { points, .. } = &shown[1].shape else { panic!() };
    assert_eq!(points[0], pt(13.0, 5.0), "i3 moved");
    assert_eq!(ink.shown("main", "p2", &page).len(), 3, "another page is untouched");
    let (one, _) = ink.next().unwrap();
    ink.answered(one, Answer::Applied(1));
    assert_eq!(ink.next(), Some((one + 1, remove)), "changes go in the order made");
    ink.answered(one + 1, Answer::Applied(2));
    assert_eq!(ink.next().map(|(_, c)| c), Some(moved));
    ink.answered(one + 2, Answer::Applied(3));
    ink.heard("main", 3);
    assert_eq!(ink.shown("main", "p1", &page).len(), 3, "the mirror holds them all now");
    assert_eq!(ink.next(), None);
}

#[test]
fn a_change_names_its_canvas_and_page() {
    let mut ink = Ink::default();
    draw(&mut ink, &[(1.0, 1.0)]);
    let (_, add) = ink.next().unwrap();
    let remove = Change::Remove { canvas: "c".into(), page: "p2".into(), ids: vec![] };
    let moved =
        Change::Move { canvas: "d".into(), page: "p3".into(), ids: vec![], by: pt(0.0, 0.0) };
    let named: Vec<(&str, &str)> = [&add, &remove, &moved].map(|c| (c.canvas(), c.page())).into();
    assert_eq!(named, [("main", "p1"), ("c", "p2"), ("d", "p3")]);
    assert!(on_server(ItemId(LOCAL - 1)) && !on_server(ItemId(LOCAL)));
}
