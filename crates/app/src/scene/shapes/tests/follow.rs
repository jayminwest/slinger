//! The arrows bound to the text being typed follow its new frame when it
//! ends (slinger-6fe682), in the same change, so one undo step.

use super::*;
use crate::scene::tools::Resize;

/// [`page`], and an arrow (i4) from the empty middle to the middle of the
/// text's right edge.
fn bound_page() -> Vec<Item> {
    let mut items = page();
    let bound = [None, Some(ItemId(2))];
    let arrow = Shape::Arrow { from: pt(150.0, 200.0), to: pt(396.0, 15.6), width: 2.0, bound };
    items.push(item(4, arrow));
    items
}

#[test]
fn a_resized_text_s_bound_arrows_follow_its_frame_in_the_one_change_that_ends_it() {
    let items = bound_page();
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(down(&mut shapes, &mut ink, Tool::Text, (310.0, 10.0), 0.0, &items));
    assert!(shapes.resize(Resize::Up));
    shapes.typed("s\u{1b}", &mut ink);

    let after = glyphs::rows_frame(pt(300.0, 0.0), "plans", 36.0, glyphs::UNWRAPPED);
    let [Change::Reshape { items: reshaped, .. }] = &changes(&mut ink)[..] else {
        panic!("one reshape")
    };
    let [text, arrow] = &reshaped[..] else { panic!("the text and its one bound arrow") };
    assert_eq!(text.id, ItemId(2));
    assert_eq!(text.colour, BLUE, "it keeps its colour");
    let Shape::Text { frame, size, .. } = &text.shape else { panic!() };
    assert_eq!((*frame, *size), (after, 36.0));
    assert_eq!(arrow.id, ItemId(4));
    let Shape::Arrow { from, to, bound, .. } = arrow.shape else { panic!() };
    assert_eq!((from, bound), (pt(150.0, 200.0), [None, Some(ItemId(2))]), "the free end stays");
    let right = pt(after.x + after.width, after.y + after.height / 2.0);
    assert!((to.x - right.x).abs() < 1e-9 && (to.y - right.y).abs() < 1e-9, "{to:?} {right:?}");
}
