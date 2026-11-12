//! What became of the last sling (SPEC P6: every state is visible), from
//! the moment it went until the agent answered. Pure: `canvases` hands in
//! what the daemon said (the sling went or not, then the agent's session as
//! it works), the agent's items as they land on the sling's page, the time
//! and where each pane looks; it reads back the pill to draw, the tap
//! target on it, the answer's panel and what to flash.
//!
//! One sling is followed at a time: a new one takes the last one's place,
//! so marks never pile up. The pill stands under where the sling was made
//! (the selection, or the button tapped), moves with the canvas, and is
//! held inside the pane when that place is off screen; a sling of the whole
//! page from the footer pins it to the pane's top left. It says, in turn:
//!
//! - `● scout working · 4s · ~12k` from the moment the sling is in the
//!   agent's process until it answers, the time counting each second (each
//!   minute past one, so the window is drawn at most once a second), then
//!   about how many tokens the agent reads for it, when the daemon counts;
//! - `■ scout blocked: <note> · ~12k` while the agent says it waits;
//! - `✓ scout answered · 2 new items` once it answered, with an arrow
//!   toward them (`→`) when none of them shows in the pane: a tap pans
//!   there, and the next tap clears it;
//! - `✓ scout answered in text · tap to read` when it answered and put
//!   nothing on the canvas: a tap opens its answer under the pill, and a
//!   tap on that closes both;
//! - `✗ <why>` with the hint under it when a tapped sling did not go, the
//!   agent's process ended before it answered, or the daemon went away.
//!
//! The sling going flashes "Slung to scout", its answer "scout answered"
//! (or "scout answered in text"). The agent's items can land after the
//! daemon says it answered: an answer with none yet waits [`SETTLE`] for
//! them before it says "in text", and its pill says only `✓ scout
//! answered` meanwhile. A tap clears an answer or a failure, and
//! so does [`CLEAR`] passing: nothing stays on the canvas for good.

use std::time::{Duration, Instant};

use crate::backend::Problem;
use crate::config::Theme;
use crate::layout::{Point, Rect, Size, Viewport};
use crate::scene::canvas::ItemId;
use crate::scene::overlay::{Align, Line, Overlay};
use crate::scene::taps::{Tap, Target};
use crate::scene::{Label, glyphs, style};
use words::{cost, count, elapsed, page};

/// How long an answer or a failure stays untapped.
pub const CLEAR: Duration = Duration::from_secs(30);
/// How long an answer with nothing on the canvas yet waits for the agent's
/// items to land before it says it answered in text.
pub const SETTLE: Duration = Duration::from_secs(1);
/// The pill's height: the pen taps it.
const HIGH: f64 = style::TAP;
/// The room inside the pill's ends, and around it in its pane.
const PAD: f64 = 10.0;
const GAP: f64 = 8.0;
/// A line under the pill (a hint, the answer), its text size, and the
/// answer panel's widest and most lines.
const LINE: f64 = 20.0;
const TEXT: f64 = 13.0;
const PANEL: f64 = 440.0;
const MOST: usize = 12;

/// The agent's session as the daemon shows it while a sling is out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    /// Its process runs.
    pub running: bool,
    /// Messages sent that it has not answered.
    pub pending: u32,
    /// What it said it waits for, while it says it is blocked.
    pub blocked: Option<String>,
    /// The text of its last answer.
    pub answer: Option<String>,
}

/// One canvas pane as the pill sees it: its frame in the window (above its
/// footer), the canvas page it shows, how it looks at it, and whether it
/// has the focus.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View<'a> {
    pub frame: Rect,
    pub canvas: &'a str,
    pub page: &'a str,
    pub viewport: Viewport,
    pub focused: bool,
}

/// What a tap on the pill does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tapped {
    Nothing,
    /// What shows changed (the answer opened, or the pill went).
    Changed,
    /// Pan the pane with this index in the views by `dx`, `dy` screen
    /// points: the new items come to its middle.
    Pan(usize, f64, f64),
}

/// What the window draws for the sling: the pill (its corners round, its
/// text as wide as it measures), labels, the answer's panel, and the tap
/// targets.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Marks {
    pub pill: Option<Line>,
    pub labels: Vec<Label>,
    pub panel: Option<Overlay>,
    pub targets: Vec<Target>,
}

#[cfg(test)]
impl Marks {
    /// The pill's label, then the labels.
    pub fn all(&self) -> Vec<Label> {
        let pill = self.pill.iter().map(|p| p.label.clone());
        pill.chain(self.labels.iter().cloned()).collect()
    }
}

/// Where the last sling is.
#[derive(Debug, Clone, PartialEq)]
enum Stage {
    /// On its way to the daemon: `scene::sling` shows it.
    Aimed,
    Working,
    Blocked(String),
    /// Answered, with the text of the answer.
    Answered(Option<String>),
    Failed(Problem),
    /// Its process ended before it answered.
    Stopped,
}

/// The last sling: where it was made, by the canvas's ids and units.
#[derive(Debug, Clone, PartialEq)]
struct Out {
    canvas: String,
    page: String,
    at: Option<Rect>,
    /// The newest item on its page when it went: the agent's items above
    /// it are its answer.
    newest: ItemId,
    agent: String,
    /// About how many tokens the agent reads for it, when the daemon said.
    tokens: Option<u64>,
    went: Instant,
    stage: Stage,
    since: Instant,
    items: Vec<(ItemId, Rect)>,
    reading: bool,
    /// Its answer was flashed: the items it brought are known.
    told: bool,
    /// The time the pill last said.
    drawn: String,
}

/// The last sling, followed.
#[derive(Debug, Default)]
pub struct Slung {
    out: Option<Out>,
    flash: Option<String>,
}

impl Slung {
    /// A sling of `page` of `canvas` leaves from `at` (canvas units; `None`:
    /// the page as a whole) at `now`, with `newest` the newest item there.
    /// What showed for the last sling goes.
    pub fn aim(
        &mut self,
        (canvas, page): (&str, &str),
        at: Option<Rect>,
        newest: ItemId,
        now: Instant,
    ) {
        let (canvas, page) = (canvas.to_string(), page.to_string());
        let (agent, items, drawn) = (String::new(), Vec::new(), String::new());
        let stage = Stage::Aimed;
        let out = Out {
            canvas,
            page,
            at,
            newest,
            agent,
            tokens: None,
            went: now,
            stage,
            since: now,
            items,
            reading: false,
            told: false,
            drawn,
        };
        self.out = Some(out);
        self.flash = None;
    }

    /// The aimed sling went into `agent`'s session at `now`, `tokens` its
    /// cost as the daemon counts it.
    pub fn went(&mut self, agent: &str, tokens: Option<u64>, now: Instant) -> bool {
        let Some(out) = self.out.as_mut().filter(|o| o.stage == Stage::Aimed) else { return false };
        (out.agent, out.tokens, out.went) = (agent.to_string(), tokens, now);
        out.to(Stage::Working, now);
        self.flash = Some(format!("Slung to {agent}"));
        true
    }

    /// The aimed sling did not go and has no entry to say so, or the
    /// daemon went away while the agent worked: the pill says why.
    pub fn failed(&mut self, problem: Problem, now: Instant) -> bool {
        let out = self.out.as_mut();
        let Some(out) =
            out.filter(|o| matches!(o.stage, Stage::Aimed | Stage::Working | Stage::Blocked(_)))
        else {
            return false;
        };
        out.to(Stage::Failed(problem), now);
        true
    }

    /// The aimed sling's entry opened again with why it failed: nothing
    /// more shows for it.
    pub fn forget(&mut self) {
        self.out = self.out.take().filter(|o| o.stage != Stage::Aimed);
    }

    /// The agent's session at `now`, while it works on the sling.
    pub fn heard(&mut self, progress: &Progress, now: Instant) -> bool {
        let Some(out) = self.out.as_mut() else { return false };
        if !matches!(out.stage, Stage::Working | Stage::Blocked(_)) {
            return false;
        }
        let next = if !progress.running {
            Stage::Stopped
        } else if progress.pending == 0 {
            Stage::Answered(progress.answer.clone())
        } else if let Some(note) = &progress.blocked {
            Stage::Blocked(note.clone())
        } else {
            Stage::Working
        };
        let changed = out.stage != next;
        if changed {
            out.to(next, now);
        }
        self.tell(now);
        changed
    }

    /// Flashes the answer once what it brought is known: its items landed,
    /// or [`SETTLE`] passed with none. Returns whether it did.
    fn tell(&mut self, now: Instant) -> bool {
        let Some(out) = self.out.as_mut().filter(|o| !o.told) else { return false };
        let Stage::Answered(answer) = &out.stage else { return false };
        let flash = match (answer, out.items.is_empty()) {
            (_, false) => "answered",
            _ if out.since + SETTLE > now => return false,
            (Some(_), true) => "answered in text",
            (None, true) => "is done",
        };
        self.flash = Some(format!("{} {flash}", out.agent));
        out.told = true;
        true
    }

    /// Whether a sling is out: on its way, or the agent works on it.
    pub fn out(&self) -> bool {
        let out = self.out.as_ref().map(|o| &o.stage);
        matches!(out, Some(Stage::Aimed | Stage::Working | Stage::Blocked(_)))
    }

    /// The canvas page the agent's answer lands on, while the sling is out
    /// or answered: what [`Slung::landed`] wants.
    pub fn watching(&self) -> Option<(&str, &str)> {
        let out = self.out.as_ref()?;
        let on = matches!(out.stage, Stage::Working | Stage::Blocked(_) | Stage::Answered(_));
        on.then_some((out.canvas.as_str(), out.page.as_str()))
    }

    /// The items of `page` of `canvas` now, each with its box and who made
    /// it (`agent:<name>`): those the sling's agent made after it went are
    /// its answer. Returns whether they changed.
    pub fn landed<'a>(
        &mut self,
        (canvas, page): (&str, &str),
        items: impl IntoIterator<Item = (ItemId, Rect, Option<&'a str>)>,
    ) -> bool {
        let Some(out) = self.out.as_mut().filter(|o| o.canvas == canvas && o.page == page) else {
            return false;
        };
        let by = format!("agent:{}", out.agent);
        let newest = out.newest;
        let mine = items.into_iter().filter(|(id, _, who)| *id > newest && *who == Some(&by));
        let mine: Vec<(ItemId, Rect)> = mine.map(|(id, frame, _)| (id, frame)).collect();
        let changed = mine != out.items;
        out.items = mine;
        changed
    }

    /// The time is `now`: whether what the pill says changed (its time, an
    /// answer settled ([`SETTLE`]), or an answer or a failure that has shown for [`CLEAR`] goes, unless its
    /// answer is open).
    pub fn tick(&mut self, now: Instant) -> bool {
        let told = self.tell(now);
        let Some(out) = self.out.as_mut() else { return false };
        match out.stage {
            Stage::Working | Stage::Blocked(_) => {
                let time = elapsed(now.saturating_duration_since(out.went));
                let changed = time != out.drawn;
                out.drawn = time;
                changed
            }
            Stage::Answered(_) | Stage::Failed(_) | Stage::Stopped
                if !out.reading && out.since + CLEAR <= now =>
            {
                self.out = None;
                true
            }
            _ => told,
        }
    }

    /// What to flash, once.
    pub fn flash(&mut self) -> Option<String> {
        self.flash.take()
    }

    /// A tap on the pill, or on the answer under it, with the panes as
    /// `views` show them.
    pub fn tapped(&mut self, views: &[View]) -> Tapped {
        let Some(out) = self.out.as_mut() else { return Tapped::Nothing };
        match &out.stage {
            Stage::Aimed | Stage::Working | Stage::Blocked(_) => return Tapped::Nothing,
            Stage::Answered(_) if !out.items.is_empty() => {
                if let Some((index, view)) = out.view(views)
                    && let Some(away) = out.away(&view)
                {
                    let middle = view.viewport.rect_to_screen(away);
                    let dx = view.frame.width / 2.0 - (middle.x + middle.width / 2.0);
                    let dy = view.frame.height / 2.0 - (middle.y + middle.height / 2.0);
                    return Tapped::Pan(index, dx, dy);
                }
            }
            Stage::Answered(Some(_)) if !out.reading => {
                out.reading = true;
                return Tapped::Changed;
            }
            _ => {}
        }
        self.out = None;
        Tapped::Changed
    }

    /// The pill in `theme`'s colours at `now`, in the pane of `views` that
    /// shows the sling's page (the focused one first; else the focused
    /// pane, pinned), and the answer's panel while it is open.
    pub fn marks(&self, theme: &Theme, views: &[View], now: Instant) -> Marks {
        let Some(out) = &self.out else { return Marks::default() };
        let Some((_, view)) = out.view(views) else { return Marks::default() };
        let (agent, cost) = (&out.agent, cost(out.tokens));
        let (text, colour, under) = match &out.stage {
            Stage::Aimed => return Marks::default(),
            Stage::Working => {
                let time = elapsed(now.saturating_duration_since(out.went));
                (format!("● {agent} working · {time}{cost}"), theme.text, Vec::new())
            }
            Stage::Blocked(note) => {
                let note = if note.is_empty() { String::new() } else { format!(": {note}") };
                (format!("■ {agent} blocked{note}{cost}"), theme.error, Vec::new())
            }
            Stage::Answered(answer) => {
                (out.answered(answer.is_some(), &view), theme.text, Vec::new())
            }
            Stage::Failed(problem) => (
                format!("✗ {}", problem.message),
                theme.error,
                problem.hint.iter().cloned().collect(),
            ),
            Stage::Stopped => {
                let hint = "start it with 'slinger session start <WORKSPACE>'".to_string();
                (format!("✗ {} stopped before it answered", out.agent), theme.error, vec![hint])
            }
        };
        let answer = match &out.stage {
            Stage::Answered(Some(answer)) if out.reading && out.items.is_empty() => Some(answer),
            _ => None,
        };
        let measured = glyphs::frame(Point { x: 0.0, y: 0.0 }, &text, TEXT, glyphs::UNWRAPPED);
        let wide = (measured.width + 2.0 * PAD).max(HIGH);
        let block = if answer.is_some() { wide.max(PANEL) } else { wide };
        let block = block.min(view.frame.width - 2.0 * GAP).max(0.0);
        let lines = answer.map_or_else(|| under.clone(), |a| page(a, block, &out.agent));
        let tall =
            HIGH + count(lines.len()) * LINE + if answer.is_some() { 2.0 * GAP } else { 0.0 };
        let pill = out.place(&view, Size { width: wide.min(block), height: tall });
        let fill = style::panel(theme);
        let label = Label { text, colour, fill: Some(fill), frame: pill };
        let line = Line::text(String::new(), colour, pill, TEXT, Align::Centre);
        let line = Line { label, radius: style::FIELD_RADIUS, ..line };
        let (mut labels, mut target) = (Vec::new(), pill);
        let panel = answer.map(|_| {
            let frame = Rect { y: pill.y + HIGH, width: block, height: tall - HIGH, ..pill };
            target = target.union(frame);
            let mut shown = vec![Line {
                edge: Some(style::edge(theme)),
                ..Line::fill(fill, frame, style::FIELD_RADIUS)
            }];
            for (n, line) in lines.iter().enumerate() {
                let y = frame.y + GAP + count(n) * LINE;
                let at = Rect { x: frame.x + PAD, y, width: block - 2.0 * PAD, height: LINE };
                let muted = n + 1 == lines.len();
                let colour = if muted { theme.muted } else { theme.text };
                shown.push(Line::text(line.clone(), colour, at, TEXT, Align::Left));
            }
            let nowhere = Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };
            Overlay { frame: nowhere, dim: theme.background, opacity: 0.0, lines: shown }
        });
        if panel.is_none() {
            for (n, line) in lines.into_iter().enumerate() {
                let frame =
                    Rect { y: pill.y + HIGH + count(n) * LINE, height: LINE, width: block, ..pill };
                labels.push(Label {
                    text: line,
                    colour: theme.muted,
                    fill: Some(theme.background),
                    frame,
                });
            }
        }
        Marks { pill: Some(line), labels, panel, targets: vec![(target, Tap::Slung)] }
    }
}

mod out;
mod words;

#[cfg(test)]
mod tests;
