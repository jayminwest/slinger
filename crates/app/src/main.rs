//! The `slinger` macOS app: a thin front end over `slingerd`. It does not
//! link the core.
//!
//! `backend` gets the data, `config` reads the config file, `commands` turns
//! keys into commands, `panes` keeps the window's panes and their focus,
//! `shared` keeps them as the server does,
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
//! (or a showing) alone in the window, `--as item` as a live item on a
//! canvas, and writes each of its replies to stdout, one `ui_reply` line each.

mod backend;
mod canvases;
mod commands;
mod config;
mod layout;
mod mirror;
mod panes;
mod platform;
mod scene;
mod shared;
mod ui;

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use slinger_control as control;

use crate::backend::{Problem, Settings, Watch};
use crate::canvases::Canvases;
use crate::commands::{Chord, Command};
use crate::config::Shown;
use crate::layout::{PaneId, Point, Rect};
use crate::panes::Panes;
use crate::platform::{Event, Step};
use crate::scene::sidebar::Sidebar;
use crate::scene::tools::{Kind, Tools};
use crate::scene::{Scene, cursor, status};
use crate::shared::Shared;

fn main() -> ExitCode {
    let settings = backend::Settings::load(backend::ConfigFiles::locate());
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        [] => window(settings),
        ["--preview", reference, ref more @ ..] => preview(&settings, reference, more),
        _ => fail("usage: slinger-app [--preview UI [--data JSON] [--as item]]", None),
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
/// with `--data` as its context's data (else the showing's), as a pane or
/// `--as item`, and writes each reply to stdout until the window closes.
fn preview(settings: &Settings, reference: &str, more: &[&str]) -> ExitCode {
    let (data, item) = match more {
        [] | ["--as", "item"] => (None, !more.is_empty()),
        ["--data", data] | ["--data", data, "--as", "item"] => (Some(*data), more.len() > 2),
        _ => return fail("usage: slinger-app --preview UI [--data JSON] [--as item]", None),
    };
    let data = match data.map(ui::data).transpose() {
        Ok(data) => data,
        Err(why) => return fail(&why, None),
    };
    let daemon = control::Client::new(settings.socket());
    let page = match backend::ui::fetch(&daemon, reference) {
        Ok(shown) => {
            let host = if item { "canvas" } else { "pane" };
            ui::Page::new(ui::Shown { data: data.unwrap_or(shown.data), ..shown }, host)
        }
        Err(problem) => return fail(&problem.message, problem.hint.as_deref()),
    };
    let page = match page {
        Ok(page) => Arc::new(page),
        Err(why) => return fail(&why, None),
    };
    let (keys, theme) = (settings.config.keys, settings.config.theme);
    let scene = scene::preview(Arc::clone(&page), &theme, item, layout::WINDOW);
    let answer = move |event| match event {
        Event::Resized(size) => {
            Step::Draw(Box::new(scene::preview(Arc::clone(&page), &theme, item, size)))
        }
        Event::Reply(line) => {
            let mut out = io::stdout().lock();
            match writeln!(out, "{line}").and_then(|()| out.flush()) {
                Ok(()) => Step::Stay,
                Err(e) => Step::Refuse(format!("stdout: {e}")),
            }
        }
        Event::Key(chord) if keys.command(chord) == Some(Command::Quit) => Step::Quit,
        Event::Key(_)
        | Event::Text(_)
        | Event::Press(_)
        | Event::Drag(_)
        | Event::Lift(_)
        | Event::Scroll { .. }
        | Event::Zoom { .. } => Step::Pass,
        Event::Tick | Event::Moved(_) | Event::Painted(_) | Event::Typed(_) => Step::Stay,
    };
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
    sidebar.show(settings.config.workspace.clone());
    let centre = Point { x: layout::WINDOW.width / 2.0, y: layout::WINDOW.height / 2.0 };
    let mut tools = Tools::new(settings.config.palette.clone(), centre);
    let mut shared = Shared::default();
    let mut canvases = Canvases::new(control::Client::new(&socket));
    let first = view(&settings, &shared, &tools, &sidebar, &mut canvases);
    // Runs on the main thread for each key press, pen, mouse or trackpad
    // event, paint of new ink, and tick of the window's timer. The open key
    // overlay takes every key and press; then the sling's entry, then a
    // pop-up, takes its keys first; a press on a divider between
    // panes drags it; the pen draws with the pen tool on a canvas pane. A
    // tick stats the two config files and takes the watch's news of the
    // agents and the panes and what the canvases' followers heard; the watch
    // and the followers follow an edit that moves the socket or the
    // workspace. Only a split, a close or a resize asks the daemon here.
    let answer = move |event| {
        let area = sidebar.panes(shared.size.rect());
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
            Event::Key(c) if taken(c, &mut shared, &mut canvases, &mut tools, &settings) => {
                return Step::Draw(view(&settings, &shared, &tools, &sidebar, &mut canvases));
            }
            Event::Key(chord) if canvases.takes(chord, &settings.config.keys) => return Step::Pass,
            Event::Key(chord) => {
                let kind = match settings.config.keys.command(chord) {
                    None => return Step::Pass,
                    Some(Command::Quit) => return Step::Quit,
                    Some(Command::ReloadConfig) => None,
                    Some(Command::PickColour) => Some(Kind::Colour),
                    Some(Command::PickShape) => Some(Kind::Shape),
                    Some(Command::PickTool) => Some(Kind::Tool),
                    // Nothing selected here: the key goes on (a delete in a UI's form).
                    Some(c) if !selection(&mut canvases, &shared, area, c) => return Step::Pass,
                    Some(command) => {
                        match command {
                            Command::ToggleSidebar => sidebar.set_open(!sidebar.is_open()),
                            Command::DismissNotice => sidebar.dismiss(),
                            _ if canvases.command(
                                &shared.panes,
                                focus(&shared.panes, area),
                                command,
                            ) => {}
                            _ if tools.command(command) => {}
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
                    tools.open(kind, focused(&shared.panes, area));
                    return Step::Draw(view(&settings, &shared, &tools, &sidebar, &mut canvases));
                }
                true
            }
            // The pen, the mouse, the trackpad, typed text and a resize.
            _ => {
                if !pen(&event, &mut tools, &mut canvases, &mut shared, area, &socket) {
                    return Step::Pass;
                }
                return Step::Draw(view(&settings, &shared, &tools, &sidebar, &mut canvases));
            }
        };
        let heard = watch.latest().is_some_and(|now| sidebar.update(now));
        let moved = watch.layout().map(|news| shared.follow(news)).is_some();
        let fetched = shared.fetched(watch.uis());
        let drawn = canvases.poll();
        let reloaded = settings.refresh(force);
        let faded = tools.flash().fade(Instant::now());
        if !reloaded && !heard && !moved && !drawn && !fetched && !faded {
            return Step::Stay;
        }
        if reloaded {
            tools.set_palette(settings.config.palette.clone());
            if settings.config.sidebar != shown {
                shown = settings.config.sidebar;
                sidebar.set_open(shown == Shown::Open);
            }
            watch.want(settings.config.workspace.clone());
            sidebar.show(settings.config.workspace.clone());
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

/// Hands `chord` to what takes keys before the keymap: the open key overlay
/// in `shared` (every key), the sling's entry, then a pop-up. False when
/// none took it.
fn taken(
    chord: Chord,
    shared: &mut Shared,
    canvases: &mut Canvases,
    tools: &mut Tools,
    settings: &Settings,
) -> bool {
    shared.overlay.key(chord, &settings.config.keys)
        || canvases.sling_key(chord)
        || tools.key(chord)
}

/// Runs a selection command (`sling`, `dictate`: the entry) when the focused pane of `shared`
/// laid in `area` shows its canvas; false when it does not. Any other command goes on (true).
fn selection(canvases: &mut Canvases, shared: &Shared, area: Rect, command: Command) -> bool {
    let panes = &shared.panes;
    match command {
        Command::DeleteSelection => canvases.delete_selection(panes, panes.focus()),
        Command::ClearSelection => canvases.clear_selection(panes, panes.focus()),
        Command::Sling | Command::Dictate => canvases.sling_command(command, panes, area),
        _ => true,
    }
}

/// Hands a press on a divider between panes (with no pop-up open), and the
/// drag and lift after it, to the panes of `shared` laid in `area`, which
/// resize; a moved divider goes to the daemon on `socket`. A resized window
/// lays its panes out again. The open key overlay takes every press (one
/// outside it closes it) and every other pen, mouse or trackpad event and
/// typed text. Any other event goes on to [`pointer`]. False when nothing
/// used it.
fn pen(
    event: &Event,
    tools: &mut Tools,
    canvases: &mut Canvases,
    shared: &mut Shared,
    area: Rect,
    socket: &Path,
) -> bool {
    let free = tools.open_kind().is_none();
    match *event {
        Event::Resized(size) => std::mem::replace(&mut shared.size, size) != size,
        Event::Press(pen) if shared.overlay.press(pen.at, shared.size) => true,
        Event::Drag(_)
        | Event::Lift(_)
        | Event::Scroll { .. }
        | Event::Zoom { .. }
        | Event::Text(_)
            if shared.overlay.is_open() =>
        {
            true
        }
        Event::Press(pen) if free && shared.dividers.down(&shared.panes, area, pen.at) => true,
        Event::Drag(pen) if shared.dividers.drag(&mut shared.panes, pen.at) => true,
        Event::Lift(pen) if shared.dividers.held() => {
            let moved = shared.dividers.up(&mut shared.panes, pen.at);
            shared.resized(moved, socket);
            true
        }
        _ => pointer(event, tools, canvases, &shared.panes, area),
    }
}

/// Hands a pen or mouse press, drag or lift to the pop-up at the pen, then to
/// the canvas pane of `panes` laid in `area` under it, where the tool draws,
/// erases, selects, places text or draws a shape; typed text to the text being
/// typed (the sling's open entry keeps its presses and keys); a scroll or a
/// pinch pans or zooms. False when nothing used it, and for a move or a paint.
/// A lift writes the stroke's delay log.
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
        Event::Press(pen) if canvases.sling_covers(pen.at) => false,
        Event::Typed(ref text) => canvases.sling_typed(text),
        // A text being typed takes the keys that type, which come back as text.
        Event::Text(ref text) => canvases.typed(text),
        Event::Press(pen) if tools.press(pen.at) => true,
        Event::Press(pen) => {
            let tool = (tools.tool(), tools.colour());
            pane_at(panes, area, pen.at).is_some_and(|u| canvases.pen_down(panes, u, pen, tool))
        }
        Event::Drag(pen) => {
            tools.moved(pen.at);
            canvases.pen_drag(panes, pen)
        }
        Event::Lift(pen) => {
            let ended = canvases.pen_up(panes, pen, pane_at(panes, area, pen.at));
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
/// window, and its canvas's frame (above its footer). Over any other pane,
/// or a footer, a gesture goes on to what it shows (an agent UI scrolls itself).
fn pane_at(panes: &Panes, area: Rect, at: Point) -> Option<(PaneId, Rect)> {
    let mut placed = panes.place(area).into_iter().map(|(id, p, f)| (id, p, status::content(f)));
    let found = placed.find(|(_, _, frame)| frame.contains(at));
    found.filter(|(_, pane, _)| pane.kind == panes::Kind::Canvas).map(|(id, _, frame)| (id, frame))
}

/// The frame of the focused pane of `panes` laid in `area`.
fn focused(panes: &Panes, area: Rect) -> Rect {
    focus(panes, area).1
}

/// The focused pane of `panes` laid in `area`, and its frame above its footer.
fn focus(panes: &Panes, area: Rect) -> (PaneId, Rect) {
    let placed = panes.place(area);
    let found = placed.into_iter().find(|(id, _, _)| *id == panes.focus());
    (panes.focus(), status::content(found.map_or(area, |(_, _, frame)| frame)))
}

/// The window: the sidebar, the panes and their footers, any problem with the
/// daemon, a canvas or the config (each said once), the notice, the pop-up
/// at the pen and the flash of a new tool, the key overlay on top, and the
/// tool's cursor.
fn view(
    settings: &Settings,
    shared: &Shared,
    tools: &Tools,
    sidebar: &Sidebar,
    canvases: &mut Canvases,
) -> Box<Scene> {
    let theme = &settings.config.theme;
    let area = sidebar.panes(shared.size.rect());
    canvases.agents(sidebar.statuses());
    let drawn = canvases.show(&shared.panes, area, theme);
    let mut problems: Vec<&Problem> = Vec::new();
    for problem in shared.trouble.iter().chain(canvases.problems()).chain(&settings.problem) {
        if !problems.contains(&problem) {
            problems.push(problem);
        }
    }
    let (size, uis, notes) = (shared.size, &shared.uis, &shared.notes);
    let mut scene = scene::build(size, &shared.panes, theme, &problems, sidebar, drawn, uis, notes);
    canvases.show_live(&mut scene);
    canvases.show_sling(&mut scene, theme, &settings.config.keys, sidebar);
    scene.labels.splice(0..0, canvases.labels(&shared.panes, area, theme));
    let pen = (tools.tool(), tools.colour());
    scene.footers = canvases.footers(&shared.panes, area, theme, pen, &settings.config.keys);
    scene.labels.extend(tools.labels(theme));
    scene.overlay = shared.overlay.shown(&settings.config.keys, theme, size).map(Box::new);
    scene.cursor = cursor::over_canvas(tools, theme);
    let now = Instant::now();
    tools.flash().follow(tools.tool(), &settings.config.keys, now);
    scene.flash = tools.flash().pill(focused(&shared.panes, area), theme, now).map(Box::new);
    Box::new(scene)
}
