use super::*;
use crate::mirror::{TEXT_SIZE, text_frame_at};

const AT: Point = Point { x: 10.0, y: 20.0 };

/// Ten points a character, thirty a line, wrapped at the column.
fn fixed(text: &str, _size: f64, column: f64) -> Size {
    let width = |l: &str| f64::from(u32::try_from(l.chars().count()).unwrap()) * 10.0;
    let widest = text.split('\n').map(width).fold(0.0, f64::max);
    let lines = f64::from(u32::try_from(text.split('\n').count()).unwrap());
    let rows = (widest / column).ceil().max(1.0);
    Size { width: widest.min(column), height: 30.0 * lines * rows }
}

#[test]
fn a_text_s_frame_is_its_glyphs_not_the_stored_cells() {
    let frame = frame(AT, "hello world", TEXT_SIZE, UNWRAPPED);
    let stored = text_frame_at(AT, "hello world");
    assert_eq!((frame.x, frame.y), (AT.x, AT.y));
    assert!(frame.width < stored.width * 0.6, "{frame:?} is about half the stored {stored:?}");
    assert!((frame.width - 11.0 * TEXT_SIZE * CHAR_WIDTH).abs() <= 2.0, "rounded up, a point more");
}

#[test]
fn the_measure_platform_hands_in_is_used_and_kept_in_its_column() {
    set_measure(fixed);
    let abc = Rect { x: 10.0, y: 20.0, width: 31.0, height: 30.0 };
    assert_eq!(frame(AT, "abc", TEXT_SIZE, UNWRAPPED), abc, "rounded up, a point more");
    let wrapped = frame(AT, "abcdefgh", TEXT_SIZE, 50.0);
    assert_eq!((wrapped.width, wrapped.height), (50.0, 60.0), "no wider than its column");
    let empty = frame(AT, "", TEXT_SIZE, UNWRAPPED);
    assert_eq!((empty.width, empty.height), (1.0, 30.0), "a point, a line");
}

#[test]
fn a_measure_is_kept_once_taken() {
    fn counting(text: &str, size: f64, column: f64) -> Size {
        CALLS.set(CALLS.get() + 1);
        fixed(text, size, column)
    }
    thread_local!(static CALLS: Cell<u32> = const { Cell::new(0) });
    set_measure(counting);
    for _ in 0..3 {
        frame(AT, "kept", TEXT_SIZE, UNWRAPPED);
    }
    frame(AT, "kept", TEXT_SIZE, 30.0);
    assert_eq!(CALLS.get(), 2, "once per text and column");
}
