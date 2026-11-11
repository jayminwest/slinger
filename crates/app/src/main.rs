//! The `slinger` macOS app: a thin front end over `slingerd`. It does not
//! link the core.
//!
//! `backend` gets the data, `config` reads the config file, `commands` turns
//! keys into commands, `panes` keeps the window's panes and their focus,
//! `canvases` keeps what each canvas pane shows (a `mirror` of a page and a
//! viewport), `scene` decides what the window shows (the agents' statuses
//! in `scene::sidebar`), `layout` does the geometry and `platform` draws.
//! Only `platform` touches `AppKit`, and only this file starts it (gate
//! check:layers).

mod backend;
mod canvases;
mod commands;
mod config;
mod layout;
mod mirror;
mod panes;
mod platform;
mod scene;

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use slinger_control::{self as control, PaneLayout};

use crate::backend::{Problem, Settings, Watch};
use crate::canvases::Canvases;
use crate::commands::Command;
use crate::config::Shown;
use crate::layout::{Axis, Direction, PaneId, Point, Rect};
use crate::panes::{Pane, Panes};
use crate::platform::{Event, Step};
use crate::scene::Scene;
use crate::scene::sidebar::Sidebar;
use crate::scene::tools::{Kind, Tools};

fn main() -> ExitCode {
    let mut settings = backend::Settings::load(backend::ConfigFiles::locate());
    let mut socket = settings.socket();
    let watch = Watch::start(socket.clone(), settings.config.workspace.clone());
    let mut shown = settings.config.sidebar;
    let mut sidebar = Sidebar::new(shown == Shown::Open);
    let window = layout::WINDOW;
    let centre = Point { x: window.width / 2.0, y: window.height / 2.0 };
    let mut tools = Tools::new(settings.config.palette.clone(), centre);
    let mut shared = Shared::default();
    let mut canvases = Canvases::new(control::Client::new(&socket));
    let first = view(&settings, &shared, &tools, &sidebar, &mut canvases);
    // Runs on the main thread for each key press, pen, mouse or trackpad
    // event, and tick of the window's timer. An open pop-up takes its keys
    // before the keymap. A tick stats the two config files and takes the
    // watch's news of the agents and the panes and what the canvases'
    // followers heard; the watch and the followers follow an edit that moves
    // the socket or the workspace. Only a split or a close asks the daemon here.
    let answer = move |event| {
        let force = match event {
            Event::Tick => false,
            Event::Moved(at) => {
                tools.moved(at);
                return Step::Stay;
            }
            Event::Press(at) if tools.press(at) => {
                return Step::Draw(view(&settings, &shared, &tools, &sidebar, &mut canvases));
            }
            Event::Press(_) => return Step::Pass,
            Event::Scroll { .. } | Event::Zoom { .. } => {
                let area = sidebar.panes(window.rect());
                if !gesture(&mut canvases, &shared.panes, area, &event) {
                    return Step::Pass;
                }
                return Step::Draw(view(&settings, &shared, &tools, &sidebar, &mut canvases));
            }
            Event::Key(chord) if tools.key(chord) => {
                return Step::Draw(view(&settings, &shared, &tools, &sidebar, &mut canvases));
            }
            Event::Key(chord) => {
                let kind = match settings.config.keys.command(chord) {
                    None => return Step::Pass,
                    Some(Command::Quit) => return Step::Quit,
                    Some(Command::ReloadConfig) => None,
                    Some(Command::PickColour) => Some(Kind::Colour),
                    Some(Command::PickShape) => Some(Kind::Shape),
                    Some(Command::PickTool) => Some(Kind::Tool),
                    Some(command) => {
                        let area = sidebar.panes(window.rect());
                        match command {
                            Command::ToggleSidebar => sidebar.set_open(!sidebar.is_open()),
                            Command::DismissNotice => sidebar.dismiss(),
                            _ if canvas_command(&mut canvases, &shared.panes, area, command) => {}
                            _ => shared.command(command, area, &socket),
                        }
                        return Step::Draw(view(
                            &settings,
                            &shared,
                            &tools,
                            &sidebar,
                            &mut canvases,
                        ));
                    }
                };
                if let Some(kind) = kind {
                    tools.open(kind, focused(&shared.panes, sidebar.panes(window.rect())));
                    return Step::Draw(view(&settings, &shared, &tools, &sidebar, &mut canvases));
                }
                true
            }
        };
        let heard = watch.latest().is_some_and(|now| sidebar.update(now));
        let moved = watch.layout().map(|news| shared.follow(news)).is_some();
        let drawn = canvases.poll();
        let reloaded = settings.refresh(force);
        if !reloaded && !heard && !moved && !drawn {
            return Step::Stay;
        }
        if reloaded {
            tools.set_palette(settings.config.palette.clone());
            if settings.config.sidebar != shown {
                shown = settings.config.sidebar;
                sidebar.set_open(shown == Shown::Open);
            }
            watch.want(settings.config.workspace.clone());
            let now = settings.socket();
            if now != socket {
                watch.follow(now.clone());
                canvases.reconnect(control::Client::new(&now));
                socket = now;
            }
        }
        Step::Draw(view(&settings, &shared, &tools, &sidebar, &mut canvases))
    };
    match platform::run(&first, answer) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // Nothing can be done if stderr is closed too.
            let _ = writeln!(io::stderr(), "✗ {e}");
            ExitCode::FAILURE
        }
    }
}

/// The window's panes as the server keeps them for its workspace: one pane
/// until the server's layout comes, or without a daemon.
#[derive(Debug)]
struct Shared {
    panes: Panes,
    /// The workspace the panes are the server's for; `None` while the
    /// server cannot say, when the window changes its own panes.
    workspace: Option<String>,
    /// The problem asking the server, if any.
    trouble: Option<Problem>,
}

impl Default for Shared {
    fn default() -> Self {
        let main = Pane { kind: panes::Kind::Canvas, name: "main".to_string() };
        Self { panes: Panes::new(main), workspace: None, trouble: None }
    }
}

impl Shared {
    /// Takes the server's newest layout, or the problem asking for it: the
    /// panes follow it, focus kept on the same number.
    fn follow(&mut self, news: Result<PaneLayout, Problem>) {
        match news {
            Ok(layout) => {
                self.panes = Panes::follow(&layout.layout, self.panes.number());
                (self.workspace, self.trouble) = (Some(layout.workspace), None);
            }
            Err(problem) => (self.workspace, self.trouble) = (None, Some(problem)),
        }
    }

    /// Runs a pane command on the panes in `area`. Opening and closing go
    /// to the daemon on `socket`, which keeps the layout (to the window's
    /// own panes without a workspace). Commands that are not about panes do
    /// nothing.
    fn command(&mut self, command: Command, area: Rect, socket: &Path) {
        let panes = &mut self.panes;
        let server = self.workspace.as_deref().map(|w| (control::Client::new(socket), w));
        let asked = match (command, server) {
            (Command::SplitRight | Command::SplitDown, Some((daemon, workspace))) => {
                let below = command == Command::SplitDown;
                backend::panes::open(&daemon, workspace, panes.focused(), panes.number(), below)
                    .map(|(layout, number)| Panes::follow(&layout, number))
            }
            (Command::ClosePane, Some((daemon, workspace))) => {
                // Focus goes to the pane before it; the first's to the next.
                let number = panes.number();
                backend::panes::close(&daemon, workspace, number)
                    .map(|layout| Panes::follow(&layout, number.saturating_sub(1)))
            }
            _ => {
                own(panes, command, area);
                return;
            }
        };
        match asked {
            Ok(now) => *panes = now,
            Err(problem) => self.trouble = Some(problem),
        }
    }
}

/// Runs a pane command on the window's own panes in `area`. Commands that
/// are not about panes do nothing.
fn own(panes: &mut Panes, command: Command, area: Rect) {
    match command {
        Command::Quit
        | Command::ReloadConfig
        | Command::PickColour
        | Command::PickShape
        | Command::PickTool
        | Command::ToggleSidebar
        | Command::DismissNotice
        | Command::PanLeft
        | Command::PanRight
        | Command::PanUp
        | Command::PanDown
        | Command::ZoomIn
        | Command::ZoomOut
        | Command::NextPage => {}
        Command::SplitRight => panes.split(Axis::Row),
        Command::SplitDown => panes.split(Axis::Column),
        Command::ClosePane => panes.close(),
        Command::FocusNext => panes.focus_next(),
        Command::FocusLeft => panes.focus_toward(Direction::Left, area),
        Command::FocusRight => panes.focus_toward(Direction::Right, area),
        Command::FocusUp => panes.focus_toward(Direction::Up, area),
        Command::FocusDown => panes.focus_toward(Direction::Down, area),
        // The window's own until resizing goes to the server (slinger-dfac).
        Command::GrowPane => panes.grow(1.0),
        Command::ShrinkPane => panes.grow(-1.0),
    }
}

/// How far a pan command moves the canvas, in screen points.
const PAN_STEP: f64 = 80.0;
/// How much a zoom command zooms in, or out.
const ZOOM_STEP: f64 = 1.25;

/// Runs a canvas command on the focused pane of `panes` laid in `area`;
/// false for a command that is not about canvases.
fn canvas_command(canvases: &mut Canvases, panes: &Panes, area: Rect, command: Command) -> bool {
    let focus = panes.focus();
    let frame = focused(panes, area);
    let centre = Point { x: frame.width / 2.0, y: frame.height / 2.0 };
    match command {
        Command::PanLeft => canvases.pan(focus, PAN_STEP, 0.0),
        Command::PanRight => canvases.pan(focus, -PAN_STEP, 0.0),
        Command::PanUp => canvases.pan(focus, 0.0, PAN_STEP),
        Command::PanDown => canvases.pan(focus, 0.0, -PAN_STEP),
        Command::ZoomIn => canvases.zoom(focus, centre, ZOOM_STEP),
        Command::ZoomOut => canvases.zoom(focus, centre, 1.0 / ZOOM_STEP),
        Command::NextPage => canvases.next_page(panes, focus),
        _ => return false,
    }
    true
}

/// Pans or zooms the canvas of the pane of `panes` laid in `area` under a
/// scroll or a pinch; false when no pane is under it, or for another event.
fn gesture(canvases: &mut Canvases, panes: &Panes, area: Rect, event: &Event) -> bool {
    let (Event::Scroll { at, .. } | Event::Zoom { at, .. }) = *event else { return false };
    let Some((pane, frame)) = pane_at(panes, area, at) else { return false };
    match *event {
        Event::Scroll { dx, dy, .. } => canvases.pan(pane, dx, dy),
        Event::Zoom { factor, .. } => {
            canvases.zoom(pane, Point { x: at.x - frame.x, y: at.y - frame.y }, factor);
        }
        _ => return false,
    }
    true
}

/// The pane of `panes` laid in `area` under `at`, a point in the window,
/// and its frame.
fn pane_at(panes: &Panes, area: Rect, at: Point) -> Option<(PaneId, Rect)> {
    let placed = panes.place(area);
    placed.into_iter().find(|(_, _, frame)| frame.contains(at)).map(|(id, _, frame)| (id, frame))
}

/// The frame of the focused pane of `panes` laid in `area`.
fn focused(panes: &Panes, area: Rect) -> Rect {
    let placed = panes.place(area);
    let found = placed.into_iter().find(|(id, _, _)| *id == panes.focus());
    found.map_or(area, |(_, _, frame)| frame)
}

/// The window: the sidebar, the panes, any problem with the daemon, a
/// canvas or the config (each said once), the notice, and the pop-up at
/// the pen on top.
fn view(
    settings: &Settings,
    shared: &Shared,
    tools: &Tools,
    sidebar: &Sidebar,
    canvases: &mut Canvases,
) -> Scene {
    let theme = &settings.config.theme;
    let area = sidebar.panes(layout::WINDOW.rect());
    let drawn = canvases.show(&shared.panes, area, theme);
    let mut problems: Vec<&Problem> = Vec::new();
    for problem in shared.trouble.iter().chain(canvases.problems()).chain(&settings.problem) {
        if !problems.contains(&problem) {
            problems.push(problem);
        }
    }
    let mut scene = scene::build(&shared.panes, theme, &problems, sidebar, drawn);
    scene.labels.extend(tools.labels(theme));
    scene
}
