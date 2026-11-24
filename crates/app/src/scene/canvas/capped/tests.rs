//! A text past `MOST_ROWS` rows shows its first rows, fades, and says how
//! many more it has; framed whole (stepped into), it draws every row.

use super::*;
use crate::layout::Size;
use crate::scene::canvas::build;

const SIZE: f64 = 24.0;
const INK: Rgb = Rgb { r: 0, g: 0, b: 0 };

/// `n` rows of text, one a line.
fn lines(n: usize) -> String {
    (1..=n).map(|i| format!("row {i}")).collect::<Vec<_>>().join("\n")
}

/// A canvas text of `n` rows in an 80-cell column, framed as the mirror
/// frames it (`whole`: as stepped into).
fn text(n: usize, whole: bool) -> Item {
    let (text, at) = (lines(n), Point { x: 0.0, y: 0.0 });
    let column = glyphs::column(glyphs::NEW_COLUMN * SIZE * glyphs::CELL, SIZE);
    let frame =
        if whole { glyphs::whole_frame } else { glyphs::rows_frame }(at, &text, SIZE, column);
    Item {
        id: ItemId(7),
        colour: INK,
        shape: Shape::Text { frame, text, size: SIZE, column: Some(column) },
    }
}

fn drawn(item: &Item) -> Vec<Draw> {
    let screen = Size { width: 800.0, height: 600.0 };
    build([item], Viewport::default(), screen, &Theme::default(), None).draws
}

fn words(d: &Draw) -> &str {
    let Mark::Text { text, .. } = &d.mark else { panic!("a text: {d:?}") };
    text
}

#[test]
fn a_text_past_most_rows_fades_and_says_how_many_more() {
    let item = text(100, false);
    let Shape::Text { frame, .. } = item.shape else { unreachable!() };
    assert_eq!(frame.height, 80.0 * SIZE, "its frame stops at 80 rows");
    let draws = drawn(&item);
    assert_eq!(draws.len(), 1 + FADING + 1, "the rows, each fading one, the note");
    assert_eq!(draws[0].id, item.id);
    assert_eq!(words(&draws[0]), lines(76), "rows 1 to 76 in its ink");
    let theme = Theme::default();
    let fading: Vec<Rgb> = draws[1..=FADING].iter().map(|d| d.colour).collect();
    assert_eq!(words(&draws[3]), "row 79");
    for (k, colour) in fading.iter().enumerate() {
        assert!(*colour != INK && *colour != theme.background, "row {} fades: {colour:?}", 77 + k);
    }
    let away = |c: &Rgb| u32::from(c.r.abs_diff(theme.background.r));
    assert!(fading.windows(2).all(|w| away(&w[1]) < away(&w[0])), "nearer the paper each row");
    let note = &draws[1 + FADING];
    assert_eq!((words(note), note.colour), ("21 more lines", theme.muted), "rows past 79, dimmed");
    assert_eq!(note.bounds.y, 79.0 * SIZE, "in the 80th row");
    let ids: std::collections::HashSet<_> = draws.iter().map(|d| d.id).collect();
    assert_eq!(ids.len(), draws.len(), "each draw its own id");
}

#[test]
fn a_text_framed_whole_or_within_most_rows_draws_as_one() {
    let whole = drawn(&text(100, true));
    assert_eq!(whole.len(), 1, "stepped into: every row");
    assert_eq!(words(&whole[0]), lines(100));
    let eighty = drawn(&text(80, false));
    assert_eq!((eighty.len(), words(&eighty[0])), (1, lines(80).as_str()), "80 rows fit");
}
