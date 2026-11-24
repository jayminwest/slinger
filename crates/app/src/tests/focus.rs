//! Write focus (slinger-9321cf): its key fits the pane to the column of
//! the text being typed, or the one text selected, and the view comes back
//! when the text ends, or on the key again.

use super::*;
use crate::commands::ALT;
use crate::layout::Viewport;

const FOCUS: Chord = Chord::new(CMD | ALT, '\r');

impl World {
    /// The focused canvas pane's view, as the window draws it now.
    fn viewport(&mut self) -> Viewport {
        let scene =
            view(&self.settings, &mut self.shared, &self.tools, &self.sidebar, &mut self.canvases);
        scene.canvases[0].scene.viewport
    }
}

#[test]
fn write_focus_fits_the_typed_column_and_gives_the_view_back_when_the_text_ends() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        w.type_word(300.0, 10.0);
        let before = w.viewport();
        assert!(w.send(Event::Key(FOCUS)), "the keymap's, not the text's");
        assert!(w.canvases.editing(), "the text is still typed");
        let focused = w.viewport();
        assert_ne!(focused, before);

        // Again: the view comes back, and again: it focuses once more.
        assert!(w.send(Event::Key(FOCUS)));
        assert_eq!(w.viewport(), before);
        assert!(w.send(Event::Key(FOCUS)));
        assert_eq!(w.viewport(), focused);

        // The text ends: the view comes back.
        w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        assert!(!w.canvases.editing());
        assert_eq!(w.viewport(), before);
        w.until(|_| count(daemon, "text") == 1);
        let items = daemon.canvas_show("notes", None).unwrap().items;
        assert_eq!(items[0]["text"], "word", "the key typed nothing");
    });
}

#[test]
fn write_focus_fits_the_one_text_selected_until_its_key_again() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        w.type_word(300.0, 10.0);
        w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        w.until(|_| count(daemon, "text") == 1);
        let before = w.viewport();
        assert!(w.send(Event::Key(FOCUS)), "no text: it only flashes");
        assert_eq!(w.viewport(), before);

        w.key(Chord::new(CMD, '7'), "7");
        assert!(w.send(Event::Press(at(305.0, 305.0, 20.0))));
        w.send(Event::Lift(at(305.0, 305.0, 20.1)));
        assert!(w.canvases.selection().is_some());
        assert!(w.send(Event::Key(FOCUS)));
        let focused = w.viewport();
        assert!(focused.zoom() < before.zoom(), "{focused:?}: 88 cells are wider than the pane");
        assert!(w.send(Event::Key(FOCUS)));
        assert_eq!(w.viewport(), before);
    });
}
