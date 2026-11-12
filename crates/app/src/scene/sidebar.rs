//! The sidebar and the notice (SPEC D17). Pure: the daemon's open
//! workspaces and their agents' statuses in, what the window shows out.
//!
//! The sidebar lists each workspace under a heading, and under each its
//! agent with one mark: working, blocked or done; the workspace the window
//! shows stands out. It is optional: one command opens or collapses
//! it, and the panes take the space it leaves. Agent status is a glance, not
//! a dashboard. A tap on a workspace's rows shows its panes; the foot is
//! kept for the button that hides the sidebar, at the spot where the
//! bottom-left footer shows it while the sidebar is collapsed.
//!
//! The notice is the one thing the app tells the user unasked: an agent
//! became blocked or done ([`notice`]). The first list the app hears, after
//! it starts or after the daemon comes back, is where it starts counting:
//! no notice for what was already so.

use crate::backend::Problem;
use crate::config::{Rgb, Theme};
use crate::layout::Rect;
use crate::scene::{Label, status, style};

pub mod notice;

/// An agent's status as the window shows it: what it last set with
/// `slinger status set`, read against its process ([`Status::of`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Working,
    Blocked,
    Done,
    /// Its process is not running: a sling cannot reach it.
    Stopped,
}

/// An agent's process as slingerd holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Process {
    Running,
    /// It ended, or was stopped: slingerd started it before.
    Exited,
    /// slingerd never started it: the agent may run outside it, and its
    /// hooks say how it is.
    Absent,
}

impl Process {
    /// The process the daemon calls `name` (`running` or `exited`; `None`:
    /// it holds none) for an agent it `started` before (it holds a session
    /// id for it).
    pub fn named(name: Option<&str>, started: bool) -> Process {
        match name {
            Some("running") => Process::Running,
            None if !started => Process::Absent,
            _ => Process::Exited,
        }
    }
}

impl Status {
    /// The status to show for an agent that `said` it is at a status, whose
    /// `process` has `pending` messages it has not answered, and which has
    /// `answered` one: the daemon's own count wins over what the agent's
    /// hooks last said, which may be late or never come. A process that
    /// exited is [`Status::Stopped`]; for an agent slingerd never started,
    /// what its hooks said shows (stopped when they said nothing). A running one with a
    /// message pending is working, or blocked when it says so; with none
    /// it is done once it said anything or answered, and has no status
    /// before.
    pub fn of(
        said: Option<Status>,
        process: Process,
        pending: u32,
        answered: bool,
    ) -> Option<Status> {
        match process {
            Process::Running => {}
            Process::Exited => return Some(Status::Stopped),
            Process::Absent => return said.or(Some(Status::Stopped)),
        }
        if pending > 0 {
            let blocked = said == Some(Status::Blocked);
            return Some(if blocked { Status::Blocked } else { Status::Working });
        }
        (said.is_some() || answered).then_some(Status::Done)
    }

    /// The status the daemon calls `name`; `None` for any other word.
    pub fn parse(name: &str) -> Option<Status> {
        match name {
            "working" => Some(Status::Working),
            "blocked" => Some(Status::Blocked),
            "done" => Some(Status::Done),
            _ => None,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Status::Working => "working",
            Status::Blocked => "blocked",
            Status::Done => "done",
            Status::Stopped => "not running",
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
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Agent {
    pub name: String,
    /// `None` until the agent sets one.
    pub status: Option<Status>,
    /// What the agent said with its status.
    pub note: Option<String>,
    /// The profile its running process runs under; `None` while it runs
    /// within its own limits or does not run.
    pub profile: Option<String>,
    /// The names of its profiles, in its workspace file's order.
    pub profiles: Vec<String>,
}

impl Agent {
    /// How the window names it: `scout`, or `scout (dev)` while it runs
    /// under the profile `dev`.
    pub fn label(&self) -> String {
        match &self.profile {
            Some(profile) => format!("{} ({profile})", self.name),
            None => self.name.clone(),
        }
    }
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
/// The sidebar's heading.
const HEADING: &str = "WORKSPACES";
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
    pub const PLAIN: Font = Font { size: None, bold: false, spacing: 0.0 };
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
    notice: Vec<notice::Noticed>,
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
                self.notice
                    .retain(|n| agents().any(|a| a.name == n.agent && a.status == Some(n.status)));
                let last = self.workspaces.as_ref();
                let was = |name: &str| {
                    last?.iter().filter_map(|w| w.agent.as_ref()).find(|a| a.name == name)
                };
                for agent in agents() {
                    let Some(status @ (Status::Blocked | Status::Done)) = agent.status else {
                        continue;
                    };
                    if was(&agent.name).is_some_and(|a| a.status != agent.status) {
                        self.notice.retain(|n| n.agent != agent.name);
                        self.notice.push(notice::Noticed::new(agent, status));
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

    /// Each open workspace's canvas and its agent, as last heard: what the
    /// canvases' buttons and footers show, and where a sling goes. `None`
    /// before the first list and after the daemon was lost.
    pub fn agents(&self) -> Option<Vec<(String, Option<Agent>)>> {
        let list = self.workspaces.as_ref()?.iter();
        Some(list.map(|w| (w.canvas.clone(), w.agent.clone())).collect())
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
    /// (`<agent> (<profile>) · <status>` under a profile) or `no agent`, and
    /// a blocked agent's note under that. Nothing when it is collapsed. Lines that do not fit above its foot ([`Sidebar::foot`])
    /// are left out.
    pub fn rows(&self, theme: &Theme, window: Rect) -> Vec<Row> {
        self.laid(theme, window).0
    }

    /// Where each workspace's lines are in `window` (its name and its
    /// agent's, as wide as the sidebar), with its index in the list: a
    /// tap there shows its panes ([`Sidebar::pick`]). None when collapsed.
    pub fn targets(&self, theme: &Theme, window: Rect) -> Vec<(Rect, usize)> {
        self.laid(theme, window).1
    }

    /// The open sidebar's whole frame in `window`, its edge left out: its
    /// foot holds its button (`scene::status::strip`). None when collapsed.
    pub fn foot(&self, window: Rect) -> Option<Rect> {
        let width = WIDTH.min(window.width) - style::HAIRLINE;
        self.open.then_some(Rect { width: width.max(0.0), ..window })
    }

    /// Shows the workspace with index `index` in the list (the window
    /// shows its panes): its name, or `None` past the last.
    pub fn pick(&mut self, index: usize) -> Option<String> {
        let name = self.workspaces.as_ref()?.get(index)?.name.clone();
        self.shown = Some(name.clone());
        Some(name)
    }

    /// The rows and the workspaces' targets.
    fn laid(&self, theme: &Theme, window: Rect) -> (Vec<Row>, Vec<(Rect, usize)>) {
        if !self.open {
            return (Vec::new(), Vec::new());
        }
        let heading = Font { size: Some(11.0), bold: false, spacing: 1.0 };
        let mut lines = vec![Line::new(HEADING.into(), theme.muted, heading)];
        match (&self.workspaces, &self.problem) {
            (_, Some(problem)) => {
                lines.push(Line::new(format!("✗ {problem}"), theme.error, Font::PLAIN));
            }
            (None, None) => return (Vec::new(), Vec::new()),
            (Some(list), None) if list.is_empty() => {
                lines.push(Line::new("no open workspaces".into(), theme.muted, Font::PLAIN));
            }
            (Some(list), None) => {
                for (i, workspace) in list.iter().enumerate() {
                    let shown = self.shown.as_ref().map_or(i == 0, |s| *s == workspace.name);
                    lines.extend(
                        block(workspace, shown, theme)
                            .into_iter()
                            .map(|line| Line { workspace: Some(i), ..line }),
                    );
                }
            }
        }
        let bottom = window.y + window.height - status::HEIGHT;
        let right = window.x + WIDTH.min(window.width) - PAD;
        let label = |text: String, colour, x: f64, y, right: f64| Label {
            text,
            colour,
            fill: None,
            frame: Rect { x, y, width: (right - x).max(0.0), height: ROW },
        };
        let (mut rows, mut lit) = (Vec::new(), None::<(f64, f64)>);
        let mut targets: Vec<(Rect, usize)> = Vec::new();
        let wide = WIDTH.min(window.width) - style::HAIRLINE;
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
            match (line.workspace, targets.last_mut()) {
                (Some(i), Some((frame, at))) if *at == i => frame.height = y + ROW - frame.y,
                (Some(i), _) => {
                    targets.push((Rect { x: window.x, y, width: wide, height: ROW }, i));
                }
                (None, _) => {}
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
        (rows, targets)
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
    /// The index of the workspace it belongs to, if any.
    workspace: Option<usize>,
}

impl Line {
    fn new(text: String, colour: Rgb, font: Font) -> Self {
        Self {
            under: false,
            mark: None,
            text,
            colour,
            font,
            gap: 0.0,
            shown: false,
            workspace: None,
        }
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
                Some(status) => format!("{} · {}", agent.label(), status.word()),
                None => agent.label(),
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

/// The shown workspace's row and each pane's footer: the background, a
/// little toward the text.
pub(super) fn lighter(theme: &Theme) -> Rgb {
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
        None | Some(Status::Stopped) => "○",
        Some(Status::Working | Status::Done) => "●",
        Some(Status::Blocked) => "■",
    }
}

/// A status mark's colour: working in the accent, blocked in the error
/// colour, done in the done colour.
pub(super) fn colour(status: Option<Status>, theme: &Theme) -> Rgb {
    match status {
        None | Some(Status::Stopped) => theme.muted,
        Some(Status::Working) => theme.focus,
        Some(Status::Blocked) => theme.error,
        Some(Status::Done) => theme.done,
    }
}

#[cfg(test)]
mod tests;
