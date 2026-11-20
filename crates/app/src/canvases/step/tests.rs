use super::*;

fn trail(steps: &[(u64, bool)]) -> (Trail, Vec<Viewport>) {
    let backs: Vec<Viewport> = [0.0, 100.0, 200.0]
        .iter()
        .take(steps.len())
        .map(|&x| Viewport::start().at(Point { x, y: 0.0 }))
        .collect();
    let steps = (steps.iter().zip(&backs))
        .map(|(&(id, folded), &back)| Step { id: ItemId(id), folded, back })
        .collect();
    (Trail { here: Some(("notes".into(), "p1".into())), steps }, backs)
}

#[test]
fn a_trail_is_on_its_own_page_only() {
    let (t, _) = trail(&[(1, true), (2, false)]);
    assert_eq!(t.on(("notes", "p1")), [(ItemId(1), true), (ItemId(2), false)]);
    assert_eq!(t.on(("notes", "p2")), []);
    assert_eq!(t.on(("other", "p1")), []);
    assert_eq!(t.page(), Some(("notes".into(), "p1".into())), "inside a folded box there");
    let (open, _) = trail(&[(2, false)]);
    assert_eq!(open.page(), None, "only open boxes: nothing hidden to select");
}

#[test]
fn stepping_out_goes_back_to_the_view_before_the_step() {
    let (mut t, backs) = trail(&[(1, true), (2, false), (3, false)]);
    let mut viewport = Viewport::start();
    t.keep(3, &mut viewport);
    assert_eq!(viewport, Viewport::start(), "all held: the view stays");
    t.keep(1, &mut viewport);
    assert_eq!(viewport, backs[1], "back to before the second step");
    assert_eq!(t.on(("notes", "p1")), [(ItemId(1), true)]);
    t.keep(0, &mut viewport);
    assert_eq!(viewport, backs[0]);
    assert_eq!(t.on(("notes", "p1")), []);
}

#[test]
fn a_press_holds_inside_a_frame_edges_included() {
    let r = Rect { x: 10.0, y: 10.0, width: 20.0, height: 20.0 };
    assert!(holds(r, Point { x: 10.0, y: 30.0 }));
    assert!(!holds(r, Point { x: 9.9, y: 20.0 }));
}

#[test]
fn the_room_draws_dashed_with_no_fill() {
    let r = Rect { x: 10.0, y: 10.0, width: 20.0, height: 20.0 };
    let item = room(r, crate::config::Rgb { r: 0, g: 0, b: 0 });
    assert_eq!(item.id, ROOM);
    assert!(
        matches!(item.shape, Shape::Panel { fill: None, dashed: true, frame, .. } if frame == r)
    );
}
