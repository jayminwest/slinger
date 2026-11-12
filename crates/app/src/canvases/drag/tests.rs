//! Dragging the selection with two panes side by side (slinger-197a49).

use super::super::*;
use crate::backend::testing::{user_stroke, wait_until};
use crate::backend::tests::{open_notes, with_store};
use crate::config::{Config, Rgb};
use crate::layout::{Axis, Point, WINDOW};
use crate::panes::{Kind, Pane};
use crate::scene::ink::Pen;
use crate::scene::status;
use crate::scene::tools::Tool;

const GREY: Rgb = Rgb { r: 128, g: 128, b: 128 };
/// The ids of the marks drawn over a page (the selection's box, a caret) start here.
const MARKS: u64 = u64::MAX - 64;

fn pen(x: f64, y: f64, when: f64) -> Pen {
    Pen { at: Point { x, y }, when, touch: false, eraser: false, shift: false }
}

/// `notes` in two panes side by side, the left one focused.
fn split() -> Panes {
    let mut panes = Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() });
    panes.split(Axis::Row);
    panes
}

/// Each pane's id and its canvas's frame in the window, left first.
fn frames(panes: &Panes) -> Vec<(PaneId, Rect)> {
    let placed = panes.place(WINDOW.rect()).into_iter();
    placed.map(|(id, _, frame)| (id, status::content(frame))).collect()
}

/// The pane under `(x, y)`, as the window finds it.
fn under(panes: &Panes, x: f64, y: f64) -> Option<(PaneId, Rect)> {
    frames(panes).into_iter().find(|(_, f)| f.contains(Point { x, y }))
}

fn scenes(canvases: &mut Canvases, panes: &Panes) -> Vec<crate::scene::CanvasPane> {
    canvases.poll();
    canvases.show(panes, WINDOW.rect(), &Config::default().theme)
}

/// The items `pane` draws (not the marks over them), as screen boxes in the window.
fn drawn(canvases: &mut Canvases, panes: &Panes, pane: PaneId) -> Vec<Rect> {
    let all = scenes(canvases, panes);
    let shown = all.into_iter().find(|s| s.pane == pane).unwrap();
    let ours = shown.scene.draws.iter().filter(|d| d.id.0 < MARKS);
    let at = |b: Rect| Rect { x: b.x + shown.frame.x, y: b.y + shown.frame.y, ..b };
    ours.map(|d| at(d.bounds)).collect()
}

/// The first point of the one stroke the daemon holds, in canvas units.
fn stroke_at(daemon: &Client) -> (f64, f64) {
    let items = daemon.canvas_show("notes", None).unwrap().items;
    let pts = items[0]["pts"].as_array().unwrap();
    let n = |i: usize| pts[i].as_f64().unwrap() / 10.0;
    (n(0), n(1))
}

/// Two panes on the first page of `notes`, which holds one stroke from
/// (100, 100) to (110, 110): at window x 116 to 126 in the left pane.
fn two_panes(daemon: &Client) -> (Canvases, Panes) {
    user_stroke(daemon, "notes");
    let (mut canvases, panes) = (Canvases::new(daemon.clone()), split());
    wait_until(|| {
        let all = scenes(&mut canvases, &panes);
        all.len() == 2 && all.iter().all(|s| s.scene.draws.len() == 1)
    });
    (canvases, panes)
}

/// Whether a server item `pane` draws covers the window point `(x, y)`.
fn covers(canvases: &mut Canvases, panes: &Panes, pane: PaneId, (x, y): (f64, f64)) -> bool {
    let boxes = drawn(canvases, panes, pane);
    boxes.iter().any(|b| b.outset(2.0).contains(Point { x, y }))
}

/// Waits until the stroke's first point is at `at`, the panes taking in news.
fn until_at(canvases: &mut Canvases, panes: &Panes, daemon: &Client, at: (f64, f64)) {
    wait_until(|| {
        scenes(canvases, panes);
        stroke_at(daemon) == at
    });
}

#[test]
fn a_drag_in_one_pane_follows_the_pen_at_any_zoom_and_pan_and_undoes_in_one_step() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let (mut canvases, panes) = two_panes(daemon);
        let [(left, a), (right, _)] = frames(&panes)[..] else { panic!() };
        // 200%, and panned: canvas (x, y) is at window ((x + 16) * 2 - 40, (y + 16) * 2).
        canvases.zoom(left, Point { x: 0.0, y: 0.0 }, 2.0);
        canvases.pan(left, -40.0, 0.0);
        let tool = (Tool::Select, GREY);
        // The stroke's middle, (105, 105), not selected yet: picked and dragged at once.
        assert!(canvases.pen_down(&panes, (left, a), pen(202.0, 242.0, 1.0), tool));
        for (i, x) in [(1.0, 222.0), (2.0, 262.0), (3.0, 302.0)] {
            assert!(canvases.pen_drag(&panes, pen(x, 242.0, 1.0 + i / 10.0), Some((left, a))));
            assert!(covers(&mut canvases, &panes, left, (x, 242.0)), "under the pen at {x}");
        }
        // The other pane on the page shows it move too: 50 canvas units right.
        assert!(covers(&mut canvases, &panes, right, (480.0 + 16.0 + 155.0, 121.0)));
        canvases.pen_up(&panes, pen(302.0, 242.0, 1.5), Some((left, a)));
        assert!(covers(&mut canvases, &panes, left, (302.0, 242.0)), "no jump back on the lift");
        until_at(&mut canvases, &panes, daemon, (150.0, 100.0));
        assert!(covers(&mut canvases, &panes, left, (302.0, 242.0)), "the echo lands there too");
        assert!(canvases.command(&panes, (left, a), crate::commands::Command::Undo));
        until_at(&mut canvases, &panes, daemon, (100.0, 100.0));
    });
}

#[test]
fn a_drag_into_the_other_pane_on_the_same_page_lands_under_the_pen_by_that_pane_s_view() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let (mut canvases, panes) = two_panes(daemon);
        let [(left, a), (right, _)] = frames(&panes)[..] else { panic!() };
        // The right pane at 200%, panned: canvas (x, y) is at window
        // (480 + (x - 4) * 2, (y + 16) * 2).
        canvases.zoom(right, Point { x: 0.0, y: 0.0 }, 2.0);
        canvases.pan(right, -40.0, 0.0);
        let tool = (Tool::Select, GREY);
        assert!(canvases.pen_down(&panes, (left, a), pen(121.0, 121.0, 1.0), tool));
        // Over the right pane at canvas (205, 105): 100 right of the grab.
        let to = (480.0 + 201.0 * 2.0, 242.0);
        assert!(canvases.pen_drag(&panes, pen(to.0, to.1, 1.1), under(&panes, to.0, to.1)));
        assert!(covers(&mut canvases, &panes, right, to), "under the pen");
        assert!(covers(&mut canvases, &panes, left, (221.0, 121.0)), "and where that is here");
        canvases.pen_up(&panes, pen(to.0, to.1, 1.2), under(&panes, to.0, to.1));
        until_at(&mut canvases, &panes, daemon, (200.0, 100.0));
        assert!(covers(&mut canvases, &panes, right, to));
        assert!(canvases.selection().is_some(), "still selected");
        assert!(canvases.command(&panes, (left, a), crate::commands::Command::Undo));
        until_at(&mut canvases, &panes, daemon, (100.0, 100.0));
    });
}

#[test]
fn a_drag_over_a_footer_shows_the_items_home_and_a_lift_there_moves_nothing() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let (mut canvases, panes) = two_panes(daemon);
        let [(left, a), (right, _)] = frames(&panes)[..] else { panic!() };
        let tool = (Tool::Select, GREY);
        assert!(canvases.pen_down(&panes, (left, a), pen(121.0, 121.0, 1.0), tool));
        assert!(canvases.pen_drag(&panes, pen(300.0, 121.0, 1.1), under(&panes, 300.0, 121.0)));
        assert!(!covers(&mut canvases, &panes, left, (121.0, 121.0)), "it went with the pen");
        let footer = (600.0, a.height + 5.0);
        assert_eq!(under(&panes, footer.0, footer.1), None, "the right pane's footer");
        assert!(canvases.pen_drag(&panes, pen(footer.0, footer.1, 1.2), None));
        assert!(covers(&mut canvases, &panes, left, (121.0, 121.0)), "home again");
        assert!(covers(&mut canvases, &panes, right, (480.0 + 121.0, 121.0)), "home in both");
        canvases.pen_up(&panes, pen(footer.0, footer.1, 1.3), None);
        assert!(covers(&mut canvases, &panes, left, (121.0, 121.0)));
        assert!(canvases.selection().is_some(), "still selected");
        std::thread::sleep(std::time::Duration::from_millis(100));
        scenes(&mut canvases, &panes);
        assert_eq!(stroke_at(daemon), (100.0, 100.0), "nothing moved");
    });
}

#[test]
fn a_press_in_the_selection_within_reach_of_a_divider_grabs_the_selection() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let (mut canvases, panes) = two_panes(daemon);
        let [(left, a), _] = frames(&panes)[..] else { panic!() };
        // The stroke at window x 466 to 476, by the divider at 480.
        canvases.pan(left, 350.0, 0.0);
        let tool = (Tool::Select, GREY);
        assert!(canvases.pen_down(&panes, (left, a), pen(471.0, 121.0, 1.0), tool));
        canvases.pen_up(&panes, pen(471.0, 121.0, 1.05), Some((left, a)));
        let near = pen(475.0, 125.0, 2.0);
        assert!(panes.divider_at(WINDOW.rect(), near.at).is_some(), "within the divider's reach");
        let over = under(&panes, near.at.x, near.at.y);
        assert!(canvases.grabs(&panes, over, near, Tool::Select));
        assert!(canvases.grabs(&panes, over, near, Tool::Lasso));
        assert!(!canvases.grabs(&panes, over, near, Tool::Pen), "the pen draws: the divider's");
        assert!(!canvases.grabs(&panes, over, pen(300.0, 300.0, 2.0), Tool::Select), "outside it");
    });
}

/// The kinds of the items on `page` of `notes` (`None`: the first).
fn kinds(daemon: &Client, page: Option<&str>) -> Vec<String> {
    let items = daemon.canvas_show("notes", page).unwrap().items;
    items.iter().map(|i| i["kind"].as_str().unwrap_or_default().to_string()).collect()
}

#[test]
fn a_carry_to_another_page_of_the_same_canvas_is_one_undo_step() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let (mut canvases, panes) = two_panes(daemon);
        let [(left, a), (right, b)] = frames(&panes)[..] else { panic!() };
        assert!(canvases.command(&panes, (right, b), crate::commands::Command::NewPage));
        let mut p2 = None;
        wait_until(|| {
            scenes(&mut canvases, &panes);
            p2 = canvases.pages_of(&panes, right).map(|(_, _, page)| page).filter(|p| p != "p1");
            p2.is_some() && drawn(&mut canvases, &panes, right).is_empty()
        });
        let p2 = p2.unwrap();
        let tool = (Tool::Select, GREY);
        assert!(canvases.pen_down(&panes, (left, a), pen(121.0, 121.0, 1.0), tool));
        let to = (480.0 + 121.0 + 100.0, 121.0);
        assert!(canvases.pen_drag(&panes, pen(to.0, to.1, 1.1), under(&panes, to.0, to.1)));
        assert!(!covers(&mut canvases, &panes, left, (121.0, 121.0)), "gone from its page");
        assert!(covers(&mut canvases, &panes, right, to), "and under the pen there");
        canvases.pen_up(&panes, pen(to.0, to.1, 1.2), under(&panes, to.0, to.1));
        assert!(covers(&mut canvases, &panes, right, to), "no jump on the lift");
        wait_until(|| {
            scenes(&mut canvases, &panes);
            kinds(daemon, Some("p1")).is_empty() && kinds(daemon, Some(&p2)) == ["stroke"]
        });
        wait_until(|| {
            scenes(&mut canvases, &panes);
            canvases.selection().is_some_and(|s| s.page == p2 && s.ids.len() == 1)
        });
        assert!(covers(&mut canvases, &panes, right, to), "selected where it landed");
        assert!(!covers(&mut canvases, &panes, left, (121.0, 121.0)));
        assert!(canvases.command(&panes, (left, a), crate::commands::Command::Undo));
        wait_until(|| {
            scenes(&mut canvases, &panes);
            kinds(daemon, Some("p1")) == ["stroke"]
        });
        assert_eq!(kinds(daemon, Some(&p2)), Vec::<String>::new(), "one step: not on both pages");
        wait_until(|| {
            scenes(&mut canvases, &panes);
            canvases.selection().is_none()
        });
    });
}

#[test]
fn a_second_press_soon_after_selecting_a_text_drags_it() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let ops = serde_json::json!({"op": "add", "item": {"page": "p1", "kind": "text",
            "color": "#000000", "x": 1000, "y": 1000, "text": "hello"}});
        let by = "user".to_string();
        daemon
            .canvas_apply(slinger_control::CanvasOps { canvas: "notes".into(), by, ops })
            .unwrap();
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), split());
        wait_until(|| {
            let all = scenes(&mut canvases, &panes);
            all.len() == 2 && all.iter().all(|s| s.scene.draws.len() == 1)
        });
        let [(left, a), _] = frames(&panes)[..] else { panic!() };
        let tool = (Tool::Select, GREY);
        assert!(canvases.pen_down(&panes, (left, a), pen(122.0, 125.0, 1.0), tool));
        canvases.pen_up(&panes, pen(122.0, 125.0, 1.05), Some((left, a)));
        let left_drawn = drawn(&mut canvases, &panes, left);
        assert!(canvases.selection().is_some(), "the tap selects the text: {left_drawn:?}");
        assert!(canvases.pen_down(&panes, (left, a), pen(122.0, 125.0, 1.2), tool));
        assert!(canvases.pen_drag(&panes, pen(222.0, 125.0, 1.3), Some((left, a))));
        canvases.pen_up(&panes, pen(222.0, 125.0, 1.4), Some((left, a)));
        assert!(!canvases.editing(), "a drag, not a double tap");
        wait_until(|| {
            scenes(&mut canvases, &panes);
            let items = daemon.canvas_show("notes", None).unwrap().items;
            items[0]["x"].as_f64() == Some(2000.0)
        });
    });
}
