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
//! `canvas_apply`, so one undo step. An undo, a redo and a change to the
//! pages wait their turn in the same queue: an undo pressed right after a
//! stroke undoes that stroke. They show nothing here; the feed brings them.
//!
//! Local ink and the server's echo: a change stays here, and shows from here,
//! until the change feed has passed the number it was written under
//! ([`Ink::heard`]). From then on the page's mirror holds it, so it never
//! blinks out between the daemon's reply and the echo. An item added shows
//! under an id of the ink's own until the daemon says the id it made
//! ([`Answer::Made`]), and under that id from then on; once the mirror
//! holds that id it draws from there alone. The eraser, the lasso and the
//! select tool take it under either: a change that names it is renamed to
//! the server's id before it goes ([`ids`]), so it is still one step. A
//! removal or a move laid over a
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
use std::collections::{HashMap, HashSet};

use crate::backend::Problem;
use crate::config::Rgb;
use crate::layout::Point;
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::pages::PageOp;

mod delay;
mod ids;
mod once;

/// The pen's line width until one is set ([`Ink::set_width`]), in canvas
/// units: tldraw's small pen.
pub const PEN_WIDTH: f64 = 2.0;
/// Ink's item ids start here, far above any id the server hands out, so
/// ink and the page's items never share an id: the stroke under the pen,
/// and the items changes not yet written add, until the daemon makes them
/// (then they show under the server's id).
pub const PENDING: u64 = 1 << 61;
/// No item is ever taken under an id from here up: the marks drawn over a
/// page are.
pub const LOCAL: u64 = 1 << 62;

/// Whether `id` is the server's, or will be once a change on its way there
/// is written ([`PENDING`]); not a mark's drawn over the page. Only these
/// items can be erased, selected or moved.
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
    /// Whether a tablet pen's eraser end made it: it erases with any tool
    /// ([`crate::scene::tools::Tools::set_eraser_end`]).
    pub eraser: bool,
    /// Whether shift was held: the select tool's press adds to the
    /// selection, or takes an item out of it.
    pub shift: bool,
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

/// One change to a page or a canvas: one `canvas_apply`, so one undo step,
/// or one `canvas_undo` or `canvas_redo`.
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
    /// Each of `items` drawn as it is there from now on, by its id: one
    /// resize of a selection, with the arrows that follow it.
    Reshape { canvas: String, page: String, items: Vec<Item> },
    /// Items carried in from another page, one `add` each: a selection
    /// dragged into this page's pane. From a page of the same canvas,
    /// `gone` names the items it takes from there, in the same step;
    /// from another canvas it is empty ([`Ink::carry`]).
    Carry { canvas: String, page: String, items: Vec<Carried>, gone: Vec<ItemId> },
    /// A change to the canvas's pages.
    Page { canvas: String, op: PageOp },
    /// The user's last step on the canvas undone, or redone when `redo`.
    /// Steps are per author, so an agent's items are never undone by it.
    Step { canvas: String, redo: bool },
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
            | Change::Reshape { canvas, .. }
            | Change::Carry { canvas, .. }
            | Change::Page { canvas, .. }
            | Change::Step { canvas, .. } => canvas,
        }
    }

    /// The page whose items it changes as it shows here; `None` for a
    /// change to the pages, an undo or a redo, which show nothing here.
    pub fn page(&self) -> Option<&str> {
        match self {
            Change::Add(stroke) => Some(&stroke.page),
            Change::Remove { page, .. }
            | Change::Move { page, .. }
            | Change::Put { page, .. }
            | Change::Set { page, .. }
            | Change::Reshape { page, .. }
            | Change::Carry { page, .. } => Some(page),
            Change::Page { .. } | Change::Step { .. } => None,
        }
    }
}

/// What the daemon said to one change.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// Written; the number of its last line in the canvas file, or 0 when
    /// it wrote none (an undo with nothing to undo): nothing to wait for.
    Applied(u64),
    /// A stroke, a put or a carry written, under that number: the ids of
    /// the items it made, in the order of its adds.
    Made { seq: u64, ids: Vec<ItemId> },
    /// A new page written, under that number: its id.
    NewPage { seq: u64, page: String },
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
    /// The ids the daemon made for the items it adds, once it said.
    made: Vec<ItemId>,
    /// The key it is applied once under, when not this run's ([`once`]).
    once: Option<String>,
}

impl Pending {
    fn new(key: u64, change: Change, after: Option<u64>) -> Self {
        Self { key, change, state: State::Waiting, after, made: Vec::new(), once: None }
    }

    /// The id the `n`th item it adds shows under: the server's once made.
    fn id(&self, n: u64) -> ItemId {
        let made = usize::try_from(n).ok().and_then(|n| self.made.get(n));
        made.copied().unwrap_or(ItemId(PENDING + self.key + n))
    }
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
    /// The width a new stroke takes; none is [`PEN_WIDTH`].
    width: Option<f64>,
    /// Each id of the ink's own the daemon made an item of, and the id it
    /// made. Kept for the app's life (a few bytes a change), so a gesture
    /// still holding an old id names the item made ([`ids`]).
    made: HashMap<ItemId, ItemId>,
    /// What this run's keys for applying once start with ([`once`]).
    run: String,
}

impl Ink {
    /// The width a new stroke takes, in canvas units.
    pub fn width(&self) -> f64 {
        self.width.unwrap_or(PEN_WIDTH)
    }

    /// New strokes take `width`, in canvas units; the stroke under the pen
    /// keeps its own.
    pub fn set_width(&mut self, width: f64) {
        self.width = Some(width);
    }

    /// The pen went down at `at` on `page` of `canvas`: a new stroke of one
    /// point. A stroke still under the pen (its lift was lost) is finished.
    pub fn down(&mut self, canvas: &str, page: &str, colour: Rgb, at: Point) {
        self.up(None);
        let stroke = Stroke {
            canvas: canvas.to_string(),
            page: page.to_string(),
            colour,
            width: self.width(),
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
        self.pending.push(Pending::new(key, Change::Add(stroke), None));
        true
    }

    /// A finished erase, delete, move, put or edit: it shows at once and waits its
    /// turn to be sent, after every change made before it.
    pub fn push(&mut self, change: Change) {
        self.queue(change, None);
    }

    /// As [`Ink::push`], and the key its answer comes back under.
    pub fn push_keyed(&mut self, change: Change) -> u64 {
        self.queue(change, None)
    }

    /// A selection carried to another page: the `add` of its items there,
    /// then the `remove` of them here, each one step on its canvas. Both
    /// show at once; the removal goes only once the add is written, and is
    /// dropped with it if the add is refused, so no item is ever lost.
    /// Returns the key of the add.
    pub fn carry(&mut self, to: Change, from: Change) -> u64 {
        let key = self.queue(to, None);
        self.queue(from, Some(key));
        key
    }

    /// Queues `change` after every change before it, the items it names
    /// that the daemon has made under the ids it made; its key. A carry's
    /// items each show under a key of their own.
    fn queue(&mut self, mut change: Change, after: Option<u64>) -> u64 {
        change.rename(|id| Some(ids::known(&self.made, id)));
        let key = self.next_key;
        let keys = match &change {
            Change::Carry { items, .. } => u64::try_from(items.len()).unwrap_or(1).max(1),
            _ => 1,
        };
        self.pending.push(Pending::new(key, change, after));
        self.next_key += keys;
        key
    }

    /// The next change to send, and its key, now marked as sending. `None`
    /// while one is sending: changes go one at a time, oldest first, so a
    /// change sent again stays ahead of every change after it. It names
    /// items by the server's ids: one naming only items never made (their
    /// add was refused) is dropped unsent.
    pub fn next(&mut self) -> Option<(u64, Change)> {
        if self.pending.iter().any(|p| p.state == State::Sending) {
            return None;
        }
        loop {
            let at = self.pending.iter().position(|p| p.state == State::Waiting)?;
            let made = &self.made;
            let first = &mut self.pending[at];
            if first.change.rename(|id| ids::sent(made, id)) {
                first.state = State::Sending;
                return Some((first.key, first.change.clone()));
            }
            self.pending.remove(at);
        }
    }

    /// The daemon's answer to the change sent under `key`. Returns the
    /// items it made that showed under an id of the ink's own: that id,
    /// and the one each shows under from now on.
    pub fn answered(&mut self, key: u64, answer: Answer) -> Vec<(ItemId, ItemId)> {
        let Some(at) = self.pending.iter().position(|p| p.key == key) else { return Vec::new() };
        match answer {
            Answer::Made { seq, ids } => {
                let mine = (0..).map(|n| self.pending[at].id(n));
                let renamed: Vec<_> = mine.zip(ids.iter().copied()).collect();
                (self.pending[at].state, self.pending[at].made) = (State::Sent(seq), ids);
                self.made.extend(renamed.iter().copied());
                let made = &self.made;
                for pending in &mut self.pending {
                    pending.change.rename(|id| Some(ids::known(made, id)));
                }
                self.problem = None;
                return renamed;
            }
            Answer::Applied(seq) | Answer::NewPage { seq, .. } => {
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
        Vec::new()
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
    /// strokes, texts, boxes and arrows on top, each under the server's id
    /// once made, and not at all once `items` holds it), then the
    /// stroke under the pen. Items no change touches are borrowed, not
    /// copied, so a big page costs little.
    pub fn shown<'a>(&self, canvas: &str, page: &str, items: &'a [Item]) -> Vec<Cow<'a, Item>> {
        let mut shown: Vec<Cow<'a, Item>> = items.iter().map(Cow::Borrowed).collect();
        let made = self.pending.iter().any(|p| !p.made.is_empty());
        let held: HashSet<ItemId> = items.iter().filter(|_| made).map(|i| i.id).collect();
        let new = |id: &ItemId| !held.contains(id);
        let here = |c: &str, p: &str| c == canvas && p == page;
        // A carry within the canvas takes its items from whatever page they were on.
        let mine = |p: &&Pending| {
            let carry = matches!(&p.change, Change::Carry { gone, .. } if !gone.is_empty());
            let on = p.change.page().is_some_and(|page| here(p.change.canvas(), page));
            on || (carry && p.change.canvas() == canvas)
        };
        for pending in self.pending.iter().filter(mine) {
            match &pending.change {
                Change::Add(stroke) => {
                    let id = pending.id(0);
                    shown.extend(new(&id).then(|| Cow::Owned(local(id, stroke))));
                }
                Change::Carry { page: to, items, gone, .. } => {
                    let gone: HashSet<&ItemId> = gone.iter().collect();
                    shown.retain(|i| !gone.contains(&i.id));
                    let ids = (0..).map(|n| pending.id(n));
                    let landed = items.iter().zip(ids).filter(|(_, id)| to == page && new(id));
                    shown.extend(landed.map(|(c, id)| Cow::Owned(Item { id, ..c.item.clone() })));
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
                    let id = pending.id(0);
                    let item = Item { id, colour: *colour, shape: shape.clone() };
                    shown.extend(new(&id).then_some(Cow::Owned(item)));
                }
                Change::Set { id, shape, .. } => {
                    for item in shown.iter_mut().filter(|i| i.id == *id) {
                        let colour = item.colour;
                        *item = Cow::Owned(Item { id: *id, colour, shape: shape.clone() });
                    }
                }
                Change::Reshape { items: after, .. } => {
                    for item in &mut shown {
                        if let Some(after) = after.iter().find(|a| a.id == item.id) {
                            *item = Cow::Owned(after.clone());
                        }
                    }
                }
                Change::Page { .. } | Change::Step { .. } => {}
            }
        }
        let drawing = self.drawing.iter().filter(|(_, s)| here(&s.canvas, &s.page));
        shown.extend(drawing.map(|(key, stroke)| Cow::Owned(local(ItemId(PENDING + key), stroke))));
        shown
    }

    /// Whether every finished change of `canvas` is written: none waits or
    /// is sending. A sling waits for it, so its packet holds them.
    pub fn settled(&self, canvas: &str) -> bool {
        let busy = |p: &Pending| matches!(p.state, State::Waiting | State::Sending);
        !self.pending.iter().any(|p| p.change.canvas() == canvas && busy(p))
    }

    /// The finished strokes the daemon has not written yet, oldest first,
    /// each with its key: what is lost if the app quits now.
    pub fn unsent(&self) -> impl Iterator<Item = (u64, &Stroke)> {
        self.pending.iter().filter_map(|p| match (&p.change, p.state) {
            (Change::Add(stroke), State::Waiting | State::Sending) => Some((p.key, stroke)),
            _ => None,
        })
    }

    /// Why the last change sent did not go, until one goes.
    pub fn problem(&self) -> Option<&Problem> {
        self.problem.as_ref()
    }
}

/// A stroke of local ink as an item, under `id`.
fn local(id: ItemId, stroke: &Stroke) -> Item {
    Item {
        id,
        colour: stroke.colour,
        shape: Shape::Stroke { points: stroke.points.clone(), width: stroke.width },
    }
}

#[cfg(test)]
mod tests;
