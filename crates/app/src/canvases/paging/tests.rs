//! Pages per pane with two panes on one canvas (slinger-352a6b).

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

/// How many items each pane draws (not the marks over them), left first,
/// once the panes take in their news.
fn counts(canvases: &mut Canvases, panes: &Panes) -> Vec<usize> {
    canvases.poll();
    let scenes = canvases.show(panes, WINDOW.rect(), &Config::default().theme);
    scenes.iter().map(|s| s.scene.draws.iter().filter(|d| d.id.0 < MARKS).count()).collect()
}

/// The page `pane` shows, and how many pages its canvas has.
fn page(canvases: &Canvases, panes: &Panes, pane: PaneId) -> Option<(String, usize)> {
    canvases.pages_of(panes, pane).map(|(_, list, page)| (page, list.len()))
}

/// How many pages of `notes` are followed.
fn followed(canvases: &Canvases) -> usize {
    canvases.open.iter().filter(|o| o.canvas == "notes").count()
}

/// The kinds of the items on `page` of `notes`.
fn kinds(daemon: &Client, page: &str) -> Vec<String> {
    let items = daemon.canvas_show("notes", Some(page)).unwrap().items;
    items.iter().map(|i| i["kind"].as_str().unwrap_or_default().to_string()).collect()
}

/// A pen stroke in `pane`, inside its `frame`.
fn draw(canvases: &mut Canvases, panes: &Panes, (pane, frame): (PaneId, Rect)) {
    let (x, y) = (frame.x + 200.0, frame.y + 200.0);
    let into = Some((pane, frame));
    assert!(canvases.pen_down(panes, (pane, frame), pen(x, y, 1.0), (Tool::Pen, GREY)));
    assert!(canvases.pen_drag(panes, pen(x + 20.0, y + 20.0, 1.1), into));
    assert!(canvases.pen_up(panes, pen(x + 40.0, y + 10.0, 1.2), into));
}

#[test]
fn each_pane_keeps_its_own_page_and_a_page_shown_twice_is_followed_once() {
    use crate::commands::Command;
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        user_stroke(daemon, "notes");
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), split());
        let [(left, a), (right, b)] = frames(&panes)[..] else { panic!() };
        wait_until(|| counts(&mut canvases, &panes) == [1, 1]);
        assert_eq!(followed(&canvases), 1, "one page in two panes: followed once");

        // A new page from the right pane turns that pane alone.
        assert!(canvases.command(&panes, (right, b), Command::NewPage));
        let mut p2 = String::new();
        wait_until(|| {
            let shown = counts(&mut canvases, &panes);
            let to = page(&canvases, &panes, right).filter(|(p, _)| p != "p1");
            p2 = to.map(|(p, _)| p).unwrap_or_default();
            !p2.is_empty() && shown == [1, 0]
        });
        assert_eq!(page(&canvases, &panes, left).map(|(p, _)| p).as_deref(), Some("p1"));
        assert_eq!(followed(&canvases), 2, "two pages shown: each followed");

        // The pen in the right pane draws on its page, not on the left's.
        draw(&mut canvases, &panes, (right, b));
        wait_until(|| {
            counts(&mut canvases, &panes);
            kinds(daemon, &p2) == ["stroke"]
        });
        assert_eq!(kinds(daemon, "p1"), ["stroke"], "the left pane's page is untouched");
        assert_eq!(counts(&mut canvases, &panes), [1, 1]);

        // Turning the left pane brings it to the right's page: followed once.
        wait_until(|| {
            counts(&mut canvases, &panes);
            page(&canvases, &panes, left).is_some_and(|(_, n)| n == 2)
        });
        assert!(canvases.command(&panes, (left, a), Command::NextPage));
        wait_until(|| {
            let shown = counts(&mut canvases, &panes);
            page(&canvases, &panes, left).is_some_and(|(p, _)| p == p2) && shown == [1, 1]
        });
        assert_eq!(followed(&canvases), 1, "one page in two panes: followed once");

        // Undo is the canvas's: pressed in the left pane, it takes back the
        // stroke drawn in the right one, and both panes show it gone.
        assert!(canvases.command(&panes, (left, a), Command::Undo));
        wait_until(|| counts(&mut canvases, &panes) == [0, 0] && kinds(daemon, &p2).is_empty());

        // Removing the page (empty: no question) moves every pane on it.
        assert!(canvases.command(&panes, (right, b), Command::RemovePage));
        wait_until(|| {
            let shown = counts(&mut canvases, &panes);
            let on_p1 = |pane| page(&canvases, &panes, pane) == Some(("p1".to_string(), 1));
            shown == [1, 1] && on_p1(left) && on_p1(right)
        });
        assert_eq!(followed(&canvases), 1);
        assert_eq!(daemon.canvas_show("notes", None).unwrap().pages.len(), 1);
    });
}

/// slinger-5e2587: removing a page with items says that undo brings it
/// back, and it does: the page and its items return, also after the app
/// starts again (the daemon keeps the history).
#[test]
fn a_removed_page_says_undo_brings_it_back_and_undo_does_after_a_restart() {
    use crate::commands::Command;
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let panes = Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() });
        let [(pane, frame)] = frames(&panes)[..] else { panic!() };
        let mut canvases = Canvases::new(daemon.clone());
        wait_until(|| {
            counts(&mut canvases, &panes);
            page(&canvases, &panes, pane).is_some()
        });
        assert!(canvases.command(&panes, (pane, frame), Command::NewPage));
        let mut p2 = String::new();
        wait_until(|| {
            counts(&mut canvases, &panes);
            p2 = page(&canvases, &panes, pane)
                .map(|(p, _)| p)
                .filter(|p| p != "p1")
                .unwrap_or_default();
            !p2.is_empty()
        });
        canvases.said();
        draw(&mut canvases, &panes, (pane, frame));
        wait_until(|| counts(&mut canvases, &panes) == [1] && kinds(daemon, &p2) == ["stroke"]);

        // It has an item: the first press asks, the second removes it.
        assert!(canvases.command(&panes, (pane, frame), Command::RemovePage));
        assert_eq!(canvases.said().as_deref(), Some("Remove “Page 2”? Undo brings it back"));
        assert!(canvases.command(&panes, (pane, frame), Command::RemovePage));
        assert_eq!(canvases.said().as_deref(), Some("Removed: Page 2 · Undo brings it back"));
        wait_until(|| daemon.canvas_show("notes", None).unwrap().pages.len() == 1);

        // The app starts again, and undo brings the page and its stroke back.
        drop(canvases);
        let mut canvases = Canvases::new(daemon.clone());
        wait_until(|| {
            counts(&mut canvases, &panes);
            page(&canvases, &panes, pane).is_some()
        });
        assert!(canvases.command(&panes, (pane, frame), Command::Undo));
        wait_until(|| {
            counts(&mut canvases, &panes);
            let back = page(&canvases, &panes, pane).is_some_and(|(_, n)| n == 2);
            back && kinds(daemon, &p2) == ["stroke"]
        });
    });
}
