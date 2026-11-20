//! Undo and redo of the text's edits (slinger-304496), grouped as a Mac
//! text field groups them.

use super::*;

/// Undoes (or with `redo`, redoes) `n` times, each a step.
fn undo(e: &mut Editor, redo: bool, n: usize) -> String {
    for _ in 0..n {
        assert!(e.undo(redo, &mut (0.0, 0.0)), "a step to {}", if redo { "redo" } else { "undo" });
    }
    shown(e)
}

#[test]
fn a_run_of_typing_is_one_step_and_a_move_ends_it() {
    let mut e = at("|");
    e.typed("one\rtwo");
    assert_eq!(undo(&mut e, false, 1), "|", "the run, lines and all");
    assert!(!e.undo(false, &mut (0.0, 0.0)), "nothing more");
    assert_eq!(undo(&mut e, true, 1), "one\ntwo|");
    assert!(!e.undo(true, &mut (0.0, 0.0)), "nothing more to redo");

    e.typed(" three");
    pressed(&mut e, &["left", "right"]);
    e.typed("!");
    assert_eq!(undo(&mut e, false, 1), "one\ntwo three|", "after the move: its own step");
    assert_eq!(undo(&mut e, false, 1), "one\ntwo|", "the run before the move");
}

#[test]
fn deletes_back_and_forward_are_runs_apart_from_typing() {
    let mut e = at("abc|def");
    e.typed("XY");
    e.typed("\u{7f}\u{7f}\u{7f}");
    pressed(&mut e, &["forward-delete", "forward-delete"]);
    assert_eq!(shown(&e), "ab|f");
    assert_eq!(undo(&mut e, false, 1), "ab|def", "the forward deletes");
    assert_eq!(undo(&mut e, false, 1), "abcXY|def", "the deletes back");
    assert_eq!(undo(&mut e, false, 1), "abc|def", "the typing");
    assert_eq!(undo(&mut e, true, 3), "ab|f");
}

#[test]
fn a_paste_a_cut_a_word_delete_and_the_emacs_edits_are_steps_each() {
    let mut e = at("one two|");
    e.insert("x");
    e.typed("y");
    pressed(&mut e, &["alt+delete", "ctrl+t", "ctrl+o"]);
    assert_eq!(shown(&e), "on e|\n");
    assert_eq!(undo(&mut e, false, 1), "on e|", "ctrl+O");
    assert_eq!(undo(&mut e, false, 1), "one |", "ctrl+T");
    assert_eq!(undo(&mut e, false, 1), "one twoxy|", "the word deleted");
    assert_eq!(undo(&mut e, false, 1), "one twox|", "typing after a paste is its own run");
    assert_eq!(undo(&mut e, false, 1), "one two|", "the paste");

    pressed(&mut e, &["shift+alt+left"]);
    assert_eq!(e.cut().as_deref(), Some("two"));
    assert_eq!(undo(&mut e, false, 1), "one [two]", "the cut, its selection back");
}

#[test]
fn typing_over_a_selection_starts_a_step_and_an_edit_drops_the_redo() {
    let mut e = at("one |two");
    pressed(&mut e, &["shift+alt+right"]);
    e.typed("2");
    e.typed("2");
    assert_eq!(undo(&mut e, false, 1), "one [two]", "the selection replaced, and the run after");
    e.typed("x");
    assert!(!e.undo(true, &mut (0.0, 0.0)), "an edit drops what was undone");

    let mut e = at("ab|");
    e.typed("\u{7f}");
    pressed(&mut e, &["left"]);
    e.typed("\u{7f}");
    assert_eq!(undo(&mut e, false, 1), "ab|", "a delete at the start changes nothing");
    assert!(!e.undo(false, &mut (0.0, 0.0)));
}
