//! The sidebar and the notice (SPEC D17). Pure: the daemon's open
//! workspaces and their agents' statuses in, what the window shows out.
//!
//! The sidebar lists each workspace under a heading, and under each its
//! agent with one mark: working, blocked or done; the workspace the window
//! shows stands out. It is optional: one command opens or collapses
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
    /// Its default canvas's id: where its agent's button shows its status.
    pub canvas: String,
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
/// The width given to a status mark.
const MARK: f64 = 16.0;
const ROW: f64 = 22.0;
/// Space above each workspace's first line.
const GAP: f64 = 4.0;
/// The notice's widest, and its gap from the bottom of the window.
const NOTICE_WIDTH: f64 = 480.0;
/// The sidebar's heading.
const HEADING: &str = "WORKSPACES";
/// A done mark's colour: the theme has no green, and done must not read as
/// working or blocked.
const DONE: Rgb = Rgb { r: 0x4c, g: 0xc3, b: 0x6b };
/// How far the shown workspace's row moves from the background toward the
/// text colour, out of 255.
const LIGHTER: u16 = 24;

/// How a sidebar line's text is set: its size in points (`None`: the
/// system's), bold or not, and the space added between its letters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Font {
    pub size: Option<f64>,
    pub bold: bool,
    pub spacing: f64,
}

impl Font {
    const PLAIN: Font = Font { size: None, bold: false, spacing: 0.0 };
}

/// One thing the sidebar draws: a label read from its left edge, in `font`.
/// A label with no text and a fill is a row's background.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub label: Label,
    pub font: Font,
}

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
    /// The workspace whose panes the window shows; `None`: the first.
    shown: Option<String>,
}

impl Sidebar {
    /// An empty sidebar, open or collapsed.
    pub fn new(open: bool) -> Self {
        Self { open, workspaces: None, problem: None, notice: Vec::new(), shown: None }
    }

    /// Marks `workspace` as the one the window shows; `None`: the first.
    pub fn show(&mut self, workspace: Option<String>) {
        self.shown = workspace;
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

    /// Each open workspace's canvas and its agent's status, as last heard:
    /// what the canvases' buttons show. Empty without a list.
    pub fn statuses(&self) -> Vec<(String, Option<Status>)> {
        let list = self.workspaces.iter().flatten();
        list.map(|w| (w.canvas.clone(), w.agent.as_ref().and_then(|a| a.status))).collect()
    }

    /// The agent of the open workspace whose canvas is `canvas`, as last
    /// heard: where a sling from it goes. `None` without one.
    pub fn agent(&self, canvas: &str) -> Option<&str> {
        let mut list = self.workspaces.iter().flatten();
        let workspace = list.find(|w| w.canvas == canvas)?;
        workspace.agent.as_ref().map(|a| a.name.as_str())
    }

    /// Where the panes go in `window`: all of it, less the sidebar when open.
    pub fn panes(&self, window: Rect) -> Rect {
        let width = if self.open { WIDTH.min(window.width) } else { 0.0 };
        Rect { x: window.x + width, width: window.width - width, ..window }
    }

    /// The sidebar's lines in `window`, read from their left edge: the
    /// heading, then each workspace's name (bold, on a lighter row, when the
    /// window shows it), under it its agent's mark with `<agent> · <status>`
    /// or `no agent`, and a blocked agent's note under that. Nothing when it
    /// is collapsed. Lines that do not fit are left out.
    pub fn rows(&self, theme: &Theme, window: Rect) -> Vec<Row> {
        if !self.open {
            return Vec::new();
        }
        let heading = Font { size: Some(11.0), bold: false, spacing: 1.0 };
        let mut lines = vec![Line::new(HEADING.into(), theme.muted, heading)];
        match (&self.workspaces, &self.problem) {
            (_, Some(problem)) => {
                lines.push(Line::new(format!("✗ {problem}"), theme.error, Font::PLAIN));
            }
            (None, None) => return Vec::new(),
            (Some(list), None) if list.is_empty() => {
                lines.push(Line::new("no open workspaces".into(), theme.muted, Font::PLAIN));
            }
            (Some(list), None) => {
                for (i, workspace) in list.iter().enumerate() {
                    let shown = self.shown.as_ref().map_or(i == 0, |s| *s == workspace.name);
                    lines.extend(block(workspace, shown, theme));
                }
            }
        }
        let (right, bottom) = (window.x + WIDTH.min(window.width) - PAD, window.y + window.height);
        let label = |text: String, colour, x: f64, y, right: f64| Label {
            text,
            colour,
            fill: None,
            frame: Rect { x, y, width: (right - x).max(0.0), height: ROW },
        };
        let (mut rows, mut lit) = (Vec::new(), None::<(f64, f64)>);
        let mut y = window.y + PAD;
        for line in lines {
            y += line.gap;
            if y + ROW > bottom {
                break;
            }
            let mut x = window.x + PAD;
            if line.under {
                if let Some((mark, colour)) = line.mark {
                    let label = label(mark.into(), colour, x, y, x + MARK);
                    rows.push(Row { label, font: Font::PLAIN });
                }
                x += MARK;
            }
            rows.push(Row { label: label(line.text, line.colour, x, y, right), font: line.font });
            if line.shown {
                lit = Some((lit.map_or(y, |(top, _)| top), y + ROW));
            }
            y += ROW;
        }
        if let Some((top, end)) = lit {
            let frame =
                Rect { x: window.x, y: top, width: WIDTH.min(window.width), height: end - top };
            let fill = Label {
                text: String::new(),
                colour: theme.text,
                fill: Some(lighter(theme)),
                frame,
            };
            rows.insert(0, Row { label: fill, font: Font::PLAIN });
        }
        rows
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

/// One line of the sidebar before it is placed.
struct Line {
    /// Whether it is under a workspace's name: set in by a mark's width.
    under: bool,
    /// A status mark and its colour, in that space.
    mark: Option<(&'static str, Rgb)>,
    text: String,
    colour: Rgb,
    font: Font,
    /// Space above it.
    gap: f64,
    /// Whether it belongs to the workspace the window shows.
    shown: bool,
}

impl Line {
    fn new(text: String, colour: Rgb, font: Font) -> Self {
        Self { under: false, mark: None, text, colour, font, gap: 0.0, shown: false }
    }
}

/// A workspace's lines: its name, its agent and status, and a blocked
/// agent's note.
fn block(workspace: &Workspace, shown: bool, theme: &Theme) -> Vec<Line> {
    let font = Font { bold: shown, ..Font::PLAIN };
    let name = Line { gap: GAP, ..Line::new(workspace.name.clone(), theme.text, font) };
    let mut lines = vec![name];
    match &workspace.agent {
        None => lines
            .push(Line { under: true, ..Line::new("no agent".into(), theme.muted, Font::PLAIN) }),
        Some(agent) => {
            let text = match agent.status {
                Some(status) => format!("{} · {}", agent.name, status.word()),
                None => agent.name.clone(),
            };
            let mark = Some(mark(agent.status, theme));
            lines.push(Line { under: true, mark, ..Line::new(text, theme.muted, Font::PLAIN) });
            if let (Some(Status::Blocked), Some(note)) = (agent.status, &agent.note) {
                lines.push(Line {
                    under: true,
                    ..Line::new(note.clone(), theme.muted, Font::PLAIN)
                });
            }
        }
    }
    for line in &mut lines {
        line.shown = shown;
    }
    lines
}

/// The shown workspace's row: the background, a little toward the text.
fn lighter(theme: &Theme) -> Rgb {
    let toward = |from: u8, to: u8| {
        let (from, to) = (u16::from(from), u16::from(to));
        let mixed = (from * (255 - LIGHTER) + to * LIGHTER) / 255;
        u8::try_from(mixed).unwrap_or(u8::MAX)
    };
    let (b, t) = (theme.background, theme.text);
    Rgb { r: toward(b.r, t.r), g: toward(b.g, t.g), b: toward(b.b, t.b) }
}

/// The mark for a status, and its colour.
fn mark(status: Option<Status>, theme: &Theme) -> (&'static str, Rgb) {
    (glyph(status), colour(status, theme))
}

/// The mark for a status: a ring while the agent has said nothing, a square
/// when blocked, so its shape differs as well as its colour.
pub(super) fn glyph(status: Option<Status>) -> &'static str {
    match status {
        None => "○",
        Some(Status::Working | Status::Done) => "●",
        Some(Status::Blocked) => "■",
    }
}

/// A status mark's colour: blocked in the error colour, done in green.
pub(super) fn colour(status: Option<Status>, theme: &Theme) -> Rgb {
    match status {
        None => theme.muted,
        Some(Status::Working) => theme.focus,
        Some(Status::Blocked) => theme.error,
        Some(Status::Done) => DONE,
    }
}

#[cfg(test)]
mod tests;
