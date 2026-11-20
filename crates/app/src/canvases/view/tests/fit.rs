//! A view's first place (slinger-ff9dbe): a page whose content the start
//! does not show opens fitted to it, once; zoom-to-fit fits it again.

use crate::backend::testing::wait_until;
use crate::backend::tests::{open_notes, with_store};
use crate::canvases::Canvases;
use crate::commands::Command;
use crate::config::Config;
use crate::layout::{Point, Rect, Size, Viewport, WINDOW};
use crate::panes::{Kind, Pane, Panes};
use crate::scene::CanvasPane;
use slinger_control::{CanvasOps, Client};

fn notes() -> Panes {
    Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() })
}

/// Adds a box at `x`, `y` points, `w` by `h`, to the first page of notes
/// (the file counts tenths of a point).
fn add_box(daemon: &Client, (x, y, w, h): (f64, f64, f64, f64)) {
    #[expect(clippy::cast_possible_truncation, reason = "whole points in the tests")]
    let units = |v: f64| (v * 10.0) as i64;
    let item = serde_json::json!({"page": "p1", "kind": "box", "color": "#000000",
        "x": units(x), "y": units(y), "w": units(w), "h": units(h)});
    let ops = serde_json::json!({"op": "add", "item": item});
    let (canvas, by) = ("notes".to_string(), "user".to_string());
    daemon.canvas_apply(CanvasOps { canvas, by, ops, once: None }).unwrap();
}

/// The one pane's scene once its page has come.
fn shown(canvases: &mut Canvases, panes: &Panes) -> CanvasPane {
    let mut scenes = Vec::new();
    wait_until(|| {
        canvases.poll();
        scenes = canvases.show(panes, WINDOW.rect(), &Config::default().theme);
        canvases.paged(panes, WINDOW.rect()).is_some() && !scenes.is_empty()
    });
    scenes.remove(0)
}

/// Whether the pane shows all of the canvas box `r`.
fn in_view(pane: &CanvasPane, r: Rect) -> bool {
    let size = Size { width: pane.frame.width, height: pane.frame.height };
    let seen = pane.scene.viewport.visible(size);
    let corners = [Point { x: r.x, y: r.y }, Point { x: r.x + r.width, y: r.y + r.height }];
    corners.iter().all(|&p| seen.contains(p))
}

const FAR: (f64, f64, f64, f64) = (5000.0, 4000.0, 200.0, 100.0);
const FAR_BOX: Rect = Rect { x: 5000.0, y: 4000.0, width: 200.0, height: 100.0 };

#[test]
fn a_page_whose_items_are_far_from_the_origin_opens_with_them_in_view_once() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        add_box(daemon, FAR);
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
        let first = shown(&mut canvases, &panes);
        assert!(in_view(&first, FAR_BOX), "{:?}", first.scene.viewport);
        assert_eq!(first.scene.viewport.zoom(), 1.0, "small content: 100%");

        canvases.pan(panes.focus(), -3000.0, 0.0);
        let panned = shown(&mut canvases, &panes);
        assert!(!in_view(&panned, FAR_BOX), "a later show does not fit again");
        add_box(daemon, (9000.0, 9000.0, 10.0, 10.0));
        wait_until(|| {
            canvases.poll();
            let theme = Config::default().theme;
            let scene = canvases.show(&panes, WINDOW.rect(), &theme).remove(0).scene;
            assert_eq!(scene.viewport, panned.scene.viewport, "a change moves nothing");
            let c = &canvases;
            c.follow.pane_shown(&c.ink, &panes, panes.focus()).len() == 2
        });

        let pane = (panes.focus(), WINDOW.rect());
        assert!(canvases.command(&panes, pane, Command::ZoomToFit));
        let both = Rect { x: 5000.0, y: 4000.0, width: 4010.0, height: 5010.0 };
        let fitted = shown(&mut canvases, &panes);
        assert!(in_view(&fitted, both), "zoom-to-fit brings all of it back");
        assert!(fitted.scene.viewport.zoom() < 1.0);
        assert_eq!(canvases.said(), Some(fitted.scene.viewport.zoom_label()));
    });
}

#[test]
fn an_empty_page_opens_at_the_start_and_so_does_its_zoom_to_fit() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
        assert_eq!(shown(&mut canvases, &panes).scene.viewport, Viewport::start());
        canvases.pan(panes.focus(), 40.0, 40.0);
        assert!(canvases.command(&panes, (panes.focus(), WINDOW.rect()), Command::ZoomToFit));
        assert_eq!(shown(&mut canvases, &panes).scene.viewport, Viewport::start());
    });
}

#[test]
fn a_pan_or_zoom_before_the_page_comes_keeps_the_start() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        add_box(daemon, FAR);
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
        canvases.zoom(panes.focus(), Point { x: 0.0, y: 0.0 }, 2.0);
        let zoomed = Viewport::start().zoom_to(Point { x: 0.0, y: 0.0 }, 2.0);
        assert_eq!(shown(&mut canvases, &panes).scene.viewport, zoomed, "placed by the user");
    });
}
