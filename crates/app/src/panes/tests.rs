use slinger_control::Pane as Shown;

use super::*;

const AREA: Rect = Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 };

fn canvas(name: &str) -> Pane {
    Pane { kind: Kind::Canvas, name: name.into() }
}

fn word(pane: &Pane) -> String {
    let kind = if pane.kind == Kind::Canvas { "canvas" } else { "ui" };
    format!("{kind}:{}", pane.name)
}

/// What each pane shows, in order, and which one has focus.
fn shown(panes: &Panes) -> (Vec<String>, usize) {
    let placed = panes.place(AREA);
    let focus = placed.iter().position(|(id, _, _)| *id == panes.focus()).unwrap();
    (placed.iter().map(|(_, pane, _)| word(pane)).collect(), focus)
}

/// A pane as the server sends it.
fn leaf(pane: u32, kind: PaneKind, target: &str) -> PaneTree {
    PaneTree::Pane(Shown { pane, kind, target: target.into() })
}

fn split(split: PaneSplit, ratio: f64, first: PaneTree, second: PaneTree) -> PaneTree {
    PaneTree::Split { split, ratio, first: Box::new(first), second: Box::new(second) }
}

/// `row 0.5 canvas:a column 0.5 ui:b canvas:c`, as the server sends it.
fn three() -> PaneTree {
    let right =
        split(PaneSplit::Column, 0.5, leaf(2, PaneKind::Ui, "b"), leaf(3, PaneKind::Canvas, "c"));
    split(PaneSplit::Row, 0.5, leaf(1, PaneKind::Canvas, "a"), right)
}

#[test]
fn starts_with_one_focused_pane_that_fills_the_area() {
    let panes = Panes::new(canvas("main"));
    let placed = panes.place(AREA);
    assert_eq!(placed.len(), 1);
    assert_eq!((placed[0].0, placed[0].1, placed[0].2), (panes.focus(), &canvas("main"), AREA));
}

#[test]
fn a_split_shows_the_same_in_a_new_pane_that_takes_focus() {
    let mut panes = Panes::new(canvas("main"));
    panes.split(Axis::Row);
    assert_eq!(shown(&panes), (vec!["canvas:main".into(), "canvas:main".into()], 1));
    let placed = panes.place(AREA);
    assert_eq!(placed[1].2, Rect { x: 400.0, y: 0.0, width: 400.0, height: 600.0 });
    panes.split(Axis::Column);
    assert_eq!(panes.place(AREA)[2].2, Rect { x: 400.0, y: 300.0, width: 400.0, height: 300.0 });
}

#[test]
fn a_full_window_does_not_split() {
    let mut panes = Panes::new(canvas("main"));
    for _ in 0..crate::layout::tree::MAX_PANES + 3 {
        panes.split(Axis::Column);
    }
    assert_eq!(panes.place(AREA).len(), crate::layout::tree::MAX_PANES);
}

#[test]
fn close_gives_focus_to_the_pane_before_and_keeps_the_last() {
    let mut panes = Panes::follow(&three(), 1);
    panes.focus_next();
    panes.focus_next();
    panes.close();
    assert_eq!(shown(&panes), (vec!["canvas:a".into(), "ui:b".into()], 1));
    panes.focus_next();
    panes.close();
    assert_eq!(shown(&panes), (vec!["ui:b".into()], 0));
    assert_eq!(panes.place(AREA)[0].2, AREA);
    panes.close();
    assert_eq!(shown(&panes), (vec!["ui:b".into()], 0));
}

#[test]
fn focus_moves_in_order_and_by_direction() {
    let mut panes = Panes::follow(&three(), 1);
    assert_eq!(shown(&panes).1, 0);
    panes.focus_toward(Direction::Left, AREA);
    assert_eq!(shown(&panes).1, 0, "at the edge, focus stays");
    panes.focus_toward(Direction::Right, AREA);
    assert_eq!(shown(&panes).1, 1);
    panes.focus_toward(Direction::Down, AREA);
    assert_eq!(shown(&panes).1, 2);
    panes.focus_next();
    assert_eq!(shown(&panes).1, 0);
}

#[test]
fn grow_and_shrink_move_the_focused_panes_edge_by_steps() {
    let mut panes = Panes::new(canvas("main"));
    panes.split(Axis::Row);
    panes.grow(2.0);
    assert_eq!(panes.place(AREA)[1].2.width, 480.0);
    panes.grow(-1.0);
    assert_eq!(panes.place(AREA)[1].2.width, 440.0);
}

#[test]
fn follows_the_servers_tree_with_focus_on_a_number() {
    let tree = split(PaneSplit::Row, 0.3, leaf(1, PaneKind::Canvas, "main"), three());
    let panes = Panes::follow(&tree, 3);
    assert_eq!(
        shown(&panes),
        (vec!["canvas:main".into(), "canvas:a".into(), "ui:b".into(), "canvas:c".into()], 2)
    );
    assert_eq!((panes.number(), panes.focused()), (3, &Pane { kind: Kind::Ui, name: "b".into() }));
    let rects: Vec<Rect> = panes.place(AREA).iter().map(|p| p.2).collect();
    assert_eq!(rects[0], Rect { x: 0.0, y: 0.0, width: 240.0, height: 600.0 });
    assert_eq!(rects[3], Rect { x: 520.0, y: 300.0, width: 280.0, height: 300.0 });
    assert_eq!(Panes::follow(&tree, 0).number(), 1);
    assert_eq!(Panes::follow(&tree, 99).number(), 4);
    assert_eq!(
        Panes::follow(&leaf(1, PaneKind::Ui, "x"), 1),
        Panes::new(Pane { kind: Kind::Ui, name: "x".into() })
    );
}

#[test]
fn a_tree_past_the_most_panes_shows_what_fits() {
    let mut tree = leaf(1, PaneKind::Canvas, "a");
    for n in 2..=70 {
        tree = split(PaneSplit::Column, 0.5, tree, leaf(n, PaneKind::Canvas, "b"));
    }
    let panes = Panes::follow(&tree, 70);
    assert_eq!(panes.place(AREA).len(), crate::layout::tree::MAX_PANES);
}

#[test]
fn grown_names_the_divider_by_the_number_of_the_pane_before_it() {
    let mut panes = Panes::follow(&three(), 3);
    assert_eq!(panes.grown(1.0), Some((2, 0.45)), "pane 3 grows up into pane 2");
    assert_eq!(panes.place(AREA)[2].2.height, 300.0, "the panes do not change");
    panes.command(Command::FocusLeft, AREA);
    assert_eq!(panes.grown(-2.0), Some((1, 0.4)));
    assert_eq!(Panes::new(canvas("main")).grown(1.0), None, "a lone pane");
    assert_eq!(panes.number_of(panes.focus()), 1);
}

#[test]
fn pane_commands_run_on_the_panes_and_others_do_nothing() {
    let mut panes = Panes::new(canvas("main"));
    panes.command(Command::SplitRight, AREA);
    panes.command(Command::SplitDown, AREA);
    assert_eq!(shown(&panes), (vec!["canvas:main".into(); 3], 2));
    panes.command(Command::GrowPane, AREA);
    assert_eq!(panes.place(AREA)[2].2.height, 330.0);
    panes.command(Command::ShrinkPane, AREA);
    panes.command(Command::FocusUp, AREA);
    panes.command(Command::FocusLeft, AREA);
    assert_eq!(shown(&panes).1, 0);
    panes.command(Command::FocusRight, AREA);
    panes.command(Command::FocusDown, AREA);
    panes.command(Command::FocusNext, AREA);
    assert_eq!(shown(&panes).1, 0);
    let before = panes.clone();
    panes.command(Command::ZoomIn, AREA);
    assert_eq!(panes, before);
    panes.command(Command::ClosePane, AREA);
    assert_eq!(shown(&panes).0.len(), 2);
}
