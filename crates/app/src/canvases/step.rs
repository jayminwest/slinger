//! Stepping a pane into a box (docs/design/fold.md, "Step in"): `step-in`
//! (or two quick taps on a folded box's miniature, with any tool but the
//! pen and the eraser) makes the pane show only what the box holds, from
//! its top left; `step-out` (or escape with nothing selected) goes back up
//! one box, to the view the pane had. `step-in-beside` splits the pane and
//! steps the new one in, so two boxes show side by side. The trail is
//! view state of the pane, like its page, never in the canvas file: a
//! pane turned to another page leaves it, and a box erased, moved out, or
//! folded or unfolded elsewhere steps the pane out of it ([`Mirror::step`]).
//!
//! What is written in a pane inside a folded box joins it: the pen's
//! strokes, boxes and arrows from the press to the lift, a text from its
//! start to its end, and what is pasted, duplicated or carried into the
//! pane, each sent with `in` ([`Ink::join`]).

use std::borrow::Cow;

use super::Canvases;
use crate::layout::{PaneId, Point, Rect, Viewport};
use crate::mirror::{Inside, LINE_WIDTH, Mirror};
use crate::panes::Panes;
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::ink::Ink;

/// The draw id of the dashed frame round the room a pane is inside: below
/// a peek's paper ([`super::fold`]).
const ROOM: ItemId = ItemId((1 << 60) - 2);

/// The boxes a pane is inside, outermost first, on one page.
#[derive(Debug, Default, Clone)]
pub(super) struct Trail {
    here: Option<(String, String)>,
    steps: Vec<Step>,
}

/// One box stepped into: whether it was folded then, and the view the
/// pane had before.
#[derive(Debug, Clone, Copy)]
struct Step {
    id: ItemId,
    folded: bool,
    back: Viewport,
}

impl Trail {
    /// The boxes stepped into on `page` of `canvas`, as [`Mirror::step`]
    /// takes them; none on any other page.
    pub(super) fn on(&self, (canvas, page): (&str, &str)) -> Vec<(ItemId, bool)> {
        let on = self.here.as_ref().is_some_and(|(c, p)| c == canvas && p == page);
        self.steps.iter().filter(|_| on).map(|s| (s.id, s.folded)).collect()
    }

    /// The page it is on, while it is inside a folded box there: a
    /// selection there may hold what folded boxes hide.
    pub(super) fn page(&self) -> Option<(String, String)> {
        self.steps.iter().any(|s| s.folded).then(|| self.here.clone()).flatten()
    }

    /// Keeps the first `n` steps; the view goes back to what it was before
    /// the first one left, if any.
    pub(super) fn keep(&mut self, n: usize, viewport: &mut Viewport) {
        if let Some(left) = self.steps.get(n) {
            *viewport = left.back;
        }
        self.steps.truncate(n);
    }
}

/// Where a text that starts joins from: the pane it starts in, or the
/// text it is chained under, which joins the box given, if any.
#[derive(Debug, Clone, Copy)]
pub(super) enum Typed {
    In(PaneId),
    Under(Option<ItemId>),
}

/// A box a pane can step into: where it is, and whether it is folded.
#[derive(Debug, Clone, PartialEq)]
pub struct Boxed {
    canvas: String,
    page: String,
    id: ItemId,
    folded: bool,
}

/// The page `mirror` holds as a pane stepped into `trail` shows it, `ink`
/// laid over it; where the pane is, if inside a box; and how many steps
/// of the trail still hold.
pub(super) fn shown_in<'a>(
    mirror: &'a Mirror,
    ink: &Ink,
    canvas: &str,
    trail: &[(ItemId, bool)],
) -> (Vec<Cow<'a, Item>>, Option<Inside>, usize) {
    let (inside, n) = mirror.step(trail);
    let source =
        if inside.as_ref().is_some_and(Inside::held) { mirror.held() } else { mirror.items() };
    let mut items = ink.shown(canvas, mirror.page(), source);
    if let Some(inside) = &inside {
        items.retain(|i| inside.shows(mirror, i));
    }
    (items, inside, n)
}

/// What a pane shows of its page, to fit it: the room of the box it is
/// inside, if any, else the extent of `items`, the page as it shows it.
pub(super) fn content(items: &[Cow<'_, Item>], within: Option<&Inside>) -> Option<Rect> {
    within.map(|w| w.room).or_else(|| crate::scene::canvas::extent(items.iter().map(AsRef::as_ref)))
}

/// The page `mirror` holds with what folded boxes hide too, `ink` laid
/// over it: what a selection may hold while a pane is inside a folded box.
pub(super) fn present<'a>(mirror: &'a Mirror, ink: &Ink, canvas: &str) -> Vec<Cow<'a, Item>> {
    let mut items = ink.shown(canvas, mirror.page(), mirror.items());
    items.extend(mirror.held().iter().map(Cow::Borrowed));
    items
}

/// The dashed frame round `room`, the box a pane is inside, in `colour`.
pub(super) fn room(room: Rect, colour: crate::config::Rgb) -> Item {
    let shape = Shape::Panel { frame: room, fill: None, width: LINE_WIDTH / 2.0, dashed: true };
    Item { id: ROOM, colour, shape }
}

impl Canvases {
    /// Steps `pane` of `panes` into the selected box. False when its page
    /// shows no selection holding a box.
    pub fn step_in(&mut self, panes: &Panes, pane: PaneId) -> bool {
        self.step_target(panes, pane).is_some_and(|target| {
            self.step_into(pane, target);
            true
        })
    }

    /// The box the selection on the page `pane` of `panes` shows would
    /// step into: of the boxes selected, the largest.
    pub fn step_target(&self, panes: &Panes, pane: PaneId) -> Option<Boxed> {
        let s = self.selection().filter(|_| self.shows_selection(panes, pane))?;
        let mirror = self.follow.mirror(&s.canvas, &s.page)?;
        let area = |id: &ItemId| match mirror.item(*id)?.shape {
            Shape::Box { frame, .. } => Some(frame.width * frame.height),
            _ => None,
        };
        let id =
            *s.ids.iter().filter(|id| area(id).is_some()).max_by(|a, b| {
                area(a).unwrap_or_default().total_cmp(&area(b).unwrap_or_default())
            })?;
        let folded = mirror.box_folded(id)?;
        Some(Boxed { canvas: s.canvas.clone(), page: s.page.clone(), id, folded })
    }

    /// Turns `pane` to the page of `target` and steps it into the box,
    /// its view starting at the box's top left; the selection is let go.
    pub fn step_into(&mut self, pane: PaneId, target: Boxed) {
        let mirror = self.follow.mirror(&target.canvas, &target.page);
        let frame = mirror.and_then(|m| match m.item(target.id)?.shape {
            Shape::Box { frame, .. } => Some(m.room(target.id).unwrap_or(frame)),
            _ => None,
        });
        let Some(frame) = frame else { return };
        self.select.clear();
        let view = self.view(pane);
        let here = (target.canvas, target.page);
        if view.trail.here.as_ref() != Some(&here) {
            view.trail = Trail { here: Some(here.clone()), steps: Vec::new() };
        }
        view.page = Some(here);
        let back = view.viewport;
        view.trail.steps.push(Step { id: target.id, folded: target.folded, back });
        view.viewport = back.at(Point { x: frame.x, y: frame.y });
        view.placed = true;
    }

    /// Steps `pane` of `panes` out of the box it is in, one level, back to
    /// the view it had. False when it is in none.
    pub fn step_out(&mut self, panes: &Panes, pane: PaneId) -> bool {
        self.follow.step_out(panes, pane)
    }

    /// Whether `pane` of `panes` is stepped into a box on the page it
    /// shows: what a step out takes it out of.
    pub fn inside(&self, panes: &Panes, pane: PaneId) -> bool {
        self.follow.depth(panes, pane) > 0
    }

    /// A press in `pane` on `page` of `canvas` at time `when`, on the
    /// miniature of folded box `mini`, if any: the second of two quick
    /// taps on one miniature steps the pane into it. False, and the press
    /// goes on, for any other.
    pub(super) fn tapped_in(
        &mut self,
        pane: PaneId,
        here: (&str, &str),
        when: f64,
        mini: Option<ItemId>,
    ) -> bool {
        let Some(id) = self.follow.tapped(pane, when, mini) else { return false };
        let (canvas, on) = (here.0.to_string(), here.1.to_string());
        self.step_into(pane, Boxed { canvas, page: on, id, folded: true });
        true
    }
}

impl super::View {
    /// Keeps `n` steps of the trail ([`Trail::keep`]) while it shows
    /// `page` of `canvas`; a trail on another page is let go.
    pub(super) fn keep_steps(&mut self, (canvas, page): (&str, &str), n: usize) {
        if self.trail.here.as_ref().is_some_and(|(c, p)| c == canvas && p == page) {
            self.trail.keep(n, &mut self.viewport);
        } else {
            self.trail = Trail::default();
        }
    }
}

/// Whether `r` holds `p`, edges included.
pub(super) fn holds(r: Rect, p: Point) -> bool {
    p.x >= r.x && p.y >= r.y && p.x <= r.x + r.width && p.y <= r.y + r.height
}

#[cfg(test)]
mod tests;
