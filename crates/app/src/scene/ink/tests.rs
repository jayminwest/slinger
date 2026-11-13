use super::*;
use std::borrow::Cow;

use crate::layout::Rect;

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

/// The ids `main` p1's ink shows under.
fn ids_of(ink: &Ink) -> Vec<ItemId> {
    ink_of(ink, "main", "p1").iter().map(|i| i.id).collect()
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
    assert!(item.id.0 >= PENDING, "ink never takes a server id");
    let Shape::Stroke { points, width } = &item.shape else { panic!() };
    assert_eq!((points.as_slice(), *width), ([pt(1.0, 2.0), pt(3.0, 4.0)].as_slice(), PEN_WIDTH));
    assert!(ink_of(&ink, "main", "p2").is_empty() && ink_of(&ink, "other", "p1").is_empty());
    assert_eq!(ink.next(), None, "nothing goes while the pen is down");
}

#[test]
fn a_new_stroke_takes_the_width_set_and_the_one_under_the_pen_keeps_its_own() {
    let mut ink = Ink::default();
    assert!((ink.width() - PEN_WIDTH).abs() < f64::EPSILON, "until one is set");
    ink.down("main", "p1", RED, pt(0.0, 0.0));
    ink.set_width(8.0);
    ink.down("main", "p1", RED, pt(5.0, 0.0));
    let widths: Vec<f64> = ink_of(&ink, "main", "p1")
        .iter()
        .map(|item| match item.shape {
            Shape::Stroke { width, .. } => width,
            _ => panic!(),
        })
        .collect();
    assert_eq!(widths, [PEN_WIDTH, 8.0]);
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
    let moved = Change::Move {
        canvas: "main".into(),
        page: "p1".into(),
        ids: vec![ItemId(3)],
        by,
        follow: vec![],
    };
    ink.push(moved.clone());
    let shown = ink.shown("main", "p1", &page);
    let ids: Vec<u64> = shown.iter().map(|i| i.id.0).collect();
    assert_eq!(ids[..2], [2, 3], "i1 is gone at once");
    assert!(ids[2] >= PENDING, "then the stroke drawn");
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
    let moved = Change::Move {
        canvas: "d".into(),
        page: "p3".into(),
        ids: vec![],
        by: pt(0.0, 0.0),
        follow: vec![],
    };
    let named: Vec<(&str, Option<&str>)> =
        [&add, &remove, &moved].map(|c| (c.canvas(), c.page())).into();
    assert_eq!(named, [("main", Some("p1")), ("c", Some("p2")), ("d", Some("p3"))]);
    assert!(
        on_server(ItemId(PENDING)) && on_server(ItemId(LOCAL - 1)) && !on_server(ItemId(LOCAL))
    );
}

#[test]
fn a_put_an_edit_and_a_move_with_its_arrows_show_at_once() {
    let mut ink = Ink::default();
    let here = || ("main".to_string(), "p1".to_string());
    let frame = Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 };
    let (canvas, page) = here();
    ink.push(Change::Put { canvas, page, colour: RED, shape: Shape::Box { frame, width: 2.0 } });
    let text = |t: &str| Shape::Text { frame, text: t.into(), size: 24.0, column: None };
    let (canvas, page) = here();
    ink.push(Change::Set { canvas, page, id: ItemId(1), shape: text("after") });
    let arrow = |x| Shape::Arrow {
        from: pt(x, 0.0),
        to: pt(50.0, 0.0),
        width: 2.0,
        bound: [Some(ItemId(1)), None],
    };
    let follow = vec![Item { id: ItemId(2), colour: RED, shape: arrow(5.0) }];
    let (canvas, page) = here();
    ink.push(Change::Move { canvas, page, ids: vec![ItemId(1)], by: pt(5.0, 0.0), follow });

    let server = [
        Item { id: ItemId(1), colour: RED, shape: text("before") },
        Item { id: ItemId(2), colour: RED, shape: arrow(0.0) },
    ];
    let shown = ink.shown("main", "p1", &server);
    let shapes: Vec<&Shape> = shown.iter().map(|i| &i.shape).collect();
    let box_at = Shape::Box { frame, width: 2.0 };
    let moved = text("after").moved(pt(5.0, 0.0));
    assert_eq!(shapes, [&moved, &arrow(5.0), &box_at]);
    assert!(shown[2].id.0 >= PENDING, "a put has no server id yet");
}

#[test]
fn a_canvas_is_settled_once_every_change_of_it_is_written() {
    let mut ink = Ink::default();
    assert!(ink.settled("main"));
    draw(&mut ink, &[(0.0, 0.0), (1.0, 1.0)]);
    assert!(!ink.settled("main") && ink.settled("other"), "waiting");
    let (key, _) = next(&mut ink).unwrap();
    assert!(!ink.settled("main"), "sending");
    ink.answered(key, Answer::Applied(7));
    assert!(ink.settled("main"), "written, though the feed has not echoed it");
}

#[test]
fn an_undo_and_a_page_change_wait_their_turn_show_nothing_and_leave_when_heard() {
    use crate::scene::pages::PageOp;
    let mut ink = Ink::default();
    ink.down("main", "p1", RED, pt(0.0, 0.0));
    ink.up(Some(pt(1.0, 1.0)));
    let undo = Change::Step { canvas: "main".into(), redo: false };
    let undo_key = ink.push_keyed(undo.clone());
    let add = Change::Page { canvas: "main".into(), op: PageOp::Add { name: "Page 2".into() } };
    let add_key = ink.push_keyed(add.clone());
    assert_eq!((undo.page(), add.page()), (None, None));
    assert_eq!(ink.shown("main", "p1", &[]).len(), 1, "only the stroke shows");

    let (stroke_key, _) = ink.next().unwrap();
    assert_eq!(ink.next(), None, "the undo waits for the stroke");
    ink.answered(stroke_key, Answer::Applied(1));
    assert_eq!(ink.next(), Some((undo_key, undo)), "then the undo, which undoes it");
    ink.answered(undo_key, Answer::Applied(2));
    assert_eq!(ink.next(), Some((add_key, add)));
    ink.answered(add_key, Answer::NewPage { seq: 3, page: "p2".into() });
    assert_eq!(ink.next(), None);
    ink.heard("main", 2);
    assert_eq!(ink.shown("main", "p1", &[]).len(), 0);
    assert_eq!(ink.pending.len(), 1, "the page add waits for line 3");
    ink.heard("main", 3);
    assert_eq!(ink.pending, []);

    let nothing = ink.push_keyed(Change::Step { canvas: "main".into(), redo: true });
    ink.next();
    ink.answered(nothing, Answer::Applied(0));
    ink.heard("main", 0);
    assert_eq!(ink.pending, [], "a step that wrote nothing leaves at once");
}

#[test]
fn an_item_made_shows_under_the_server_s_id_and_once_the_mirror_holds_it_from_there_alone() {
    let mut ink = Ink::default();
    draw(&mut ink, &[(1.0, 0.0), (2.0, 0.0)]);
    let (key, _) = next(&mut ink).unwrap();
    assert!(ink_of(&ink, "main", "p1")[0].id.0 >= PENDING, "no server id until it says");
    ink.answered(key, Answer::Made { seq: 3, ids: vec![ItemId(3)] });
    let items = ink_of(&ink, "main", "p1");
    assert_eq!(items.iter().map(|i| i.id).collect::<Vec<_>>(), [ItemId(3)]);
    let echoed = items.clone();
    let shown = ink.shown("main", "p1", &echoed);
    assert!(matches!(shown.as_slice(), [Cow::Borrowed(_)]), "drawn once, the mirror's");
    ink.push(Change::Remove { canvas: "main".into(), page: "p1".into(), ids: vec![ItemId(3)] });
    assert_eq!(ink_of(&ink, "main", "p1"), [], "a removal by its server id takes it");
}

#[test]
fn ink_not_yet_written_can_be_erased_and_moved_and_the_change_names_the_server_s_id() {
    let mut ink = Ink::default();
    draw(&mut ink, &[(1.0, 0.0), (2.0, 0.0)]);
    draw(&mut ink, &[(5.0, 0.0), (6.0, 0.0)]);
    let (one, _) = next(&mut ink).unwrap();
    ink.answered(one, Answer::Unreachable(gone()));
    let [first, second] = ids_of(&ink)[..] else { panic!() };
    assert!(on_server(first) && on_server(second), "both can be taken");
    let here = || ("main".to_string(), "p1".to_string());
    let (canvas, page) = here();
    ink.push(Change::Remove { canvas, page, ids: vec![first, ItemId(7)] });
    let arrow = Shape::Arrow {
        from: pt(0.0, 0.0),
        to: pt(5.0, 0.0),
        width: 2.0,
        bound: [Some(second), None],
    };
    let follow = vec![Item { id: ItemId(8), colour: RED, shape: arrow }];
    let (canvas, page) = here();
    ink.push(Change::Move { canvas, page, ids: vec![second], by: pt(0.0, 3.0), follow });
    assert_eq!(ids_of(&ink), [second], "erased at once");

    let (key, _) = next(&mut ink).unwrap();
    assert_eq!(key, one, "sent again");
    let renamed = ink.answered(one, Answer::Made { seq: 1, ids: vec![ItemId(20)] });
    assert_eq!(renamed, [(first, ItemId(20))]);
    let (two, _) = next(&mut ink).unwrap();
    assert_eq!(ink.answered(two, Answer::Made { seq: 2, ids: vec![ItemId(21)] }).len(), 1);
    let (_, remove) = ink.next().unwrap();
    let Change::Remove { ids, .. } = &remove else { panic!("{remove:?}") };
    assert_eq!(ids, &[ItemId(20), ItemId(7)], "one removal, by the server's ids");
    ink.answered(two + 1, Answer::Applied(3));
    let (_, moved) = ink.next().unwrap();
    let Change::Move { ids, follow, .. } = &moved else { panic!("{moved:?}") };
    assert_eq!(ids, &[ItemId(21)]);
    let Shape::Arrow { bound, .. } = &follow[0].shape else { panic!() };
    assert_eq!(bound[0], Some(ItemId(21)), "the arrow follows it by its new id");
    assert_eq!(ids_of(&ink), [ItemId(21)]);
    let shown = ink_of(&ink, "main", "p1");
    let Shape::Stroke { points, .. } = &shown[0].shape else { panic!() };
    assert_eq!(points[0], pt(5.0, 3.0), "moved");
}

#[test]
fn a_change_made_after_an_item_s_add_was_written_or_refused_names_what_is_there() {
    let mut ink = Ink::default();
    draw(&mut ink, &[(1.0, 0.0)]);
    draw(&mut ink, &[(2.0, 0.0)]);
    let [kept, lost] = ids_of(&ink)[..] else { panic!() };
    let remove = |ids| Change::Remove { canvas: "main".into(), page: "p1".into(), ids };
    let (one, _) = next(&mut ink).unwrap();
    ink.answered(one, Answer::Made { seq: 1, ids: vec![ItemId(4)] });
    ink.heard("main", 1);
    ink.push(remove(vec![kept]));
    let (two, _) = next(&mut ink).unwrap();
    ink.push(remove(vec![lost]));
    ink.push(remove(vec![lost, ItemId(9)]));
    ink.answered(two, Answer::Refused(gone()));
    assert_eq!(ink.next().map(|(_, c)| c), Some(remove(vec![ItemId(4)])), "by the id made");
    ink.answered(two + 1, Answer::Applied(2));
    let (_, other) = ink.next().unwrap();
    assert_eq!(
        other,
        remove(vec![ItemId(9)]),
        "a removal of only the item never made is dropped; this one leaves it out"
    );
    ink.answered(two + 3, Answer::Applied(3));
    assert_eq!(ink.next(), None);
}
