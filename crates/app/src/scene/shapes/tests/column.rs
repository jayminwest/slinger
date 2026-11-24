//! A typed text wraps in a column (slinger-d96bfb): a new text gets one of
//! [`glyphs::NEW_COLUMN`] characters, and a drag of its frame's right edge
//! sets it.

use super::*;

/// The right edge of the dashed frame of the text being typed, at zoom 1,
/// level with its first row.
pub(super) fn edge(shapes: &Shapes) -> (f64, f64) {
    let around = shapes.shown(HERE, Vec::new(), 1.0, &THEME);
    let frame = around.iter().find(|i| i.id == FRAME_ID).expect("a frame");
    let Shape::Panel { frame, .. } = frame.shape else { panic!() };
    (frame.x + frame.width, frame.y + 8.0)
}

/// The rows the text being typed shows in.
fn rows(shapes: &Shapes) -> String {
    let showing = shapes.shown(HERE, Vec::new(), 1.0, &THEME);
    let text = showing.iter().find(|i| i.id == EDITING).expect("a text");
    let Shape::Text { text, size, column, .. } = &text.shape else { panic!() };
    glyphs::rows(text, *size, column.unwrap())
}

#[test]
fn a_new_text_wraps_at_its_column_of_80_characters() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(down(&mut shapes, &mut ink, Tool::Text, (10.0, 212.0), 0.0, &[]));
    let words = "word ".repeat(20);
    shapes.typed(words.trim_end(), &mut ink);
    let row = "word ".repeat(16);
    let rest = "word word word word";
    assert_eq!(rows(&shapes), format!("{}\n{rest}", row.trim_end()), "80 cells a row");
    shapes.typed("\u{1b}", &mut ink);
    let [Change::Put { shape: Shape::Text { frame, column, .. }, .. }] = &changes(&mut ink)[..]
    else {
        panic!()
    };
    assert_eq!(*column, NEW);
    assert_eq!(frame.height, 2.0 * TEXT_SIZE, "two rows");
}

#[test]
fn a_drag_of_the_frame_s_right_edge_sets_the_column() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(down(&mut shapes, &mut ink, Tool::Text, (10.0, 212.0), 0.0, &[]));
    shapes.typed("one two three", &mut ink);
    let (x, y) = edge(&shapes);
    assert!(shapes.on_edge(HERE, pt(x, y), 1.0), "on the dashed edge");
    assert!(!shapes.on_edge(HERE, pt(x - 40.0, y), 1.0), "inside the text is not");
    assert!(!shapes.on_edge(("main", "p2"), pt(x, y), 1.0), "only on its page");

    // Grabbed on the dash, 4 past the column, and dragged so the column is
    // a little over eight cells of 12: "one two" and the space it breaks at.
    assert!(down(&mut shapes, &mut ink, Tool::Text, (x, y), 1.0, &[]));
    assert!(shapes.editing(), "the press keeps typing");
    assert!(shapes.drag(pt(10.0 + 8.0 * 12.0 + 3.0 + 4.0, y)));
    assert_eq!(rows(&shapes), "one two\nthree", "eight cells a row");
    assert!(shapes.up(None, &[], &mut ink));
    assert!(changes(&mut ink).is_empty(), "nothing is sent while typing");
    shapes.typed(" four", &mut ink);
    assert_eq!(rows(&shapes), "one two\nthree\nfour", "typing keeps the column");

    shapes.typed("\u{1b}", &mut ink);
    let [Change::Put { shape: Shape::Text { column, .. }, .. }] = &changes(&mut ink)[..] else {
        panic!()
    };
    assert_eq!(*column, Some(8.5 * 12.0));
}

#[test]
fn the_column_is_at_least_a_character_wide() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(down(&mut shapes, &mut ink, Tool::Text, (10.0, 212.0), 0.0, &[]));
    shapes.typed("ab", &mut ink);
    let (x, y) = edge(&shapes);
    assert!(down(&mut shapes, &mut ink, Tool::Text, (x, y), 1.0, &[]));
    assert!(shapes.drag(pt(-500.0, y)));
    assert_eq!(rows(&shapes), "a\nb");
}

/// A text typed before texts wrapped has no column: a drag of its edge
/// gives it one, and that alone is an edit.
#[test]
fn a_drag_of_an_unwrapped_text_s_edge_gives_it_a_column_as_one_set() {
    let items = page();
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(down(&mut shapes, &mut ink, Tool::Text, (310.0, 10.0), 0.0, &items));
    assert_eq!(rows(&shapes), "plan");
    let (x, y) = edge(&shapes);
    assert!(x < 400.0, "its frame is its glyphs': {x}");
    assert!(down(&mut shapes, &mut ink, Tool::Text, (x, y), 1.0, &items));
    assert!(shapes.drag(pt(x - 20.0, y)));
    assert!(shapes.up(None, &shown(&items), &mut ink));
    assert_eq!(rows(&shapes), "pl\nan");
    shapes.typed("\u{1b}", &mut ink);
    let [Change::Set { id, shape: Shape::Text { text, column, .. }, .. }] = &changes(&mut ink)[..]
    else {
        panic!()
    };
    assert_eq!((id, text.as_str(), *column), (&ItemId(2), "plan", Some(2.5 * 12.0)));
}

/// A text started from the keyboard (`new-text`, [`Shapes::start`]) wraps
/// in a new text's column too.
#[test]
fn a_text_started_from_the_keyboard_wraps_in_a_new_text_s_column() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    shapes.start(HERE, pt(40.0, 100.0), RED, &mut ink);
    let words = "word ".repeat(20);
    shapes.typed(words.trim_end(), &mut ink);
    assert_eq!(rows(&shapes).lines().count(), 2, "80 cells a row");
    shapes.typed("\u{1b}", &mut ink);
    let [Change::Put { shape: Shape::Text { column, .. }, .. }] = &changes(&mut ink)[..] else {
        panic!()
    };
    assert_eq!(*column, NEW);
}

/// An edit that changes only the text's size is a `set`, and so is one
/// that changes only its column (above); a resize keeps the column's
/// cells, so a new text stays 80 cells wide (slinger-5285e4).
#[test]
fn an_edit_of_only_the_size_is_one_set() {
    let items = page();
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(down(&mut shapes, &mut ink, Tool::Text, (310.0, 10.0), 0.0, &items));
    assert!(shapes.resize(Resize::Up));
    shapes.typed("\u{1b}", &mut ink);
    let [Change::Set { id, shape: Shape::Text { text, size, column, .. }, .. }] =
        &changes(&mut ink)[..]
    else {
        panic!()
    };
    assert_eq!((id, text.as_str(), *size), (&ItemId(2), "plan", 36.0));
    assert_eq!(*column, Some(glyphs::UNWRAPPED), "an unwrapped text stays so");

    assert!(down(&mut shapes, &mut ink, Tool::Text, (10.0, 212.0), 1.0, &[]));
    assert!(shapes.resize(Resize::Up));
    shapes.typed("hi\u{1b}", &mut ink);
    let [Change::Put { shape: Shape::Text { column, .. }, .. }] = &changes(&mut ink)[..] else {
        panic!()
    };
    assert_eq!(*column, Some(80.5 * 18.0), "80 cells of 36, as 966 was of 24");
}
