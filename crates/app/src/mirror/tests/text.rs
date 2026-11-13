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
