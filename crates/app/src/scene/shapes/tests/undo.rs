//! A new size (the size keys) and a new column (a drag of the frame's
//! edge) are steps in the undo of the text being typed (slinger-0b424a).

use super::column::edge;
use super::*;
use crate::scene::tools::Resize;

/// The text being typed, its size and its column, as it shows.
fn typing(shapes: &Shapes) -> (String, f64, f64) {
    let showing = shapes.shown(HERE, Vec::new(), 1.0, &THEME);
    let text = showing.iter().find(|i| i.id == EDITING).expect("a text");
    let Shape::Text { text, size, column, .. } = &text.shape else { panic!() };
    (text.clone(), *size, column.unwrap())
}

/// Undoes (with `redo`, redoes) one step, which there must be: the text then.
fn step(shapes: &mut Shapes, redo: bool) -> (String, f64, f64) {
    assert!(shapes.undo(redo), "a step to {}", if redo { "redo" } else { "undo" });
    typing(shapes)
}

/// A new text typed `note`, then sized up and typed `s`: the text as it
/// was before the size key, and after the `s`.
fn resized(shapes: &mut Shapes, ink: &mut Ink) -> [(String, f64, f64); 2] {
    assert!(down(shapes, ink, Tool::Text, (10.0, 212.0), 0.0, &[]));
    shapes.typed("note", ink);
    let before = typing(shapes);
    assert!(shapes.resize(Resize::Up));
    shapes.typed("s", ink);
    [before, typing(shapes)]
}

#[test]
fn a_size_key_on_the_text_being_typed_is_a_step_its_undo_takes_back() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    let [(_, size, column), big] = resized(&mut shapes, &mut ink);
    assert_eq!((big.0.as_str(), big.1), ("notes", 36.0));
    assert!(big.2 > column, "a wrapped text's column grows with it");

    assert_eq!(step(&mut shapes, false), ("note".into(), 36.0, big.2), "the s typed after it");
    assert_eq!(step(&mut shapes, false), ("note".into(), size, column), "the size and column");
    assert_eq!(step(&mut shapes, false).0, "", "the typing before it");
    assert!(!shapes.undo(false), "nothing more");
}

#[test]
fn a_size_step_redoes_and_a_size_the_text_has_makes_none() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    let [before, big] = resized(&mut shapes, &mut ink);
    step(&mut shapes, false);
    step(&mut shapes, false);
    assert_eq!(step(&mut shapes, true), ("note".into(), 36.0, big.2), "redo resizes it again");
    assert!(shapes.resize(Resize::To(36)), "the text takes the size it has");
    assert_eq!(step(&mut shapes, false), before, "which made no step");
}

#[test]
fn one_drag_of_the_frame_s_edge_is_one_step_its_undo_takes_back() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(down(&mut shapes, &mut ink, Tool::Text, (10.0, 212.0), 0.0, &[]));
    shapes.typed("one two three", &mut ink);
    let before = typing(&shapes);
    let (x, y) = edge(&shapes);
    assert!(down(&mut shapes, &mut ink, Tool::Text, (x, y), 1.0, &[]));
    for right in [200.0, 150.0, 10.0 + 7.0 * 12.0 + 3.0 + 4.0] {
        assert!(shapes.drag(pt(right, y)));
    }
    assert!(shapes.up(None, &[], &mut ink));
    // A press on the edge that does not move it is no step.
    let (x, y) = edge(&shapes);
    assert!(down(&mut shapes, &mut ink, Tool::Text, (x, y), 2.0, &[]));
    assert!(shapes.up(None, &[], &mut ink));

    assert_eq!(step(&mut shapes, false), before, "the whole drag");
    assert_eq!(step(&mut shapes, true).2, 7.5 * 12.0);
    step(&mut shapes, false);
    assert_eq!(step(&mut shapes, false).0, "", "then the typing");
    assert!(!shapes.undo(false));
}
