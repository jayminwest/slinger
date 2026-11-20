//! The notice: the bar that says an agent became blocked or done. It names
//! each such agent, with what it said. A done agent leaves it on its own
//! ([`FADE`] after it showed, or at the pen's next press), since done asks
//! nothing of the user; a blocked agent stays until the user taps its ✕ or
//! `dismiss-notice`, or its status moves on. A tap on the bar shows the
//! first named agent's workspace. It sits at the bottom of the panes'
//! area, above the footers, so it hides no footer button.

use std::time::{Duration, Instant};

use super::{Agent, PAD, Sidebar, Status};
use crate::commands::Command;
use crate::config::Theme;
use crate::layout::Rect;
use crate::scene::taps::{Tap, Target};
use crate::scene::{Label, status, style};

/// The notice's widest.
pub(super) const WIDTH: f64 = 480.0;
/// How long a done agent stays in the notice once it shows.
pub const FADE: Duration = Duration::from_secs(5);
/// The ✕ that dismisses the notice.
pub const CLOSE: &str = "✕";

/// An agent in the notice: the status it became, what it said, and when
/// the notice first showed it (stamped by [`Sidebar::fade`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Noticed {
    pub agent: String,
    pub status: Status,
    note: Option<String>,
    since: Option<Instant>,
}

impl Noticed {
    pub fn new(agent: &Agent, status: Status) -> Self {
        Self { agent: agent.name.clone(), status, note: agent.note.clone(), since: None }
    }
}

impl Sidebar {
    /// Hides the notice until an agent next becomes blocked or done, and
    /// the offer to restart the daemon for that daemon.
    pub fn dismiss(&mut self) {
        self.notice.clear();
        self.waive();
    }

    /// Lets time pass to `now`: an agent the notice shows for the first
    /// time is stamped, and a done agent shown [`FADE`] ago leaves. Returns
    /// whether the notice changed.
    pub fn fade(&mut self, now: Instant) -> bool {
        let before = self.notice.len();
        for n in &mut self.notice {
            n.since.get_or_insert(now);
        }
        self.notice.retain(|n| {
            n.status != Status::Done || n.since.is_some_and(|t| now.duration_since(t) < FADE)
        });
        self.notice.len() != before
    }

    /// The pen went down somewhere: the user is back, so done agents leave
    /// the notice. Returns whether it changed.
    pub fn pressed(&mut self) -> bool {
        let before = self.notice.len();
        self.notice.retain(|n| n.status != Status::Done);
        self.notice.len() != before
    }

    /// The notice's frame in the panes' area `panes`, above the footers.
    fn notice_frame(&self, panes: Rect) -> Option<Rect> {
        if self.notice.is_empty() {
            return None;
        }
        let width = WIDTH.min(panes.width - 2.0 * PAD).max(0.0);
        Some(Rect {
            x: panes.x + (panes.width - width) / 2.0,
            y: panes.y + panes.height - status::HEIGHT - PAD - style::TAP,
            width,
            height: style::TAP,
        })
    }

    /// The notice in the panes' area `panes`: its text, then its ✕ at its
    /// right end; then the offer to restart the daemon above it
    /// ([`Sidebar::offer`]). Empty when there is none.
    pub fn notice(&self, theme: &Theme, panes: Rect) -> Vec<Label> {
        let (offer, _) = self.offer_bar(theme, panes, self.notice_frame(panes));
        let Some(frame) = self.notice_frame(panes) else {
            return offer;
        };
        let parts: Vec<String> = self
            .notice
            .iter()
            .map(|n| match &n.note {
                Some(note) => format!("{} is {}: {note}", n.agent, n.status.word()),
                None => format!("{} is {}", n.agent, n.status.word()),
            })
            .collect();
        let (text, close) = split(frame);
        let fill = Some(theme.focus);
        let mut labels = vec![
            Label { text: parts.join(" · "), colour: theme.background, fill, frame: text },
            Label { text: CLOSE.into(), colour: theme.background, fill, frame: close },
        ];
        labels.extend(offer);
        labels
    }

    /// The notice's tap targets: the offer's ([`Sidebar::offer`]); its ✕
    /// runs `dismiss-notice`, and the rest shows the workspace of the first
    /// agent it names.
    pub fn notice_targets(&self, theme: &Theme, panes: Rect) -> Vec<Target> {
        let (_, mut targets) = self.offer_bar(theme, panes, self.notice_frame(panes));
        let Some(frame) = self.notice_frame(panes) else {
            return targets;
        };
        let (text, close) = split(frame);
        targets.push((close, Tap::Run(Command::DismissNotice)));
        let list = self.workspaces.iter().flatten();
        let first = &self.notice[0].agent;
        let index = list
            .enumerate()
            .find_map(|(i, w)| w.agent.as_ref().is_some_and(|a| a.name == *first).then_some(i));
        targets.extend(index.map(|i| (text, Tap::Workspace(i))));
        targets
    }
}

/// The notice's `frame` split into its text and its ✕, a tap wide.
fn split(frame: Rect) -> (Rect, Rect) {
    let tap = style::TAP.min(frame.width);
    let text = Rect { width: frame.width - tap, ..frame };
    let close = Rect { x: frame.x + frame.width - tap, width: tap, ..frame };
    (text, close)
}
