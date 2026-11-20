//! Quick taps inside the text being typed (slinger-ed74af).

use super::*;

#[test]
fn a_double_tap_inside_the_text_selects_a_word_a_triple_tap_its_row() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(down(&mut shapes, &mut ink, Tool::Text, (10.0, 212.0), 0.0, &[]));
    shapes.typed("one two\nthree", &mut ink);
    let mut tap = |shapes: &mut Shapes, when| {
        assert!(down(shapes, &mut ink, Tool::Box, (12.0, 212.0), when, &[]));
        assert!(shapes.drag(pt(12.5, 212.0)), "a pen's jitter");
        assert!(shapes.up(None, &[], &mut ink));
        shapes.copy(false)
    };
    assert_eq!(tap(&mut shapes, 0.2), None, "the tap that opened it does not count");
    assert_eq!(tap(&mut shapes, 0.4).as_deref(), Some("one"));
    assert_eq!(tap(&mut shapes, 0.6).as_deref(), Some("one two"));
    assert_eq!(tap(&mut shapes, 5.0), None, "a slow tap places the caret");
}
