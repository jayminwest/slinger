//! The gesture table (docs/design/figures.md, "Lock"), for the selection's
//! rows: the eraser, the lasso, the select tool's tap and box, select all
//! and next item pass over a locked item; `unlock-figure` grabs its whole
//! figure, which then moves, nudges and deletes as one, shows no handles,
//! and locks again when the selection ends.

use std::borrow::Cow;

use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect};
use crate::scene::canvas::{Item, ItemId, Member, Shape};
use crate::scene::ink::{Change, Ink};
use crate::scene::select::Select;
use crate::scene::tools::Tool;

const HERE: (&str, &str) = ("main", "p1");
const THEME: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 1, g: 1, b: 1 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
    done: Rgb { r: 5, g: 5, b: 5 },
    agent: Rgb { r: 6, g: 6, b: 6 },
};

fn pt(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn frame(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect { x, y, width, height }
}

fn free(id: u64, shape: Shape) -> Item {
    Item::plain(ItemId(id), Rgb { r: 255, g: 0, b: 0 }, shape)
}

fn locked(id: u64, fig: Option<&str>, shape: Shape) -> Item {
    let member = Member { locked: true, fig: fig.map(str::to_string), part: None };
    Item { member, ..free(id, shape) }
}

fn boxed(x: f64, y: f64, w: f64, h: f64) -> Shape {
    Shape::Box { frame: frame(x, y, w, h), width: 2.0 }
}

/// Figure `sheet`: a box (i1) and a text in it (i2); a stroke the user
/// wrote across the box (i3); a free box beside it (i4); a box of figure
/// `other` (i5) and a locked box in no figure (i6) further off.
fn page() -> Vec<Item> {
    let text = Shape::Text {
        frame: frame(10.0, 10.0, 40.0, 20.0),
        text: "1a".into(),
        size: 16.0,
        column: None,
    };
    let ink = Shape::Stroke { points: vec![pt(20.0, 60.0), pt(80.0, 60.0)], width: 2.0 };
    vec![
        locked(1, Some("sheet"), boxed(0.0, 0.0, 100.0, 100.0)),
        locked(2, Some("sheet"), text),
        free(3, ink),
        free(4, boxed(200.0, 0.0, 50.0, 50.0)),
        locked(5, Some("other"), boxed(400.0, 0.0, 50.0, 50.0)),
        locked(6, None, boxed(600.0, 0.0, 50.0, 50.0)),
    ]
}

fn shown(items: &[Item]) -> Vec<Cow<'_, Item>> {
    items.iter().map(Cow::Borrowed).collect()
}

fn selected(select: &Select) -> Vec<u64> {
    select.selection().map(|s| s.ids.iter().map(|i| i.0).collect()).unwrap_or_default()
}

/// A press of `tool` at `path`'s first point (the select tool's pick for
/// `Tool::Select`), a drag along the rest, and the lift.
fn gesture(select: &mut Select, ink: &mut Ink, tool: Tool, path: &[(f64, f64)]) {
    let page = page();
    let items = shown(&page);
    let (&(x, y), rest) = path.split_first().unwrap();
    if tool == Tool::Select {
        select.pick(HERE, pt(x, y), 1.0, false, &items);
    } else {
        assert!(select.down(tool, HERE, pt(x, y), 1.0, &items));
    }
    for &(x, y) in rest {
        select.drag(pt(x, y), &items);
    }
    select.up(None, &items, ink);
}

#[test]
fn the_eraser_passes_over_a_locked_item_and_takes_the_ink_on_it() {
    let (mut select, mut ink) = (Select::default(), Ink::default());
    gesture(&mut select, &mut ink, Tool::Eraser, &[(0.0, 60.0), (100.0, 60.0), (100.0, 0.0)]);
    let Some((_, Change::Remove { ids, .. })) = ink.next() else { panic!("an erase") };
    assert_eq!(ids, [ItemId(3)], "the stroke, not the figure's box under it");
}

#[test]
fn the_lasso_and_the_select_tool_s_box_take_only_free_items() {
    let around = [(-10.0, -10.0), (300.0, -10.0), (300.0, 120.0), (-10.0, 120.0)];
    let (mut select, mut ink) = (Select::default(), Ink::default());
    gesture(&mut select, &mut ink, Tool::Lasso, &around);
    assert_eq!(selected(&select), [3, 4], "the lasso");
    select.clear();
    gesture(&mut select, &mut ink, Tool::Select, &[(-10.0, -10.0), (300.0, 120.0)]);
    assert_eq!(selected(&select), [3, 4], "the box");
    select.clear();
    gesture(&mut select, &mut ink, Tool::Select, &[(30.0, 80.0), (40.0, 90.0)]);
    assert_eq!(selected(&select), [] as [u64; 0], "never the locked box it lies inside");
    assert_eq!(ink.next(), None);
}

#[test]
fn a_tap_on_a_locked_item_selects_what_is_under_it_or_nothing() {
    let (mut select, mut ink) = (Select::default(), Ink::default());
    gesture(&mut select, &mut ink, Tool::Select, &[(0.0, 50.0)]);
    assert_eq!(selected(&select), [] as [u64; 0], "the box's outline is passed over");
    gesture(&mut select, &mut ink, Tool::Select, &[(50.0, 60.0)]);
    assert_eq!(selected(&select), [3], "the ink on it");
}

#[test]
fn select_all_and_next_item_pass_over_locked_items() {
    let page = page();
    let items = shown(&page);
    let mut select = Select::default();
    assert!(select.all(HERE, &items));
    assert_eq!(selected(&select), [3, 4]);
    select.clear();
    for want in [4, 3, 4] {
        assert!(select.step(HERE, &items, true).is_some());
        assert_eq!(selected(&select), [want], "reading order: i4's top, then i3's");
    }
}

#[test]
fn unlock_grabs_the_whole_figure_under_the_pointer_and_nothing_where_none_is() {
    let page = page();
    let items = shown(&page);
    let mut select = Select::default();
    assert!(!select.unlock(HERE, pt(150.0, 50.0), 6.0, &items), "empty paper");
    assert!(!select.unlock(HERE, pt(225.0, 25.0), 6.0, &items), "a free box is not a figure");
    assert!(!select.grabbed());
    assert!(select.unlock(HERE, pt(50.0, 50.0), 6.0, &items), "inside the figure's box");
    assert_eq!(selected(&select), [1, 2], "every item of sheet, not the ink on it");
    assert!(select.grabbed());
    assert!(select.unlock(HERE, pt(625.0, 25.0), 6.0, &items));
    assert_eq!(selected(&select), [6], "a locked item in no figure is a figure of one");
}

#[test]
fn a_grabbed_figure_moves_whole_with_no_handles() {
    let page = page();
    let items = shown(&page);
    let (mut select, mut ink) = (Select::default(), Ink::default());
    assert!(select.unlock(HERE, pt(50.0, 50.0), 6.0, &items));
    let over = select.shown(HERE, shown(&page), 1.0, &THEME);
    assert_eq!(over.len(), page.len() + 2, "the box and the count; no resize handles");
    assert!(select.handle_cursor(HERE, pt(-3.0, -3.0), 1.0, &items).is_none());
    gesture(&mut select, &mut ink, Tool::Select, &[(50.0, 50.0), (60.0, 70.0)]);
    let Some((_, Change::Move { ids, by, .. })) = ink.next() else { panic!("a move") };
    assert_eq!((ids, by), (vec![ItemId(1), ItemId(2)], pt(10.0, 20.0)), "all of it");
    assert!(select.grabbed(), "still held after the move");
    gesture(&mut select, &mut ink, Tool::Select, &[(50.0, 50.0)]);
    assert!(select.grabbed(), "a tap inside never picks one of its items alone");
}

#[test]
fn a_grabbed_figure_nudges_and_deletes_whole() {
    let page = page();
    let items = shown(&page);
    let mut select = Select::default();
    assert!(select.unlock(HERE, pt(50.0, 50.0), 6.0, &items));
    let mut ink = Ink::default();
    assert!(select.nudge(pt(1.0, 0.0), &items, &mut ink));
    assert!(matches!(ink.next(), Some((_, Change::Move { ids, .. })) if ids.len() == 2));
    let mut ink = Ink::default();
    assert!(select.delete(&mut ink));
    assert!(matches!(ink.next(), Some((_, Change::Remove { ids, .. })) if ids.len() == 2));
    assert!(!select.grabbed());
}

#[test]
fn any_new_selection_ends_the_grab_and_never_keeps_a_locked_item() {
    let page = page();
    let items = shown(&page);
    let (mut select, mut ink) = (Select::default(), Ink::default());
    assert!(select.unlock(HERE, pt(50.0, 50.0), 6.0, &items));
    select.keep(HERE, &items);
    assert_eq!(selected(&select), [1, 2], "a grab keeps its locked items");
    select.pick(HERE, pt(225.0, 0.0), 1.0, true, &items);
    assert_eq!(selected(&select), [4], "shift on another item: that item alone, the figure let go");
    assert!(!select.grabbed());
    assert!(select.unlock(HERE, pt(50.0, 50.0), 6.0, &items));
    gesture(&mut select, &mut ink, Tool::Select, &[(300.0, 300.0)]);
    assert_eq!(select.selection(), None, "a press outside");
    assert!(select.unlock(HERE, pt(50.0, 50.0), 6.0, &items));
    assert!(select.all(HERE, &items));
    assert!(!select.grabbed(), "select all");
    // A selection that is no grab lets go of a locked item (a find's jump, say).
    assert!(select.only(HERE, ItemId(1)));
    select.keep(HERE, &items);
    assert_eq!(select.selection(), None);
}
