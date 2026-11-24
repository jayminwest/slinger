//! While an Nvim edits the text being typed, its grid shows in the text's
//! place (slinger-be5d24).

use std::sync::Arc;

use super::*;
use crate::nvim::grid::Grid;
use crate::nvim::pack::Value;

/// A grid `cols` by `rows`, flushed, whose first row reads `text`.
fn grid(cols: i64, rows: i64, text: &str) -> Arc<Grid> {
    let (a, n) = (Value::Array, Value::Int);
    let cells = text.chars().map(|c| a(vec![Value::str(&c.to_string())])).collect();
    let mut grid = Grid::new(1);
    grid.redraw(&[
        a(vec![Value::str("grid_resize"), a(vec![n(1), n(cols), n(rows)])]),
        a(vec![Value::str("grid_line"), a(vec![n(1), n(0), n(0), a(cells), Value::Bool(false)])]),
        a(vec![Value::str("flush"), a(vec![])]),
    ]);
    Arc::new(grid)
}

#[test]
fn nvims_grid_shows_from_the_texts_top_left_and_a_press_inside_it_keeps_the_text() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(down(&mut shapes, &mut ink, Tool::Text, (10.0, 212.0), 0.0, &[]));
    let typed = shapes.shown(HERE, Vec::new(), 1.0, &THEME);
    let text = typed.iter().find(|i| i.id == EDITING).unwrap().shape.bounds().unwrap();
    shapes.set_grid(Some(grid(80, 2, "hello")));
    let showing = shapes.shown(HERE, Vec::new(), 1.0, &THEME);
    let ids: Vec<ItemId> = showing.iter().map(|i| i.id).collect();
    assert_eq!(ids, [EDITING, FRAME_ID, HINT_ID], "no caret, no selection: the grid has its own");
    let Shape::Cells { frame, size, grid: shown } = &showing[0].shape else { panic!() };
    assert_eq!((frame.x, frame.y, *size), (text.x, text.y, TEXT_SIZE), "the text's place");
    assert_eq!((frame.width, frame.height), (80.0 * glyphs::CELL * size, 2.0 * size), "its cells");
    assert_eq!(shown.row_text(0), "hello");

    let (right, bottom) = (frame.x + frame.width, frame.y + frame.height);
    assert!(right > text.x + text.width, "the grid is wider than the text typed so far");
    assert!(down(&mut shapes, &mut ink, Tool::Text, (right - 2.0, bottom - 2.0), 1.0, &[]));
    assert!(shapes.editing(), "inside the grid: Nvim's press, the text stays");
    assert!(down(&mut shapes, &mut ink, Tool::Text, (right + 40.0, frame.y), 2.0, &[]));
    assert!(!shapes.editing(), "outside it: the text ends");
    shapes.set_grid(Some(grid(80, 2, "late")));
    assert!(shapes.shown(HERE, Vec::new(), 1.0, &THEME).is_empty(), "no text, no grid");
}
