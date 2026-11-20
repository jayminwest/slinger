//! The synthetic canvas for measuring (slinger-6c0f10): over 5 MB, the same
//! bytes for the same settings, and a file that opens like the daemon's own.
#![cfg(test)]

use slinger_core::canvas::Shape;
use slinger_core::canvas::file::{Log, Synthetic};

#[test]
fn the_synthetic_canvas_is_big_the_same_each_time_and_opens() {
    let bytes = Synthetic::default().file("big").unwrap();
    assert!(bytes.len() > 5 * 1024 * 1024, "{} bytes", bytes.len());

    let (log, torn) = Log::open(bytes.as_bytes()).unwrap();
    assert_eq!(torn, 0);
    let canvas = log.canvas();
    let pages: Vec<_> = canvas.pages().map(slinger_core::canvas::Page::id).collect();
    assert_eq!(pages.len(), 6);
    let items: Vec<_> = pages.iter().flat_map(|&p| canvas.items(p)).collect();
    let count = |f: fn(&Shape) -> bool| items.iter().filter(|i| f(i.shape())).count();
    assert!(count(|s| matches!(s, Shape::Stroke { .. })) > 5000);
    assert!(count(|s| matches!(s, Shape::Text { .. })) > 200);
    assert!(count(|s| matches!(s, Shape::Arrow { .. })) > 20);
    assert_eq!(count(|s| matches!(s, Shape::Pdf { .. })), 4);
    assert!(count(|s| matches!(s, Shape::Box { fold: Some(_), .. })) > 0);
    let sketch = |s: &Shape| matches!(s, Shape::Stroke { points, .. } if points.len() > 500);
    assert!(items.iter().filter(|i| sketch(i.shape())).count() > 100);
    // The churn of real use: moves, erases, edits, undo and redo.
    let ops = |op: &str| bytes.matches(&format!(r#""op":"{op}""#)).count();
    for op in ["move", "remove", "set", "restore"] {
        assert!(ops(op) > 50, "{op}: {}", ops(op));
    }
}

#[test]
fn the_same_settings_make_the_same_bytes_and_another_seed_others() {
    let small = |seed| Synthetic { sessions: 6, seed, ..Synthetic::default() }.file("s").unwrap();
    assert_eq!(small(1), small(1));
    assert_ne!(small(1), small(2));
}
