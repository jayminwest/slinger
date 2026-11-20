//! The right and middle buttons (docs/design/pen-less.md): a drag pans the
//! pane under the press, as a two-finger scroll there would; a right
//! click (the secondary button, also a ctrl+click, a two-finger trackpad
//! click or a tablet button set to right click) is the context click.
//! Pure: `route` hands in the presses, drags and lifts; it reads back the
//! pans and the click.
//!
//! The pan waits until the pointer has gone more than [`SLOP`] from the
//! press, so a click does not nudge the canvas by its jitter; then it pans
//! the whole way so far at once, so the canvas does not lag the pointer.
//! A press that once left the slop is a pan to its lift, even if it comes
//! back. The middle button only pans.

use crate::layout::Point;

/// How far the pointer may move, in points, and still click.
pub const SLOP: f64 = 4.0;

/// Which button: the secondary (right) one, or the middle one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Secondary,
    Middle,
}

/// Where in its press a button event is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Down,
    Drag,
    Up,
}

/// The right or middle press under way, if any.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Secondary {
    press: Option<Press>,
}

/// A press: its button, where it went down, where the pointer was last
/// panned to, and whether it has left the slop.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Press {
    button: Button,
    from: Point,
    last: Point,
    panning: bool,
}

impl Secondary {
    /// `button` went down at `at`; a press under way (its lift lost) is
    /// replaced.
    pub fn down(&mut self, button: Button, at: Point) {
        self.press = Some(Press { button, from: at, last: at, panning: false });
    }

    /// The pointer dragged to `at`: where the press was and how far to pan
    /// since the last pan, in points. `None` while it is within [`SLOP`] of
    /// the press and has never left it, and with no press.
    pub fn drag(&mut self, at: Point) -> Option<(Point, f64, f64)> {
        let press = self.press.as_mut()?;
        press.panning = press.panning || far(press.from, at);
        if !press.panning {
            return None;
        }
        let (dx, dy) = (at.x - press.last.x, at.y - press.last.y);
        press.last = at;
        Some((press.from, dx, dy))
    }

    /// The button came up at `at`: the press point when it was a context
    /// click (the secondary button, never past the slop). `None` after a
    /// pan, for the middle button, and with no press.
    pub fn up(&mut self, at: Point) -> Option<Point> {
        let press = self.press.take()?;
        let click = press.button == Button::Secondary && !press.panning && !far(press.from, at);
        click.then_some(press.from)
    }

    /// Whether a press is under way.
    pub fn held(&self) -> bool {
        self.press.is_some()
    }

    /// Forgets the press under way: another view took it.
    pub fn drop_press(&mut self) {
        self.press = None;
    }
}

/// Whether `at` is more than [`SLOP`] from `from`.
fn far(from: Point, at: Point) -> bool {
    (at.x - from.x).hypot(at.y - from.y) > SLOP
}

#[cfg(test)]
mod tests;
