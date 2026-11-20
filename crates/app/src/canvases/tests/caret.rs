//! The caret inside a canvas text being typed (slinger-ea308d,
//! docs/design/pen-less.md 2A): its keys come before the keymap, a press
//! inside places it, a drag selects, and copy and cut take the selected
//! characters.

use super::*;
use crate::commands::Chord;
use crate::scene::paste::Clip;

fn key(canvases: &mut Canvases, chord: &str) -> bool {
    canvases.edit_key(Chord::parse(chord).unwrap())
}

/// What the daemon holds of the page's texts.
fn texts(daemon: &Client) -> Vec<String> {
    let items = daemon.canvas_show("notes", None).unwrap().items;
    items.iter().filter_map(|i| i["text"].as_str().map(str::to_string)).collect()
}

/// A pane on an empty `notes` page with "word" being typed in a text at
/// (300, 300) of the window.
fn typing_word(daemon: &Client) -> (Canvases, Panes) {
    let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
    wait_until(|| shown(&mut canvases, &panes) == 1);
    assert!(!key(&mut canvases, "alt+left"), "no text typed: the keymap's (a pan)");
    type_word(&mut canvases, &panes, (300.0, 300.0), 1.0);
    (canvases, panes)
}

#[test]
fn the_caret_keys_come_before_the_keymap_and_copy_takes_the_selected_characters() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let (mut canvases, panes) = typing_word(daemon);
        assert!(key(&mut canvases, "alt+left"), "the text's, before the keymap");
        assert!(canvases.typed("a "));
        assert!(!canvases.can_copy(&panes), "nothing selected");
        assert!(key(&mut canvases, "shift+alt+right") && canvases.can_copy(&panes));
        let copied = canvases.copy(&panes, false).unwrap();
        assert_eq!((copied.text.as_deref(), copied.items), (Some("word"), None));
        assert!(!key(&mut canvases, "cmd+z"), "a chord the text does not use goes on");
        assert!(canvases.select_text(), "select all from the menu");
        assert_eq!(canvases.copy(&panes, false).unwrap().text.as_deref(), Some("a word"));
        assert!(key(&mut canvases, "right") && canvases.typed("s\u{1b}"));
        assert!(!canvases.select_text(), "no text typed");
        until(&mut canvases, &panes, daemon, |d| texts(d) == ["a words"]);
    });
}

#[test]
fn a_press_inside_places_the_caret_a_drag_selects_and_cut_takes_it_out() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let (mut canvases, panes) = typing_word(daemon);
        // A press at the text's left edge puts the caret before "word".
        let pane = (panes.focus(), WINDOW.rect());
        assert!(canvases.pen_down(&panes, pane, pen(300.0, 300.0, 3.0), (Tool::Text, GREY)));
        canvases.pen_up(&panes, pen(300.0, 300.0, 3.1), Some(pane));
        assert!(canvases.editing() && canvases.typed("<"), "a press inside keeps typing");
        // A drag from its left edge to past the end selects it all.
        assert!(canvases.pen_down(&panes, pane, pen(296.0, 300.0, 5.0), (Tool::Pen, GREY)));
        assert!(canvases.pen_drag(&panes, pen(600.0, 300.0, 5.1), Some(pane)));
        canvases.pen_up(&panes, pen(600.0, 300.0, 5.2), Some(pane));
        let cut = canvases.copy(&panes, true).unwrap();
        assert_eq!(cut.text.as_deref(), Some("<word"));
        assert!(canvases.typed("x") && key(&mut canvases, "cmd+a") && key(&mut canvases, "left"));
        let more = Clip::new(Some("say ".into()), None);
        assert!(canvases.paste(&more, &panes, WINDOW.rect(), GREY), "at the caret");
        assert!(canvases.typed("\u{1b}"));
        until(&mut canvases, &panes, daemon, |d| texts(d) == ["say x"]);
    });
}
