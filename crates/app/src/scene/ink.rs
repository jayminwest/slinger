//! The pen's ink (SPEC D6, P7): the stroke under the pen, and the strokes
//! drawn but not yet in the server's copy of their page. Pure: `canvases`
//! hands in pen points in canvas units and the daemon's answers, and reads
//! back the items to draw over the page and the next stroke to send.
//!
//! Ink shows at once from here, drawn over the page the server holds. The
//! daemon is told on another thread, one stroke at a time, in the order the
//! strokes were drawn: a stroke goes only once every stroke before it is
//! written, so the canvas file holds them in that order. One stroke is one
//! `add`, so one undo step.
//!
//! Local ink and the server's echo: a stroke stays here, and draws from here,
//! until the change feed has passed the number its `add` was written under
//! ([`Ink::heard`]). From then on the page's mirror holds it, so it never
//! blinks out between the daemon's reply and the echo. For that moment both
//! draw, in the same place.
//!
//! A stroke the daemon did not get stays on screen and is sent again, ahead
//! of every stroke after it. A stroke the daemon refuses (its canvas is gone)
//! is dropped, and the refusal is the problem shown.
//!
//! Each paint that shows new ink adds its delay from the pen event, and the
//! end of a stroke gives the log line for SPEC O2 ([`Ink::delay_log`]).

use crate::backend::Problem;
use crate::config::Rgb;
use crate::layout::Point;
use crate::scene::canvas::{Item, ItemId, Shape};

/// The pen's line width, in canvas units: tldraw's small pen.
pub const PEN_WIDTH: f64 = 2.0;
/// Ink's item ids start here, far above any id the server hands out, so
/// local ink and the page's items never share an id.
const LOCAL: u64 = 1 << 62;

/// One pen, mouse or touch event, as `platform` hears it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pen {
    /// Where, in the window's points from its top left.
    pub at: Point,
    /// When, in seconds since the machine started.
    pub when: f64,
    /// Whether a finger made it (a touch the system turned into a mouse
    /// event): a palm on a tablet display. Touch never draws.
    pub touch: bool,
}

/// A pen line on a page, in canvas units.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub canvas: String,
    pub page: String,
    pub colour: Rgb,
    pub width: f64,
    pub points: Vec<Point>,
}

/// What the daemon said to one stroke.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// Written; the number of its line in the canvas file.
    Applied(u64),
    /// Not reached: send it again.
    Unreachable(Problem),
    /// Refused: drop it.
    Refused(Problem),
}

/// Where a finished stroke is on its way to the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Waiting,
    Sending,
    /// Written under this number; drawn from here until the feed passes it.
    Sent(u64),
}

#[derive(Debug, Clone, PartialEq)]
struct Pending {
    key: u64,
    stroke: Stroke,
    state: State,
}

/// The ink not yet in the server's copy of its page.
#[derive(Debug, Default)]
pub struct Ink {
    /// The stroke under the pen, and its key.
    drawing: Option<(u64, Stroke)>,
    /// Finished strokes, oldest first.
    pending: Vec<Pending>,
    next_key: u64,
    problem: Option<Problem>,
    /// Pen-to-paint delays of the stroke under the pen, in seconds.
    delays: Vec<f64>,
}

impl Ink {
    /// The pen went down at `at` on `page` of `canvas`: a new stroke of one
    /// point. A stroke still under the pen (its lift was lost) is finished.
    pub fn down(&mut self, canvas: &str, page: &str, colour: Rgb, at: Point) {
        self.up(None);
        let stroke = Stroke {
            canvas: canvas.to_string(),
            page: page.to_string(),
            colour,
            width: PEN_WIDTH,
            points: vec![at],
        };
        self.drawing = Some((self.next_key, stroke));
        self.next_key += 1;
        self.delays.clear();
    }

    /// The pen moved to `at` while down. Returns false when no stroke is
    /// under it. A point where the last one is adds nothing.
    pub fn drag(&mut self, at: Point) -> bool {
        let Some((_, stroke)) = &mut self.drawing else { return false };
        if stroke.points.last() != Some(&at) {
            stroke.points.push(at);
        }
        true
    }

    /// The pen lifted, at `at` if it said where: the stroke is finished and
    /// waits its turn to be sent. Returns false when no stroke was under it.
    pub fn up(&mut self, at: Option<Point>) -> bool {
        if let Some(at) = at {
            self.drag(at);
        }
        let Some((key, stroke)) = self.drawing.take() else { return false };
        self.pending.push(Pending { key, stroke, state: State::Waiting });
        true
    }

    /// The next stroke to send, and its key, now marked as sending. `None`
    /// while one is sending: strokes go one at a time, oldest first, so a
    /// stroke sent again stays ahead of every stroke after it.
    pub fn next(&mut self) -> Option<(u64, Stroke)> {
        if self.pending.iter().any(|p| p.state == State::Sending) {
            return None;
        }
        let first = self.pending.iter_mut().find(|p| p.state == State::Waiting)?;
        first.state = State::Sending;
        Some((first.key, first.stroke.clone()))
    }

    /// The daemon's answer to the stroke sent under `key`.
    pub fn answered(&mut self, key: u64, answer: Answer) {
        let Some(at) = self.pending.iter().position(|p| p.key == key) else { return };
        match answer {
            Answer::Applied(seq) => {
                self.pending[at].state = State::Sent(seq);
                self.problem = None;
            }
            Answer::Unreachable(problem) => {
                self.pending[at].state = State::Waiting;
                self.problem = Some(problem);
            }
            Answer::Refused(problem) => {
                self.pending.remove(at);
                self.problem = Some(problem);
            }
        }
    }

    /// The change feed of `canvas` has passed line `upto`: every stroke
    /// written at or below it is in the page's mirror now, and leaves.
    pub fn heard(&mut self, canvas: &str, upto: u64) {
        self.pending.retain(|p| {
            p.stroke.canvas != canvas || !matches!(p.state, State::Sent(seq) if seq <= upto)
        });
    }

    /// The ink on `page` of `canvas`, to draw over the page's items: the
    /// finished strokes oldest first, then the one under the pen.
    pub fn items(&self, canvas: &str, page: &str) -> Vec<Item> {
        let finished = self.pending.iter().map(|p| (p.key, &p.stroke));
        let drawing = self.drawing.iter().map(|(key, stroke)| (*key, stroke));
        finished
            .chain(drawing)
            .filter(|(_, s)| s.canvas == canvas && s.page == page)
            .map(|(key, s)| Item {
                id: ItemId(LOCAL + key),
                colour: s.colour,
                shape: Shape::Stroke { points: s.points.clone(), width: s.width },
            })
            .collect()
    }

    /// Why the last stroke sent did not go, until one goes.
    pub fn problem(&self) -> Option<&Problem> {
        self.problem.as_ref()
    }

    /// A paint showed ink `delay` seconds after its pen event.
    pub fn painted(&mut self, delay: f64) {
        if self.drawing.is_some() && delay.is_finite() {
            self.delays.push(delay);
        }
    }

    /// The pen-to-paint delays of the stroke last drawn as one line, and
    /// then forgets them; `None` without any.
    pub fn delay_log(&mut self) -> Option<String> {
        if self.delays.is_empty() {
            return None;
        }
        let mut ms: Vec<f64> = self.delays.drain(..).map(|s| s * 1000.0).collect();
        ms.sort_by(f64::total_cmp);
        let at = |part: f64| {
            #[expect(clippy::cast_precision_loss, reason = "a stroke has far fewer paints")]
            let last = (ms.len() - 1) as f64;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a rounded index into the list"
            )]
            ms[(last * part).round() as usize]
        };
        let (median, high, most) = (at(0.5), at(0.95), at(1.0));
        Some(format!(
            "ink: pen to paint over {} paints, ms: median {median:.1}, 95th {high:.1}, most {most:.1}",
            ms.len()
        ))
    }
}

#[cfg(test)]
mod tests;
