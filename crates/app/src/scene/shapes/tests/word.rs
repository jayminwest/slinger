//! A double tap on a word of a text not being typed edits it at that
//! word (docs/design/writing-column.md, 4A); the text tool's tap at its end.

use super::*;

/// A text at the origin, size 10 (cells 5 wide), in a wide column.
fn text(body: &str) -> Item {
    let frame = glyphs::rows_frame(pt(0.0, 0.0), body, 10.0, 1e7);
    let shape = Shape::Text { frame, text: body.into(), size: 10.0, column: Some(1e7) };
    Item::plain(ItemId(3), RED, shape)
}

/// The caret of the text being typed, as a byte index.
fn caret(shapes: &Shapes) -> Option<usize> {
    shapes.typing_caret()
}

#[test]
fn a_double_tap_with_the_select_tool_edits_at_the_cell_under_it_once_the_pen_lifts() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    let items = [text("one two\nthree")];
    // "two" is cells 4 to 6 of the first row; 31 points is in cell 6, its "o".
    assert!(!down(&mut shapes, &mut ink, Tool::Select, (31.0, 4.0), 0.0, &items));
    assert!(!shapes.tapped(pt(31.0, 4.0)), "one tap selects, it does not edit");
    assert!(!down(&mut shapes, &mut ink, Tool::Select, (31.0, 4.0), 0.2, &items));
    assert!(shapes.tapped(pt(31.0, 4.0)));
    assert_eq!(caret(&shapes), Some(6), "before the o of two");
    shapes.end(&mut ink);
    // In the second row, right of its cells (within reach): the row's end.
    assert!(!down(&mut shapes, &mut ink, Tool::Lasso, (40.0, 14.0), 1.0, &items));
    assert!(!down(&mut shapes, &mut ink, Tool::Lasso, (40.0, 14.0), 1.2, &items));
    assert!(shapes.tapped(pt(40.0, 14.0)));
    assert_eq!(caret(&shapes), Some(13));
}

#[test]
fn a_double_tap_with_a_shape_tool_edits_at_the_cell_and_the_text_tool_at_the_end() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    let items = [text("one two")];
    assert!(down(&mut shapes, &mut ink, Tool::Box, (2.0, 4.0), 0.0, &items));
    assert!(shapes.up(None, &shown(&items), &mut ink));
    assert!(down(&mut shapes, &mut ink, Tool::Box, (2.0, 4.0), 0.2, &items));
    assert_eq!(caret(&shapes), Some(0), "the first cell: before the o");
    shapes.end(&mut ink);
    assert!(down(&mut shapes, &mut ink, Tool::Text, (2.0, 4.0), 5.0, &items));
    assert_eq!(caret(&shapes), Some(7), "the text tool opens it at its end, as before");
}
