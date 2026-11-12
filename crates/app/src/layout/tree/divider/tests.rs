use super::*;

const AREA: Rect = Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 };

fn id(n: u32) -> PaneId {
    PaneId(n)
}

fn at(x: f64, y: f64) -> Point {
    Point { x, y }
}

/// `0 | (1 / 2)`: pane 0 on the left, panes 1 over 2 on the right.
fn three() -> Tree {
    let mut tree = Tree::default();
    tree.split(id(0), Axis::Row);
    tree.split(id(1), Axis::Column);
    tree
}

#[test]
fn each_split_has_one_divider_named_by_the_pane_before_it() {
    let found = three().dividers(AREA);
    let row = Divider { before: id(0), axis: Axis::Row, area: AREA, at: 400.0 };
    let right = Rect { x: 400.0, width: 400.0, ..AREA };
    let column = Divider { before: id(1), axis: Axis::Column, area: right, at: 300.0 };
    assert_eq!(found, [row, column]);
    assert_eq!(Tree::default().dividers(AREA), []);
    let none = Rect { width: -5.0, ..AREA };
    assert_eq!(three().dividers(none)[0].area.width, 0.0, "no negative area");
}

#[test]
fn a_divider_is_named_by_the_last_pane_before_it_however_deep() {
    // `(0 / 2) | 1`: the row's divider comes after pane 2, the left column's last.
    let mut tree = Tree::default();
    tree.split(id(0), Axis::Row);
    tree.split(id(0), Axis::Column);
    let befores: Vec<PaneId> = tree.dividers(AREA).iter().map(|d| d.before).collect();
    assert_eq!(befores, [id(2), id(0)]);
    assert!(tree.set_ratio(id(2), 0.25));
    let right = tree.rects(AREA).into_iter().find(|(p, _)| *p == id(1)).unwrap().1;
    assert_eq!(right.x, 200.0, "pane 1 starts where the row's divider went");
}

#[test]
fn the_pen_grabs_a_divider_within_reach_of_it_along_its_span() {
    let tree = three();
    let grab = |x, y| tree.divider_at(AREA, at(x, y)).map(|d| d.before);
    assert_eq!(grab(400.0, 100.0), Some(id(0)));
    assert_eq!(grab(400.0 - REACH, 100.0), Some(id(0)), "the band's edge");
    assert_eq!(grab(400.0 + REACH, 500.0), Some(id(0)));
    assert_eq!(grab(400.0 - REACH - 1.0, 100.0), None, "out of reach");
    assert_eq!(grab(600.0, 300.0 + REACH), Some(id(1)));
    assert_eq!(grab(200.0, 300.0), None, "the column's divider spans the right side only");
    assert_eq!(grab(400.0, 700.0), None, "past the end of the span");
    // Where the two meet, the nearer wins, and the inner one on a tie.
    assert_eq!(grab(403.0, 300.0), Some(id(1)));
    assert_eq!(grab(402.0, 296.0), Some(id(0)));
    assert_eq!(Tree::default().divider_at(AREA, at(400.0, 300.0)), None);
}

#[test]
fn dragging_a_divider_gives_its_split_the_share_up_to_the_pen() {
    let tree = three();
    let row = tree.divider_at(AREA, at(400.0, 10.0)).unwrap();
    assert_eq!(row.ratio(at(200.0, 999.0)), 0.25);
    assert_eq!(row.ratio(at(333.3333, 0.0)), 0.417, "in thousandths");
    assert_eq!(row.ratio(at(-50.0, 0.0)), MIN_RATIO, "held in its limits");
    assert_eq!(row.ratio(at(5000.0, 0.0)), 1.0 - MIN_RATIO);
    let column = tree.divider_at(AREA, at(600.0, 300.0)).unwrap();
    assert_eq!(column.ratio(at(0.0, 150.0)), 0.25);
    let flat = Divider { area: Rect { width: 0.0, ..AREA }, ..row };
    assert_eq!(flat.ratio(at(10.0, 10.0)), 0.5, "a split with no room");
}

#[test]
fn set_ratio_moves_the_divider_after_a_pane() {
    let mut tree = three();
    assert!(tree.set_ratio(id(1), 0.25));
    assert_eq!(tree.rects(AREA)[1].1, Rect { x: 400.0, y: 0.0, width: 400.0, height: 150.0 });
    assert!(tree.set_ratio(id(0), 0.0));
    assert_eq!(tree.rects(AREA)[0].1.width, 80.0, "held in its limits");
    let before = tree.clone();
    assert!(!tree.set_ratio(id(2), 0.5), "no divider after the last pane");
    assert!(!tree.set_ratio(id(9), 0.5), "not in the tree");
    assert!(!tree.set_ratio(id(0), f64::NAN));
    assert_eq!(tree, before);
}

#[test]
fn grown_names_the_divider_a_grow_would_move() {
    let tree = three();
    assert_eq!(tree.grown(id(0), 0.05), Some((id(0), 0.55)));
    assert_eq!(tree.grown(id(2), 0.05), Some((id(1), 0.45)), "the pane after it grows");
    assert_eq!(tree.grown(id(2), -1.0), Some((id(1), 0.9)));
    let mut full = three();
    full.grow(id(0), 1.0);
    assert_eq!(full.grown(id(0), 0.05), None, "already at its limit");
    assert_eq!(Tree::default().grown(id(0), 0.05), None);
    assert_eq!(tree, three(), "the tree does not change");
}
