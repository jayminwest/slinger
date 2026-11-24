//! How a wrapped text's column is stored (slinger-08633c).

use super::*;

/// slinger-08633c: a resized column is stored on whole tenths that hold as
/// many cells of the stored size as it did, and reads back so.
#[test]
fn a_wrapped_text_s_column_is_stored_to_hold_its_cells() {
    let frame = Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };
    let shape = Shape::Text { frame, text: "a".into(), size: 32.88, column: Some(3289.0) };
    let edit = Change::Set { canvas: "main".into(), page: "p1".into(), id: ItemId(1), shape };
    let written = ops(&edit);
    assert_eq!(written["fields"]["w"], 32982, "3289.0 / 16.45 is 199 cells, not 200");
    assert_eq!(written["fields"]["h"], 329, "a size a row");
    let mut object = written["fields"].clone();
    object["id"] = "i1".into();
    object["kind"] = "text".into();
    let Some(Item { shape: Shape::Text { size, column, .. }, .. }) = item(&object) else {
        panic!()
    };
    assert_eq!(glyphs::rows(&"x".repeat(200), size, column.unwrap()), "x".repeat(200));
}

/// slinger-ce52ee: a text typed before the cell was narrowed is stored a
/// full size a character wide, and still reads as one that does not wrap.
#[test]
fn a_text_typed_before_the_cell_was_narrowed_still_does_not_wrap() {
    for w in [480, 240] {
        let object = json!({"id": "i1", "page": "p1", "kind": "text", "color": "#000000",
                            "x": 0, "y": 0, "w": w, "h": 312, "text": "hi"});
        let Some(Item { shape: Shape::Text { column, .. }, .. }) = item(&object) else { panic!() };
        assert_eq!(column, Some(glyphs::UNWRAPPED), "stored {w} wide");
    }
}

/// slinger-d96bfb: a typed text's column ([`glyphs::column`]) is stored as
/// it is and reads back as that column, at every size, even when its
/// longest line fills the whole cells it holds: never as a text that does
/// not wrap.
#[test]
fn a_typed_text_s_column_reads_back_as_its_column() {
    let frame = Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };
    for size in [12.0, 16.0, 24.0, 36.0, 48.0, 72.0, 32.88] {
        for cells in [1.0, 7.0, 48.0] {
            let column = glyphs::column(cells * size * glyphs::CELL, size);
            #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "small")]
            let text = "x".repeat(cells as usize);
            let shape = Shape::Text { frame, text: text.clone(), size, column: Some(column) };
            let edit =
                Change::Set { canvas: "main".into(), page: "p1".into(), id: ItemId(1), shape };
            let mut object = ops(&edit)["fields"].clone();
            object["id"] = "i1".into();
            object["kind"] = "text".into();
            let Some(Item { shape: Shape::Text { column: read, .. }, .. }) = item(&object) else {
                panic!()
            };
            let read = read.unwrap();
            assert!(read < glyphs::UNWRAPPED, "{cells} cells at {size}");
            let rows = |column| glyphs::rows(&format!("{text}y"), size, column);
            assert_eq!(rows(read), format!("{text}\ny"), "{cells} cells at {size}");
            assert_eq!(rows(read), rows(column));
        }
    }
}
