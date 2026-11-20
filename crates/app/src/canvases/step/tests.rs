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

/// A stroke carried from the left pane into the right one, stepped into a
/// folded box on another page, joins that box (slinger-cd04ab).
#[test]
fn a_carry_into_a_pane_inside_a_folded_box_joins_it() {
    use crate::backend::testing::{user_stroke, wait_until};
    use crate::backend::tests::{open_notes, with_store};
    use crate::canvases::find::find;
    use crate::commands::Command;
    use crate::config::Config;
    use crate::layout::{Axis, WINDOW};
    use crate::panes::{Kind, Pane, Panes};
    use crate::scene::ink::Pen;
    use crate::scene::tools::Tool;

    let pen = |x, y, when| Pen {
        at: Point { x, y },
        when,
        touch: false,
        eraser: false,
        shift: false,
        tablet: false,
    };
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        user_stroke(daemon, "notes");
        let mut panes = Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() });
        panes.split(Axis::Row);
        let mut canvases = Canvases::new(daemon.clone());
        let placed = panes.place(WINDOW.rect()).into_iter();
        let frames: Vec<_> =
            placed.map(|(id, _, f)| (id, crate::scene::status::content(f))).collect();
        let [(left, a), (right, b)] = frames[..] else { panic!("two panes") };
        let show = |c: &mut Canvases| {
            c.poll();
            c.show(&panes, WINDOW.rect(), &Config::default().theme).len()
        };
        wait_until(|| show(&mut canvases) == 2 && canvases.pages_of(&panes, left).is_some());
        assert!(canvases.command(&panes, (right, b), Command::NewPage));
        let mut p2 = None;
        wait_until(|| {
            show(&mut canvases);
            p2 = canvases.pages_of(&panes, right).map(|(_, _, p)| p).filter(|p| p != "p1");
            p2.is_some()
        });
        let p2 = p2.unwrap();
        let item = serde_json::json!({"page": p2, "kind": "box", "color": "#000000",
            "x": 0, "y": 0, "w": 4000, "h": 3000});
        let ops = serde_json::json!({"op": "add", "item": item});
        let (canvas, by) = ("notes".to_string(), "user".to_string());
        let ask = slinger_control::CanvasOps { canvas, by, ops, once: None };
        let made = daemon.canvas_apply(ask).unwrap().made.remove(0);
        let fold = slinger_control::CanvasFold {
            canvas: "notes".into(),
            item: made.clone(),
            by: "user".into(),
            once: None,
        };
        daemon.canvas_fold(&slinger_control::FoldAsk::CanvasFold(fold)).unwrap();
        let id = crate::mirror::item_id(&made).unwrap();
        wait_until(|| {
            show(&mut canvases);
            let open =
                find(canvases.follow.open(), "notes", Some(&p2)).and_then(|o| o.mirror.as_ref());
            open.is_some_and(|m| m.box_folded(id) == Some(true))
        });
        let boxed = Boxed { canvas: "notes".into(), page: p2.clone(), id, folded: true };
        canvases.step_into(right, boxed);

        // The stroke from (100, 100) to (110, 110) in the left pane, dragged right.
        let tool = (Tool::Select, crate::config::Rgb { r: 128, g: 128, b: 128 });
        assert!(canvases.pen_down(&panes, (left, a), pen(121.0, 121.0, 1.0), tool));
        let to = Point { x: b.x + 100.0, y: 121.0 };
        let under = Some((right, b));
        assert!(canvases.pen_drag(&panes, pen(to.x, to.y, 1.1), under));
        canvases.pen_up(&panes, pen(to.x, to.y, 1.2), under);
        let holds = || {
            let items = daemon.canvas_show("notes", Some(&p2)).unwrap().items;
            let boxed = items.iter().find(|i| i["id"] == made.as_str()).cloned();
            boxed.map(|b| b["fold"]["items"].as_array().map_or(0, Vec::len))
        };
        wait_until(|| {
            show(&mut canvases);
            holds() == Some(1)
        });
        let left_on_p1 = daemon.canvas_show("notes", Some("p1")).unwrap().items;
        assert!(left_on_p1.is_empty(), "carried, not copied: {left_on_p1:?}");
    });
}

/// Zoom-to-fit in a pane inside a box fits the box's room, not what it
/// holds (slinger-ff9dbe).
#[test]
fn zoom_to_fit_inside_a_box_fits_its_room() {
    use crate::backend::testing::wait_until;
    use crate::backend::tests::{open_notes, with_store};
    use crate::canvases::find::find;
    use crate::commands::Command;
    use crate::config::Config;
    use crate::layout::{Size, WINDOW};
    use crate::panes::{Kind, Pane, Panes};

    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let add = |x: i64, y: i64, w: i64, h: i64| {
            let item = serde_json::json!({"page": "p1", "kind": "box", "color": "#000000",
                "x": x, "y": y, "w": w, "h": h});
            let ops = serde_json::json!({"op": "add", "item": item});
            let (canvas, by) = ("notes".to_string(), "user".to_string());
            let ask = slinger_control::CanvasOps { canvas, by, ops, once: None };
            daemon.canvas_apply(ask).unwrap().made.remove(0)
        };
        // Tenths of a point: a 4000 by 3000 point box holding a small one.
        let big = crate::mirror::item_id(&add(0, 0, 40_000, 30_000)).unwrap();
        add(1000, 1000, 100, 100);
        let panes = Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() });
        let mut canvases = Canvases::new(daemon.clone());
        let mut scenes = Vec::new();
        wait_until(|| {
            canvases.poll();
            scenes = canvases.show(&panes, WINDOW.rect(), &Config::default().theme);
            let open =
                find(canvases.follow.open(), "notes", Some("p1")).and_then(|o| o.mirror.as_ref());
            open.is_some_and(|m| m.items().len() == 2)
        });
        let pane = panes.focus();
        assert!(canvases.view(pane).viewport.zoom() < 0.25, "the page opened fitted");
        assert!(canvases.command(&panes, (pane, WINDOW.rect()), Command::ZoomReset));
        let boxed = Boxed { canvas: "notes".into(), page: "p1".into(), id: big, folded: false };
        canvases.step_into(pane, boxed);
        assert_eq!(canvases.view(pane).viewport.zoom(), 1.0, "a step in keeps the zoom");
        assert!(canvases.command(&panes, (pane, WINDOW.rect()), Command::ZoomToFit));
        let scene = canvases.show(&panes, WINDOW.rect(), &Config::default().theme).remove(0);
        let size = Size { width: scene.frame.width, height: scene.frame.height };
        let seen = scene.scene.viewport.visible(size);
        assert!(scene.scene.viewport.zoom() < 0.25, "the room, not the small box");
        assert!(seen.contains(Point { x: 0.0, y: 0.0 }));
        assert!(seen.contains(Point { x: 4000.0, y: 3000.0 }));
    });
}
