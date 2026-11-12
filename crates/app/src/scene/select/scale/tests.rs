use super::*;
use crate::config::{Rgb, Theme};
use crate::scene::select::BOX;
use crate::scene::tools::Tool;

const HERE: (&str, &str) = ("main", "p1");
const THEME: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 1, g: 1, b: 1 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
    done: Rgb { r: 5, g: 5, b: 5 },
};

fn pt(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn frame(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect { x, y, width, height }
}

fn item(id: u64, shape: Shape) -> Item {
    Item { id: ItemId(id), colour: Rgb { r: 255, g: 0, b: 0 }, shape }
}

fn boxed(id: u64, r: Rect) -> Item {
    item(id, Shape::Box { frame: r, width: 0.0 })
}

fn stroke(id: u64, points: Vec<Point>) -> Item {
    item(id, Shape::Stroke { points, width: 0.0 })
}

fn text(id: u64, r: Rect) -> Item {
    sized(id, r, 24.0)
}

fn sized(id: u64, r: Rect, size: f64) -> Item {
    item(id, Shape::Text { frame: r, text: "hi".into(), size })
}

fn arrow(id: u64, from: Point, to: Point, bound: [Option<ItemId>; 2]) -> Item {
    item(id, Shape::Arrow { from, to, width: 0.0, bound })
}

/// A box, a stroke across it and a text in it, all held by the lasso
/// (their box is 0,0 to 20,10, with no width to their lines), and an
/// arrow from the box's right side out to 60, not held.
fn page() -> Vec<Item> {
    vec![
        boxed(1, frame(0.0, 0.0, 20.0, 10.0)),
        stroke(2, vec![pt(0.0, 0.0), pt(20.0, 10.0)]),
        text(3, frame(10.0, 5.0, 4.0, 4.0)),
        arrow(4, pt(20.0, 5.0), pt(60.0, 5.0), [Some(ItemId(1)), None]),
    ]
}

fn shown(items: &[Item]) -> Vec<Cow<'_, Item>> {
    items.iter().map(Cow::Borrowed).collect()
}

/// The page with i1 to i3 selected by the lasso.
fn selected(page: &[Item]) -> (Select, Ink) {
    let (mut select, mut ink) = (Select::default(), Ink::default());
    let items = shown(page);
    assert!(select.down(Tool::Lasso, HERE, pt(-5.0, -5.0), 1.0, &items));
    for (x, y) in [(25.0, -5.0), (25.0, 15.0), (-5.0, 15.0)] {
        select.drag(pt(x, y), &items);
    }
    assert!(select.up(None, &items, &mut ink));
    assert_eq!(select.selection().unwrap().ids, [ItemId(1), ItemId(2), ItemId(3)]);
    (select, ink)
}

#[test]
fn a_drag_of_a_corner_handle_scales_the_selection_about_the_opposite_corner_in_one_change() {
    let page = page();
    let (mut select, mut ink) = selected(&page);
    let items = shown(&page);
    // The bottom right handle sits on the box's corner, 6 out from 20,10.
    assert!(select.grabs(HERE, pt(26.0, 16.0), 1.0, &items));
    assert!(select.down(Tool::Lasso, HERE, pt(26.0, 16.0), 1.0, &items));
    assert!(select.drag(pt(46.0, 26.0), &items), "twice as wide and twice as tall");
    let after = [
        boxed(1, frame(0.0, 0.0, 40.0, 20.0)),
        stroke(2, vec![pt(0.0, 0.0), pt(40.0, 20.0)]),
        sized(3, frame(20.0, 10.0, 8.0, 8.0), 48.0),
        arrow(4, pt(40.0, 10.0), pt(60.0, 5.0), [Some(ItemId(1)), None]),
    ];
    let over = select.shown(HERE, shown(&page), 1.0, &THEME);
    let scaled: Vec<Item> = over[..4].iter().map(|i| (**i).clone()).collect();
    assert_eq!(scaled, after, "a text's size scales; the bound arrow end follows");
    let chrome = over.iter().find(|i| i.id == BOX).unwrap();
    let Shape::Panel { frame: b, .. } = chrome.shape else { panic!() };
    assert_eq!(b, frame(-6.0, -6.0, 52.0, 32.0), "the box follows the scale");
    assert!(select.up(None, &items, &mut ink));
    let reshape =
        Change::Reshape { canvas: "main".into(), page: "p1".into(), items: after.to_vec() };
    assert_eq!(ink.next().map(|(_, c)| c), Some(reshape), "one change, so one undo step");
    let laid = ink.shown("main", "p1", &page);
    assert_eq!(laid.iter().map(|i| (**i).clone()).collect::<Vec<_>>(), after);
    assert!(select.selection().is_some(), "the selection stays after a resize");
}

#[test]
fn the_select_tool_resizes_too_and_a_side_never_turns_over() {
    let page = page();
    let (mut select, mut ink) = selected(&page);
    let items = shown(&page);
    // The top left handle, 9 screen points wide: a press 3 out from its centre.
    assert!(select.grabs(HERE, pt(-9.0, -9.0), 1.0, &items), "outside the box, on the handle");
    select.pick(HERE, pt(-9.0, -9.0), 1.0, false, &items);
    assert!(select.drag(pt(100.0, 100.0), &items), "far past the opposite corner");
    assert!(select.up(None, &items, &mut ink));
    let Some((_, Change::Reshape { items: after, .. })) = ink.next() else { panic!() };
    // The anchor is 20,10; each side is 5% of what it was.
    assert_eq!(after[0], boxed(1, frame(19.0, 9.5, 1.0, 0.5)));
    assert_eq!(select.selection().unwrap().ids.len(), 3, "the selection stays");
}

#[test]
fn a_handle_pressed_and_lifted_unmoved_changes_nothing_and_its_reach_follows_the_zoom() {
    let page = page();
    let (mut select, mut ink) = selected(&page);
    let items = shown(&page);
    assert!(select.down(Tool::Lasso, HERE, pt(26.0, -6.0), 1.0, &items), "the top right handle");
    assert!(select.up(Some(pt(26.0, -6.0)), &items, &mut ink));
    assert_eq!(ink.next(), None);
    assert!(select.grabs(HERE, pt(23.0, 13.0), 2.0, &items), "3 screen points out at zoom 2");
    assert!(!select.grabs(HERE, pt(26.0, 16.0), 2.0, &items), "past the handle and the box");
    assert!(!select.grabs(("main", "p2"), pt(26.0, 16.0), 1.0, &items), "another page");
}

#[test]
fn with_shift_held_both_sides_scale_by_the_larger_and_letting_go_frees_them() {
    let page = page();
    let (mut select, mut ink) = selected(&page);
    let items = shown(&page);
    // The bottom right handle: 30 right is 2.5 times as wide, 5 down 1.5 times as tall.
    assert!(select.down(Tool::Lasso, HERE, pt(26.0, 16.0), 1.0, &items));
    select.keep_ratio(true);
    assert!(select.drag(pt(56.0, 21.0), &items));
    let over = select.shown(HERE, shown(&page), 1.0, &THEME);
    assert_eq!(*over[0], boxed(1, frame(0.0, 0.0, 50.0, 25.0)), "its shape kept");
    select.keep_ratio(false);
    assert!(select.drag(pt(56.0, 21.0), &items));
    let over = select.shown(HERE, shown(&page), 1.0, &THEME);
    assert_eq!(*over[0], boxed(1, frame(0.0, 0.0, 50.0, 15.0)), "each side its own");
    select.keep_ratio(true);
    assert!(select.up(Some(pt(36.0, 6.0)), &items, &mut ink), "1.5 times as wide, half as tall");
    let Some((_, Change::Reshape { items: after, .. })) = ink.next() else { panic!() };
    assert_eq!(after[0], boxed(1, frame(0.0, 0.0, 30.0, 15.0)), "the larger is the width's");
}

#[test]
fn shift_scales_a_selection_with_no_height_by_its_width_alone() {
    let page = vec![stroke(1, vec![pt(0.0, 0.0), pt(20.0, 0.0)])];
    let (mut select, mut ink) = (Select::default(), Ink::default());
    let items = shown(&page);
    select.pick(HERE, pt(10.0, 0.0), 1.0, false, &items);
    assert!(select.up(None, &items, &mut ink));
    assert_eq!(select.selection().map(|s| s.ids.clone()), Some(vec![ItemId(1)]));
    assert!(select.down(Tool::Lasso, HERE, pt(26.0, 6.0), 1.0, &items), "the bottom right handle");
    select.keep_ratio(true);
    assert!(select.up(Some(pt(46.0, 30.0)), &items, &mut ink));
    let next = ink.next();
    let Some((_, Change::Reshape { items: after, .. })) = next else { panic!("{next:?}") };
    assert_eq!(after[0], stroke(1, vec![pt(0.0, 0.0), pt(40.0, 0.0)]), "twice as long, still flat");
}

#[test]
fn a_text_scales_evenly_by_the_mean_of_the_sides_and_within_the_sizes() {
    let page = vec![text(1, frame(0.0, 0.0, 20.0, 10.0))];
    let resize = |to: Point| {
        let (mut select, mut ink) = (Select::default(), Ink::default());
        let items = shown(&page);
        select.pick(HERE, pt(10.0, 5.0), 1.0, false, &items);
        assert!(select.up(None, &items, &mut ink));
        assert!(select.down(Tool::Lasso, HERE, pt(26.0, 16.0), 1.0, &items), "bottom right");
        assert!(select.up(Some(to), &items, &mut ink));
        let next = ink.next();
        let Some((_, Change::Reshape { items: after, .. })) = next else { panic!("{next:?}") };
        after[0].clone()
    };
    // Four times as wide and as tall as it was: twice the size, evenly.
    assert_eq!(resize(pt(86.0, 16.0)), sized(1, frame(0.0, 0.0, 40.0, 20.0), 48.0));
    let Shape::Text { frame: big, size, .. } = resize(pt(10_000.0, 10_000.0)).shape else {
        panic!()
    };
    assert!((size - 400.0).abs() < 1e-9, "never above the largest: {size}");
    assert!((big.width - 20.0 * 400.0 / 24.0).abs() < 1e-9, "the frame grows with the size");
    let Shape::Text { size, .. } = resize(pt(-100.0, -100.0)).shape else { panic!() };
    assert!((size - 4.0).abs() < 1e-9, "never below the smallest: {size}");
}
