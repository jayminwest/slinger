//! The gesture table (docs/design/figures.md, "Lock"), for the text and
//! arrow tools: a locked text is not edited (a tap starts a new text over
//! it) and an arrow's end on a locked item stays free.

use super::*;
use crate::scene::canvas::Member;

/// [`page`] with its box (i1) and its text (i2) locked in a figure.
fn locked_page() -> Vec<Item> {
    let member = Member { locked: true, fig: Some("sheet".into()), part: None };
    let lock = |i: Item| if i.id.0 <= 2 { Item { member: member.clone(), ..i } } else { i };
    page().into_iter().map(lock).collect()
}

#[test]
fn the_text_tool_on_a_locked_text_starts_a_new_text_over_it() {
    let items = locked_page();
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(down(&mut shapes, &mut ink, Tool::Text, (310.0, 10.0), 0.0, &items));
    assert!(shapes.typed("x", &mut ink));
    assert!(shapes.typed("\u{1b}", &mut ink));
    let [Change::Put { shape: Shape::Text { text, .. }, .. }] = &changes(&mut ink)[..] else {
        panic!("a new text, not an edit of i2")
    };
    assert_eq!(text, "x");
    assert!(down(&mut shapes, &mut ink, Tool::Box, (310.0, 10.0), 1.0, &items));
    assert!(shapes.up(None, &shown(&items), &mut ink));
    assert!(down(&mut shapes, &mut ink, Tool::Box, (310.0, 10.0), 1.1, &items));
    assert!(!shapes.editing(), "a double tap does not open a locked text either");
}

#[test]
fn an_arrow_end_on_a_locked_item_stays_free() {
    let items = locked_page();
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    drag(&mut shapes, &mut ink, Tool::Arrow, (50.0, 50.0), (305.0, -5.0), &items);
    let [Change::Put { shape: Shape::Arrow { bound, .. }, .. }] = &changes(&mut ink)[..] else {
        panic!()
    };
    assert_eq!(*bound, [None, None]);
}
