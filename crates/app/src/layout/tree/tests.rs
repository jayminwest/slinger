use super::*;

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect { x, y, width, height }
}

const AREA: Rect = Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 };

fn id(n: u32) -> PaneId {
    PaneId(n)
}

/// `0 | (1 / 2)`: pane 0 on the left, panes 1 over 2 on the right.
fn three() -> Tree {
    let mut tree = Tree::default();
    assert_eq!(tree.split(id(0), Axis::Row), Some(id(1)));
    assert_eq!(tree.split(id(1), Axis::Column), Some(id(2)));
    tree
}

#[test]
fn one_pane_fills_the_area() {
    let tree = Tree::default();
    assert_eq!(tree.panes(), [id(0)]);
    assert_eq!(tree.rects(AREA), [(id(0), AREA)]);
}

#[test]
fn nested_splits_divide_the_area_by_their_ratios() {
    assert_eq!(
        three().rects(AREA),
        [
            (id(0), rect(0.0, 0.0, 400.0, 600.0)),
            (id(1), rect(400.0, 0.0, 400.0, 300.0)),
            (id(2), rect(400.0, 300.0, 400.0, 300.0)),
        ]
    );
    let moved = rect(10.0, 20.0, 100.0, 50.0);
    assert_eq!(three().rects(moved)[2].1, rect(60.0, 45.0, 50.0, 25.0));
}

#[test]
fn a_closed_panes_space_goes_to_its_sibling() {
    let mut tree = three();
    assert!(tree.close(id(1)));
    assert_eq!(
        tree.rects(AREA),
        [(id(0), rect(0.0, 0.0, 400.0, 600.0)), (id(2), rect(400.0, 0.0, 400.0, 600.0))]
    );
    let mut tree = three();
    assert!(tree.close(id(0)));
    assert_eq!(
        tree.rects(AREA),
        [(id(1), rect(0.0, 0.0, 800.0, 300.0)), (id(2), rect(0.0, 300.0, 800.0, 300.0))]
    );
    assert!(tree.close(id(2)));
    assert_eq!(tree.rects(AREA), [(id(1), AREA)]);
}

#[test]
fn the_last_pane_and_an_unknown_pane_stay() {
    let mut tree = Tree::default();
    assert!(!tree.close(id(0)));
    assert!(!tree.close(id(7)));
    let mut tree = three();
    assert!(!tree.close(id(7)));
    assert_eq!(tree, three());
    assert_eq!(tree.split(id(7), Axis::Row), None);
    assert_eq!(tree, three());
}

#[test]
fn ids_are_never_reused() {
    let mut tree = three();
    assert!(tree.close(id(2)));
    assert_eq!(tree.split(id(1), Axis::Row), Some(id(3)));
    assert_eq!(tree.panes(), [id(0), id(1), id(3)]);
}

#[test]
fn a_full_tree_splits_no_more() {
    let mut tree = Tree::default();
    let mut last = id(0);
    for _ in 1..MAX_PANES {
        last = tree.split(last, Axis::Column).unwrap();
    }
    assert_eq!(tree.panes().len(), MAX_PANES);
    assert_eq!(tree.split(last, Axis::Row), None);
    let text = tree.encode(|_| "p".into());
    assert_eq!(Tree::decode(&text, |_, _| true).unwrap().panes().len(), MAX_PANES);
}

#[test]
fn very_small_areas_give_no_negative_and_no_oversized_rectangles() {
    for area in [
        rect(0.0, 0.0, 0.0, 0.0),
        rect(5.0, 5.0, 1.0, 1.0),
        rect(0.0, 0.0, 0.6, 3.0),
        rect(0.0, 0.0, -10.0, -4.0),
    ] {
        let mut tree = three();
        tree.grow(id(0), 1.0);
        for (_, r) in tree.rects(area) {
            assert!(r.width >= 0.0 && r.height >= 0.0, "{area:?} {r:?}");
            assert!(
                r.width <= area.width.max(0.0) && r.height <= area.height.max(0.0),
                "{area:?} {r:?}"
            );
        }
    }
    // An odd width rounds the first side up, and the second gets the rest.
    let rects = three().rects(rect(0.0, 0.0, 101.0, 3.0));
    assert_eq!(rects[0].1, rect(0.0, 0.0, 51.0, 3.0));
    assert_eq!(rects[1].1, rect(51.0, 0.0, 50.0, 2.0));
    assert_eq!(rects[2].1, rect(51.0, 2.0, 50.0, 1.0));
}

#[test]
fn grow_changes_the_nearest_split_within_its_limits() {
    let mut tree = three();
    tree.grow(id(1), 0.1);
    assert_eq!(tree.rects(AREA)[1].1, rect(400.0, 0.0, 400.0, 360.0));
    tree.grow(id(2), 0.1);
    tree.grow(id(2), 0.1);
    assert_eq!(tree.rects(AREA)[2].1, rect(400.0, 240.0, 400.0, 360.0));
    tree.grow(id(0), -0.25);
    assert_eq!(tree.rects(AREA)[0].1, rect(0.0, 0.0, 200.0, 600.0));
    tree.grow(id(0), -1.0);
    assert_eq!(tree.rects(AREA)[0].1.width, 80.0);
    tree.grow(id(0), 5.0);
    assert_eq!(tree.rects(AREA)[0].1.width, 720.0);
}

#[test]
fn a_ratio_grown_in_steps_reads_back_as_written() {
    let mut tree = three();
    for _ in 0..3 {
        tree.grow(id(0), 0.05);
    }
    assert_eq!(tree.encode(|p| format!("p{}", p.0)), "row 0.65 p0 column 0.5 p1 p2");
}

#[test]
fn growing_a_lone_or_unknown_pane_changes_nothing() {
    let mut tree = Tree::default();
    tree.grow(id(0), 0.1);
    assert_eq!(tree, Tree::default());
    let mut tree = three();
    tree.grow(id(9), 0.1);
    assert_eq!(tree, three());
}

#[test]
fn next_goes_in_order_and_wraps() {
    let tree = three();
    assert_eq!(tree.next(id(0)), id(1));
    assert_eq!(tree.next(id(1)), id(2));
    assert_eq!(tree.next(id(2)), id(0));
    assert_eq!(tree.next(id(9)), id(9));
    assert_eq!(Tree::default().next(id(0)), id(0));
}

#[test]
fn toward_finds_the_pane_beside_and_none_at_the_edge() {
    let tree = three();
    assert_eq!(
        tree.toward(id(0), Direction::Right, AREA),
        Some(id(1)),
        "shares the edge, first in order"
    );
    assert_eq!(tree.toward(id(2), Direction::Left, AREA), Some(id(0)));
    assert_eq!(tree.toward(id(1), Direction::Down, AREA), Some(id(2)));
    assert_eq!(tree.toward(id(2), Direction::Up, AREA), Some(id(1)));
    assert_eq!(tree.toward(id(0), Direction::Left, AREA), None);
    assert_eq!(tree.toward(id(0), Direction::Up, AREA), None);
    assert_eq!(tree.toward(id(1), Direction::Right, AREA), None);
    assert_eq!(tree.toward(id(9), Direction::Right, AREA), None);
}

#[test]
fn toward_prefers_the_pane_that_shares_more_of_the_edge() {
    let mut tree = three();
    tree.grow(id(2), 0.2);
    assert_eq!(tree.toward(id(0), Direction::Right, AREA), Some(id(2)));
}

#[test]
fn toward_prefers_the_nearest_pane() {
    // 0 | 1 | 2, all full height: from 0, 1 is nearer than 2.
    let mut tree = Tree::default();
    let one = tree.split(id(0), Axis::Row).unwrap();
    tree.split(one, Axis::Row).unwrap();
    assert_eq!(tree.toward(id(0), Direction::Right, AREA), Some(one));
    assert_eq!(tree.toward(id(2), Direction::Left, AREA), Some(one));
}

#[test]
fn panes_with_no_size_have_no_neighbours() {
    assert_eq!(three().toward(id(0), Direction::Right, rect(0.0, 0.0, 0.0, 0.0)), None);
}

#[test]
fn a_tree_encodes_and_decodes_to_itself() {
    let mut tree = three();
    tree.grow(id(2), 0.125);
    let text = tree.encode(|p| format!("p{}", p.0));
    assert_eq!(text, "row 0.5 p0 column 0.375 p1 p2");
    let mut seen = Vec::new();
    let back = Tree::decode(&text, |p, word| {
        seen.push((p, word.to_string()));
        true
    });
    assert_eq!(back, Some(tree));
    assert_eq!(seen, [(id(0), "p0".into()), (id(1), "p1".into()), (id(2), "p2".into())]);
    assert_eq!(Tree::decode("  only \n", |_, _| true), Some(Tree::default()));
}

#[test]
fn a_ratio_past_its_limits_is_held_in_them() {
    let tree = Tree::decode("column 2 a row -1 b c", |_, _| true).unwrap();
    assert_eq!(tree.encode(|_| "x".into()), "column 0.9 x row 0.1 x x");
}

#[test]
fn text_that_is_not_one_tree_is_refused() {
    for text in [
        "",
        "a b",
        "row",
        "row 0.5 a",
        "row half a b",
        "row NaN a b",
        "column inf a b",
        "row 0.5 a b c",
    ] {
        assert_eq!(Tree::decode(text, |_, _| true), None, "{text:?}");
    }
    assert_eq!(Tree::decode("row 0.5 a b", |_, word| word == "a"), None);
}

#[test]
fn text_with_too_many_panes_or_splits_is_refused() {
    fn balanced(depth: u32) -> String {
        if depth == 0 {
            "a".into()
        } else {
            format!("row 0.5 {} {}", balanced(depth - 1), balanced(depth - 1))
        }
    }
    assert_eq!(Tree::decode(&balanced(6), |_, _| true).unwrap().panes().len(), MAX_PANES);
    assert_eq!(Tree::decode(&balanced(7), |_, _| true), None);
    let deep = "row 0.5 ".repeat(100_000);
    assert_eq!(Tree::decode(&deep, |_, _| true), None);
}
