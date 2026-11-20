//! The sidebar's layout: its lines, where each is placed, and the
//! workspaces' targets.

use super::notice::CLOSE;
use super::{Font, GAP, HEADING, MARK, PAD, ROW, Row, START, Sidebar, Status, WIDTH, Workspace};
use super::{broken, lighter, mark};
use crate::commands::Command;
use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect};
use crate::scene::taps::{Tap, Target};
use crate::scene::{Label, glyphs, status, style};

impl Sidebar {
    /// The index of the workspace whose rows are under `at` in `window`,
    /// laid as the targets are ([`Sidebar::targets`]): its name, its ✕,
    /// its agent's lines, and the shown one's command line. `None`
    /// elsewhere, and when collapsed.
    pub fn row_at(&self, theme: &Theme, window: Rect, at: Point) -> Option<usize> {
        let targets = self.laid(theme, window).1;
        let (_, tap) = targets.into_iter().find(|(frame, _)| frame.contains(at))?;
        match tap {
            Tap::Workspace(i) | Tap::Close(i) => Some(i),
            // Only the shown workspace has a command line.
            Tap::Run(Command::StartAgent | Command::CloseWorkspace) => self.at(),
            _ => None,
        }
    }

    /// The rows and the workspaces' targets.
    pub(super) fn laid(&self, theme: &Theme, window: Rect) -> (Vec<Row>, Vec<Target>) {
        if !self.open {
            return (Vec::new(), Vec::new());
        }
        let heading = Font { size: Some(11.0), bold: false, spacing: 1.0 };
        let mut lines = vec![Line::new(HEADING.into(), theme.muted, heading)];
        match (&self.workspaces, &self.problem) {
            _ if let Some(daemon) = self.daemon() => {
                let line = |(t, c, run)| Line { run, ..Line::new(t, c, Font::PLAIN) };
                lines.extend(daemon.sidebar(theme).into_iter().map(line));
            }
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
            let close = (line.name && line.workspace.is_some()).then_some(Rect {
                x: window.x + wide - style::TAP,
                y,
                width: style::TAP,
                height: ROW,
            });
            let end = close.map_or(right, |c| right.min(c.x));
            rows.push(Row { label: label(line.text, line.colour, x, y, end), font: line.font });
            if let (Some(frame), Some(i)) = (close, line.workspace) {
                let width = glyphs::frame(Point { x: 0.0, y: 0.0 }, CLOSE, broken::TEXT).width;
                let x = frame.x + (frame.width - width) / 2.0 - broken::INSET;
                rows.push(Row {
                    label: label(CLOSE.into(), theme.muted, x, y, frame.x + frame.width),
                    font: Font::PLAIN,
                });
                targets.push((frame, Tap::Close(i)));
            }
            if line.shown {
                lit = Some((lit.map_or(y, |(top, _)| top), y + ROW));
            }
            let tap = line.run.map(Tap::Run).or(line.workspace.map(Tap::Workspace));
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
pub(super) struct Line {
    /// Whether it is under a workspace's name: set in by a mark's width.
    pub(super) under: bool,
    /// A status mark and its colour, in that space.
    pub(super) mark: Option<(&'static str, Rgb)>,
    pub(super) text: String,
    pub(super) colour: Rgb,
    pub(super) font: Font,
    /// Space above it.
    pub(super) gap: f64,
    /// Whether it belongs to the workspace the window shows.
    pub(super) shown: bool,
    /// The index of the workspace it belongs to, if any.
    pub(super) workspace: Option<usize>,
    /// The command a tap on it runs, rather than show its workspace.
    pub(super) run: Option<Command>,
    /// Whether it is its workspace's name: a ✕ at its end closes it.
    pub(super) name: bool,
}

impl Line {
    pub(super) fn new(text: String, colour: Rgb, font: Font) -> Self {
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
            name: false,
        }
    }

    /// A line under a workspace's name, without a mark.
    pub(super) fn under(text: String, colour: Rgb) -> Self {
        Self { under: true, ..Line::new(text, colour, Font::PLAIN) }
    }
}

/// A workspace's lines: its name (its folder's for a broken file that
/// names none), then why a broken one cannot be used, or its agent and
/// status, a blocked agent's note, and while the window shows it and its
/// agent does not run, "Start agent".
fn block(workspace: &Workspace, shown: bool, theme: &Theme) -> Vec<Line> {
    let font = Font { bold: shown, ..Font::PLAIN };
    let name = Line::new(workspace.title(), theme.text, font);
    let mut lines = vec![Line { gap: GAP, name: true, ..name }];
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
