//! The items from the keyboard (slinger-e96ea3): select all, next item,
//! nudge and duplicate by their default keys, through the window's routing
//! to the daemon; and the keys left to a text being typed or an entry.

use super::*;
use crate::commands::SHIFT;

/// The right arrow, as macOS sends it.
const RIGHT: char = '\u{f703}';

/// Each box's `x` on the daemon's `notes` canvas (in tenths), in its order there.
fn boxes(daemon: &control::Client) -> Vec<f64> {
    let items = daemon.canvas_show("notes", None).unwrap().items;
    let boxes = items.iter().filter(|i| i["kind"] == "box");
    boxes.map(|i| i["x"].as_f64().unwrap()).collect()
}

fn selected(w: &World) -> usize {
    w.canvases.selection().map_or(0, |s| s.ids.len())
}

#[test]
fn select_all_next_nudge_and_duplicate_act_on_the_focused_page() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        w.drag_box(400.0, 10.0);
        w.drag_box(200.0, 11.0);
        w.until(|_| boxes(daemon).len() == 2);
        // Once the daemon has named both boxes, the keys reach them.
        wait_until(|| {
            w.send(Event::Tick);
            w.send(Event::Key(Chord::new(CMD, 'a')));
            selected(&w) == 2
        });
        assert!(w.send(Event::Key(Chord::new(0, '\u{1b}'))), "escape selects nothing");
        assert!(!w.send(Event::Key(Chord::new(0, RIGHT))), "nothing selected: the arrow goes on");
        assert!(w.send(Event::Key(Chord::new(CMD, 'a'))));
        assert_eq!(selected(&w), 2);
        assert!(w.send(Event::Key(Chord::new(0, '\t'))));
        assert_eq!(selected(&w), 1, "tab selects one item");
        let first = w.canvases.selection().unwrap().ids[0];
        assert!(w.send(Event::Key(Chord::new(SHIFT, '\u{19}'))), "shift+tab, as macOS sends it");
        assert!(w.send(Event::Key(Chord::new(SHIFT, '\u{19}'))));
        assert_eq!(w.canvases.selection().unwrap().ids, [first], "two items: back to the first");

        let before = boxes(daemon);
        assert!(w.send(Event::Key(Chord::new(0, RIGHT))));
        w.until(|_| boxes(daemon) != before);
        let moved: Vec<f64> = boxes(daemon).iter().zip(&before).map(|(a, b)| a - b).collect();
        let ten = moved.contains(&100.0) && moved.contains(&0.0);
        assert!(ten, "one box 10 points right, in the file's tenths: {moved:?}");

        assert!(w.send(Event::Key(Chord::new(CMD, 'j'))));
        let copied = |w: &World| w.canvases.selection().is_some_and(|s| s.ids != [first]);
        w.until(|w| boxes(daemon).len() == 3 && copied(w));
        assert_eq!(selected(&w), 1, "the copy is the selection");
    });
}

#[test]
fn a_text_being_typed_or_an_open_entry_keeps_the_keys() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        w.drag_box(400.0, 10.0);
        w.until(|_| boxes(daemon).len() == 1);
        w.type_word(100.0, 11.0);
        // slinger-ea308d: select all selects the text's characters, not items.
        assert!(w.send(Event::Key(Chord::new(CMD, 'a'))), "the text's select all");
        assert!(w.send(Event::Command(Command::SelectAll)), "chosen from the menu too");
        assert_eq!(selected(&w), 0);
        assert!(!w.send(Event::Key(Chord::new(0, '\t'))), "tab types");
        assert_eq!(selected(&w), 0);
        w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        w.until(|_| count(daemon, "text") == 1);

        assert!(w.send(Event::Command(Command::LeaveFeedback)));
        assert!(!w.send(Event::Key(Chord::new(CMD, 'a'))), "the entry's field selects its text");
        assert!(!w.send(Event::Command(Command::SelectAll)), "chosen from the menu too");
        assert_eq!(selected(&w), 0);
        w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        assert!(w.send(Event::Command(Command::SelectAll)));
        assert_eq!(selected(&w), 2, "the box and the text");
    });
}
