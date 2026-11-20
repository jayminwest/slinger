//! Stepping a pane into a box (slinger-b1ccf1, docs/design/fold.md, "Step
//! in"): `step-in` (cmd+alt+i) shows only what the selected box holds,
//! escape steps out, a stroke drawn inside a folded box joins it, and
//! `step-in-beside` (cmd+alt+shift+i) splits the pane to step in.

use super::*;
use crate::commands::{ALT, SHIFT};
use crate::scene::canvas::Draw;

const STEP_IN: Chord = Chord::new(CMD | ALT, 'i');
const BESIDE: Chord = Chord::new(CMD | ALT | SHIFT, 'i');
const FOLD: Chord = Chord::new(CMD | ALT, 'f');
const ESCAPE: Chord = Chord::new(0, '\u{1b}');
/// The dashed frame round the room a pane is inside (`canvases::step`).
const ROOM: u64 = (1 << 60) - 2;

fn apply(daemon: &control::Client, item: &serde_json::Value) {
    let ops = serde_json::json!({"op": "add", "item": item});
    let (canvas, by) = ("notes".to_string(), "user".to_string());
    daemon.canvas_apply(control::CanvasOps { canvas, by, ops, once: None }).unwrap();
}

fn stroke(x: i64, y: i64) -> serde_json::Value {
    serde_json::json!({"page": "p1", "kind": "stroke", "color": "#000000", "width": 20,
        "pts": [x, y, 100, 100]})
}

/// Box `i1` from (100, 100), 400 by 300; stroke `i2` inside it, `i3` outside.
fn page(daemon: &control::Client) {
    apply(
        daemon,
        &serde_json::json!({"page": "p1", "kind": "box", "color": "#000000",
            "x": 1000, "y": 1000, "w": 4000, "h": 3000}),
    );
    apply(daemon, &stroke(3000, 3000));
    apply(daemon, &stroke(5500, 500));
}

/// The ids the first pane draws.
fn drawn(w: &mut World) -> Vec<u64> {
    let area = w.sidebar.panes(w.shared.size.rect());
    let theme = w.settings.config.theme;
    let panes = w.canvases.show(&w.shared.panes, area, &theme);
    panes[0].scene.draws.iter().map(|d: &Draw| d.id.0).collect()
}

/// Ticks until the first pane draws what `met` wants.
fn settle(w: &mut World, met: impl Fn(&[u64]) -> bool) {
    wait_until(|| {
        w.send(Event::Tick);
        met(&drawn(w))
    });
}

/// Selects all (cmd+a) until `n` items are selected.
fn select_all(w: &mut World, n: usize) {
    wait_until(|| {
        w.send(Event::Tick);
        w.send(Event::Key(Chord::new(CMD, 'a')));
        w.canvases.selection().is_some_and(|s| s.ids.len() == n)
    });
}

fn held(daemon: &control::Client) -> serde_json::Value {
    let items = daemon.canvas_show("notes", None).unwrap().items;
    items
        .iter()
        .find(|i| i["id"] == "i1")
        .map_or(serde_json::Value::Null, |b| b["fold"]["items"].clone())
}

#[test]
fn step_in_shows_only_the_box_escape_steps_out_and_a_stroke_inside_a_folded_box_joins_it() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        page(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        settle(&mut w, |d| d.contains(&3));
        w.send(Event::Key(STEP_IN));
        assert!(drawn(&mut w).contains(&3), "nothing selected: nothing to step into");

        select_all(&mut w, 3);
        assert!(w.send(Event::Key(STEP_IN)));
        let inside = drawn(&mut w);
        assert!(inside.contains(&2) && inside.contains(&ROOM), "{inside:?}");
        assert!(!inside.contains(&1) && !inside.contains(&3), "only what the box holds");
        assert!(w.send(Event::Key(ESCAPE)), "nothing selected: escape steps out");
        settle(&mut w, |d| d.contains(&1) && d.contains(&3) && !d.contains(&ROOM));
        assert!(!w.send(Event::Key(ESCAPE)), "out of every box: escape passes");

        // Fold the box over the stroke, step into it, and draw there.
        let ops = serde_json::json!({"op": "remove", "ids": ["i3"]});
        let (canvas, by) = ("notes".to_string(), "user".to_string());
        daemon.canvas_apply(control::CanvasOps { canvas, by, ops, once: None }).unwrap();
        select_all(&mut w, 2);
        assert!(w.send(Event::Key(FOLD)));
        w.until(|_| held(daemon) == serde_json::json!(["i2"]));
        select_all(&mut w, 1);
        assert!(w.send(Event::Key(STEP_IN)));
        settle(&mut w, |d| d.contains(&2) && !d.contains(&1));
        w.key(Chord::new(CMD, '1'), "1");
        assert!(w.send(Event::Press(at(300.0, 300.0, 10.0))));
        w.send(Event::Drag(at(340.0, 330.0, 10.1)));
        w.send(Event::Lift(at(360.0, 340.0, 10.2)));
        w.until(|_| held(daemon).as_array().is_some_and(|h| h.len() == 2));
        assert_eq!(count(daemon, "stroke"), 2, "the stroke is on the page, held by the box");
    });
}

#[test]
fn step_in_beside_splits_the_pane_and_steps_the_new_one_in() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        page(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        select_all(&mut w, 3);
        let first = w.shared.panes.focus();
        assert!(w.send(Event::Key(BESIDE)));
        let area = w.sidebar.panes(w.shared.size.rect());
        assert_eq!(w.shared.panes.place(area).len(), 2);
        assert_ne!(w.shared.panes.focus(), first, "the new pane has focus");
        let theme = w.settings.config.theme;
        let panes = w.canvases.show(&w.shared.panes, area, &theme);
        let ids = |n: usize| panes[n].scene.draws.iter().map(|d| d.id.0).collect::<Vec<_>>();
        assert!(ids(0).contains(&1), "the first pane shows the page: {:?}", ids(0));
        assert!(ids(1).contains(&ROOM) && !ids(1).contains(&1), "the new one, the box");
    });
}
