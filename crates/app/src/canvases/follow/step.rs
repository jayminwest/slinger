//! What stepping into boxes keeps besides each pane's trail
//! ([`super::super::step`]): the last tap on a miniature, and what the pen
//! and the text being typed join; and what each pane's trail says: how
//! deep it is, its footer's names, the box what is written there joins.

use std::borrow::Cow;

use super::Follow;
use crate::canvases::canvas_of;
use crate::canvases::step::{Typed, holds};
use crate::layout::{PaneId, Point};
use crate::panes::Panes;
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::ink::{Change, Ink};
use crate::scene::select::Selection;
use crate::scene::shapes::{DOUBLE_TAP, Shapes};

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

impl Steps {
    /// The panes are another workspace's: a first tap waits no more.
    pub(super) fn switched(&mut self) {
        self.tapped = None;
    }
}

impl Follow {
    /// Steps `pane` of `panes` out of the box it is in, one level, back to
    /// the view it had. False when it is in none.
    pub(in crate::canvases) fn step_out(&mut self, panes: &Panes, pane: PaneId) -> bool {
        let n = self.depth(panes, pane);
        n > 0 && {
            let view = self.views.get(pane);
            view.trail.keep(n - 1, &mut view.viewport);
            true
        }
    }

    /// How many boxes deep `pane` of `panes` is on the page it shows.
    pub(in crate::canvases) fn depth(&self, panes: &Panes, pane: PaneId) -> usize {
        let Some(canvas) = canvas_of(panes, pane) else { return 0 };
        let Some(mirror) = self.shown(canvas, pane).and_then(|o| o.mirror.as_ref()) else {
            return 0;
        };
        let view = self.views.iter().find(|(p, _)| *p == pane).map(|(_, v)| v);
        view.map_or(0, |v| v.trail.on((canvas, mirror.page())).len())
    }

    /// The trail of `pane` of `panes` as its footer says it after the
    /// page's line: " › " and each box's name; empty outside any box.
    pub(in crate::canvases) fn trail_of(&self, panes: &Panes, pane: PaneId) -> String {
        let Some(canvas) = canvas_of(panes, pane) else { return String::new() };
        let Some(mirror) = self.shown(canvas, pane).and_then(|o| o.mirror.as_ref()) else {
            return String::new();
        };
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
    fn joins_in(&self, pane: PaneId, here: (&str, &str)) -> Option<ItemId> {
        let view = self.views.iter().find(|(p, _)| *p == pane).map(|(_, v)| v)?;
        let mirror = self.mirror(here.0, here.1)?;
        mirror.step(&view.trail.on(here)).0?.joins()
    }

    /// The box a sling of `selection` from `pane` is of (docs/design/fold.md,
    /// "Sling a box"): a box selected alone, else the innermost box the
    /// pane is stepped into on the selection's page, if any.
    pub(in crate::canvases) fn sling_box(
        &self,
        pane: PaneId,
        selection: &Selection,
    ) -> Option<ItemId> {
        let here = (selection.canvas.as_str(), selection.page.as_str());
        let mirror = self.mirror(here.0, here.1)?;
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
    pub(in crate::canvases) fn miniature_under(
        &self,
        (canvas, page): (&str, &str),
        at: Point,
        items: &[Cow<'_, Item>],
    ) -> Option<ItemId> {
        let mirror = self.mirror(canvas, page)?;
        let on = |i: &&Cow<'_, Item>| match i.shape {
            Shape::Box { frame, .. } => mirror.box_folded(i.id) == Some(true) && holds(frame, at),
            _ => false,
        };
        items.iter().rev().find(on).map(|i| i.id)
    }

    /// A press in `pane` at time `when`, on the miniature of folded box
    /// `mini`, if any: the second of two quick taps on one miniature is
    /// that box, to step into. `None`, and the press goes on, for any other.
    pub(in crate::canvases) fn tapped(
        &mut self,
        pane: PaneId,
        when: f64,
        mini: Option<ItemId>,
    ) -> Option<ItemId> {
        let last = self.steps.tapped.take();
        let id = mini?;
        let again = last.is_some_and(|(p, b, t)| p == pane && b == id && when - t <= DOUBLE_TAP);
        if !again {
            self.steps.tapped = Some((pane, id, when));
            return None;
        }
        Some(id)
    }

    /// The pen went down in `pane` on `page` of `canvas`, the ink's next
    /// key `mark`: what it adds until it lifts joins the folded box the
    /// pane is inside, if any.
    pub(in crate::canvases) fn pen_in(&mut self, pane: PaneId, here: (&str, &str), mark: u64) {
        self.steps.pen = self.joins_in(pane, here).map(|inside| Joining {
            mark,
            canvas: here.0.to_string(),
            page: here.1.to_string(),
            inside,
        });
    }

    /// The pen lifted: its strokes, boxes and arrows in `ink` join the box
    /// it went down inside, if any. A text is the text's
    /// ([`Follow::typed_in`]).
    pub(in crate::canvases) fn pen_joined(&mut self, ink: &mut Ink) {
        let Some(j) = self.steps.pen.take() else { return };
        let text = |c: &Change| matches!(c, Change::Put { shape: Shape::Text { .. }, .. });
        ink.join(j.mark, (&j.canvas, &j.page), j.inside, |c| !text(c));
    }

    /// After a press or a new text: a text that started now in `shapes`
    /// (`typed` in a pane, or under a text) joins the box the pane is
    /// inside (or the one that text joins) when it ends; one that ended
    /// joins its own in `ink`.
    pub(in crate::canvases) fn typed_in(&mut self, typed: Typed, shapes: &Shapes, ink: &mut Ink) {
        let at = shapes.typing_frame().map(|f| Point { x: f.x, y: f.y });
        if at.is_some() && self.steps.typing.as_ref().is_some_and(|(_, was)| *was == at) {
            return;
        }
        self.text_joined(ink);
        let Some((canvas, on)) = shapes.typing_on() else { return };
        let (canvas, on) = (canvas.to_string(), on.to_string());
        let inside = match typed {
            Typed::In(pane) => self.joins_in(pane, (&canvas, &on)),
            Typed::Under(joined) => joined,
        };
        let mark = ink.mark();
        let joining = |inside| Joining { mark, canvas, page: on, inside };
        self.steps.typing = inside.map(|inside| (joining(inside), at));
    }

    /// What was queued in `ink` from key `mark` on `here`, a page of a
    /// canvas (a paste, a duplicate, a carry), joins the folded box `pane`
    /// is inside there, if any.
    pub(in crate::canvases) fn added_in(
        &self,
        (mark, ink): (u64, &mut Ink),
        pane: PaneId,
        here: (&str, &str),
    ) {
        if let Some(inside) = self.joins_in(pane, here) {
            let carry = |c: &Change| matches!(c, Change::Carry { .. });
            ink.join(mark, here, inside, carry);
        }
    }

    /// The text that joins a box ended: its `add` in `ink` joins it.
    pub(in crate::canvases) fn text_joined(&mut self, ink: &mut Ink) {
        let Some((j, _)) = self.steps.typing.take() else { return };
        let text = |c: &Change| matches!(c, Change::Put { shape: Shape::Text { .. }, .. });
        ink.join(j.mark, (&j.canvas, &j.page), j.inside, text);
    }

    /// The box the text being typed joins, if any.
    pub(in crate::canvases) fn typing_in(&self) -> Option<ItemId> {
        self.steps.typing.as_ref().map(|(j, _)| j.inside)
    }
}
