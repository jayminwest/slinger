use super::*;
use crate::config::Rgb;

const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };
const THEME: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 1, g: 1, b: 1 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
    done: Rgb { r: 5, g: 5, b: 5 },
};
const HERE: (&str, &str) = ("main", "p1");

fn pt(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn item(id: u64, shape: Shape) -> Item {
    Item { id: ItemId(id), colour: RED, shape }
}

fn stroke(id: u64, points: &[(f64, f64)]) -> Item {
    let points = points.iter().map(|&(x, y)| pt(x, y)).collect();
    item(id, Shape::Stroke { points, width: 2.0 })
}

fn frame(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect { x, y, width: w, height: h }
}

/// Three notes in a row (a word, a box, a text), an arrow below them, and a
/// stroke of local ink: i1 to i4, then ink.
fn page() -> Vec<Item> {
    vec![
        stroke(1, &[(0.0, 0.0), (10.0, 0.0)]),
        item(2, Shape::Box { frame: frame(20.0, -5.0, 10.0, 10.0), width: 2.0 }),
        item(
            3,
            Shape::Text { frame: frame(40.0, -5.0, 10.0, 10.0), text: "hi".into(), size: 24.0 },
        ),
        item(
            4,
            Shape::Arrow { from: pt(0.0, 50.0), to: pt(50.0, 50.0), width: 2.0, bound: [None; 2] },
        ),
        stroke(LOCAL_INK, &[(0.0, 20.0), (50.0, 20.0)]),
    ]
}

const LOCAL_INK: u64 = crate::scene::ink::LOCAL;

fn shown(items: &[Item]) -> Vec<Cow<'_, Item>> {
    items.iter().map(Cow::Borrowed).collect()
}

fn ids(items: &[Cow<'_, Item>]) -> Vec<u64> {
    items.iter().map(|i| i.id.0).collect()
}

/// A gesture with `tool` along `path`, then the lift.
fn gesture(select: &mut Select, ink: &mut Ink, tool: Tool, path: &[(f64, f64)], items: &[Item]) {
    let items = shown(items);
    let (&(x, y), rest) = path.split_first().unwrap();
    assert!(select.down(tool, HERE, pt(x, y), 1.0, &items));
    for &(x, y) in rest {
        assert!(select.drag(pt(x, y), &items));
    }
    assert!(select.up(None, &items, ink));
}

/// A lasso around the three notes, x -5 to 55, y -10 to 10.
const AROUND: [(f64, f64); 4] = [(-5.0, -10.0), (55.0, -10.0), (55.0, 10.0), (-5.0, 10.0)];

#[test]
fn the_eraser_hides_what_it_touches_at_once_and_removes_it_in_one_change() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    let items = shown(&page);
    assert!(select.down(Tool::Eraser, HERE, pt(5.0, 30.0), 1.0, &items));
    assert!(
        select.drag(pt(5.0, 5.0), &items),
        "within reach of i1: 5 from its line, 1 its half width"
    );
    let over = select.shown(HERE, shown(&page), 1.0, &THEME);
    assert_eq!(ids(&over), [2, 3, 4, LOCAL_INK], "i1 hides at once; ink is not erased");
    assert_eq!(
        ids(&select.shown(("main", "p2"), shown(&page), 1.0, &THEME)),
        [1, 2, 3, 4, LOCAL_INK]
    );
    assert!(select.drag(pt(25.0, 0.0), &items), "across the box's left side");
    assert!(select.up(Some(pt(25.0, 1.0)), &items, &mut ink));
    let ids = vec![ItemId(1), ItemId(2)];
    let remove = Change::Remove { canvas: "main".into(), page: "p1".into(), ids };
    assert_eq!(ink.next().map(|(_, c)| c), Some(remove));
    assert!(!select.up(None, &items, &mut ink), "no gesture is under the pen now");
    assert!(!select.drag(pt(0.0, 0.0), &items));
}

#[test]
fn an_eraser_that_touches_nothing_changes_nothing_and_its_reach_follows_the_zoom() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    gesture(&mut select, &mut ink, Tool::Eraser, &[(5.0, 8.0)], &page);
    assert_eq!(ink.next(), None, "8 from i1 is out of reach at zoom 1: 6 and 1");
    let items = shown(&page);
    assert!(select.down(Tool::Eraser, HERE, pt(5.0, 8.0), 0.5, &items), "a dab at zoom 0.5");
    assert!(select.up(None, &items, &mut ink));
    assert!(matches!(ink.next(), Some((_, Change::Remove { ids, .. })) if ids == [ItemId(1)]));
}

#[test]
fn a_lasso_selects_the_items_it_holds_whole_and_draws_a_box_around_them() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    let items = shown(&page);
    assert!(select.down(Tool::Lasso, HERE, pt(-5.0, -10.0), 1.0, &items));
    assert!(select.drag(pt(55.0, -10.0), &items));
    assert!(select.drag(pt(55.0, -10.0), &items), "the same point again");
    let over = select.shown(HERE, shown(&page), 1.0, &THEME);
    let Shape::Stroke { points, .. } = &over.last().unwrap().shape else { panic!() };
    assert_eq!(points, &[pt(-5.0, -10.0), pt(55.0, -10.0), pt(-5.0, -10.0)], "the loop, closed");
    assert!(select.drag(pt(55.0, 10.0), &items));
    assert!(select.up(Some(pt(-5.0, 10.0)), &items, &mut ink));
    let selection = select.selection().unwrap();
    assert_eq!(selection.ids, [ItemId(1), ItemId(2), ItemId(3)], "not the arrow, not the ink");
    assert_eq!((selection.canvas.as_str(), selection.page.as_str()), HERE);
    assert_eq!(ink.next(), None, "selecting changes nothing on the server");
    assert_eq!(select.shown(("main", "p2"), shown(&page), 1.0, &THEME).len(), page.len());
}

#[test]
fn a_selection_shows_a_tinted_box_four_handles_and_its_count_at_screen_size() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    gesture(&mut select, &mut ink, Tool::Lasso, &AROUND, &page);
    let over = select.shown(HERE, shown(&page), 2.0, &THEME);
    let chrome = &over[page.len()..];
    assert_eq!(chrome.len(), 6, "the box, four handles and the count");
    assert!(chrome.iter().all(|i| i.colour == THEME.focus));
    let Shape::Panel { frame: b, fill, width, dashed } = chrome[0].shape else { panic!() };
    assert_eq!((chrome[0].id, width, dashed), (BOX, 0.5, false), "one screen point at zoom 2");
    assert_eq!(fill, Some(Fill { colour: THEME.focus, alpha: TINT }), "a faint tint");
    assert_eq!(b, frame(-1.0 - 3.0, -6.0 - 3.0, 51.0 + 6.0, 12.0 + 6.0));
    let (right, bottom) = (b.x + b.width, b.y + b.height);
    for (handle, (x, y)) in
        chrome[1..5].iter().zip([(b.x, b.y), (right, b.y), (right, bottom), (b.x, bottom)])
    {
        let Shape::Panel { frame: h, fill, width, .. } = handle.shape else { panic!() };
        assert_eq!(h, frame(x - 2.25, y - 2.25, 4.5, 4.5), "9 screen points, on the corner");
        assert_eq!((fill, width), (Some(Fill { colour: THEME.background, alpha: 1.0 }), 0.75));
    }
    let Shape::Text { frame: label, text, size } = &chrome[5].shape else { panic!() };
    assert_eq!((text.as_str(), *size), ("3 items", 5.5), "11 screen points");
    assert_eq!(label.x, b.x);
    assert!(label.y + label.height < b.y - 2.25, "above the handles");
}

#[test]
fn a_lasso_that_holds_nothing_or_presses_outside_clears_the_selection() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    gesture(&mut select, &mut ink, Tool::Lasso, &AROUND, &page);
    assert!(select.selection().is_some());
    gesture(&mut select, &mut ink, Tool::Lasso, &[(100.0, 100.0), (110.0, 100.0)], &page);
    assert_eq!(select.selection(), None);
    gesture(&mut select, &mut ink, Tool::Lasso, &AROUND, &page);
    assert!(select.clear());
    assert!(!select.clear(), "nothing left to clear");
    assert!(!select.delete(&mut ink));
    assert_eq!(ink.next(), None);
}

#[test]
fn a_drag_inside_the_box_moves_the_selection_in_one_change() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    gesture(&mut select, &mut ink, Tool::Lasso, &AROUND, &page);
    let items = shown(&page);
    assert!(select.down(Tool::Lasso, HERE, pt(52.0, 0.0), 1.0, &items), "inside the box's margin");
    assert!(select.drag(pt(62.04, 4.96), &items));
    let over = select.shown(HERE, shown(&page), 1.0, &THEME);
    let Shape::Stroke { points, .. } = &over[0].shape else { panic!() };
    assert!((points[0].x - 10.04).abs() < 1e-9, "the selection follows the pen");
    assert!(matches!(over[3], Cow::Borrowed(_)), "what is not selected stays put");
    assert!(select.up(None, &items, &mut ink));
    let Some((_, Change::Move { ids, by, .. })) = ink.next() else { panic!() };
    assert_eq!(ids, [ItemId(1), ItemId(2), ItemId(3)]);
    assert_eq!(by, pt(10.0, 5.0), "whole tenths, as the file stores them");
    assert!(select.selection().is_some(), "the selection stays after a move");
    gesture(&mut select, &mut ink, Tool::Lasso, &[(52.0, 0.0), (52.01, 0.0)], &page);
    assert_eq!(ink.next(), None, "a move that rounds to nothing sends nothing");
}

#[test]
fn delete_removes_the_selection_in_one_change_and_clears_it() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    gesture(&mut select, &mut ink, Tool::Lasso, &AROUND, &page);
    assert!(select.delete(&mut ink));
    assert_eq!(select.selection(), None);
    let Some((_, Change::Remove { ids, .. })) = ink.next() else { panic!() };
    assert_eq!(ids, [ItemId(1), ItemId(2), ItemId(3)]);
}

#[test]
fn items_removed_elsewhere_leave_the_selection() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    gesture(&mut select, &mut ink, Tool::Lasso, &AROUND, &page);
    select.keep(("main", "p2"), &[]);
    assert_eq!(select.selection().unwrap().ids.len(), 3, "another page says nothing");
    select.keep(HERE, &shown(&page[1..]));
    assert_eq!(select.selection().unwrap().ids, [ItemId(2), ItemId(3)]);
    select.keep(HERE, &[]);
    assert_eq!(select.selection(), None, "nothing left selected");
}

#[test]
fn only_the_eraser_and_the_lasso_start_a_gesture_and_a_selection_is_per_page() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    let items = shown(&page);
    for tool in [Tool::Pen, Tool::Text, Tool::Box, Tool::Arrow, Tool::Select] {
        assert!(!select.down(tool, HERE, pt(0.0, 0.0), 1.0, &items));
    }
    gesture(&mut select, &mut ink, Tool::Lasso, &AROUND, &page);
    assert!(select.down(Tool::Lasso, ("main", "p2"), pt(0.0, 0.0), 1.0, &items));
    assert_eq!(select.selection(), None, "a press on another page does not drag it");
}

#[test]
fn a_lasso_holds_by_winding_and_needs_three_points() {
    let note = stroke(1, &[(1.0, 1.0)]).shape;
    let square = [pt(0.0, 0.0), pt(2.0, 0.0), pt(2.0, 2.0), pt(0.0, 2.0)];
    assert!(hit::holds(&square, &note));
    assert!(!hit::holds(&square[..2], &note));
    let twice: Vec<Point> = square.iter().chain(&square).copied().collect();
    assert!(hit::holds(&twice, &note), "a loop drawn twice still holds");
    let backwards: Vec<Point> = square.iter().rev().copied().collect();
    assert!(hit::holds(&backwards, &note));
    let wide = item(2, Shape::Box { frame: frame(1.0, 1.0, 5.0, 0.5), width: 2.0 }).shape;
    assert!(!hit::holds(&square, &wide), "a corner outside");
    let empty = Shape::Stroke { points: vec![], width: 1.0 };
    assert!(!hit::holds(&square, &empty));
}

#[test]
fn an_eraser_crossing_a_stroke_between_its_points_touches_it() {
    let line = stroke(1, &[(0.0, 0.0), (10.0, 10.0)]).shape;
    assert!(hit::erases(&line, pt(0.0, 10.0), pt(10.0, 0.0), 0.0), "crosses it");
    assert!(!hit::erases(&line, pt(0.0, 20.0), pt(20.0, 20.0), 0.0));
    assert!(hit::erases(&line, pt(0.0, 20.0), pt(20.0, 20.0), 9.0), "the end is 10 away, width 2");
    assert!(hit::erases(&line, pt(5.0, 5.0), pt(5.0, 5.0), -1.0), "a dab on it; no reach is none");
}

#[test]
fn the_eraser_takes_text_boxes_and_arrows_but_not_ink_on_its_way() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    let path = [(45.0, -20.0), (45.0, 60.0)];
    gesture(&mut select, &mut ink, Tool::Eraser, &path, &page);
    let removed = |ink: &mut Ink| match ink.next() {
        Some((_, Change::Remove { ids, .. })) => ids.iter().map(|i| i.0).collect::<Vec<_>>(),
        other => panic!("expected one remove, got {other:?}"),
    };
    assert_eq!(removed(&mut ink), [3, 4], "the text and the arrow; ink is not erased");
    let mut ink = Ink::default();
    gesture(&mut select, &mut ink, Tool::Eraser, &[(25.0, 0.0), (25.0, 30.0)], &page);
    assert_eq!(removed(&mut ink), [2], "from inside the box across its bottom side");
}

#[test]
fn an_eraser_reaches_a_text_frame_a_box_outline_and_an_arrow_by_their_width() {
    let text = Shape::Text { frame: frame(0.0, 0.0, 10.0, 10.0), text: "hi".into(), size: 24.0 };
    assert!(hit::erases(&text, pt(5.0, 5.0), pt(5.0, 5.0), 0.0), "a dab inside");
    assert!(hit::erases(&text, pt(-3.0, -10.0), pt(-3.0, 20.0), 3.0), "past its side, in reach");
    assert!(!hit::erases(&text, pt(-4.0, -10.0), pt(-4.0, 20.0), 3.0));
    let boxed = Shape::Box { frame: frame(0.0, 0.0, 10.0, 10.0), width: 2.0 };
    assert!(hit::erases(&boxed, pt(-3.0, 5.0), pt(-3.0, 5.0), 2.0), "2 + half its width");
    assert!(!hit::erases(&boxed, pt(5.0, 5.0), pt(5.0, 5.0), 2.0), "its middle is empty");
    let arrow =
        Shape::Arrow { from: pt(0.0, 0.0), to: pt(10.0, 0.0), width: 4.0, bound: [None; 2] };
    assert!(hit::erases(&arrow, pt(5.0, 3.0), pt(5.0, 3.0), 1.0));
    assert!(!hit::erases(&arrow, pt(5.0, 4.0), pt(5.0, 4.0), 1.0));
    let button = Shape::Button { frame: frame(0.0, 0.0, 10.0, 10.0), status: None };
    assert!(!hit::erases(&button, pt(5.0, 5.0), pt(5.0, 5.0), 9.0), "the button is not ink");
}

#[test]
fn one_selected_item_is_counted_as_one() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    let around_stroke = [(-5.0, -5.0), (15.0, -5.0), (15.0, 5.0), (-5.0, 5.0)];
    gesture(&mut select, &mut ink, Tool::Lasso, &around_stroke, &page);
    let over = select.shown(HERE, shown(&page), 1.0, &THEME);
    assert!(matches!(&over.last().unwrap().shape, Shape::Text { text, .. } if text == "1 item"));
}
