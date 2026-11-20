//! The size commands on texts already there (slinger-c29905): the text
//! being typed takes them, its alt chords resizing it rather than typing;
//! else the selected texts do, in one undo step; else a new text's size
//! changes, as before.

use super::*;

/// Each text's size on the daemon's `notes` canvas, in its order there:
/// the file keeps tenths, and leaves the usual 24 out.
fn sizes(daemon: &control::Client) -> Vec<f64> {
    let items = daemon.canvas_show("notes", None).unwrap().items;
    let texts = items.iter().filter(|i| i["kind"] == "text");
    texts.map(|i| i["size"].as_f64().map_or(24.0, |tenths| tenths / 10.0)).collect()
}

/// The default chords of size-up and size-down.
fn steps(w: &World) -> (Chord, Chord) {
    let keys = w.settings.config.keys;
    (keys.chord(Command::SizeUp).unwrap(), keys.chord(Command::SizeDown).unwrap())
}

#[test]
fn the_text_being_typed_takes_the_size_keys_rather_than_typing_them() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        let (up, down) = steps(&w);
        assert!(up.types(), "alt+shift+]: a chord that would type a character");

        w.type_word(300.0, 10.0);
        assert!(w.send(Event::Key(up)), "it resizes the text, and types nothing");
        w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        w.until(|_| sizes(daemon) == [36.0]);
        assert_eq!(w.tools.size(), 24, "a new text's size stays");

        // A tap with the text tool opens it again; a size alone is an edit.
        assert!(w.send(Event::Press(at(302.0, 300.0, 12.0))));
        w.send(Event::Lift(at(302.0, 300.0, 12.1)));
        assert!(w.canvases.editing());
        assert!(w.send(Event::Key(down)));
        w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        w.until(|_| sizes(daemon) == [24.0]);
    });
}

#[test]
fn the_selected_texts_take_a_size_in_one_undo_step_and_with_none_a_new_text_does() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        let (_, down) = steps(&w);
        w.type_word(300.0, 10.0);
        w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        w.until(|_| sizes(daemon) == [24.0]);
        // Once the daemon has named the text, select all reaches it.
        wait_until(|| {
            w.send(Event::Tick);
            w.send(Event::Key(Chord::new(CMD, 'a')));
            w.canvases.selection().is_some()
        });
        assert!(w.send(Event::Command(Command::PickSize)));
        w.key(Chord::new(0, '6'), "6");
        w.until(|_| sizes(daemon) == [72.0]);
        assert_eq!(w.tools.size(), 72, "a size picked is a new text's too, as a colour is");
        assert!(w.send(Event::Key(down)));
        w.until(|_| sizes(daemon) == [48.0]);
        assert_eq!(w.tools.size(), 72, "a step leaves a new text's size");
        assert!(w.send(Event::Command(Command::Undo)));
        w.until(|_| sizes(daemon) == [72.0]);

        assert!(w.send(Event::Key(Chord::new(0, '\u{1b}'))), "escape selects nothing");
        assert!(w.send(Event::Key(down)));
        assert_eq!((w.tools.size(), sizes(daemon)), (48, vec![72.0]), "a new text's size only");
    });
}
