use super::*;
use crate::config::Rgb;
use crate::layout::Rect;
use crate::scene::canvas::Shape;
use crate::scene::ink::Answer;
use crate::scene::tools::Tool;

const HERE: (&str, &str) = ("main", "p1");

fn pt(x: f64, y: f64) -> Point {
    Point { x, y }
}

/// A box, i1, and a stroke, i2, on `main`'s page p1.
fn page() -> Vec<Item> {
    let colour = Rgb { r: 255, g: 0, b: 0 };
    let frame = Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 };
    let points = vec![pt(40.0, 40.0), pt(50.0, 40.0)];
    vec![
        Item { id: ItemId(1), colour, shape: Shape::Box { frame, width: 2.0 } },
        Item { id: ItemId(2), colour, shape: Shape::Stroke { points, width: 2.0 } },
    ]
}

fn shown(items: &[Item]) -> Vec<Cow<'_, Item>> {
    items.iter().map(Cow::Borrowed).collect()
}

/// Selects the box with the lasso, then presses inside its box at (5, 5)
/// and drags the selection off towards another pane.
fn grab(select: &mut Select, ink: &mut Ink, items: &[Cow<'_, Item>]) {
    let around = [(-5.0, -5.0), (15.0, -5.0), (15.0, 15.0), (-5.0, 15.0)];
    assert!(select.down(Tool::Lasso, HERE, pt(-5.0, -5.0), 1.0, items));
    for (x, y) in around {
        select.drag(pt(x, y), items);
    }
    assert!(select.up(None, items, ink));
    assert!(select.down(Tool::Lasso, HERE, pt(5.0, 5.0), 1.0, items));
    assert!(select.drag(pt(900.0, 5.0), items));
}

/// The items moved by `by` as the mirror would carry them to page p7.
fn carried(ids: &[ItemId], by: Point) -> Vec<Carried> {
    let page = page();
    let found = page.into_iter().filter(|i| ids.contains(&i.id));
    let carry = |i: Item| Carried {
        object: serde_json::json!({ "page": "p7", "kind": "box" }),
        item: Item { shape: i.shape.moved(by), ..i },
    };
    found.map(carry).collect()
}

#[test]
fn a_drag_that_ends_in_a_pane_on_another_canvas_carries_the_items_there() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    let items = shown(&page);
    grab(&mut select, &mut ink, &items);
    let landing = Landing { canvas: "board", page: "p7", at: pt(100.04, 205.0) };
    let mut asked = None;
    let carry = |ids: &[ItemId], by| {
        asked = Some((ids.to_vec(), by));
        carried(ids, by)
    };
    assert!(select.lift(pt(900.0, 5.0), Some(landing), &items, &mut ink, carry));
    let by = pt(95.0, 200.0);
    assert_eq!(asked, Some((vec![ItemId(1)], by)), "kept where it was under the pen, in tenths");
    assert_eq!(select.selection(), None, "the items are not here any more");

    // Both show at once: gone from main, on the board under the ink's own id.
    let here: Vec<u64> = ink.shown("main", "p1", &page).iter().map(|i| i.id.0).collect();
    assert_eq!(here, [2]);
    let there = ink.shown("board", "p7", &[]);
    assert_eq!(there.len(), 1);
    // Its bounds take in half its line width.
    assert_eq!(there[0].shape.bounds().map(|b| (b.x, b.y)), Some((94.0, 199.0)));
    assert!(there[0].id.0 >= crate::scene::ink::LOCAL, "{:?}", there[0].id);

    // The add there goes first, then the removal here: one step on each.
    let Some((key, Change::Carry { canvas, page, items })) = ink.next() else { panic!() };
    assert_eq!((canvas.as_str(), page.as_str(), items.len()), ("board", "p7", 1));
    assert_eq!(ink.next(), None, "the removal waits for the add");
    ink.answered(key, Answer::Applied(3));
    let remove = Change::Remove { canvas: "main".into(), page: "p1".into(), ids: vec![ItemId(1)] };
    assert_eq!(ink.next().map(|(_, c)| c), Some(remove));
}

#[test]
fn a_carry_the_other_canvas_refuses_removes_nothing() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    let items = shown(&page);
    grab(&mut select, &mut ink, &items);
    let landing = Landing { canvas: "gone", page: "p1", at: pt(5.0, 5.0) };
    assert!(select.lift(pt(900.0, 5.0), Some(landing), &items, &mut ink, carried));
    let (key, _) = ink.next().unwrap();
    let refused =
        crate::backend::Problem { message: "canvas \"gone\" not found".into(), hint: None };
    ink.answered(key, Answer::Refused(refused));
    assert_eq!(ink.next(), None, "the removal went with it");
    assert_eq!(ink.shown("main", "p1", &page).len(), 2, "the box shows here again");
}

#[test]
fn a_drag_that_ends_outside_every_pane_moves_nothing() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    let items = shown(&page);
    grab(&mut select, &mut ink, &items);
    let carry = |_: &[ItemId], _| -> Vec<Carried> { panic!("nothing is carried") };
    assert!(select.lift(pt(900.0, 5.0), None, &items, &mut ink, carry));
    assert_eq!(ink.next(), None);
    assert_eq!(select.selection().map(|s| s.ids.clone()), Some(vec![ItemId(1)]), "still selected");
    let over = select.shown(HERE, shown(&page), 1.0, Rgb { r: 0, g: 0, b: 0 });
    let boxed = over.iter().find(|i| i.id == ItemId(1)).unwrap();
    assert_eq!(boxed.shape, page[0].shape, "drawn where it was");
}

#[test]
fn a_drag_that_ends_in_another_pane_on_the_same_page_moves_the_items() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    let items = shown(&page);
    grab(&mut select, &mut ink, &items);
    // The other pane looks at main's p1 elsewhere: (25, 35) there.
    let landing = Landing { canvas: "main", page: "p1", at: pt(25.0, 35.0) };
    let carry = |_: &[ItemId], _| -> Vec<Carried> { panic!("a move, not a carry") };
    assert!(select.lift(pt(900.0, 5.0), Some(landing), &items, &mut ink, carry));
    let Some((_, Change::Move { ids, by, .. })) = ink.next() else { panic!() };
    assert_eq!((ids, by), (vec![ItemId(1)], pt(20.0, 30.0)));
}

#[test]
fn a_carry_of_items_no_longer_there_changes_nothing() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    let items = shown(&page);
    grab(&mut select, &mut ink, &items);
    let landing = Landing { canvas: "board", page: "p7", at: pt(5.0, 5.0) };
    assert!(select.lift(pt(0.0, 0.0), Some(landing), &items, &mut ink, |_, _| Vec::new()));
    assert_eq!(ink.next(), None);
}

#[test]
fn any_other_gesture_ends_where_the_pen_lifted_on_its_own_page() {
    let (page, mut select, mut ink) = (page(), Select::default(), Ink::default());
    let items = shown(&page);
    assert!(select.down(Tool::Eraser, HERE, pt(40.0, 45.0), 1.0, &items));
    let landing = Landing { canvas: "board", page: "p7", at: pt(0.0, 0.0) };
    let carry = |_: &[ItemId], _| -> Vec<Carried> { panic!("an erase carries nothing") };
    assert!(select.lift(pt(45.0, 41.0), Some(landing), &items, &mut ink, carry));
    let Some((_, Change::Remove { ids, .. })) = ink.next() else { panic!() };
    assert_eq!(ids, [ItemId(2)]);
    assert!(!select.lift(pt(0.0, 0.0), None, &items, &mut ink, carried), "no gesture");
}
