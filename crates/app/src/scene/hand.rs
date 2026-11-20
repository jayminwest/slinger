//! The hand: hold space and drag with the left button (a mouse, a
//! trackpad or the pen) to pan the pane under the press, as a two-finger
//! scroll there would (docs/design/pen-less.md, pick 1). Pure: `route`
//! hands in space going down or up, whether anything takes typed text, and
//! the presses, drags and lifts; it reads back the pans and the cursor.
//!
//! Space is the hand's only while nothing takes typed text (no canvas text
//! typed, no entry, pop-up, palette, key overlay or page menu open); else it
//! types a space. Once held, its key repeats are the hand's too. A drag
//! begun with space held pans to its lift, even if space comes up first, so
//! no tool sees half a press.

use crate::layout::Point;
use crate::scene::cursor::Cursor;

/// Whether space is held for a pan, and the pan under way.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Hand {
    held: bool,
    /// Where the pan's press was, and where the pointer was last.
    pan: Option<(Point, Point)>,
}

impl Hand {
    /// Space went down (`down`, its repeats too) or up. `free` is whether
    /// nothing takes typed text. True when the hand took it, so it types
    /// nothing; space up is never taken, but ends the hold.
    pub fn space(&mut self, down: bool, free: bool) -> bool {
        self.held = down && (self.held || free);
        self.held
    }

    /// A press at `at`: with space held it starts a pan and is the hand's
    /// (true); else it is not (false).
    pub fn press(&mut self, at: Point) -> bool {
        if self.held {
            self.pan = Some((at, at));
        }
        self.held
    }

    /// A drag to `at` during a pan: where the pan's press was, and how far
    /// the pointer moved since the last, in points. `None` with no pan.
    pub fn drag(&mut self, at: Point) -> Option<(Point, f64, f64)> {
        let (from, last) = self.pan?;
        self.pan = Some((from, at));
        Some((from, at.x - last.x, at.y - last.y))
    }

    /// A lift: the pan ends. True when there was one.
    pub fn lift(&mut self) -> bool {
        self.pan.take().is_some()
    }

    /// The pointer over a canvas: the closed hand during a pan, the open
    /// hand while space is held, else `None` (the tool's).
    pub fn cursor(&self) -> Option<Cursor> {
        match (self.pan, self.held) {
            (Some(_), _) => Some(Cursor::ClosedHand),
            (None, true) => Some(Cursor::OpenHand),
            (None, false) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    #[test]
    fn space_held_turns_a_drag_into_a_pan_from_its_press() {
        let mut hand = Hand::default();
        assert!(hand.space(true, true));
        assert_eq!(hand.cursor(), Some(Cursor::OpenHand));
        assert!(hand.press(p(10.0, 10.0)));
        assert_eq!(hand.cursor(), Some(Cursor::ClosedHand));
        assert_eq!(hand.drag(p(15.0, 7.0)), Some((p(10.0, 10.0), 5.0, -3.0)));
        assert_eq!(hand.drag(p(16.0, 9.0)), Some((p(10.0, 10.0), 1.0, 2.0)), "since the last");
        assert!(hand.lift());
        assert_eq!(hand.cursor(), Some(Cursor::OpenHand), "space is still held");
        assert!(!hand.space(false, true));
        assert_eq!(hand.cursor(), None);
    }

    #[test]
    fn space_types_while_something_takes_text_and_a_press_is_the_tool_s() {
        let mut hand = Hand::default();
        assert!(!hand.space(true, false), "a text is typed: space types");
        assert!(!hand.press(p(1.0, 1.0)));
        assert_eq!((hand.drag(p(2.0, 2.0)), hand.lift(), hand.cursor()), (None, false, None));
    }

    #[test]
    fn a_held_space_keeps_its_repeats_and_a_pan_outlasts_space_up() {
        let mut hand = Hand::default();
        assert!(hand.space(true, true));
        assert!(hand.space(true, false), "a repeat: still the hand's");
        assert!(hand.press(p(0.0, 0.0)));
        assert!(!hand.space(false, true));
        assert_eq!(hand.cursor(), Some(Cursor::ClosedHand), "the pan goes on to its lift");
        assert_eq!(hand.drag(p(4.0, 0.0)), Some((p(0.0, 0.0), 4.0, 0.0)));
        assert!(hand.lift());
        assert!(!hand.press(p(0.0, 0.0)), "space is up: the press is the tool's");
    }
}
