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
//!
//! An agent's line is `<agent> · <status>` (`<agent> (<profile>) ·
//! <status>` while it runs under a profile) or `no agent`; its mark is
//! round for working, round green for done, square red for blocked, and
//! hollow and muted for `not running` (its process exited or was stopped).
//! Each workspace's name line ends in a muted ✕, a 28 pt target listed
//! before its row's (`Tap::Close`). [`Sidebar::dir`] is a row's folder,
//! what closes it; [`Sidebar::closed`] takes a row out once the daemon
//! closed it (then the first usable one shows, if it was the shown one);
//! `shown_dir` is the folder that `close-workspace`, the agent commands
//! and `show-workspace-folder` go to. The layout is [`rows`]'.

use std::path::{Path, PathBuf};

use crate::backend::Problem;
use crate::config::{Rgb, Theme};
use crate::layout::Rect;
use crate::scene::taps::Target;
use crate::scene::{Label, style};

mod agent;
pub mod broken;
pub mod launched;
pub mod notice;
mod rows;

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

impl Workspace {
    /// What the sidebar calls it: its name, or its folder's for a broken
    /// file that names none.
    pub fn title(&self) -> String {
        match self.name.as_str() {
            "" => self.dir.file_name().unwrap_or_default().to_string_lossy().into(),
            name => name.into(),
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
    /// What the app says of the daemon it starts, in place of the list,
    /// and of its build.
    launched: launched::Launched,
    /// The agents in the notice, oldest first, with the status each became.
    notice: Vec<notice::Noticed>,
    /// The workspace whose panes the window shows; `None`: the first usable.
    shown: Option<Shown>,
    /// The workspace the window showed when the app last quit: the one
    /// shown while none is named or tapped, if it is open and usable.
    last: Option<String>,
}

/// How the sidebar knows the workspace the window shows: by the name the
/// config or an open gave, or by the folder of the one tapped, which is
/// unique even among broken files that name none.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Shown {
    Name(String),
    Dir(PathBuf),
}

impl Sidebar {
    /// An empty sidebar, open or collapsed.
    pub fn new(open: bool) -> Self {
        let (workspaces, problem, launched, notice) =
            (None, None, launched::Launched::default(), Vec::new());
        Self { open, workspaces, problem, launched, notice, shown: None, last: None }
    }

    /// The sidebar, showing `last` (the workspace the window showed when the
    /// app last quit) while none is named or tapped, as the panes do.
    #[must_use]
    pub fn preferring(self, last: Option<String>) -> Self {
        Self { last, ..self }
    }

    /// Marks `workspace` as the one the window shows; `None`: the first
    /// that is not broken, as the panes do (`backend::panes::layout`).
    pub fn show(&mut self, workspace: Option<String>) {
        self.shown = workspace.map(Shown::Name);
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
    /// shows its panes): what the panes ask for, its name, or its directory
    /// for a broken file that names none, so the daemon says why that file
    /// is broken (slinger-7e27ff); `None` past the last.
    pub fn pick(&mut self, index: usize) -> Option<String> {
        let workspace = self.workspaces.as_ref()?.get(index)?;
        self.shown = Some(Shown::Dir(workspace.dir.clone()));
        Some(match workspace.name.as_str() {
            "" => workspace.dir.to_string_lossy().into_owned(),
            name => name.to_string(),
        })
    }

    /// The directory of the workspace with index `index` in the list: what
    /// closes it, whatever its file holds. `None` past the last.
    pub fn dir(&self, index: usize) -> Option<PathBuf> {
        Some(self.workspaces.as_ref()?.get(index)?.dir.clone())
    }

    /// Takes the workspace in `dir` out of the list, after the daemon closed
    /// it: the workspace taken, and whether the window showed it (then the
    /// first usable shows, as at the start). `None` when it was not listed.
    pub fn closed(&mut self, dir: &Path) -> Option<(Workspace, bool)> {
        let shown = self.at();
        let list = self.workspaces.as_mut()?;
        let at = list.iter().position(|w| w.dir == dir)?;
        let gone = list.remove(at);
        let was = shown == Some(at);
        if was {
            self.shown = None;
        }
        Some((gone, was))
    }

    /// The open workspace in the folder `dir`, as last heard.
    pub fn workspace(&self, dir: &Path) -> Option<&Workspace> {
        self.workspaces.as_ref()?.iter().find(|w| w.dir == dir)
    }

    /// Whether the daemon said no workspace is open: an empty list, not
    /// one still to come or lost with the daemon.
    pub fn none_open(&self) -> bool {
        self.workspaces.as_ref().is_some_and(Vec::is_empty)
    }

    /// The directory of the workspace the window shows: what its agent's
    /// commands go to, whatever its file holds. `None` when none shows.
    pub fn shown_dir(&self) -> Option<PathBuf> {
        Some(self.workspaces.as_ref()?.get(self.at()?)?.dir.clone())
    }

    /// The agent of the workspace the window shows, as last heard.
    pub fn shown_agent(&self) -> Option<&Agent> {
        self.workspaces.as_ref()?.get(self.at()?)?.agent.as_ref()
    }

    /// The name of the workspace the window shows, to show it again at the
    /// next launch. `None` when none shows, and for a broken file that names
    /// none.
    pub fn shown_name(&self) -> Option<&str> {
        let name = self.workspaces.as_ref()?.get(self.at()?)?.name.as_str();
        Some(name).filter(|n| !n.is_empty())
    }

    /// The index of the workspace the window shows: the one named (else
    /// the broken file in a folder of that name, [`broken::by_folder`]) or
    /// tapped, else the last shown if it is open and not broken, else the
    /// first that is not broken, as the panes show (`backend::panes::layout`).
    fn at(&self) -> Option<usize> {
        let list = self.workspaces.as_ref()?;
        let at = |is: &dyn Fn(&Workspace) -> bool| list.iter().position(is);
        match &self.shown {
            Some(Shown::Name(name)) => at(&|w| w.name == *name)
                .or_else(|| at(&|w| broken::by_folder(&w.name, &w.dir, name))),
            Some(Shown::Dir(dir)) => at(&|w| w.dir == *dir),
            None => at(&|w| w.broken.is_none() && Some(&w.name) == self.last.as_ref())
                .or_else(|| at(&|w| w.broken.is_none())),
        }
    }
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
