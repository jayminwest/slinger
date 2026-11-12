use super::*;
use crate::config::{Rgb, Theme};
use crate::scene::canvas::Shape;
use crate::scene::ink::{Answer, Change, Ink, LOCAL};

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

fn stroke(id: u64, (x1, y1): (f64, f64), (x2, y2): (f64, f64)) -> Item {
    item(id, Shape::Stroke { points: vec![pt(x1, y1), pt(x2, y2)], width: 2.0 })
}

/// A big box (i1) with a stroke drawn in it (i2), a text (i3), an arrow
/// (i4), a stroke between them (i5), and local ink.
fn page() -> Vec<Item> {
    let text = Shape::Text { frame: frame(200.0, 0.0, 40.0, 20.0), text: "hi".into(), size: 16.0 };
    let arrow =
        Shape::Arrow { from: pt(200.0, 100.0), to: pt(300.0, 100.0), width: 2.0, bound: [None; 2] };
    vec![
        item(1, Shape::Box { frame: frame(0.0, 0.0, 100.0, 100.0), width: 2.0 }),
        stroke(2, (20.0, 50.0), (80.0, 50.0)),
        item(3, text),
        item(4, arrow),
        stroke(5, (260.0, 40.0), (270.0, 40.0)),
        stroke(LOCAL, (400.0, 0.0), (410.0, 0.0)),
    ]
}

fn shown(items: &[Item]) -> Vec<Cow<'_, Item>> {
    items.iter().map(Cow::Borrowed).collect()
}

fn selected(select: &Select) -> Vec<u64> {
    select.selection().map(|s| s.ids.iter().map(|i| i.0).collect()).unwrap_or_default()
}

/// A press of the select tool at `path`'s first point, a drag along the
/// rest, and the lift, at `zoom`, with shift or not. A shift press on an
/// item starts nothing to lift.
fn press(select: &mut Select, ink: &mut Ink, path: &[(f64, f64)], shift: bool, zoom: f64) {
    let page = page();
    let items = shown(&page);
    let (&(x, y), rest) = path.split_first().unwrap();
    select.pick(HERE, pt(x, y), zoom, shift, &items);
    for &(x, y) in rest {
        assert!(select.drag(pt(x, y), &items));
    }
    select.up(None, &items, ink);
}

/// The next change `ink` sends, answered at once.
fn sent(ink: &mut Ink) -> Option<Change> {
    let (key, change) = ink.next()?;
    ink.answered(key, Answer::Applied(1));
    Some(change)
}

fn click(select: &mut Select, ink: &mut Ink, x: f64, y: f64) {
    press(select, ink, &[(x, y)], false, 1.0);
}

#[test]
fn a_press_selects_the_topmost_item_it_picks_alone() {
    let (mut select, mut ink) = (Select::default(), Ink::default());
    for ((x, y), id) in [
        ((50.0, 56.0), 2),   // the stroke in the box: its line, within reach
        ((50.0, 80.0), 1),   // inside the box: a box of no size touches it
        ((-5.0, 30.0), 1),   // just outside the box's outline
        ((230.0, 24.0), 3),  // the text, by its frame
        ((250.0, 106.0), 4), // the arrow, by its line
    ] {
        click(&mut select, &mut ink, x, y);
        assert_eq!(selected(&select), [id], "at ({x}, {y})");
    }
    assert_eq!(ink.next(), None, "selecting changes nothing on the server");
    click(&mut select, &mut ink, 405.0, 0.0);
    assert_eq!(selected(&select), [] as [u64; 0], "local ink is not the server's yet");
}

#[test]
fn the_reach_is_in_screen_points() {
    let (mut select, mut ink) = (Select::default(), Ink::default());
    press(&mut select, &mut ink, &[(250.0, 104.0)], false, 2.0);
    assert_eq!(selected(&select), [4], "3 canvas units of reach and half the line at zoom 2");
    select.clear();
    press(&mut select, &mut ink, &[(250.0, 105.0)], false, 2.0);
    assert_eq!(selected(&select), [] as [u64; 0], "out of reach");
}

#[test]
fn a_press_on_empty_canvas_clears_the_selection_and_a_drag_selects_what_its_box_touches() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    click(&mut select, &mut ink, 50.0, 56.0);
    click(&mut select, &mut ink, 150.0, 150.0);
    assert_eq!(select.selection(), None, "a press on nothing selects nothing");
    let items = shown(&page);
    select.pick(HERE, pt(150.0, -10.0), 1.0, false, &items);
    assert!(select.drag(pt(255.0, 110.0), &items));
    let over = select.shown(HERE, shown(&page), 1.0, &THEME);
    let mark = over.last().unwrap();
    let Shape::Panel { frame: drawn, fill: None, .. } = mark.shape else { panic!() };
    assert_eq!((drawn, mark.colour), (frame(150.0, -10.0, 105.0, 120.0), THEME.muted));
    assert!(select.up(None, &items, &mut ink));
    assert_eq!(selected(&select), [3, 4], "the text and the arrow touch it; i5 is beyond it");
    press(&mut select, &mut ink, &[(30.0, 40.0), (40.0, 60.0)], false, 1.0);
    assert_eq!(selected(&select), [2], "a box drawn inside a box takes its ink, not the box");
    press(&mut select, &mut ink, &[(-10.0, 40.0), (40.0, 60.0)], false, 1.0);
    assert_eq!(selected(&select), [1, 2], "one across its outline takes the box too");
}

/// An outer box (i1) with an inner box (i2) in it, a stroke in the inner
/// box (i3), and a box of the same size as the inner one drawn later (i4).
fn nested() -> Vec<Item> {
    let boxed =
        |id, x, y, size| item(id, Shape::Box { frame: frame(x, y, size, size), width: 2.0 });
    vec![
        boxed(1, 0.0, 0.0, 200.0),
        boxed(2, 50.0, 50.0, 100.0),
        stroke(3, (70.0, 100.0), (130.0, 100.0)),
        boxed(4, 300.0, 0.0, 100.0),
        boxed(5, 300.0, 0.0, 100.0),
    ]
}

#[test]
fn a_box_inside_a_box_is_picked_not_the_outer_one() {
    let page = nested();
    let items = shown(&page);
    let mut select = Select::default();
    // Each from no selection: a press in the last one would grab it.
    let mut drag = |path: &[(f64, f64)]| {
        select.clear();
        let (&(x, y), rest) = path.split_first().unwrap();
        select.pick(HERE, pt(x, y), 1.0, false, &items);
        for &(x, y) in rest {
            assert!(select.drag(pt(x, y), &items));
        }
        select.up(None, &items, &mut Ink::default());
        selected(&select)
    };
    assert_eq!(drag(&[(40.0, 40.0), (160.0, 160.0)]), [2, 3], "around the inner box");
    assert_eq!(drag(&[(60.0, 60.0), (65.0, 65.0)]), [2], "inside it, on nothing: the inner box");
    assert_eq!(drag(&[(60.0, 60.0)]), [2], "a press there that does not move: the inner box");
    assert_eq!(drag(&[(20.0, 20.0)]), [1], "a press between the two: the outer box");
    assert_eq!(drag(&[(20.0, 20.0), (25.0, 25.0)]), [1]);
    assert_eq!(drag(&[(40.0, 40.0), (60.0, 60.0)]), [2], "across the inner outline only");
    assert_eq!(drag(&[(350.0, 50.0)]), [5], "of equal boxes, the last drawn");
    assert_eq!(drag(&[(250.0, 50.0)]), [] as [u64; 0], "outside every box: nothing");
}

#[test]
fn shift_adds_an_item_takes_one_out_and_adds_what_a_box_touches() {
    let (mut select, mut ink) = (Select::default(), Ink::default());
    click(&mut select, &mut ink, 230.0, 10.0);
    press(&mut select, &mut ink, &[(250.0, 100.0)], true, 1.0);
    assert_eq!(selected(&select), [3, 4]);
    press(&mut select, &mut ink, &[(230.0, 10.0)], true, 1.0);
    assert_eq!(selected(&select), [4]);
    press(&mut select, &mut ink, &[(150.0, 40.0), (130.0, 60.0)], true, 1.0);
    assert_eq!(selected(&select), [4], "a box on nothing adds nothing");
    press(&mut select, &mut ink, &[(10.0, 40.0), (110.0, 60.0)], true, 1.0);
    assert_eq!(selected(&select), [4, 1, 2]);
    press(&mut select, &mut ink, &[(250.0, 100.0)], true, 1.0);
    press(&mut select, &mut ink, &[(50.0, 98.0)], true, 1.0);
    press(&mut select, &mut ink, &[(50.0, 50.0)], true, 1.0);
    assert_eq!(select.selection(), None, "the last one out");
    assert_eq!(ink.next(), None);
}

#[test]
fn a_drag_from_an_item_moves_it_and_a_drag_in_the_selection_moves_all_of_it() {
    let (mut select, mut ink) = (Select::default(), Ink::default());
    press(&mut select, &mut ink, &[(50.0, 50.0), (60.0, 55.0)], false, 1.0);
    let Some(Change::Move { ids, by, .. }) = sent(&mut ink) else { panic!() };
    assert_eq!((ids, by), (vec![ItemId(2)], pt(10.0, 5.0)), "selected, then moved");
    assert_eq!(selected(&select), [2]);
    click(&mut select, &mut ink, 230.0, 10.0);
    press(&mut select, &mut ink, &[(250.0, 100.0)], true, 1.0);
    press(&mut select, &mut ink, &[(220.0, 60.0), (230.0, 60.0)], false, 1.0);
    let Some(Change::Move { ids, .. }) = sent(&mut ink) else { panic!() };
    assert_eq!(ids, [ItemId(3), ItemId(4)], "empty space inside the selection's box grabs it");
    press(&mut select, &mut ink, &[(265.0, 40.0), (275.0, 40.0)], false, 1.0);
    let Some(Change::Move { ids, .. }) = sent(&mut ink) else { panic!() };
    assert_eq!(ids, [ItemId(3), ItemId(4)], "so does an item inside it, as the lasso's");
    click(&mut select, &mut ink, 265.0, 40.0);
    assert_eq!(selected(&select), [5], "a press on it that does not move selects it alone");
    assert_eq!(ink.next(), None);
}

#[test]
fn the_select_tool_s_selection_deletes_like_the_lasso_s() {
    let (mut select, mut ink) = (Select::default(), Ink::default());
    press(&mut select, &mut ink, &[(150.0, -10.0), (255.0, 110.0)], false, 1.0);
    assert!(select.delete(&mut ink));
    let Some(Change::Remove { ids, .. }) = sent(&mut ink) else { panic!() };
    assert_eq!(ids, [ItemId(3), ItemId(4)]);
    assert_eq!(select.selection(), None);
}

#[test]
fn a_box_touches_a_stroke_within_half_its_width_and_an_arrow_on_its_line() {
    let line = Shape::Stroke { points: vec![pt(0.0, 0.0), pt(10.0, 0.0)], width: 4.0 };
    assert!(hit::touches(&line, &frame(0.0, 2.0, 5.0, 5.0)));
    assert!(!hit::touches(&line, &frame(0.0, 2.1, 5.0, 5.0)));
    let arrow =
        Shape::Arrow { from: pt(0.0, 0.0), to: pt(10.0, 0.0), width: 4.0, bound: [None; 2] };
    assert!(hit::touches(&arrow, &frame(0.0, 0.0, 5.0, 5.0)), "an edge on the line");
    assert!(!hit::touches(&arrow, &frame(0.0, 0.1, 5.0, 5.0)), "an arrow's width does not count");
    assert_eq!(spanned(pt(5.0, 1.0), pt(2.0, 4.0)), frame(2.0, 1.0, 3.0, 3.0));
}
