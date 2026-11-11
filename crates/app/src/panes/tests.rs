use super::*;

const AREA: Rect = Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 };

fn canvas(name: &str) -> Pane {
    Pane { kind: Kind::Canvas, name: name.into() }
}

/// What each pane shows, in order, and which one has focus.
fn shown(panes: &Panes) -> (Vec<String>, usize) {
    let placed = panes.place(AREA);
    let focus = placed.iter().position(|(id, _, _)| *id == panes.focus()).unwrap();
    (placed.iter().map(|(_, pane, _)| pane.word()).collect(), focus)
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
    let mut panes = Panes::decode(1, "row 0.5 canvas:a column 0.5 ui:b canvas:c").unwrap();
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
    let mut panes = Panes::decode(1, "row 0.5 canvas:a column 0.5 ui:b canvas:c").unwrap();
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
    assert_eq!(panes.encode(), "row 0.4 canvas:main canvas:main");
    panes.grow(-1.0);
    assert_eq!(panes.encode(), "row 0.45 canvas:main canvas:main");
}

#[test]
fn a_layout_encodes_and_decodes_to_the_same_panes() {
    let text = "row 0.3 canvas:main column 0.5 ui:board canvas:notes";
    let panes = Panes::decode(VERSION, text).unwrap();
    assert_eq!(panes.encode(), text);
    assert_eq!(
        shown(&panes),
        (vec!["canvas:main".into(), "ui:board".into(), "canvas:notes".into()], 0)
    );
    assert_eq!(panes.place(AREA)[1].1, &Pane { kind: Kind::Ui, name: "board".into() });
    let mut changed = panes.clone();
    changed.split(Axis::Column);
    changed.grow(1.0);
    assert_eq!(Panes::decode(VERSION, &changed.encode()).unwrap().encode(), changed.encode());
}

#[test]
fn a_layout_this_app_cannot_read_is_none() {
    assert_eq!(Panes::decode(2, "canvas:main"), None);
    assert_eq!(Panes::decode(0, "canvas:main"), None);
    for text in ["", "main", "canvas:", "browser:x", "row 0.5 canvas:a", "canvas:a canvas:b"] {
        assert_eq!(Panes::decode(VERSION, text), None, "{text:?}");
    }
}
