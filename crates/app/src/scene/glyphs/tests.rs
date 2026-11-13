use super::*;
use crate::mirror::{TEXT_SIZE, text_frame_at};

const AT: Point = Point { x: 10.0, y: 20.0 };

/// Ten points a character, thirty a line.
fn fixed(text: &str, _size: f64) -> Size {
    let width = |l: &str| f64::from(u32::try_from(l.chars().count()).unwrap()) * 10.0;
    let widest = text.split('\n').map(width).fold(0.0, f64::max);
    let lines = f64::from(u32::try_from(text.split('\n').count()).unwrap());
    Size { width: widest, height: 30.0 * lines }
}

#[test]
fn a_text_s_frame_is_its_glyphs_not_the_stored_cells() {
    let frame = frame(AT, "hello world", TEXT_SIZE);
    let stored = text_frame_at(AT, "hello world", TEXT_SIZE);
    assert_eq!((frame.x, frame.y), (AT.x, AT.y));
    assert!(frame.width < stored.width * 0.6, "{frame:?} is about half the stored {stored:?}");
    assert!((frame.width - 11.0 * TEXT_SIZE * CHAR_WIDTH).abs() <= 2.0, "rounded up, a point more");
}

#[test]
fn the_measure_platform_hands_in_is_used() {
    set_measure(fixed);
    let abc = Rect { x: 10.0, y: 20.0, width: 31.0, height: 30.0 };
    assert_eq!(frame(AT, "abc", TEXT_SIZE), abc, "rounded up, a point more");
    let two = frame(AT, "abcdefgh\nab", TEXT_SIZE);
    assert_eq!((two.width, two.height), (81.0, 60.0), "its widest line, its lines");
    let empty = frame(AT, "", TEXT_SIZE);
    assert_eq!((empty.width, empty.height), (1.0, 30.0), "a point, a line");
}

#[test]
fn a_measure_is_kept_once_taken() {
    fn counting(text: &str, size: f64) -> Size {
        CALLS.set(CALLS.get() + 1);
        fixed(text, size)
    }
    thread_local!(static CALLS: Cell<u32> = const { Cell::new(0) });
    set_measure(counting);
    for _ in 0..3 {
        frame(AT, "kept", TEXT_SIZE);
    }
    frame(AT, "kept", 30.0);
    assert_eq!(CALLS.get(), 2, "once per text and size");
}

/// The core's rows (`slinger_core::render`'s wrap), for a column of whole
/// cells of 10: the same cases its own tests hold it to.
#[test]
fn a_canvas_text_breaks_into_the_core_s_rows() {
    let rows = |text: &str, cells: f64| rows(text, 10.0, cells * 10.0);
    assert_eq!(rows("one two three", 7.0), "one two\nthree");
    assert_eq!(rows("ab cd ef gh", 5.0), "ab cd\nef gh", "a space takes a cell");
    assert_eq!(rows("abcdefghij", 4.0), "abcd\nefgh\nij", "a word longer than a line is cut");
    assert_eq!(rows("a\nb c", 10.0), "a\nb c");
    assert_eq!(rows("  a", 10.0), "  a", "spaces are kept");
    assert_eq!(rows("", 10.0), "");
    assert_eq!(rows("one two", 7.9), "one two", "only whole cells count");
    assert_eq!(rows("no wrap at all", 0.5), "no wrap at all", "narrower than a cell");
    // Zero-width characters draw and take no cell, but are kept.
    assert_eq!(rows("ab\u{200D}cd\u{200B}ef", 6.0), "ab\u{200D}cd\u{200B}ef");
    assert_eq!(rows("one\u{2060} two", 7.0), "one\u{2060} two");
}

/// slinger-f66a97: a canvas text is as tall as the box the core gives it,
/// a size a row, however narrow the system font's glyphs are.
#[test]
fn a_canvas_text_is_a_size_tall_a_row_as_the_core_boxes_it() {
    set_measure(fixed);
    // Size 70 in a column of 40 cells: 120 characters are three rows.
    let words = ["abcdefghi"; 12].join(" ");
    let frame = rows_frame(AT, &words, 70.0, 2800.0);
    assert_eq!(frame.height, 3.0 * 70.0, "the core's 3 rows, not the system font's 2");
    assert_eq!(frame.width, 39.0 * 10.0 + 1.0, "its widest row's glyphs, a point more");
    assert_eq!(rows_frame(AT, "", 70.0, UNWRAPPED).height, 70.0, "empty is one row");
}
