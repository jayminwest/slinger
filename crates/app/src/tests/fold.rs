//! Folding a box from the keyboard (slinger-1350e7, docs/design/fold.md):
//! `fold` (cmd+alt+f) folds the selected box to a miniature that draws
//! what it holds, and unfolds it; an unfold into a taken room peeks, and
//! escape or a press outside closes the peek, writing nothing.

use super::*;
use crate::commands::ALT;
use crate::layout::Rect;
use crate::scene::canvas::{Draw, ItemId};

const FOLD: Chord = Chord::new(CMD | ALT, 'f');
const ESCAPE: Chord = Chord::new(0, '\u{1b}');
/// The fold marks' draw ids start here (`canvases::fold`).
const MARK: u64 = 1 << 60;

fn apply(daemon: &control::Client, item: &serde_json::Value) {
    let ops = serde_json::json!({"op": "add", "item": item});
    let (canvas, by) = ("notes".to_string(), "user".to_string());
    daemon.canvas_apply(control::CanvasOps { canvas, by, ops, once: None }).unwrap();
}

/// A stroke on the first page from (`x`, `y`) to 10 units down and right, in tenths.
fn stroke(x: i64, y: i64) -> serde_json::Value {
    serde_json::json!({"page": "p1", "kind": "stroke", "color": "#000000", "width": 20,
        "pts": [x, y, 100, 100]})
}

/// The `fold` of box `i1` on the daemon, `null` when open.
fn fold(daemon: &control::Client) -> serde_json::Value {
    let items = daemon.canvas_show("notes", None).unwrap().items;
    items.iter().find(|i| i["id"] == "i1").map_or(serde_json::Value::Null, |b| b["fold"].clone())
}

/// What the one pane draws now.
fn draws(w: &mut World) -> Vec<Draw> {
    let area = w.sidebar.panes(w.shared.size.rect());
    let theme = w.settings.config.theme;
    let panes = w.canvases.show(&w.shared.panes, area, &theme);
    panes[0].scene.draws.clone()
}

fn drawn(w: &mut World, id: u64) -> Option<Draw> {
    draws(w).into_iter().find(|d| d.id == ItemId(id))
}

fn has(draws: &[Draw], id: u64) -> bool {
    draws.iter().any(|d| d.id == ItemId(id))
}

/// Ticks until what the pane draws meets `met`; returns it.
fn settle(w: &mut World, met: impl Fn(&[Draw]) -> bool) -> Vec<Draw> {
    let mut last = Vec::new();
    wait_until(|| {
        w.send(Event::Tick);
        last = draws(w);
        met(&last)
    });
    last
}

/// Whether the pane shows a peek: its paper, just below the marks' ids.
fn peeking(w: &mut World) -> bool {
    drawn(w, MARK - 1).is_some()
}

/// A box from (100, 100), 400 by 300: item `i1` on an empty canvas.
fn put_box(daemon: &control::Client) {
    apply(
        daemon,
        &serde_json::json!({"page": "p1", "kind": "box", "color": "#000000",
            "x": 1000, "y": 1000, "w": 4000, "h": 3000}),
    );
}

/// Selects all (cmd+a) until `n` items are selected.
fn select_all(w: &mut World, n: usize) {
    wait_until(|| {
        w.send(Event::Tick);
        w.send(Event::Key(Chord::new(CMD, 'a')));
        w.canvases.selection().is_some_and(|s| s.ids.len() == n)
    });
}

/// `fold` with box `i1` and stroke `i2` selected, the stroke drawn in
/// `open`: the box folds, and the stroke draws in its miniature.
fn folds(w: &mut World, daemon: &control::Client, open: Rect) {
    assert!(w.send(Event::Key(FOLD)), "the box is selected: fold acts");
    w.until(|_| fold(daemon)["items"] == serde_json::json!(["i2"]));
    settle(w, |d| has(d, MARK + 1));
    let mini = drawn(w, 2).expect("the stroke draws in the miniature");
    assert!(mini.bounds.width < open.width / 2.0, "{mini:?} is smaller than {open:?}");
    assert!(mini.bounds.x < open.x && mini.bounds.y < open.y, "toward the box's top left");
    let boxed = drawn(w, 1).unwrap().bounds;
    assert!(boxed.width < 110.0, "the box is its miniature, {boxed:?}");
}

/// With a stroke in the old room, `fold` peeks, writing nothing, with the
/// stroke at full size in `open`; escape closes it, and so does a press
/// outside, which draws nothing.
fn peeks(w: &mut World, daemon: &control::Client, open: Rect) {
    select_all(w, 2);
    assert!(w.send(Event::Key(FOLD)));
    settle(w, |d| has(d, MARK - 1));
    assert_ne!(fold(daemon), serde_json::Value::Null, "still folded");
    let peek = drawn(w, 2).unwrap().bounds;
    assert!((peek.width - open.width).abs() < 1.0, "full size: {peek:?}, {open:?}");
    assert!(w.send(Event::Key(ESCAPE)), "escape is the peek's");
    assert!(!peeking(w));

    assert!(w.send(Event::Key(FOLD)));
    settle(w, |d| has(d, MARK - 1));
    let strokes = count(daemon, "stroke");
    w.key(Chord::new(CMD, '1'), "1");
    assert!(w.send(Event::Press(at(800.0, 450.0, 30.0))), "a press outside closes it");
    w.send(Event::Lift(at(800.0, 450.0, 30.1)));
    assert!(!peeking(w));
    w.send(Event::Tick);
    assert_eq!(count(daemon, "stroke"), strokes, "and draws nothing");
}

#[test]
fn fold_draws_a_miniature_and_an_unfold_into_a_taken_room_peeks_until_escape_or_a_press() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        put_box(daemon);
        apply(daemon, &stroke(3000, 3000));
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        let open = settle(&mut w, |d| has(d, 2)).into_iter().find(|d| d.id == ItemId(2));
        let open = open.unwrap().bounds;
        select_all(&mut w, 2);
        folds(&mut w, daemon, open);

        // A stroke written in the old room: the unfold peeks.
        apply(daemon, &stroke(4000, 3500));
        settle(&mut w, |d| has(d, 3));
        peeks(&mut w, daemon, open);

        // With the room free, the unfold opens the box in place.
        let ops = serde_json::json!({"op": "remove", "ids": ["i3"]});
        let (canvas, by) = ("notes".to_string(), "user".to_string());
        daemon.canvas_apply(control::CanvasOps { canvas, by, ops, once: None }).unwrap();
        settle(&mut w, |d| !has(d, 3));
        assert!(w.send(Event::Key(FOLD)));
        w.until(|_| fold(daemon).is_null());
        let at_full = |d: &[Draw]| d.iter().any(|d| d.id == ItemId(2) && d.bounds == open);
        settle(&mut w, at_full);
    });
}

#[test]
fn a_press_on_a_box_s_fold_mark_folds_it_and_one_on_the_miniature_s_unfolds_it() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        put_box(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        settle(&mut w, |d| has(d, 1));
        assert!(!has(&draws(&mut w), MARK + 1), "an open box shows no mark");
        select_all(&mut w, 1);
        w.key(Chord::new(CMD, '1'), "1");
        let strokes = count(daemon, "stroke");
        for (folded, when) in [(true, 10.0), (false, 20.0)] {
            let mark =
                settle(&mut w, |d| has(d, MARK + 1)).into_iter().find(|d| d.id == ItemId(MARK + 1));
            let mark = mark.unwrap().bounds;
            let (x, y) = (mark.x + mark.width / 2.0, mark.y + mark.height / 2.0);
            assert!(w.send(Event::Press(at(x, y, when))), "the mark takes the pen");
            w.send(Event::Lift(at(x, y, when + 0.1)));
            w.until(|_| fold(daemon).is_null() != folded);
        }
        w.send(Event::Tick);
        assert_eq!(count(daemon, "stroke"), strokes, "the pen drew nothing");
    });
}
