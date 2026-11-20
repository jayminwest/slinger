use super::*;
use crate::layout::Point;

fn pane(n: u32) -> PaneId {
    PaneId::numbered(n)
}

fn panned(views: &mut Views, n: u32, canvas: &str, by: f64) {
    let view = views.get(pane(n));
    view.on(canvas);
    view.viewport = view.viewport.pan(by, 0.0);
}

fn origin(views: &mut Views, n: u32) -> Point {
    views.get(pane(n)).viewport.to_canvas(Point { x: 0.0, y: 0.0 })
}

#[test]
fn a_view_of_another_canvas_starts_again() {
    let mut view = View::default();
    view.on("a");
    view.viewport = view.viewport.pan(40.0, 0.0);
    view.page = Some(("a".into(), "p2".into()));
    view.on("a");
    assert!(view.page.is_some(), "the same canvas: it stays");
    view.on("b");
    assert!(view.page.is_none() && view.last.is_none());
    assert_eq!(view.viewport, Viewport::start());
}

#[test]
fn each_workspace_has_its_own_views_and_gets_them_back() {
    let mut views = Views::default();
    assert!(views.within(Some("a")));
    assert!(!views.within(Some("a")), "the same workspace");
    panned(&mut views, 1, "main", 40.0);
    let left = origin(&mut views, 1);
    assert!(views.within(Some("b")));
    assert_eq!(origin(&mut views, 1), origin(&mut Views::default(), 1), "pane 1 of b starts new");
    panned(&mut views, 1, "notes", -10.0);
    assert!(views.within(Some("a")));
    assert_eq!(origin(&mut views, 1), left, "a's pane is where it was left");
    assert!(views.within(None), "the window's own panes are one more workspace");
    assert!(views.iter().all(|(_, v)| v.page.is_none()));
}

#[test]
fn keep_forgets_the_panes_that_closed_only() {
    let mut views = Views::default();
    panned(&mut views, 1, "main", 10.0);
    panned(&mut views, 2, "main", 30.0);
    let second = origin(&mut views, 2);
    views.keep(&[pane(2)]);
    assert_eq!(views.len(), 1);
    assert_eq!(origin(&mut views, 2), second, "the other pane keeps its view");
}

#[test]
fn a_turn_finds_the_panes_view_of_that_canvas_shown_or_parked() {
    let mut views = Views::default();
    views.within(Some("a"));
    panned(&mut views, 1, "main", 0.0);
    views.within(Some("b"));
    panned(&mut views, 1, "notes", 0.0);
    views.turn(pane(1), "main".into(), "p2".into());
    assert_eq!(views.get(pane(1)).page, None, "b's pane shows another canvas");
    views.turn(pane(2), "notes".into(), "p3".into());
    assert_eq!(views.get(pane(2)).page, Some(("notes".into(), "p3".into())), "a pane not seen yet");
    views.turn(pane(1), "gone".into(), "p9".into());
    views.within(Some("a"));
    assert_eq!(views.get(pane(1)).page, Some(("main".into(), "p2".into())), "a's, parked");
}

mod fit;

mod shown {
    use slinger_control::{Pane as Shown, PaneKind, PaneSplit, PaneTree};

    use super::pane;
    use crate::backend::testing::wait_until;
    use crate::backend::tests::{open_notes, with_store};
    use crate::canvases::Canvases;
    use crate::config::Config;
    use crate::layout::{Viewport, WINDOW};
    use crate::panes::Panes;
    use crate::scene::CanvasPane;

    /// `workspace`'s canvas panes, side by side, as the server sends them:
    /// each an id and a canvas.
    fn laid(workspace: &str, panes: &[(u32, &str)]) -> Panes {
        let leaf = |(at, &(id, canvas)): (usize, &(u32, &str))| {
            let pane = u32::try_from(at + 1).unwrap();
            PaneTree::Pane(Shown { pane, id, kind: PaneKind::Canvas, target: canvas.into() })
        };
        let mut leaves = panes.iter().enumerate().map(leaf).rev();
        let last = leaves.next().unwrap();
        let tree = leaves.fold(last, |second, first| PaneTree::Split {
            split: PaneSplit::Row,
            ratio: 0.5,
            first: first.into(),
            second: second.into(),
        });
        Panes::follow(&tree, 1).of(workspace)
    }

    fn show(canvases: &mut Canvases, panes: &Panes) -> Vec<CanvasPane> {
        canvases.poll();
        canvases.show(panes, WINDOW.rect(), &Config::default().theme)
    }

    fn viewport(scenes: &[CanvasPane], n: u32) -> Option<Viewport> {
        scenes.iter().find(|s| s.pane == pane(n)).map(|s| s.scene.viewport)
    }

    #[test]
    fn a_switch_shows_nothing_of_the_workspace_left_which_comes_back_as_it_was_left() {
        with_store(None, |daemon| {
            let _home = open_notes(daemon);
            let mut canvases = Canvases::new(daemon.clone());
            let a = laid("a", &[(1, "notes"), (2, "notes")]);
            wait_until(|| show(&mut canvases, &a).len() == 2);
            canvases.pan(pane(2), 30.0, 0.0);
            canvases.zoom(pane(2), crate::layout::Point { x: 0.0, y: 0.0 }, 2.0);
            let left = viewport(&show(&mut canvases, &a), 2).unwrap();
            assert_ne!(left, Viewport::start());

            let b = laid("b", &[(1, "elsewhere"), (2, "elsewhere")]);
            assert!(show(&mut canvases, &b).is_empty(), "nothing of a's canvas while b's comes");
            assert_eq!(canvases.view(pane(2)).viewport, Viewport::start(), "nor its pan or zoom");

            let back = show(&mut canvases, &a);
            assert_eq!(viewport(&back, 2), Some(left), "a draws as left before its page comes");
            wait_until(|| {
                show(&mut canvases, &a);
                canvases.paged(&a, WINDOW.rect()).is_some()
            });
            assert_eq!(viewport(&show(&mut canvases, &a), 2), Some(left));

            let opened = laid("a", &[(1, "notes"), (3, "notes"), (2, "notes")]);
            wait_until(|| show(&mut canvases, &opened).len() == 3);
            assert_eq!(viewport(&show(&mut canvases, &opened), 2), Some(left), "a pane opened");
            let closed = laid("a", &[(2, "notes")]);
            assert_eq!(viewport(&show(&mut canvases, &closed), 2), Some(left), "the first closed");
            let other = laid("a", &[(2, "elsewhere")]);
            assert!(show(&mut canvases, &other).is_empty(), "another canvas: nothing of notes");
            assert_eq!(canvases.view(pane(2)).viewport, Viewport::start());
        });
    }
}

#[test]
fn a_copy_takes_the_page_and_the_place_and_none_from_a_pane_with_no_view() {
    let mut views = Views::default();
    panned(&mut views, 1, "main", 40.0);
    views.get(pane(1)).page = Some(("main".into(), "p3".into()));
    views.copy(pane(1), pane(2));
    assert_eq!(origin(&mut views, 2), origin(&mut views, 1));
    let copy = views.get(pane(2));
    assert!(copy.placed && copy.last.is_none(), "placed, nothing drawn yet");
    assert_eq!(copy.page, Some(("main".into(), "p3".into())));
    views.copy(pane(9), pane(2));
    views.copy(pane(2), pane(2));
    assert_eq!(views.get(pane(2)).page, Some(("main".into(), "p3".into())), "kept");
}
