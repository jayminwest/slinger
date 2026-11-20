//! What became of the last sling ([`crate::scene::slung`]) tied to the
//! canvas panes: the pill over them, and a tap on it. What the slinger's
//! thread heard and the agent's items as they land are [`super::flow`]'s.
//! Also the one-tap sling (`sling-now`, the "➤ Sling" buttons), what a
//! sling takes, and the words a sling with none sends (`[sling] prompt`).
//! Runs on the main thread and never waits.

use super::{Canvases, canvas_of, find, wanted};
use crate::config::{Prompt, Theme};
use crate::layout::{PaneId, Rect, Viewport};
use crate::panes::{Kind, Panes};
use crate::scene::select::Selection;
use crate::scene::sidebar::Agent;
use crate::scene::sling::{Profile, buttons};
use crate::scene::slung::{Marks, Tapped, View};
use crate::scene::status;

/// What `sling-now` flashes on a canvas whose workspace has no agent.
pub const NO_AGENT: &str = "No agent in this workspace";
/// What it flashes on a PDF's canvas, which no workspace has: a sling goes
/// to the agent of its canvas's workspace (docs/service.md, `sling_send`).
pub const PDF: &str = "A PDF cannot be slung: sling a crop of it";
/// What it flashes on any other canvas that no open workspace has.
pub const NO_WORKSPACE: &str = "No open workspace has this canvas";
/// Where a sling of a whole page with no button on it is said to be made:
/// this far into its pane from the top left.
const CORNER: f64 = 8.0;

/// One canvas pane as the pill sees it, owned: its id, frame (above its
/// footer), canvas, page, viewport, and whether it has the focus.
type Seen = (PaneId, Rect, String, String, Viewport, bool);

impl Canvases {
    /// Takes the config's `[sling] prompt`: the words every sling the user
    /// wrote none for sends.
    pub fn set_prompt(&mut self, prompt: &Prompt) {
        self.flow.set_prompt(prompt);
    }

    /// Each open workspace's canvas and its agent, as the sidebar last
    /// heard (`None`: nothing heard): what the buttons and the footers show,
    /// and whether `sling-now` has an agent to sling to.
    pub fn agents(&mut self, agents: Option<Vec<(String, Option<Agent>)>>) {
        self.flow.set_agents(agents);
    }

    /// The `sling-now` command on the focused pane of `panes` laid in
    /// `area`: sends the open entry, as return does; else slings the
    /// selection there with no words, or with nothing selected what is new
    /// on its page. A canvas that has no agent to sling to flashes why
    /// ([`unslingable`]) and sends nothing. False when the pane shows no canvas page, or a sling
    /// is on its way.
    /// The sling goes under `profile` (a tap on the profile menu's row).
    pub fn sling_now_command(&mut self, panes: &Panes, area: Rect, profile: Profile) -> bool {
        if self.flow.slinging() {
            self.flow.send(profile);
            self.poll_sling();
            return true;
        }
        let Some((selection, around, within)) = self.sling_target(panes, area, true) else {
            return false;
        };
        if let Some(why) = self.flow.unslingable(&selection.canvas) {
            self.zoomed = Some(why.to_string());
            return true;
        }
        let inside = self.sling_box(panes.focus(), &selection);
        self.sling_now((&selection, inside), ("", profile), around, within)
    }

    /// What a sling from the focused pane of `panes` laid in `area` takes,
    /// where it is said to be made and the frame it stands in: the
    /// selection there, else the first button on its page, else its page
    /// when `whole` (`sling-now`), or the box it is stepped into (`sling`).
    pub(super) fn sling_target(
        &self,
        panes: &Panes,
        area: Rect,
        whole: bool,
    ) -> Option<(Selection, Rect, Rect)> {
        let selected = self.selection_in(panes, area).map(|(s, a, w)| (s.clone(), a, w));
        let target = selected.or_else(|| self.button_in(panes, area));
        let rest = || if whole { self.page_in(panes, area) } else { self.room_in(panes, area) };
        target.or_else(rest)
    }

    /// Whether `sling` would open its entry on the focused pane of `panes`
    /// laid in `area` ([`Canvases::open_sling`]).
    pub fn can_sling(&self, panes: &Panes, area: Rect) -> bool {
        !self.flow.noting() && self.sling_target(panes, area, false).is_some()
    }

    /// Whether `sling-now` would send ([`Canvases::sling_now_command`]):
    /// the open entry, or a target with an agent to sling to.
    pub fn can_sling_now(&self, panes: &Panes, area: Rect) -> bool {
        let agent = |s: &Selection| self.flow.unslingable(&s.canvas).is_none();
        self.flow.slinging()
            || self.sling_target(panes, area, true).is_some_and(|(s, ..)| agent(&s))
    }

    /// The page the focused pane of `panes` laid in `area` shows, as
    /// [`Canvases::page_in`] gives it, while that pane is stepped into a
    /// box there: a sling of the box.
    fn room_in(&self, panes: &Panes, area: Rect) -> Option<(Selection, Rect, Rect)> {
        let target = self.page_in(panes, area)?;
        self.sling_box(panes.focus(), &target.0).is_some().then_some(target)
    }

    /// The page the focused pane of `panes` laid in `area` shows, as a
    /// sling of what is new on it, said to be made at the pane's top left;
    /// with the pane's frame, in the window.
    fn page_in(&self, panes: &Panes, area: Rect) -> Option<(Selection, Rect, Rect)> {
        let pane = panes.focus();
        let (canvas, _, shown) = self.pages_of(panes, pane)?;
        let frame = panes.place(area).into_iter().find(|(id, ..)| *id == pane)?.2;
        let frame = status::content(frame);
        let at = Rect { x: frame.x + CORNER, y: frame.y + CORNER, width: 0.0, height: 0.0 };
        Some((Selection { canvas, page: shown, ids: Vec::new() }, at, frame))
    }

    /// A tap on the pill or the answer under it, the canvas panes of
    /// `panes` laid in `area` as they are: it may pan a pane to the new
    /// items. Returns whether anything shown changed.
    pub fn tap_slung(&mut self, panes: &Panes, area: Rect) -> bool {
        let seen = self.seen(panes, area);
        let views = views(&seen);
        match self.flow.tap(&views) {
            Tapped::Nothing => false,
            Tapped::Changed => true,
            Tapped::Pan(index, dx, dy) => {
                self.pan(seen[index].0, dx, dy);
                true
            }
        }
    }

    /// What the window draws for the sling over the canvas panes of
    /// `panes` laid in `area`, in `theme`'s colours: "➤ Sling", "+
    /// prompt" and "Delete" beside the selection while no entry is open and
    /// no sling waits for the daemon's answer, the pen's buttons beside the open entry, the "✓" that resolves
    /// each recent note under a note's, and the pill; their labels (the
    /// buttons' first), tap targets, and the
    /// answer's panel.
    pub fn sling_marks(&self, panes: &Panes, area: Rect, theme: &Theme) -> Marks {
        let idle = self.flow.idle() && !self.typing();
        let selected = self.selection_in(panes, area).filter(|(s, ..)| idle && !s.ids.is_empty());
        let buttons = selected.map(|(_, around, within)| buttons::buttons(around, within, theme));
        let seen = self.seen(panes, area);
        self.flow.marks(buttons.unwrap_or_default(), &self.asks, theme, &views(&seen))
    }

    /// The canvas panes of `panes` laid in `area` whose page has arrived.
    fn seen(&self, panes: &Panes, area: Rect) -> Vec<Seen> {
        let placed = panes.place(area).into_iter().filter(|(_, p, _)| p.kind == Kind::Canvas);
        let seen = placed.filter_map(|(id, pane, frame)| {
            let canvas = canvas_of(panes, id)?;
            let open = find(&self.open, canvas, wanted(&self.views, id, canvas))?;
            let page = open.mirror.as_ref()?.page().to_string();
            let view = self.views.iter().find(|(p, _)| *p == id);
            let viewport = view.map_or_else(Viewport::start, |(_, v)| v.viewport);
            let focused = id == panes.focus();
            Some((id, status::content(frame), pane.name.clone(), page, viewport, focused))
        });
        seen.collect()
    }
}

/// The agent in `agents` of the workspace whose canvas is `canvas`.
pub(super) fn agent_in<'a>(
    agents: Option<&'a [(String, Option<Agent>)]>,
    canvas: &str,
) -> Option<&'a Agent> {
    let list = agents.into_iter().flatten();
    list.filter(|(c, _)| c == canvas).find_map(|(_, agent)| agent.as_ref())
}

/// Why a sling of `canvas` cannot go, as `agents` (each open workspace's
/// canvas and its agent) has it: what `sling-now` flashes. `None` when its
/// workspace has an agent, or before the sidebar heard (the daemon says).
pub(super) fn unslingable(
    agents: Option<&[(String, Option<Agent>)]>,
    canvas: &str,
) -> Option<&'static str> {
    let agents = agents?;
    if agent_in(Some(agents), canvas).is_some() {
        None
    } else if agents.iter().any(|(c, _)| c == canvas) {
        Some(NO_AGENT)
    } else if crate::scene::pdf::reader(canvas) {
        Some(PDF)
    } else {
        Some(NO_WORKSPACE)
    }
}

/// `seen` as the pill wants it.
fn views(seen: &[Seen]) -> Vec<View<'_>> {
    seen.iter().map(view).collect()
}

fn view((_, frame, canvas, page, viewport, focused): &Seen) -> View<'_> {
    View { frame: *frame, canvas, page, viewport: *viewport, focused: *focused }
}

#[cfg(test)]
mod tests;
