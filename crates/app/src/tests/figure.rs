//! Figures in the window (slinger-dd5add, docs/design/figures.md, "Lock"):
//! the pen writes over a locked figure as on paper, the outline says it is
//! locked, `unlock-figure` grabs the figure under the pointer (from its
//! key or a context click on it), a duplicate of the grab is a new figure,
//! and escape locks it again.

use serde_json::json;

use super::step::apply;
use super::*;
use crate::commands::ALT;
use crate::commands::context::Subject;
use crate::layout::Rect;
use crate::scene::canvas::ItemId;

const UNLOCK: Chord = Chord::new(CMD | ALT, 'u');

/// Figure `sheet`: box `i1` from (100, 100), 400 by 300, part `q1`, and
/// text `i2` in it; a free box `i3` beside it.
fn page(daemon: &control::Client) {
    apply(
        daemon,
        &json!({"page": "p1", "kind": "box", "color": "#8a5cf5", "x": 1000, "y": 1000,
            "w": 4000, "h": 3000, "locked": true, "fig": "sheet", "part": "q1"}),
    );
    apply(
        daemon,
        &json!({"page": "p1", "kind": "text", "color": "#8a5cf5", "x": 1500, "y": 1500,
            "text": "1a", "locked": true, "fig": "sheet"}),
    );
    apply(
        daemon,
        &json!({"page": "p1", "kind": "box", "color": "#000000", "x": 6000, "y": 1000,
            "w": 2000, "h": 2000}),
    );
}

/// Where the one pane draws item `id`, once it does.
fn bounds(w: &mut World, id: u64) -> Rect {
    let mut found = None;
    wait_until(|| {
        w.send(Event::Tick);
        let area = w.sidebar.panes(w.shared.size.rect());
        let theme = w.settings.config.theme;
        let panes = w.canvases.show(&w.shared.panes, area, &theme);
        found = panes[0].scene.draws.iter().find(|d| d.id == ItemId(id)).map(|d| d.bounds);
        found.is_some()
    });
    found.unwrap()
}

fn said(w: &mut World) -> String {
    let scene = view(&w.settings, &mut w.shared, &w.tools, &w.sidebar, &mut w.canvases);
    crate::drive::outline(&scene)
}

fn selected(w: &World) -> Vec<u64> {
    w.canvases.selection().map(|s| s.ids.iter().map(|i| i.0).collect()).unwrap_or_default()
}

fn items(daemon: &control::Client) -> Vec<serde_json::Value> {
    daemon.canvas_show("notes", None).unwrap().items
}

#[test]
fn the_pen_eraser_and_lasso_pass_over_a_figure_and_the_outline_says_it_is_locked() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        page(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        let sheet = bounds(&mut w, 1);
        let out = said(&mut w);
        let line = |id: &str| out.lines().find(|l| l.starts_with(&format!("  {id} "))).unwrap();
        assert!(line("i1").ends_with(" locked"), "{out}");
        assert!(!line("i3").ends_with(" locked"), "{out}");

        // The pen writes across the figure's box as on empty paper.
        let (x, y) = (sheet.x - 20.0, sheet.y + sheet.height / 2.0);
        assert!(w.send(Event::Press(at(x, y, 1.0))));
        w.send(Event::Drag(at(x + 60.0, y, 1.1)));
        w.send(Event::Lift(at(x + 60.0, y, 1.2)));
        w.until(|_| count(daemon, "stroke") == 1);

        // The eraser across the box's edge takes the ink only.
        w.key(Chord::new(CMD, '2'), "2");
        assert_eq!(w.tools.tool(), Tool::Eraser);
        w.send(Event::Press(at(x + 10.0, y - 10.0, 2.0)));
        w.send(Event::Drag(at(x + 30.0, y + 10.0, 2.1)));
        w.send(Event::Lift(at(x + 30.0, y + 10.0, 2.2)));
        w.until(|_| count(daemon, "stroke") == 0);
        assert_eq!(count(daemon, "box"), 2, "the figure's box stays");

        // A lasso round everything takes the free box only.
        w.key(Chord::new(CMD, '3'), "3");
        let around = [(5.0, 5.0), (900.0, 5.0), (900.0, 600.0), (5.0, 600.0), (5.0, 5.0)];
        w.send(Event::Press(at(around[0].0, around[0].1, 3.0)));
        for (n, &(x, y)) in around.iter().enumerate().skip(1) {
            w.send(Event::Drag(at(x, y, 3.0 + 0.1 * f64::from(u8::try_from(n).unwrap()))));
        }
        w.send(Event::Lift(at(5.0, 5.0, 3.9)));
        assert_eq!(selected(&w), [3]);
    });
}

#[test]
fn unlock_grabs_the_figure_under_the_pointer_and_its_duplicate_is_a_new_figure() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        page(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        let sheet = bounds(&mut w, 1);

        // Nothing locked under the pointer: the command is grey and flashes why.
        w.send(Event::Moved(Point { x: sheet.x + sheet.width + 40.0, y: sheet.y }));
        w.send(Event::Key(UNLOCK));
        assert!(said(&mut w).contains("No figure under the pointer"));
        assert_eq!(selected(&w), [] as [u64; 0]);

        w.send(Event::Moved(Point { x: sheet.x + 300.0, y: sheet.y + 250.0 }));
        let grey = |w: &World| w.offers(&Subject::Figure)[0].enabled;
        assert!(grey(&w), "a locked item is under the pointer");
        w.send(Event::Key(UNLOCK));
        assert_eq!(selected(&w), [1, 2], "the whole figure");

        w.key(Chord::new(CMD, 'j'), "");
        w.until(|_| items(daemon).len() == 5);
        let copies: Vec<_> = items(daemon).into_iter().filter(|i| i["fig"] == "sheet-2").collect();
        assert_eq!(copies.len(), 2, "a new figure, its own name");
        assert!(copies.iter().all(|i| i["locked"] == true));
        assert_eq!(copies[0]["part"], "q1", "its parts kept");
        w.until(|w| {
            w.canvases.selection().is_some_and(|s| s.ids.len() == 2 && !s.ids.contains(&ItemId(1)))
        });

        w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        assert_eq!(w.canvases.selection(), None, "escape locks it again");
    });
}

#[test]
fn a_context_click_on_a_figure_offers_unlock_first_and_its_pick_grabs_it() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        page(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        let sheet = bounds(&mut w, 1);
        let popup = w.context(sheet.x, sheet.y + sheet.height / 2.0).expect("a menu");
        assert_eq!(popup.subject, Subject::Figure);
        assert_eq!(super::context::titles(&popup)[0], "Unlock figure");
        assert_eq!(selected(&w), [] as [u64; 0], "the click selects nothing");
        assert!(w.send(Event::Picked(Command::UnlockFigure, Subject::Figure)), "the pick");
        assert_eq!(selected(&w), [1, 2]);
    });
}
