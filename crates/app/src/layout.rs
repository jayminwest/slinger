//! Layout math. Pure: sizes in, rectangles out, no `AppKit`, so every branch is
//! unit-tested (this file has a 100% coverage floor). Units are points. The
//! origin is the top left; `platform` converts to `AppKit`'s bottom left.

/// A width and a height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// A rectangle, by its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The content size of the window.
pub const WINDOW: Size = Size { width: 440.0, height: 160.0 };

const MARGIN: f64 = 24.0;
const LINE_HEIGHT: f64 = 22.0;
const LINE_GAP: f64 = 6.0;

/// One rectangle per text line: full width inside the margins, stacked, and
/// centered vertically in `window`. A stack taller than the window starts at
/// the top margin and runs off the bottom.
pub fn lines(window: Size, count: usize) -> Vec<Rect> {
    let width = (window.width - 2.0 * MARGIN).max(0.0);
    let step = LINE_HEIGHT + LINE_GAP;
    let offsets: Vec<f64> =
        std::iter::successors(Some(0.0), |y| Some(y + step)).take(count).collect();
    let stack = offsets.last().map_or(0.0, |last| last + LINE_HEIGHT);
    let top = ((window.height - stack) / 2.0).max(MARGIN);
    offsets.iter().map(|y| Rect { x: MARGIN, y: top + y, width, height: LINE_HEIGHT }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDE: Size = Size { width: 400.0, height: 200.0 };

    #[test]
    fn centers_one_line() {
        let expected = Rect { x: 24.0, y: 89.0, width: 352.0, height: 22.0 };
        assert_eq!(lines(WIDE, 1), [expected]);
    }

    #[test]
    fn stacks_lines_with_a_gap_and_centers_the_stack() {
        let at = |y| Rect { x: 24.0, y, width: 352.0, height: 22.0 };
        assert_eq!(lines(WIDE, 2), [at(75.0), at(103.0)]);
    }

    #[test]
    fn no_lines_is_no_rectangles() {
        assert_eq!(lines(WIDE, 0), []);
    }

    #[test]
    fn a_window_too_small_keeps_the_top_margin_and_a_zero_width() {
        let rects = lines(Size { width: 30.0, height: 40.0 }, 2);
        assert_eq!(rects[0], Rect { x: 24.0, y: 24.0, width: 0.0, height: 22.0 });
        assert_eq!(rects[1].y, 52.0);
    }

    #[test]
    fn the_window_fits_two_lines_inside_its_margins() {
        let rects = lines(WINDOW, 2);
        assert!(rects[0].y >= MARGIN);
        assert!(rects[1].y + rects[1].height <= WINDOW.height - MARGIN);
    }
}
