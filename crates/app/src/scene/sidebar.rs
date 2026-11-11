//! The sidebar and the notice (SPEC D17, P6). Pure: the daemon's open
//! workspaces and their agents' statuses in, what the window shows out.
//!
//! The sidebar lists each workspace, and under it its agent with one mark:
//! working, blocked or done. It is optional: one command opens or collapses
//! it, and the panes take the space it leaves. Agent status is a glance, not
//! a dashboard.
//!
//! The notice is the one thing the app tells the user unasked: an agent
//! became blocked or done. It names each such agent and stays until the
//! user dismisses it or the agent's status moves on. The first list the app
//! hears, after it starts or after the daemon comes back, is where it
//! starts counting: no notice for what was already so.

use crate::backend::Problem;
use crate::config::{Rgb, Theme};
use crate::layout::Rect;
use crate::scene::Label;

/// An agent's status, as `slinger status set` sets it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Working,
    Blocked,
    Done,
}

impl Status {
    /// The status the daemon calls `name`; `None` for any other word.
    pub fn parse(name: &str) -> Option<Status> {
        match name {
            "working" => Some(Status::Working),
            "blocked" => Some(Status::Blocked),
            "done" => Some(Status::Done),
            _ => None,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Status::Working => "working",
            Status::Blocked => "blocked",
            Status::Done => "done",
        }
    }
}

/// One open workspace, as the daemon last said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub name: String,
    pub agent: Option<Agent>,
}

/// The agent of a workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    pub name: String,
    /// `None` until the agent sets one.
    pub status: Option<Status>,
    /// What the agent said with its status.
    pub note: Option<String>,
}

/// The sidebar's width, in points, when it is open.
pub const WIDTH: f64 = 200.0;
/// Space between the sidebar's edge and its text, and above its first line.
const PAD: f64 = 12.0;
/// How far an agent's line is indented under its workspace.
const INDENT: f64 = 12.0;
/// The width given to a status mark.
const MARK: f64 = 16.0;
const ROW: f64 = 22.0;
/// The notice's widest, and its gap from the bottom of the window.
const NOTICE_WIDTH: f64 = 480.0;

/// What the sidebar shows, and the notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sidebar {
    open: bool,
    /// The last list; `None` before the first, and after the daemon was lost.
    workspaces: Option<Vec<Workspace>>,
    /// Why there is no list.
    problem: Option<String>,
    /// The agents in the notice, oldest first, with the status each became.
    notice: Vec<(String, Status, Option<String>)>,
}

impl Sidebar {
    /// An empty sidebar, open or collapsed.
    pub fn new(open: bool) -> Self {
        Self { open, workspaces: None, problem: None, notice: Vec::new() }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Opens or collapses the sidebar.
    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    /// Hides the notice until an agent next becomes blocked or done.
    pub fn dismiss(&mut self) {
        self.notice.clear();
    }

    /// Takes what the daemon says now. An agent that was in the last list
    /// and has become blocked or done joins the notice; an agent in the
    /// notice whose status moved on leaves it. A problem empties the list,
    /// so the next list is a new start. Returns whether anything shown
    /// changed.
    pub fn update(&mut self, now: Result<Vec<Workspace>, Problem>) -> bool {
        let before = self.clone();
        match now {
            Ok(now) => {
                let agents = || now.iter().filter_map(|w| w.agent.as_ref());
                self.notice.retain(|(name, status, _)| {
                    agents().any(|a| a.name == *name && a.status == Some(*status))
                });
                let last = self.workspaces.as_ref();
                let was = |name: &str| {
                    last?.iter().filter_map(|w| w.agent.as_ref()).find(|a| a.name == name)
                };
                for agent in agents() {
                    let Some(status @ (Status::Blocked | Status::Done)) = agent.status else {
                        continue;
                    };
                    if was(&agent.name).is_some_and(|a| a.status != agent.status) {
                        self.notice.retain(|(name, ..)| *name != agent.name);
                        self.notice.push((agent.name.clone(), status, agent.note.clone()));
                    }
                }
                (self.workspaces, self.problem) = (Some(now), None);
            }
            Err(problem) => {
                (self.workspaces, self.problem) = (None, Some(problem.message));
                self.notice.clear();
            }
        }
        *self != before
    }

    /// Where the panes go in `window`: all of it, less the sidebar when open.
    pub fn panes(&self, window: Rect) -> Rect {
        let width = if self.open { WIDTH.min(window.width) } else { 0.0 };
        Rect { x: window.x + width, width: window.width - width, ..window }
    }

    /// The sidebar's lines in `window`, read from their left edge: each
    /// workspace, and under it its agent's mark and name. Nothing when it
    /// is collapsed. Lines that do not fit are left out.
    pub fn rows(&self, theme: &Theme, window: Rect) -> Vec<Label> {
        if !self.open {
            return Vec::new();
        }
        let mut lines: Vec<Line<'_>> = Vec::new();
        let problem = self.problem.as_ref().map(|p| format!("✗ {p}"));
        match (&self.workspaces, &problem) {
            (_, Some(problem)) => lines.push((None, problem, theme.error)),
            (None, None) => {}
            (Some(list), None) if list.is_empty() => {
                lines.push((None, "no open workspaces", theme.muted));
            }
            (Some(list), None) => {
                for workspace in list {
                    lines.push((None, &workspace.name, theme.text));
                    if let Some(agent) = &workspace.agent {
                        lines.push((Some(mark(agent.status, theme)), &agent.name, theme.muted));
                    }
                }
            }
        }
        let right = window.x + WIDTH.min(window.width) - PAD;
        let label = |text: &str, colour, x: f64, y, right: f64| Label {
            text: text.to_string(),
            colour,
            fill: None,
            frame: Rect { x, y, width: (right - x).max(0.0), height: ROW },
        };
        let mut labels = Vec::new();
        let rows = (0..).map(|i| window.y + PAD + f64::from(i) * ROW);
        for ((mark, text, colour), y) in lines.into_iter().zip(rows) {
            if y + ROW > window.y + window.height {
                break;
            }
            let mut x = window.x + PAD;
            if let Some((mark, mark_colour)) = mark {
                x += INDENT;
                labels.push(label(mark, mark_colour, x, y, x + MARK));
                x += MARK;
            }
            labels.push(label(text, colour, x, y, right));
        }
        labels
    }

    /// The notice, at the bottom of the panes' area `panes`: each agent that
    /// became blocked or done, with what it said. `None` when there is none.
    pub fn notice(&self, theme: &Theme, panes: Rect) -> Option<Label> {
        if self.notice.is_empty() {
            return None;
        }
        let parts: Vec<String> = self
            .notice
            .iter()
            .map(|(agent, status, note)| match note {
                Some(note) => format!("{agent} is {}: {note}", status.word()),
                None => format!("{agent} is {}", status.word()),
            })
            .collect();
        let width = NOTICE_WIDTH.min(panes.width - 2.0 * PAD).max(0.0);
        Some(Label {
            text: parts.join(" · "),
            colour: theme.background,
            fill: Some(theme.focus),
            frame: Rect {
                x: panes.x + (panes.width - width) / 2.0,
                y: panes.y + panes.height - PAD - ROW,
                width,
                height: ROW,
            },
        })
    }
}

/// One line of the sidebar: a status mark and its colour, or none; then
/// its text and colour.
type Line<'a> = (Option<(&'static str, Rgb)>, &'a str, Rgb);

/// The mark for a status, and its colour: blocked in the error colour, so it
/// is the one that stands out.
fn mark(status: Option<Status>, theme: &Theme) -> (&'static str, Rgb) {
    match status {
        None => ("○", theme.muted),
        Some(Status::Working) => ("●", theme.focus),
        Some(Status::Blocked) => ("●", theme.error),
        Some(Status::Done) => ("✓", theme.text),
    }
}

#[cfg(test)]
mod tests;
