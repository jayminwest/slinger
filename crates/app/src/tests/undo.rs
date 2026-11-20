//! Undo while a canvas text is typed (slinger-304496): the undo and redo
//! keys step through the text's edits; once it ends, undo is the canvas's.

use super::*;
use crate::commands::SHIFT;

/// Each text on the daemon's `notes` canvas.
fn texts(daemon: &control::Client) -> Vec<String> {
    let items = daemon.canvas_show("notes", None).unwrap().items;
    let texts = items.iter().filter(|i| i["kind"] == "text");
    texts.map(|i| i["text"].as_str().unwrap().to_string()).collect()
}

#[test]
fn undo_and_redo_step_through_a_typed_text_and_then_the_canvas() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        let (undo, redo) = (Chord::new(CMD, 'z'), Chord::new(CMD | SHIFT, 'z'));
        w.type_word(300.0, 10.0);
        w.key(Chord::new(0, '\u{7f}'), "\u{7f}");
        assert!(w.send(Event::Key(undo)), "the delete undone");
        assert!(w.send(Event::Key(undo)), "the typing undone");
        assert!(w.send(Event::Key(redo)));
        assert!(w.canvases.editing(), "still typed");
        w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        w.until(|_| texts(daemon) == ["word"]);

        // Ended, the text is the canvas's undo step.
        assert!(w.send(Event::Key(undo)));
        w.until(|_| texts(daemon).is_empty());

        // With none of its edits left, undo ends the text, as before.
        w.type_word(500.0, 20.0);
        assert!(w.send(Event::Key(undo)));
        assert!(w.canvases.editing(), "its typing undone");
        assert!(w.send(Event::Key(undo)));
        assert!(!w.canvases.editing(), "nothing left: it ends");
    });
}
