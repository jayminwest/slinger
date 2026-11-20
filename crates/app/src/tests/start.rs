//! A text started from the keyboard (slinger-b16804): `new-text` by its
//! default key chains a text under the one being typed, and starts one at
//! the pointer when none is, or opens the text under the pointer.

use super::*;
use crate::commands::CTRL;
use crate::scene::canvas::{Draw, Mark};

/// Each text on the daemon's `notes` canvas: its text, `x` and `y`.
fn texts(daemon: &control::Client) -> Vec<(String, f64, f64)> {
    let items = daemon.canvas_show("notes", None).unwrap().items;
    let texts = items.iter().filter(|i| i["kind"] == "text");
    let at = |i: &serde_json::Value, k: &str| i[k].as_f64().unwrap();
    texts.map(|i| (i["text"].as_str().unwrap().to_string(), at(i, "x"), at(i, "y"))).collect()
}

impl World {
    fn type_in(&mut self, text: &str) {
        for c in text.chars() {
            self.key(Chord::new(0, c), &c.to_string());
        }
    }
}

#[test]
fn new_text_chains_under_the_text_being_typed_and_else_starts_at_the_pointer() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        let new_text = Chord::new(CTRL, '\r');
        w.type_word(300.0, 10.0);
        let scene = view(&w.settings, &mut w.shared, &w.tools, &w.sidebar, &mut w.canvases);
        let hint = "esc or a tap outside ends the text · ⌃↩ new text";
        let named = |d: &Draw| matches!(&d.mark, Mark::Text { text, .. } if text == hint);
        let mut draws = scene.canvases.iter().flat_map(|c| c.scene.draws.iter());
        assert!(draws.any(named), "the hint names the live key");
        assert!(w.send(Event::Key(new_text)), "the keymap's, not the text's");
        assert!(w.canvases.editing(), "the next text is being typed");
        w.type_in("next");
        assert!(w.send(Event::Key(new_text)));
        w.type_in("last");
        w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        w.until(|_| texts(daemon).len() == 3);
        let chain = texts(daemon);
        let words: Vec<&str> = chain.iter().map(|(t, _, _)| t.as_str()).collect();
        assert_eq!(words, ["word", "next", "last"]);
        let left = |(_, x, _): &(String, f64, f64)| (x - chain[0].1).abs() < 0.05;
        assert!(chain.iter().all(left), "one left edge: {chain:?}");
        assert!(chain[0].2 < chain[1].2 && chain[1].2 < chain[2].2, "each under: {chain:?}");

        // With no text being typed, it starts at the pointer.
        w.send(Event::Moved(Point { x: 600.0, y: 500.0 }));
        assert!(w.send(Event::Key(new_text)));
        w.type_in("far");
        w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        w.until(|_| texts(daemon).len() == 4);
        let far = texts(daemon).into_iter().find(|(t, _, _)| t == "far").unwrap();
        assert!(far.1 > chain[0].1 + 250.0 && far.2 > chain[2].2, "at the pointer: {far:?}");

        // With the pointer over a text, it opens that text as a tap does (slinger-fe632e).
        w.send(Event::Moved(Point { x: 610.0, y: 500.0 }));
        assert!(w.send(Event::Key(new_text)));
        w.type_in("!");
        w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        w.until(|_| texts(daemon).iter().any(|(t, _, _)| t == "far!"));
        assert_eq!(texts(daemon).len(), 4, "edited, not one more");
    });
}
