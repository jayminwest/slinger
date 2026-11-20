use super::*;
use crate::config::Rgb;
use crate::scene::canvas::Shape;
use crate::scene::ink::{Answer, LOCAL};

const HERE: (&str, &str) = ("main", "p1");

fn pt(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn stroke(id: u64, (x1, y1): (f64, f64), (x2, y2): (f64, f64)) -> Item {
    let shape = Shape::Stroke { points: vec![pt(x1, y1), pt(x2, y2)], width: 2.0 };
    Item { id: ItemId(id), colour: Rgb { r: 255, g: 0, b: 0 }, shape }
}

/// Drawn out of reading order: i1 low on the page, i2 at its top right,
/// i3 at its top left, i4 between; an arrow (i5) bound to i3 at its
/// start; and a mark over the page (from [`LOCAL`] up).
fn page() -> Vec<Item> {
    let arrow = Shape::Arrow {
        from: pt(10.0, 20.0),
        to: pt(300.0, 400.0),
        width: 2.0,
        bound: [Some(ItemId(3)), None],
    };
    vec![
        stroke(1, (0.0, 300.0), (50.0, 300.0)),
        stroke(2, (200.0, 0.0), (250.0, 0.0)),
        stroke(3, (0.0, 0.0), (50.0, 0.0)),
        stroke(4, (100.0, 150.0), (150.0, 150.0)),
        Item { id: ItemId(5), colour: Rgb { r: 0, g: 0, b: 0 }, shape: arrow },
        stroke(LOCAL, (0.0, -50.0), (10.0, -50.0)),
    ]
}

fn shown(items: &[Item]) -> Vec<Cow<'_, Item>> {
    items.iter().map(Cow::Borrowed).collect()
}

fn selected(select: &Select) -> Vec<u64> {
    select.selection().map(|s| s.ids.iter().map(|i| i.0).collect()).unwrap_or_default()
}

#[test]
fn each_nudge_and_duplicate_command_is_keyed_and_no_other() {
    assert_eq!(Keyed::of(Command::SelectAll), Some(Keyed::All));
    assert_eq!(Keyed::of(Command::PreviousItem), Some(Keyed::Step(false)));
    assert_eq!(Keyed::of(Command::NudgeUp), Some(Keyed::Nudge(0.0, -NUDGE)));
    assert_eq!(Keyed::of(Command::Duplicate), Some(Keyed::Duplicate));
    let keyed = Command::ALL.iter().filter(|(c, _)| Keyed::of(*c).is_some()).count();
    assert_eq!(keyed, 8);
    assert_eq!(Keyed::of(Command::DeleteSelection), None);
}

#[test]
fn select_all_takes_every_item_of_the_page_but_the_marks() {
    let (page, mut select) = (page(), Select::default());
    assert!(select.all(HERE, &shown(&page)));
    assert_eq!(selected(&select), [1, 2, 3, 4, 5], "in drawing order, the mark left out");
    assert!(!select.all(HERE, &[]), "an empty page selects nothing");
    assert_eq!(selected(&select), [1, 2, 3, 4, 5], "and keeps the selection");
}

#[test]
fn next_and_previous_go_in_reading_order_and_wrap() {
    let (page, mut select) = (page(), Select::default());
    let items = shown(&page);
    let mut steps = |forward| {
        select.step(HERE, &items, forward).expect("an item");
        selected(&select)[0]
    };
    let forward: Vec<u64> = (0..6).map(|_| steps(true)).collect();
    assert_eq!(forward, [3, 2, 5, 4, 1, 3], "top first, left before right at a tie");
    let back: Vec<u64> = (0..2).map(|_| steps(false)).collect();
    assert_eq!(back, [1, 4]);
}

#[test]
fn a_step_starts_from_the_last_item_selected_or_the_page_s_end() {
    let (page, mut select) = (page(), Select::default());
    let items = shown(&page);
    let frame = select.step(HERE, &items, false).unwrap();
    assert_eq!(selected(&select), [1], "nothing selected: back from the end");
    assert_eq!(frame, Rect { x: -1.0, y: 299.0, width: 52.0, height: 2.0 }, "its box");
    select.all(HERE, &items);
    select.step(HERE, &items, true);
    assert_eq!(selected(&select), [4], "after i5, the last selected");
    select.all(HERE, &items);
    select.step(("main", "p2"), &[], true);
    assert_eq!(selected(&select), [1, 2, 3, 4, 5], "a page with no items selects nothing");
    select.step(("main", "p2"), &items, true);
    assert_eq!(selected(&select), [3], "a selection on another page counts for nothing");
}

#[test]
fn a_nudge_moves_the_selection_in_one_step_with_its_arrows_following() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    let items = shown(&page);
    assert!(!select.nudge(pt(1.0, 0.0), &items, &mut ink), "nothing selected");
    select.step(HERE, &items, true);
    assert!(select.nudge(pt(10.0 / 3.0, 0.04), &items, &mut ink));
    let (key, change) = ink.next().expect("one change");
    ink.answered(key, Answer::Applied(1));
    let Change::Move { ids, by, follow, .. } = change else { panic!("{change:?}") };
    assert_eq!((ids, by), (vec![ItemId(3)], pt(3.3, 0.1)), "to the tenth, never to nothing");
    assert_eq!(follow.len(), 1, "the arrow bound to it follows");
    let Shape::Arrow { from, .. } = follow[0].shape else { panic!() };
    assert_eq!(from, pt(13.3, 20.1));
    assert_eq!(ink.next(), None, "one step");
}

#[test]
fn a_duplicate_lands_down_and_right_of_the_selection_in_screen_points() {
    let (page, mut select) = (page(), Select::default());
    let items = shown(&page);
    assert_eq!(select.duplicate_at(&items, 1.0), None, "nothing selected");
    select.step(HERE, &items, true);
    let at = select.duplicate_at(&items, 2.0).unwrap();
    assert_eq!(at, pt(25.0 + DUPLICATE / 2.0, DUPLICATE / 2.0));
}

#[test]
fn an_item_off_the_screen_comes_to_its_middle() {
    let view = Viewport::start();
    let size = Size { width: 400.0, height: 300.0 };
    let shown = view.visible(size);
    let inside = Rect { x: shown.x + 10.0, y: shown.y + 10.0, width: 20.0, height: 20.0 };
    assert_eq!(into_view(view, size, inside), None, "shown whole: no pan");
    let below = Rect { y: shown.y + 290.0, ..inside };
    let (dx, dy) = into_view(view, size, below).unwrap();
    let mid = view.pan(dx, dy).to_screen(pt(inside.x + 10.0, below.y + 10.0));
    assert_eq!(mid, pt(200.0, 150.0));
}

/// Whether `column` is on half a cell of `size`, as `glyphs::column` puts it.
fn on_half_a_cell(column: f64, size: f64) -> bool {
    let cells = column / (size * glyphs::CELL);
    (cells - cells.floor() - 0.5).abs() < 1e-9
}

#[test]
fn a_size_step_puts_a_selected_text_s_column_on_half_a_cell_of_its_new_size() {
    // A row of 97 characters in a new text's column at 24 (48.5 cells of
    // 12): at 12 the column kept would be 97 whole cells of 6, as wide as
    // the line, and read back as a text that does not wrap.
    let text = "a".repeat(97);
    let column = glyphs::column(glyphs::NEW_COLUMN * 12.0, 24.0);
    let frame = glyphs::rows_frame(pt(0.0, 0.0), &text, 24.0, column);
    let shape = Shape::Text { frame, text, size: 24.0, column: Some(column) };
    let page = [Item { id: ItemId(1), colour: Rgb { r: 0, g: 0, b: 0 }, shape }];
    let (mut select, mut ink) = (Select::default(), Ink::default());
    assert!(select.only(HERE, ItemId(1)));
    assert!(select.resize(Resize::To(12), &shown(&page), &mut ink));
    let Some((_, Change::Reshape { items, .. })) = ink.next() else { panic!() };
    let Shape::Text { size, column: Some(column), .. } = items[0].shape else { panic!() };
    assert_eq!(size, 12.0);
    assert!(on_half_a_cell(column, 12.0), "{column}");
    assert!((column - 97.0 * 6.0).abs() >= 3.0, "about as wide, off the line's width: {column}");
}

#[test]
fn an_arrow_bound_to_a_resized_text_follows_its_new_frame_in_the_same_step() {
    let at = pt(100.0, 100.0);
    let before = glyphs::rows_frame(at, "hello", 24.0, glyphs::UNWRAPPED);
    let text = Shape::Text { frame: before, text: "hello".into(), size: 24.0, column: None };
    // From the middle of the text's right side out to 400,400.
    let start = pt(before.x + before.width, before.y + before.height / 2.0);
    let arrow = Shape::Arrow {
        from: start,
        to: pt(400.0, 400.0),
        width: 2.0,
        bound: [Some(ItemId(1)), None],
    };
    let black = Rgb { r: 0, g: 0, b: 0 };
    let page = [
        Item { id: ItemId(1), colour: black, shape: text },
        Item { id: ItemId(2), colour: black, shape: arrow },
    ];
    let (mut select, mut ink) = (Select::default(), Ink::default());
    assert!(select.only(HERE, ItemId(1)));
    assert!(select.resize(Resize::To(48), &shown(&page), &mut ink));
    let Some((_, Change::Reshape { items, .. })) = ink.next() else { panic!() };
    assert_eq!(ink.next(), None, "one step, so one undo");
    let [text, arrow] = &items[..] else { panic!("{items:?}") };
    let Shape::Text { frame: after, .. } = text.shape else { panic!() };
    assert!(after.width > before.width, "the text grew");
    let Shape::Arrow { from, to, .. } = arrow.shape else { panic!() };
    let mid = pt(after.x + after.width, after.y + after.height / 2.0);
    assert!((from.x - mid.x).abs() < 1e-9 && (from.y - mid.y).abs() < 1e-9, "{from:?} {mid:?}");
    assert_eq!(to, pt(400.0, 400.0), "the free end stays");
}
