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

use super::{Canvases, canvas_of, find, wanted};
use crate::layout::{PaneId, Point, Rect, Viewport};
use crate::mirror::{Inside, LINE_WIDTH, Mirror};
use crate::panes::Panes;
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::ink::{Change, Ink};
use crate::scene::select::Selection;
use crate::scene::shapes::DOUBLE_TAP;

/// The draw id of the dashed frame round the room a pane is inside: below
/// a peek's paper ([`super::fold`]).
const ROOM: ItemId = ItemId((1 << 60) - 2);

/// The boxes a pane is inside, outermost first, on one page.
#[derive(Debug, Default)]
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
    fn keep(&mut self, n: usize, viewport: &mut Viewport) {
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

/// What stepping keeps besides each pane's trail: the last tap on a
/// miniature, and what the pen and the text being typed join.
#[derive(Debug, Default)]
pub(super) struct Steps {
    tapped: Option<(PaneId, ItemId, f64)>,
    /// The pen's gesture in a pane inside a folded box.
    pen: Option<Joining>,
    /// The text being typed in a pane inside a folded box, and where it
    /// started (a text starting elsewhere is another).
    typing: Option<(Joining, Option<Point>)>,
}

/// Adds that join folded box `inside` on `page` of `canvas`: those queued
/// from the ink's `mark` on.
#[derive(Debug, Clone)]
struct Joining {
    mark: u64,
    canvas: String,
    page: String,
    inside: ItemId,
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
        let mirror = find(&self.open, &s.canvas, Some(&s.page))?.mirror.as_ref()?;
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
        let open = find(&self.open, &target.canvas, Some(&target.page));
        let mirror = open.and_then(|o| o.mirror.as_ref());
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
    }

    /// Steps `pane` of `panes` out of the box it is in, one level, back to
    /// the view it had. False when it is in none.
    pub fn step_out(&mut self, panes: &Panes, pane: PaneId) -> bool {
        let Some(canvas) = canvas_of(panes, pane) else { return false };
        let open = find(&self.open, canvas, wanted(&self.views, pane, canvas));
        let Some(shown) = open.and_then(|o| o.mirror.as_ref()).map(|m| m.page().to_string()) else {
            return false;
        };
        let view = self.view(pane);
        let n = view.trail.on((canvas, &shown)).len();
        n > 0 && {
            view.trail.keep(n - 1, &mut view.viewport);
            true
        }
    }

    /// The trail of `pane` of `panes` as its footer says it after the
    /// page's line: " › " and each box's name; empty outside any box.
    pub(super) fn trail_of(&self, panes: &Panes, pane: PaneId) -> String {
        let Some(canvas) = canvas_of(panes, pane) else { return String::new() };
        let open = find(&self.open, canvas, wanted(&self.views, pane, canvas));
        let Some(mirror) = open.and_then(|o| o.mirror.as_ref()) else { return String::new() };
        let view = self.views.iter().find(|(p, _)| *p == pane).map(|(_, v)| v);
        let trail = view.map(|v| v.trail.on((canvas, mirror.page()))).unwrap_or_default();
        let mut line = String::new();
        for n in 1..=trail.len() {
            let (inside, held) = mirror.step(&trail[..n]);
            let Some(inside) = inside.filter(|_| held == n) else { break };
            line.push_str(" › ");
            line.push_str(&mirror.named(&inside));
        }
        line
    }

    /// The folded box what is written in `pane` on `page` of `canvas`
    /// joins, if any: the innermost folded box of its trail.
    pub(super) fn joins_in(&self, pane: PaneId, here: (&str, &str)) -> Option<ItemId> {
        let view = self.views.iter().find(|(p, _)| *p == pane).map(|(_, v)| v)?;
        let mirror = find(&self.open, here.0, Some(here.1))?.mirror.as_ref()?;
        mirror.step(&view.trail.on(here)).0?.joins()
    }

    /// The box a sling of `selection` from `pane` is of (docs/design/fold.md,
    /// "Sling a box"): a box selected alone, else the innermost box the
    /// pane is stepped into on the selection's page, if any.
    pub(super) fn sling_box(&self, pane: PaneId, selection: &Selection) -> Option<ItemId> {
        let here = (selection.canvas.as_str(), selection.page.as_str());
        let mirror = find(&self.open, here.0, Some(here.1))?.mirror.as_ref()?;
        let boxed = |id| mirror.item(id).is_some_and(|i| matches!(i.shape, Shape::Box { .. }));
        if let [id] = selection.ids[..]
            && boxed(id)
        {
            return Some(id);
        }
        let view = self.views.iter().find(|(p, _)| *p == pane).map(|(_, v)| v)?;
        mirror.step(&view.trail.on(here)).0?.innermost()
    }

    /// The folded box whose miniature a press at `at` on `items` (`page`
    /// of `canvas` as a pane shows it) is on, the topmost, if any.
    pub(super) fn miniature_under(
        &self,
        (canvas, page): (&str, &str),
        at: Point,
        items: &[Cow<'_, Item>],
    ) -> Option<ItemId> {
        let mirror = find(&self.open, canvas, Some(page))?.mirror.as_ref()?;
        let on = |i: &&Cow<'_, Item>| match i.shape {
            Shape::Box { frame, .. } => mirror.box_folded(i.id) == Some(true) && holds(frame, at),
            _ => false,
        };
        items.iter().rev().find(on).map(|i| i.id)
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
        let last = self.steps.tapped.take();
        let Some(id) = mini else { return false };
        let again = last.is_some_and(|(p, b, t)| p == pane && b == id && when - t <= DOUBLE_TAP);
        if !again {
            self.steps.tapped = Some((pane, id, when));
            return false;
        }
        let (canvas, on) = (here.0.to_string(), here.1.to_string());
        self.step_into(pane, Boxed { canvas, page: on, id, folded: true });
        true
    }

    /// The pen went down in `pane` on `page` of `canvas`: what it adds
    /// until it lifts joins the folded box the pane is inside, if any.
    pub(super) fn pen_in(&mut self, pane: PaneId, here: (&str, &str)) {
        let mark = self.ink.mark();
        self.steps.pen = self.joins_in(pane, here).map(|inside| Joining {
            mark,
            canvas: here.0.to_string(),
            page: here.1.to_string(),
            inside,
        });
    }

    /// The pen lifted: its strokes, boxes and arrows join the box it went
    /// down inside, if any. A text is the text's ([`Canvases::typed_in`]).
    pub(super) fn pen_joined(&mut self) {
        let Some(j) = self.steps.pen.take() else { return };
        let text = |c: &Change| matches!(c, Change::Put { shape: Shape::Text { .. }, .. });
        self.ink.join(j.mark, (&j.canvas, &j.page), j.inside, |c| !text(c));
    }

    /// After a press or a new text: a text that started now (`typed` in a
    /// pane, or under a text) joins the box the pane is inside (or the
    /// one that text joins) when it ends; one that ended joins its own.
    pub(super) fn typed_in(&mut self, typed: Typed) {
        let at = self.shapes.typing_frame().map(|f| Point { x: f.x, y: f.y });
        if at.is_some() && self.steps.typing.as_ref().is_some_and(|(_, was)| *was == at) {
            return;
        }
        self.text_joined();
        let Some((canvas, on)) = self.shapes.typing_on() else { return };
        let (canvas, on) = (canvas.to_string(), on.to_string());
        let inside = match typed {
            Typed::In(pane) => self.joins_in(pane, (&canvas, &on)),
            Typed::Under(joined) => joined,
        };
        let mark = self.ink.mark();
        let joining = |inside| Joining { mark, canvas, page: on, inside };
        self.steps.typing = inside.map(|inside| (joining(inside), at));
    }

    /// What was queued from key `mark` on `here`, a page of a canvas (a
    /// paste, a duplicate, a carry), joins the folded box `pane` is inside
    /// there, if any.
    pub(super) fn added_in(&mut self, mark: u64, pane: PaneId, here: (&str, &str)) {
        if let Some(inside) = self.joins_in(pane, here) {
            let carry = |c: &Change| matches!(c, Change::Carry { .. });
            self.ink.join(mark, here, inside, carry);
        }
    }

    /// The text that joins a box ended: its `add` joins it.
    pub(super) fn text_joined(&mut self) {
        let Some((j, _)) = self.steps.typing.take() else { return };
        let text = |c: &Change| matches!(c, Change::Put { shape: Shape::Text { .. }, .. });
        self.ink.join(j.mark, (&j.canvas, &j.page), j.inside, text);
    }

    /// The box the text being typed joins, if any.
    pub(super) fn typing_in(&self) -> Option<ItemId> {
        self.steps.typing.as_ref().map(|(j, _)| j.inside)
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
fn holds(r: Rect, p: Point) -> bool {
    p.x >= r.x && p.y >= r.y && p.x <= r.x + r.width && p.y <= r.y + r.height
}

#[cfg(test)]
mod tests;
