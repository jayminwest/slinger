//! What became of the last sling ([`crate::scene::slung`]) tied to the
//! canvases: where the sling was made and the newest item then, what the
//! slinger's thread heard of the agent, the agent's items as they land, and
//! a tap on the pill. Also the one-tap sling (`sling-now`, the "➤ Sling"
//! buttons) and the words a sling with none sends (`[sling] prompt`). Runs
//! on the main thread and never waits.

use std::time::Instant;

use super::{Canvases, canvas_of, find, wanted};
use crate::backend::Heard;
use crate::config::{Prompt, Theme};
use crate::layout::{PaneId, Rect, Viewport};
use crate::panes::{Kind, Panes};
use crate::scene::canvas::{ItemId, Shape};
use crate::scene::select::Selection;
use crate::scene::sidebar::Agent;
use crate::scene::sling::{Answered, Ask, buttons};
use crate::scene::slung::{Marks, Tapped, View};
use crate::scene::status;

/// What `sling-now` flashes on a canvas whose workspace has no agent.
pub const NO_AGENT: &str = "No agent in this workspace";
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
        prompt.text().clone_into(&mut self.prompt);
    }

    /// Each open workspace's canvas and its agent, as the sidebar last
    /// heard (`None`: nothing heard): what the buttons and the footers show,
    /// and whether `sling-now` has an agent to sling to.
    pub fn agents(&mut self, agents: Option<Vec<(String, Option<Agent>)>>) {
        self.agents = agents;
    }

    /// The agent of the workspace whose canvas is `canvas`; `None` when it
    /// has none, or before the sidebar heard.
    pub(super) fn agent_of(&self, canvas: &str) -> Option<&Agent> {
        agent_in(self.agents.as_deref(), canvas)
    }

    /// The `sling-now` command on the focused pane of `panes` laid in
    /// `area`: sends the open entry, as return does; else slings the
    /// selection there with no words, or with nothing selected what is new
    /// on its page. A canvas whose workspace has no agent flashes so and
    /// sends nothing. False when the pane shows no canvas page, or a sling
    /// is on its way.
    pub fn sling_now_command(&mut self, panes: &Panes, area: Rect) -> bool {
        if self.sling.is_typing() {
            self.sling.send();
            self.entry_closed();
            self.poll_sling();
            return true;
        }
        let selected = self.selection_in(panes, area).map(|(s, a, w)| (s.clone(), a, w));
        let target = selected.or_else(|| self.button_in(panes, area));
        let Some((selection, around, within)) = target.or_else(|| self.page_in(panes, area)) else {
            return false;
        };
        if self.agents.is_some() && self.agent_of(&selection.canvas).is_none() {
            self.zoomed = Some(NO_AGENT.to_string());
            return true;
        }
        self.sling_now(&selection, "", around, within)
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

    /// Sends `ask`, now ready, with the words of `[sling] prompt` when it
    /// has none, and follows it from where it was made.
    pub(super) fn going(&mut self, mut ask: Ask) {
        if ask.prompt.trim().is_empty() {
            ask.prompt.clone_from(&self.prompt);
        }
        let (at, newest) = self.made_at(&ask);
        self.slung.aim((&ask.canvas, &ask.page), at, newest, Instant::now());
        self.slinger.send(ask);
    }

    /// Where on its page `ask` was made, in canvas units (around its items;
    /// with none, the page's first button; else nowhere in particular), and
    /// the newest item there now.
    fn made_at(&self, ask: &Ask) -> (Option<Rect>, ItemId) {
        let open = self.open.iter().filter(|o| o.canvas == ask.canvas);
        let mirror = open.filter_map(|o| o.mirror.as_ref()).find(|m| m.page() == ask.page);
        let Some(items) = mirror.map(crate::mirror::Mirror::items) else {
            return (None, ItemId(0));
        };
        let newest = items.iter().map(|i| i.id).max().unwrap_or(ItemId(0));
        let chosen = items.iter().filter(|i| ask.ids.contains(&i.id));
        let around = chosen.filter_map(|i| i.shape.bounds()).reduce(Rect::union);
        let button = items.iter().find_map(|i| match i.shape {
            Shape::Button { frame, .. } => Some(frame),
            _ => None,
        });
        (around.or(button), newest)
    }

    /// Takes what the slinger's thread heard, the agent's items on the
    /// sling's page, and the time. Returns whether anything shown changed.
    pub(super) fn poll_slung(&mut self) -> bool {
        let now = Instant::now();
        let mut changed = self.landed();
        for heard in self.slinger.heard() {
            changed = true;
            match heard {
                Heard::Went(agent, tokens) => {
                    self.sling.answered(Ok(()));
                    self.slung.went(&agent, tokens, now);
                }
                Heard::Failed(problem) => match self.sling.answered(Err(problem.clone())) {
                    Answered::Reopened => self.slung.forget(),
                    Answered::Closed => drop(self.slung.failed(problem, now)),
                    Answered::Not => {}
                },
                Heard::Lost(problem) => drop(self.slung.failed(problem, now)),
                Heard::Progress(progress) => drop(self.slung.heard(&progress, now)),
            }
        }
        self.slung.tick(now) || changed
    }

    /// Hands the items of the sling's page to the pill, each with who made
    /// it. Returns whether its answer's items changed.
    fn landed(&mut self) -> bool {
        let Some((canvas, page)) = self.slung.watching() else { return false };
        let (canvas, page) = (canvas.to_string(), page.to_string());
        let open = self.open.iter().filter(|o| o.canvas == canvas);
        let Some(mirror) = open.filter_map(|o| o.mirror.as_ref()).find(|m| m.page() == page) else {
            return false;
        };
        let items = mirror.items().iter().enumerate();
        let items = items.filter_map(|(at, i)| Some((i.id, i.shape.bounds()?, mirror.by(at))));
        self.slung.landed((&canvas, &page), items)
    }

    /// A tap on the pill or the answer under it, the canvas panes of
    /// `panes` laid in `area` as they are: it may pan a pane to the new
    /// items. Returns whether anything shown changed.
    pub fn tap_slung(&mut self, panes: &Panes, area: Rect) -> bool {
        let seen = self.seen(panes, area);
        let views = views(&seen);
        match self.slung.tapped(&views) {
            Tapped::Nothing => false,
            Tapped::Changed => true,
            Tapped::Pan(index, dx, dy) => {
                self.pan(seen[index].0, dx, dy);
                true
            }
        }
    }

    /// What the window draws for the sling over the canvas panes of
    /// `panes` laid in `area`, in `theme`'s colours: "➤ Sling" and "+
    /// prompt" beside the selection while no sling is out, "Delete" beside
    /// it, the pen's buttons beside the open entry, and the pill; their labels (the buttons' first), tap targets, and the
    /// answer's panel.
    pub fn sling_marks(&self, panes: &Panes, area: Rect, theme: &Theme) -> Marks {
        let (mut labels, mut targets) = (Vec::new(), Vec::new());
        let idle = self.sling.canvas().is_none() && !self.typing();
        let selected = self.selection_in(panes, area).filter(|(s, ..)| idle && !s.ids.is_empty());
        if let Some((_, around, within)) = selected {
            (labels, targets) = buttons::buttons(around, within, theme, self.slung.out());
        }
        let (pen, tapped) = self.entry_buttons(theme);
        (labels, targets) = ([labels, pen].concat(), [targets, tapped].concat());
        let seen = self.seen(panes, area);
        let mut marks = self.slung.marks(theme, &views(&seen), Instant::now());
        labels.append(&mut marks.labels);
        targets.append(&mut marks.targets);
        Marks { labels, targets, ..marks }
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

/// `seen` as the pill wants it.
fn views(seen: &[Seen]) -> Vec<View<'_>> {
    seen.iter().map(view).collect()
}

fn view((_, frame, canvas, page, viewport, focused): &Seen) -> View<'_> {
    View { frame: *frame, canvas, page, viewport: *viewport, focused: *focused }
}

#[cfg(test)]
mod tests;
