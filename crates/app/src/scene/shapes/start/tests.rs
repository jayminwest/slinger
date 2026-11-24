//! `new-text` chains a text under the one being typed, else starts one at
//! the pointer.

use super::*;
use crate::mirror::TEXT_SIZE;
use crate::scene::canvas::Shape;
use crate::scene::ink::Change;

const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };
const HERE: (&str, &str) = ("main", "p1");

/// Every change waiting in `ink`, oldest first.
fn changes(ink: &mut Ink) -> Vec<Change> {
    let mut all = Vec::new();
    while let Some((key, change)) = ink.next() {
        ink.answered(key, crate::scene::ink::Answer::Applied(1));
        all.push(change);
    }
    all
}

/// The frame each text added to `ink` ends at.
fn added(ink: &mut Ink) -> Vec<Rect> {
    let frames = changes(ink).into_iter().map(|c| match c {
        Change::Put { shape: Shape::Text { frame, .. }, .. } => frame,
        other => panic!("{other:?}"),
    });
    frames.collect()
}

#[test]
fn a_started_text_sits_where_a_tap_puts_it_and_the_next_chains_half_a_line_under() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert_eq!(shapes.chained(&mut ink), None, "no text has ended");
    shapes.start(HERE, Point { x: 40.0, y: 100.0 }, RED, &mut ink);
    let line = glyphs::rows_frame(Point { x: 0.0, y: 0.0 }, "", TEXT_SIZE, glyphs::UNWRAPPED);
    let first = shapes.typing_frame().unwrap();
    assert_eq!((first.x, first.y), (40.0, snap(Point { x: 0.0, y: 100.0 - line.height / 2.0 }).y));
    assert!(shapes.typed("one\rtwo", &mut ink));

    let (canvas, page, at) = shapes.chained(&mut ink).unwrap();
    assert!(!shapes.editing(), "the text being typed ended");
    let [ended] = added(&mut ink)[..] else { panic!("one text added") };
    assert_eq!((canvas.as_str(), page.as_str()), HERE);
    assert_eq!(at, Point { x: 40.0, y: ended.y + ended.height + line.height });
    shapes.start((&canvas, &page), at, RED, &mut ink);
    let next = shapes.typing_frame().unwrap();
    let gap = next.y - (ended.y + ended.height);
    assert!((gap - line.height / 2.0).abs() < 0.1, "half a line between: {gap}");

    // A blank text ends with no change, and the chain still runs from the last one ended.
    shapes.set_size(48.0);
    let (_, _, again) = shapes.chained(&mut ink).unwrap();
    assert_eq!(changes(&mut ink), [], "blank: nothing added");
    assert_eq!(again.x, 40.0);
    assert!(again.y > at.y, "under the blank text, at the new size's line");
}

#[test]
fn the_hint_names_the_new_text_key_when_it_has_one() {
    let mut shapes = Shapes::default();
    assert_eq!(shapes.hint(), "esc or a tap outside ends the text");
    shapes.set_next(Chord::parse("ctrl+return"));
    assert_eq!(shapes.hint(), "esc or a tap outside ends the text · ⌃↩ new text");
    shapes.set_next(Chord::parse("cmd+shift+n"));
    assert_eq!(shapes.hint(), "esc or a tap outside ends the text · ⇧⌘N new text");
}

#[test]
fn start_at_over_a_text_edits_it_and_elsewhere_starts_a_new_one() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    let frame = Rect { x: 10.0, y: 20.0, width: 60.0, height: 30.0 };
    let shape = Shape::Text { frame, text: "old".into(), size: 36.0, column: None };
    let id = crate::scene::canvas::ItemId(2);
    let items = [Cow::Owned(Item::plain(id, RED, shape))];
    shapes.start_at(HERE, (Point { x: 30.0, y: 30.0 }, 4.0), &items, RED, &mut ink);
    assert_eq!(shapes.typing_frame().map(|f| (f.x, f.y)), Some((10.0, 20.0)), "the text, opened");
    assert!(shapes.typed("er\u{1b}", &mut ink), "the caret at its end");
    let [Change::Set { id: set, shape: Shape::Text { text, size, .. }, .. }] =
        &changes(&mut ink)[..]
    else {
        panic!("one set")
    };
    assert_eq!((*set, text.as_str(), *size), (id, "older", 36.0), "an edit keeps its size");

    shapes.start_at(HERE, (Point { x: 200.0, y: 30.0 }, 4.0), &items, RED, &mut ink);
    assert_eq!(shapes.typing_frame().map(|f| f.x), Some(200.0), "away from it: a new text");
    assert!(shapes.typed("new\u{1b}", &mut ink));
    assert_eq!(added(&mut ink).len(), 1);
}
