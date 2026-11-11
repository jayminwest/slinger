//! The `slinger` macOS app: a thin front end over `slingerd`. It does not
//! link the core.
//!
//! `backend` gets the data, `config` reads the config file, `commands` turns
//! keys into commands, `panes` keeps the window's panes and their focus,
//! `canvases` keeps what each canvas pane shows (a `mirror` of a page, a
//! viewport and the pen's ink), `scene` decides what the window shows (the
//! agents' statuses in `scene::sidebar`), `ui` holds an agent UI's page and
//! its bridge, `layout` does the geometry and `platform` draws.
//! Only `platform` touches `AppKit`, and only this file starts it (gate
//! check:layers).
//!
//! A UI pane's reply goes to the daemon, which hands it to the agent that
//! opened the showing (`backend::ui::reply`); the pane notes one its agent
//! did not take. `slinger-app --preview UI [--data JSON]` shows one agent UI
//! (or a showing) alone in the window and writes each of its replies to
//! stdout, one `ui_reply` line each.

mod backend;
mod canvases;
mod commands;
mod config;
mod layout;
mod mirror;
mod panes;
mod platform;
mod scene;
mod ui;

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use slinger_control::{self as control, PaneLayout};

use crate::backend::{Problem, Settings, Watch};
use crate::canvases::Canvases;
use crate::commands::Command;
use crate::config::Shown;
use crate::layout::{Axis, Direction, PaneId, Point, Rect};
use crate::panes::{Pane, Panes};
use crate::platform::{Event, Step};
use crate::scene::sidebar::Sidebar;
use crate::scene::tools::{Kind, Tool, Tools};
use crate::scene::{Notes, Scene, Uis};

fn main() -> ExitCode {
    let settings = backend::Settings::load(backend::ConfigFiles::locate());
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        [] => window(settings),
        ["--preview", reference] => preview(&settings, reference, None),
        ["--preview", reference, "--data", data] => preview(&settings, reference, Some(data)),
        _ => fail("usage: slinger-app [--preview UI [--data JSON]]", None),
    }
}

/// Writes `message` and its `hint` to stderr; the exit code for a failure.
fn fail(message: &str, hint: Option<&str>) -> ExitCode {
    // Nothing can be done if stderr is closed too.
    let _ = writeln!(io::stderr(), "✗ {message}");
    if let Some(hint) = hint {
        let _ = writeln!(io::stderr(), "  {hint}");
    }
    ExitCode::FAILURE
}

/// Shows the agent UI `reference` names (or a showing, by its id) alone,
/// with `data` as its context's data (else the showing's), and writes each
/// reply to stdout until the window closes.
fn preview(settings: &Settings, reference: &str, data: Option<&str>) -> ExitCode {
    let data = match data.map(ui::data).transpose() {
        Ok(data) => data,
        Err(why) => return fail(&why, None),
    };
    let daemon = control::Client::new(settings.socket());
    let page = match backend::ui::fetch(&daemon, reference) {
        Ok(shown) => ui::Page::new(ui::Shown { data: data.unwrap_or(shown.data), ..shown }, "pane"),
        Err(problem) => return fail(&problem.message, problem.hint.as_deref()),
    };
    let page = match page {
        Ok(page) => Arc::new(page),
        Err(why) => return fail(&why, None),
    };
    let keys = settings.config.keys;
    let answer = move |event| match event {
        Event::Reply(line) => {
            let mut out = io::stdout().lock();
            match writeln!(out, "{line}").and_then(|()| out.flush()) {
                Ok(()) => Step::Stay,
                Err(e) => Step::Refuse(format!("stdout: {e}")),
            }
        }
        Event::Key(chord) if keys.command(chord) == Some(Command::Quit) => Step::Quit,
        Event::Key(_)
        | Event::Press(_)
        | Event::Drag(_)
        | Event::Lift(_)
        | Event::Scroll { .. }
        | Event::Zoom { .. } => Step::Pass,
        Event::Tick | Event::Moved(_) | Event::Painted(_) => Step::Stay,
    };
    let scene = scene::preview(page, &settings.config.theme);
    match platform::run(&scene, answer) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e.to_string(), None),
    }
}

/// The window onto the daemon: the panes of its workspace, the sidebar and
/// the pen.
fn window(mut settings: Settings) -> ExitCode {
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
    // event, paint of new ink, and tick of the window's timer. An open
    // pop-up takes its keys and presses first; the pen draws with the pen
    // tool on a canvas pane. A tick stats the two config files and takes the
    // watch's news of the agents and the panes and what the canvases'
    // followers heard; the watch and the followers follow an edit that moves
    // the socket or the workspace. Only a split or a close asks the daemon here.
    let answer = move |event| {
        let force = match event {
            Event::Tick => false,
            Event::Reply(line) => {
                return match shared.reply(&line, &socket) {
                    Ok(false) => Step::Stay,
                    Ok(true) => {
                        Step::Draw(view(&settings, &shared, &tools, &sidebar, &mut canvases))
                    }
                    Err(why) => Step::Refuse(why),
                };
            }
            Event::Moved(_)
            | Event::Painted(_)
            | Event::Press(_)
            | Event::Drag(_)
            | Event::Lift(_)
            | Event::Scroll { .. }
            | Event::Zoom { .. } => {
                let area = sidebar.panes(window.rect());
                if !pointer(&event, &mut tools, &mut canvases, &shared.panes, area) {
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
        let fetched = shared.fetched(watch.uis());
        let drawn = canvases.poll();
        let reloaded = settings.refresh(force);
        if !reloaded && !heard && !moved && !drawn && !fetched {
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
        Err(e) => fail(&e.to_string(), None),
    }
}

/// The window's panes as the server keeps them for its workspace: one pane
/// until the server's layout comes, or without a daemon.
#[derive(Debug)]
struct Shared {
    panes: Panes,
    /// The agent UIs the panes show, as last fetched.
    uis: Uis,
    /// The workspace the panes are the server's for; `None` while the
    /// server cannot say, when the window changes its own panes.
    workspace: Option<String>,
    /// The problem asking the server, if any.
    trouble: Option<Problem>,
    /// Why an agent UI's last reply did not reach its agent, by showing.
    notes: Notes,
}

impl Default for Shared {
    fn default() -> Self {
        let main = Pane { kind: panes::Kind::Canvas, name: "main".to_string() };
        let (uis, notes) = (Uis::new(), Notes::new());
        Self { panes: Panes::new(main), uis, workspace: None, trouble: None, notes }
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

    /// Takes the agent UIs' code the watch fetched, each as a page for a
    /// pane or why it cannot show; whether there was any.
    fn fetched(&mut self, codes: Vec<(String, Result<ui::Shown, Problem>)>) -> bool {
        let any = !codes.is_empty();
        for (name, shown) in codes {
            let page = shown.and_then(|shown| {
                let page = ui::Page::new(shown, "pane");
                page.map(Arc::new).map_err(|why| Problem { message: why, hint: None })
            });
            self.uis.insert(name, page);
        }
        any
    }

    /// Sends an agent UI's reply `line` to the daemon on `socket`, which
    /// records it and hands it to the agent that opened the showing. The
    /// showing's pane notes an agent that did not take it, until one does;
    /// whether that changed what the window shows, or why the reply cannot
    /// go on, for the page's promise.
    fn reply(&mut self, line: &str, socket: &Path) -> Result<bool, String> {
        Ok(match backend::ui::reply(&control::Client::new(socket), line)? {
            (open, Some(unreachable)) => {
                self.notes.insert(open, unreachable);
                true
            }
            (open, None) => self.notes.remove(&open).is_some(),
        })
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

/// Hands a pen or mouse press, drag or lift to the pop-up at the pen, then
/// to the canvas pane of `panes` laid in `area` under it, where the pen tool
/// draws; a scroll or a pinch pans or zooms. False when nothing used it, and
/// for a move or a paint, which need no new scene. A lift writes the
/// stroke's delay log.
fn pointer(
    event: &Event,
    tools: &mut Tools,
    canvases: &mut Canvases,
    panes: &Panes,
    area: Rect,
) -> bool {
    match *event {
        Event::Moved(at) => {
            tools.moved(at);
            false
        }
        Event::Painted(delay) => {
            canvases.painted(delay);
            false
        }
        Event::Press(pen) if tools.press(pen.at) => true,
        Event::Press(pen) => {
            let under = pane_at(panes, area, pen.at);
            let drawing = tools.tool() == Tool::Pen;
            drawing && under.is_some_and(|u| canvases.pen_down(panes, u, pen, tools.colour()))
        }
        Event::Drag(pen) => {
            tools.moved(pen.at);
            canvases.pen_drag(pen)
        }
        Event::Lift(pen) => {
            let ended = canvases.pen_up(pen);
            if let Some(line) = canvases.delay_log() {
                // Nothing can be done if stderr is closed.
                let _ = writeln!(io::stderr(), "{line}");
            }
            ended
        }
        _ => gesture(canvases, panes, area, event),
    }
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

/// The canvas pane of `panes` laid in `area` under `at`, a point in the
/// window, and its frame. Over any other pane a gesture goes on to what it
/// shows (an agent UI scrolls itself).
fn pane_at(panes: &Panes, area: Rect, at: Point) -> Option<(PaneId, Rect)> {
    let placed = panes.place(area);
    let found = placed.into_iter().find(|(_, _, frame)| frame.contains(at));
    found.filter(|(_, pane, _)| pane.kind == panes::Kind::Canvas).map(|(id, _, frame)| (id, frame))
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
    let mut scene =
        scene::build(&shared.panes, theme, &problems, sidebar, drawn, &shared.uis, &shared.notes);
    scene.labels.extend(tools.labels(theme));
    scene
}
