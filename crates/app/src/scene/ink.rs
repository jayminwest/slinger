//! The pen's ink (SPEC D6, P7): the stroke under the pen, and the changes
//! made but not yet in the server's copy of their page: strokes drawn, and
//! items erased, deleted or moved ([`Change`]). Pure: `canvases` hands in pen
//! points in canvas units and the daemon's answers, and reads back the page
//! as the user should see it ([`Ink::shown`]) and the next change to send.
//!
//! Ink shows at once from here, laid over the page the server holds. The
//! daemon is told on another thread, one change at a time, in the order the
//! changes were made: a change goes only once every change before it is
//! written, so the canvas file holds them in that order. One change is one
//! `canvas_apply`, so one undo step.
//!
//! Local ink and the server's echo: a change stays here, and shows from here,
//! until the change feed has passed the number it was written under
//! ([`Ink::heard`]). From then on the page's mirror holds it, so it never
//! blinks out between the daemon's reply and the echo. For that moment a
//! stroke draws twice, in the same place; a removal or a move laid over a
//! mirror that already has it changes nothing more, since the mirror takes
//! the feed and the change leaves in the same poll.
//!
//! A change the daemon did not get stays on screen and is sent again, ahead
//! of every change after it. A change the daemon refuses (its canvas is gone)
//! is dropped, and the refusal is the problem shown.
//!
//! Each paint that shows new ink adds its delay from the pen event, and the
//! end of a stroke gives the log line for SPEC O2 ([`Ink::delay_log`]).

use std::borrow::Cow;
use std::collections::HashSet;

use crate::backend::Problem;
use crate::config::Rgb;
use crate::layout::Point;
use crate::scene::canvas::{Item, ItemId, Shape};

/// The pen's line width, in canvas units: tldraw's small pen.
pub const PEN_WIDTH: f64 = 2.0;
/// Ink's item ids start here, far above any id the server hands out, so
/// local ink and the page's items never share an id.
pub const LOCAL: u64 = 1 << 62;

/// Whether `id` is the server's, not local ink's or a mark's drawn over
/// the page: only the server's items can be erased, selected or moved.
pub fn on_server(id: ItemId) -> bool {
    id.0 < LOCAL
}

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

/// One change to a page: one `canvas_apply`, so one undo step.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// A pen line to add.
    Add(Stroke),
    /// Items to remove: one erase gesture, or a deleted selection.
    Remove { canvas: String, page: String, ids: Vec<ItemId> },
    /// Items to move by `by` canvas units: one drag of a selection. `follow`
    /// holds the arrows bound to them, not moved themselves, as they are
    /// after the move: their bound ends follow ([`crate::scene::shapes::follow`]).
    Move { canvas: String, page: String, ids: Vec<ItemId>, by: Point, follow: Vec<Item> },
    /// A text, a box or an arrow to add: one placed text or one drag.
    Put { canvas: String, page: String, colour: Rgb, shape: Shape },
    /// Item `id` drawn as `shape` from now on: one edit of a text.
    Set { canvas: String, page: String, id: ItemId, shape: Shape },
    /// Items carried in from another page, one `add` each: a selection
    /// dragged into this page's pane ([`Ink::carry`]).
    Carry { canvas: String, page: String, items: Vec<Carried> },
}

/// An item carried to another page: as it will draw there (under its old
/// id; the ink shows it under one of its own), and the canvas file's item
/// object its `add` writes, already on that page and moved.
#[derive(Debug, Clone, PartialEq)]
pub struct Carried {
    pub item: Item,
    pub object: serde_json::Value,
}

impl Change {
    /// The canvas it changes.
    pub fn canvas(&self) -> &str {
        match self {
            Change::Add(stroke) => &stroke.canvas,
            Change::Remove { canvas, .. }
            | Change::Move { canvas, .. }
            | Change::Put { canvas, .. }
            | Change::Set { canvas, .. }
            | Change::Carry { canvas, .. } => canvas,
        }
    }

    /// The page it changes.
    pub fn page(&self) -> &str {
        match self {
            Change::Add(stroke) => &stroke.page,
            Change::Remove { page, .. }
            | Change::Move { page, .. }
            | Change::Put { page, .. }
            | Change::Set { page, .. }
            | Change::Carry { page, .. } => page,
        }
    }
}

/// What the daemon said to one change.
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
    /// Written under this number; shown from here until the feed passes it.
    Sent(u64),
}

#[derive(Debug, Clone, PartialEq)]
struct Pending {
    key: u64,
    change: Change,
    state: State,
    /// The change this one goes with: refused, it takes this one too.
    after: Option<u64>,
}

/// The ink and changes not yet in the server's copy of their page.
#[derive(Debug, Default)]
pub struct Ink {
    /// The stroke under the pen, and its key.
    drawing: Option<(u64, Stroke)>,
    /// Finished changes, oldest first.
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
        let change = Change::Add(stroke);
        self.pending.push(Pending { key, change, state: State::Waiting, after: None });
        true
    }

    /// A finished erase, delete, move, put or edit: it shows at once and waits its
    /// turn to be sent, after every change made before it.
    pub fn push(&mut self, change: Change) {
        self.queue(change, None);
    }

    /// A selection carried to another page: the `add` of its items there,
    /// then the `remove` of them here, each one step on its canvas. Both
    /// show at once; the removal goes only once the add is written, and is
    /// dropped with it if the add is refused, so no item is ever lost.
    pub fn carry(&mut self, to: Change, from: Change) {
        let key = self.queue(to, None);
        self.queue(from, Some(key));
    }

    /// Queues `change` after every change before it; its key. A carry's
    /// items each show under a key of their own.
    fn queue(&mut self, change: Change, after: Option<u64>) -> u64 {
        let key = self.next_key;
        let keys = match &change {
            Change::Carry { items, .. } => u64::try_from(items.len()).unwrap_or(1).max(1),
            _ => 1,
        };
        self.pending.push(Pending { key, change, state: State::Waiting, after });
        self.next_key += keys;
        key
    }

    /// The next change to send, and its key, now marked as sending. `None`
    /// while one is sending: changes go one at a time, oldest first, so a
    /// change sent again stays ahead of every change after it.
    pub fn next(&mut self) -> Option<(u64, Change)> {
        if self.pending.iter().any(|p| p.state == State::Sending) {
            return None;
        }
        let first = self.pending.iter_mut().find(|p| p.state == State::Waiting)?;
        first.state = State::Sending;
        Some((first.key, first.change.clone()))
    }

    /// The daemon's answer to the change sent under `key`.
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
                self.pending.retain(|p| p.after != Some(key));
                self.problem = Some(problem);
            }
        }
    }

    /// The change feed of `canvas` has passed line `upto`: every change
    /// written at or below it is in the page's mirror now, and leaves.
    pub fn heard(&mut self, canvas: &str, upto: u64) {
        self.pending.retain(|p| {
            p.change.canvas() != canvas || !matches!(p.state, State::Sent(seq) if seq <= upto)
        });
    }

    /// `items`, the server's copy of `page` of `canvas`, as the user should
    /// see it: each change not yet heard laid over it in the order made
    /// (removed items gone, moved ones moved, edited ones edited, new
    /// strokes, texts, boxes and arrows on top), then the
    /// stroke under the pen. Items no change touches are borrowed, not
    /// copied, so a big page costs little.
    pub fn shown<'a>(&self, canvas: &str, page: &str, items: &'a [Item]) -> Vec<Cow<'a, Item>> {
        let mut shown: Vec<Cow<'a, Item>> = items.iter().map(Cow::Borrowed).collect();
        let here = |c: &str, p: &str| c == canvas && p == page;
        for pending in self.pending.iter().filter(|p| here(p.change.canvas(), p.change.page())) {
            match &pending.change {
                Change::Add(stroke) => shown.push(Cow::Owned(local(pending.key, stroke))),
                Change::Carry { items, .. } => {
                    shown.extend(items.iter().zip(pending.key..).map(|(c, key)| {
                        Cow::Owned(Item { id: ItemId(LOCAL + key), ..c.item.clone() })
                    }));
                }
                Change::Remove { ids, .. } => {
                    let ids: HashSet<&ItemId> = ids.iter().collect();
                    shown.retain(|i| !ids.contains(&i.id));
                }
                Change::Move { ids, by, follow, .. } => {
                    let ids: HashSet<&ItemId> = ids.iter().collect();
                    for item in &mut shown {
                        if let Some(after) = follow.iter().find(|f| f.id == item.id) {
                            *item = Cow::Owned(after.clone());
                        } else if ids.contains(&item.id) {
                            let (id, colour) = (item.id, item.colour);
                            *item = Cow::Owned(Item { id, colour, shape: item.shape.moved(*by) });
                        }
                    }
                }
                Change::Put { colour, shape, .. } => {
                    let id = ItemId(LOCAL + pending.key);
                    shown.push(Cow::Owned(Item { id, colour: *colour, shape: shape.clone() }));
                }
                Change::Set { id, shape, .. } => {
                    for item in shown.iter_mut().filter(|i| i.id == *id) {
                        let colour = item.colour;
                        *item = Cow::Owned(Item { id: *id, colour, shape: shape.clone() });
                    }
                }
            }
        }
        let drawing = self.drawing.iter().filter(|(_, s)| here(&s.canvas, &s.page));
        shown.extend(drawing.map(|(key, stroke)| Cow::Owned(local(*key, stroke))));
        shown
    }

    /// Why the last change sent did not go, until one goes.
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

/// A stroke of local ink as an item, under an id no server item has.
fn local(key: u64, stroke: &Stroke) -> Item {
    Item {
        id: ItemId(LOCAL + key),
        colour: stroke.colour,
        shape: Shape::Stroke { points: stroke.points.clone(), width: stroke.width },
    }
}

#[cfg(test)]
mod tests;
