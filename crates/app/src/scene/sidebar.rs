//! The sidebar and the notice (SPEC D17). Pure: the daemon's open
//! workspaces and their agents' statuses in, what the window shows out.
//!
//! The sidebar lists each workspace under a heading, and under each its
//! agent with one mark: working, blocked or done; the workspace the window
//! shows stands out. It is optional: one command opens or collapses
//! it, and the panes take the space it leaves. Agent status is a glance, not
//! a dashboard. A workspace whose file the daemon cannot use shows why
//! ([`broken`]); the shown one's agent that does not run, a "Start agent"
//! line. A tap on a workspace's rows shows its panes; the foot is
//! kept for the button that hides the sidebar, at the spot where the
//! bottom-left footer shows it while the sidebar is collapsed.
//!
//! The notice is the one thing the app tells the user unasked: an agent
//! became blocked or done ([`notice`]). The first list the app hears, after
//! it starts or after the daemon comes back, is where it starts counting:
//! no notice for what was already so.

use std::path::PathBuf;

use crate::backend::Problem;
use crate::commands::Command;
use crate::config::{Rgb, Theme};
use crate::layout::Rect;
use crate::scene::taps::{Tap, Target};
use crate::scene::{Label, status, style};

mod agent;
pub mod broken;
pub mod notice;

pub use agent::{Agent, Process, Status};
pub use broken::Broken;

/// One open workspace, as the daemon last said.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Workspace {
    /// Empty only for a broken file that names none.
    pub name: String,
    /// Its default canvas's id: where its agent's button shows its status.
    pub canvas: String,
    pub agent: Option<Agent>,
    /// The directory of its file: what closes it, whatever the file holds.
    pub dir: PathBuf,
    /// Why the daemon cannot use its file, if it cannot.
    pub broken: Option<Broken>,
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
/// The line under a shown workspace's stopped agent: a tap starts it.
pub const START: &str = "Start agent";
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
    /// The workspace whose panes the window shows; `None`: the first usable.
    shown: Option<String>,
}

impl Sidebar {
    /// An empty sidebar, open or collapsed.
    pub fn new(open: bool) -> Self {
        Self { open, workspaces: None, problem: None, notice: Vec::new(), shown: None }
    }

    /// Marks `workspace` as the one the window shows; `None`: the first
    /// that is not broken, as the panes do (`backend::panes::layout`).
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
    /// changed: the notice changes only with the list.
    pub fn update(&mut self, now: Result<Vec<Workspace>, Problem>) -> bool {
        let same = match now {
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
                let same = self.problem.is_none() && self.workspaces.as_ref() == Some(&now);
                (self.workspaces, self.problem) = (Some(now), None);
                same
            }
            Err(problem) => {
                let same = self.problem.as_ref() == Some(&problem.message);
                (self.workspaces, self.problem) = (None, Some(problem.message));
                self.notice.clear();
                same
            }
        };
        !same
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
    /// agent's, as wide as the sidebar): a tap there shows its panes
    /// ([`Sidebar::pick`]); and a shown workspace's command line, which
    /// runs its command: a broken one's `close-workspace`, a stopped
    /// agent's `start-agent`. None when collapsed.
    pub fn targets(&self, theme: &Theme, window: Rect) -> Vec<Target> {
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

    /// Takes the workspace the window shows out of the list, and shows the
    /// first usable: the directory of the one taken, to close it by. `None`
    /// when no workspace shows.
    pub fn close(&mut self) -> Option<PathBuf> {
        let at = self.at();
        self.shown = None;
        Some(self.workspaces.as_mut()?.remove(at?).dir)
    }

    /// The directory of the workspace the window shows: what its agent's
    /// commands go to, whatever its file holds. `None` when none shows.
    pub fn shown_dir(&self) -> Option<PathBuf> {
        Some(self.workspaces.as_ref()?.get(self.at()?)?.dir.clone())
    }

    /// The index of the workspace the window shows: the one named, else
    /// the first that is not broken, as the panes show.
    fn at(&self) -> Option<usize> {
        let mut list = self.workspaces.as_ref()?.iter();
        match &self.shown {
            Some(name) => list.position(|w| w.name == *name),
            None => list.position(|w| w.broken.is_none()),
        }
    }

    /// The rows and the workspaces' targets.
    fn laid(&self, theme: &Theme, window: Rect) -> (Vec<Row>, Vec<Target>) {
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
                let at = self.at();
                for (i, workspace) in list.iter().enumerate() {
                    let shown = at == Some(i);
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
        let mut targets: Vec<Target> = Vec::new();
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
            let tap = line.workspace.map(|i| line.run.map_or(Tap::Workspace(i), Tap::Run));
            match (tap, targets.last_mut()) {
                (Some(t), Some((frame, at))) if *at == t => frame.height = y + ROW - frame.y,
                (Some(t), _) => {
                    targets.push((Rect { x: window.x, y, width: wide, height: ROW }, t));
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
    /// The command a tap on it runs, rather than show its workspace.
    run: Option<Command>,
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
            run: None,
        }
    }

    /// A line under a workspace's name, without a mark.
    fn under(text: String, colour: Rgb) -> Self {
        Self { under: true, ..Line::new(text, colour, Font::PLAIN) }
    }
}

/// A workspace's lines: its name (its folder's for a broken file that
/// names none), then why a broken one cannot be used, or its agent and
/// status, a blocked agent's note, and while the window shows it and its
/// agent does not run, "Start agent".
fn block(workspace: &Workspace, shown: bool, theme: &Theme) -> Vec<Line> {
    let font = Font { bold: shown, ..Font::PLAIN };
    let mut name = workspace.name.clone();
    if name.is_empty() {
        name = workspace.dir.file_name().unwrap_or_default().to_string_lossy().into();
    }
    let mut lines = vec![Line { gap: GAP, ..Line::new(name, theme.text, font) }];
    match (&workspace.broken, &workspace.agent) {
        (Some(broken), _) => lines.extend(broken.lines(shown, theme)),
        (None, None) => lines.push(Line::under("no agent".into(), theme.muted)),
        (None, Some(agent)) => {
            let text = match agent.status {
                Some(status) => format!("{} · {}", agent.label(), status.word()),
                None => agent.label(),
            };
            let mark = Some(mark(agent.status, theme));
            lines.push(Line { under: true, mark, ..Line::new(text, theme.muted, Font::PLAIN) });
            if let (Some(Status::Blocked), Some(note)) = (agent.status, &agent.note) {
                lines.push(Line::under(note.clone(), theme.muted));
            }
            if shown && agent.status == Some(Status::Stopped) {
                let start = Line::under(START.into(), theme.focus);
                lines.push(Line { run: Some(Command::StartAgent), ..start });
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
