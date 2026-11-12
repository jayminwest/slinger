//! The `slinger` macOS app: a thin front end over `slingerd`. It does not
//! link the core.
//!
//! `backend` gets the data, `config` reads the config file, `commands` turns
//! keys into commands, `panes` keeps the window's panes and their focus,
//! `shared` keeps them as the server does,
//! `canvases` keeps what each canvas pane shows (a `mirror` of a page, a
//! viewport and the pen's ink), `scene` decides what the window shows (the
//! agents' statuses in `scene::sidebar`), `ui` holds an agent UI's page and
//! its bridge, `layout` does the geometry, `window` runs a command and builds the
//! window's scene, and `platform` draws.
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
mod window;

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use slinger_control as control;

use crate::backend::{Settings, Watch};
use crate::canvases::Canvases;
use crate::commands::{Chord, Command};
use crate::config::Shown;
use crate::layout::{Point, Rect};
use crate::panes::Panes;
use crate::platform::{Event, Step};
use crate::scene::paste::Clip;
use crate::scene::sidebar::Sidebar;
use crate::scene::taps::{Tap, Target, Took};
use crate::scene::tools::{Kind, Tools};
use crate::shared::Shared;
use crate::window::{Edited, grabs, pane_at, run, tap, view};

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
        Event::Tick | Event::Moved(_) | Event::Painted(_) | Event::Typed(_) | Event::Edit(_) => {
            Step::Stay
        }
        _ => Step::Pass,
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
    let mut sidebar = Sidebar::new(Shown::starts_open(shown, platform::remembered_sidebar()));
    sidebar.show(settings.config.workspace.clone());
    let widths = settings.config.widths.clone();
    let mut tools = Tools::new(settings.config.palette.clone()).with_widths(widths);
    let mut shared = Shared::default();
    let mut canvases = canvases(&socket);
    let first = view(&settings, &mut shared, &tools, &sidebar, &mut canvases);
    // The workspace the config names, to tell when an edit changes it; a
    // tap on another in the sidebar shows that one until it does.
    let mut named = settings.config.workspace.clone();
    // Runs on the main thread for each key press, menu item, pen, mouse or
    // trackpad event, paint of new ink, cut, copy or paste, and tick of the window's timer. The
    // open command palette, then the open key overlay, takes every key and
    // press (the palette's run and a menu item's go as a key's); then the
    // sling's entry, then a pop-up, takes its keys first; a press on a
    // divider between panes drags it; the pen draws with the pen tool on a
    // canvas pane. A tick stats the two config files and takes the watch's news of the
    // agents and the panes and what the canvases' followers heard; the watch
    // and the followers follow an edit that moves the socket or the
    // workspace. Only a split, a close or a resize asks the daemon here.
    // Any pen or mouse press first focuses the pane under it; one an agent
    // UI's page takes still draws the new focus.
    let step = move |event, sidebar: &mut Sidebar| {
        let area = sidebar.panes(shared.size.rect());
        // A press focuses the pane under it first, so the press works there.
        let refocused = match event {
            Event::Press(pen) => window::press(&mut shared, sidebar, &canvases, &tools, pen),
            _ => false,
        };
        let asked = asked(&event, &mut shared, &mut canvases, &mut tools, &settings, area);
        let force = match (asked, event) {
            (Ask::Draw(clip), _) => {
                let scene = view(&settings, &mut shared, &tools, sidebar, &mut canvases);
                return match clip {
                    Some(clip) => Step::Copy(clip, scene),
                    None => Step::Draw(scene),
                };
            }
            (Ask::Step(step), _) => return step,
            (Ask::Not, Event::Tick) => false,
            (Ask::Not, Event::Reply(line)) => {
                return match shared.reply(&line, &socket) {
                    Ok(false) => Step::Stay,
                    Ok(true) => {
                        Step::Draw(view(&settings, &mut shared, &tools, sidebar, &mut canvases))
                    }
                    Err(why) => Step::Refuse(why),
                };
            }
            (Ask::Tap(target), _) => {
                if let Some(name) = tap(target, &mut canvases, &mut shared, sidebar) {
                    watch.want(Some(name));
                }
                return Step::Draw(view(&settings, &mut shared, &tools, sidebar, &mut canvases));
            }
            (Ask::Run(Command::Quit), _) => return Step::Quit,
            // It flashes, and the window is drawn whether or not the files changed.
            (Ask::Run(Command::ReloadConfig), _) => {
                tools.flash().command(Command::ReloadConfig, &settings.config.keys, Instant::now());
                true
            }
            (Ask::Run(command), _) => {
                if !run(command, &mut tools, &mut canvases, sidebar, &mut shared, &socket) {
                    return Step::Pass;
                }
                return Step::Draw(view(&settings, &mut shared, &tools, sidebar, &mut canvases));
            }
            // The pen, the mouse, the trackpad, typed text and a resize.
            (Ask::Not, event)
                if pen(&event, &mut tools, &mut canvases, &mut shared, area, &socket) =>
            {
                return Step::Draw(view(&settings, &mut shared, &tools, sidebar, &mut canvases));
            }
            (Ask::Pass | Ask::Not, _) if refocused => {
                return Step::Through(view(&settings, &mut shared, &tools, sidebar, &mut canvases));
            }
            (Ask::Pass | Ask::Not, _) => return Step::Pass,
        };
        let heard = watch.latest().is_some_and(|now| sidebar.update(now));
        let moved = watch.layout().map(|news| shared.follow(news)).is_some();
        let fetched = shared.fetched(watch.uis());
        let drawn = canvases.poll();
        let reloaded = settings.refresh(force);
        let faded = tools.flash().fade(Instant::now()) | sidebar.fade(Instant::now());
        if !force && !reloaded && !heard && !moved && !drawn && !fetched && !faded {
            return Step::Stay;
        }
        if reloaded {
            tools.set_palette(settings.config.palette.clone());
            tools.set_widths(settings.config.widths.clone());
            if settings.config.sidebar != shown {
                shown = settings.config.sidebar;
                sidebar.set_open(Shown::starts_open(shown, Some(sidebar.is_open())));
            }
            if settings.config.workspace != named {
                named.clone_from(&settings.config.workspace);
                watch.want(named.clone());
                sidebar.show(named.clone());
            }
            let now = settings.socket();
            if now != socket {
                watch.follow(now.clone());
                canvases.reconnect(control::Client::new(&now));
                socket = now;
            }
        }
        Step::Draw(view(&settings, &mut shared, &tools, sidebar, &mut canvases))
    };
    match platform::run(&first, remembering(step, sidebar)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e.to_string(), None),
    }
}

/// The canvases of the window, through the daemon on `socket`, keeping
/// the strokes not yet written on disk ([`backend::Unsent`]).
fn canvases(socket: &Path) -> Canvases {
    let canvases = Canvases::new(control::Client::new(socket));
    backend::Unsent::locate().into_iter().fold(canvases, Canvases::keeping)
}

/// `step` with `sidebar`, which it may open or collapse: the sidebar starts
/// the next launch as it is left.
fn remembering(
    mut step: impl FnMut(Event, &mut Sidebar) -> Step,
    mut sidebar: Sidebar,
) -> impl FnMut(Event) -> Step {
    move |event| {
        let was = sidebar.is_open();
        let step = step(event, &mut sidebar);
        if sidebar.is_open() != was {
            platform::remember_sidebar(sidebar.is_open());
        }
        step
    }
}

/// What `event` asks ([`ask`]); `show-canvas-file` is done here, as only
/// `platform` reaches the Finder: the focused pane's canvas file shows
/// there, or the flash says why it cannot. A cut, copy or paste no text
/// field took is [`window::edit`]'s in `area` (a pass greys its menu item).
fn asked(
    event: &Event,
    shared: &mut Shared,
    canvases: &mut Canvases,
    tools: &mut Tools,
    settings: &Settings,
    area: Rect,
) -> Ask {
    let asked = match event {
        Event::Can(edit) if window::can_edit(edit, shared, canvases) => {
            return Ask::Step(Step::Stay);
        }
        Event::Can(_) => return Ask::Step(Step::Pass),
        Event::Edit(edit) => {
            return match window::edit(edit, shared, canvases, area) {
                Edited::No => Ask::Step(Step::Stay),
                Edited::Drawn(clip) => Ask::Draw(clip),
            };
        }
        _ => ask(event, shared, canvases, tools, settings),
    };
    if !matches!(asked, Ask::Run(Command::ShowCanvasFile)) {
        return asked;
    }
    let said = match canvases.file(&shared.panes, shared.panes.focus()) {
        None => Some(scene::title::NO_CANVAS.to_string()),
        Some((canvas, None)) => Some(format!("slingerd has not said where {canvas} is saved")),
        Some((_, Some(file))) if platform::reveal(file) => None,
        Some((_, Some(file))) => Some(format!("The Finder cannot show {file}")),
    };
    if let Some(said) = said {
        tools.flash().say(said, Instant::now());
    }
    Ask::Draw(None)
}

/// What a key or a press asks of the window.
enum Ask {
    /// Run a command, as its key does.
    Run(Command),
    /// A tap on a button that does more than run a command.
    Tap(Target),
    /// Draw again: something took it; and put a copy's clip on the clipboard.
    Draw(Option<Clip>),
    /// Answer the window this, and do nothing more.
    Step(Step),
    /// Let it go on to the focused view.
    Pass,
    /// It is not a key or a menu item, nor a press the palette took.
    Not,
}

/// What `event` asks: a menu item chosen with the pointer runs its command,
/// and closes what is open over `shared` first; the palette takes
/// every key and press, and runs a command as its key would (a row on the
/// lift, as the key overlay's); then a key goes to what takes keys first
/// ([`taken`]), to the canvases (a text being typed), and to the keymap. A
/// menu item's key comes as that key. A press on a button (or an open
/// overlay's or menu's row) is theirs; the lift on it taps it: a pop-up
/// opens where the pen lifted, another closes, and undo and redo flash.
fn ask(
    event: &Event,
    shared: &mut Shared,
    canvases: &mut Canvases,
    tools: &mut Tools,
    settings: &Settings,
) -> Ask {
    let keys = &settings.config.keys;
    let palette = match *event {
        Event::Key(chord) => shared.palette.key(chord, keys).into(),
        Event::Press(pen) => shared.press(pen.at),
        Event::Lift(pen) => {
            tools.moved(pen.at);
            shared.lift(pen.at)
        }
        Event::Command(command) => shared.chosen(command),
        _ => return Ask::Not,
    };
    match (palette, event) {
        (Took::Tap((_, Tap::Run(command))), &Event::Lift(_)) => {
            if Kind::opened_by(command).is_none() {
                tools.close();
            }
            if matches!(command, Command::Undo | Command::Redo) {
                tools.flash().command(command, keys, Instant::now());
            }
            Ask::Run(command)
        }
        (Took::Tap((_, Tap::Run(command))), _) | (Took::No, &Event::Command(command)) => {
            Ask::Run(command)
        }
        (Took::Tap(target), _) => Ask::Tap(target),
        (Took::Yes, _) => Ask::Draw(None),
        (Took::No, &Event::Key(c)) if taken(c, shared, canvases, tools, settings) => {
            Ask::Draw(None)
        }
        (Took::No, &Event::Key(chord)) if canvases.takes(chord, keys) => Ask::Pass,
        (Took::No, &Event::Key(chord)) => keys.command(chord).map_or(Ask::Pass, Ask::Run),
        (Took::No, _) => Ask::Not,
    }
}

/// Hands `chord` to what takes keys before the keymap: the open key overlay
/// in `shared` (every key), the page menu (escape), the sling's entry, then
/// a pop-up. False when none took it.
fn taken(
    chord: Chord,
    shared: &mut Shared,
    canvases: &mut Canvases,
    tools: &mut Tools,
    settings: &Settings,
) -> bool {
    shared.overlay.key(chord, &settings.config.keys)
        || shared.menu.key(chord)
        || canvases.sling_key(chord)
        || tools.key(chord)
}

/// Hands a press on a divider between panes (with no pop-up open, and not
/// grabbing the selection: [`grabs`]), and the
/// drag and lift after it, to the panes of `shared` laid in `area`, which
/// resize; a moved divider goes to the daemon on `socket`. A resized window
/// lays its panes out again. A move goes to the pop-ups (where one
/// opens) and lights the button, or the open overlay's or menu's row, under the pen. The open
/// key overlay scrolls with a drag or a scroll, and with the palette and the page menu takes
/// every other pen, mouse or trackpad event and typed text; a drag after a press on a button is
/// the button's. Any other event goes on to [`pointer`]. False when nothing used it.
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
        Event::Moved(at) => {
            tools.moved(at);
            shared.hover(at)
        }
        Event::Drag(pen) if shared.drag(pen.at) => true,
        Event::Scroll { dy, .. } if shared.scroll(dy) => true,
        Event::Lift(_) | Event::Zoom { .. } | Event::Text(_) if shared.modal() => true,
        Event::Press(pen)
            if free
                && !grabs(canvases, &shared.panes, tools, area, pen)
                && shared.dividers.down(&shared.panes, area, pen.at) =>
        {
            true
        }
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
/// pinch pans or zooms. The pen's eraser end, near or pressed, erases with
/// any tool. False when nothing used it, and for a paint.
/// A lift writes the stroke's delay log.
fn pointer(
    event: &Event,
    tools: &mut Tools,
    canvases: &mut Canvases,
    panes: &Panes,
    area: Rect,
) -> bool {
    match *event {
        Event::Painted(delay) => {
            canvases.painted(delay);
            false
        }
        Event::Press(pen) if canvases.sling_covers(pen.at) => false,
        Event::Typed(ref text) => canvases.sling_typed(text),
        // A text being typed takes the keys that type, which come back as text.
        Event::Text(ref text) => canvases.typed(text),
        Event::EraserEnd(eraser) => tools.set_eraser_end(eraser),
        Event::Press(pen) if tools.press(pen.at) => true,
        Event::Press(pen) => {
            tools.set_eraser_end(pen.eraser);
            let tool = (tools.tool(), tools.colour());
            canvases.set_pen_width(tools.width().points());
            pane_at(panes, area, pen.at).is_some_and(|u| canvases.pen_down(panes, u, pen, tool))
        }
        Event::Drag(pen) => {
            tools.moved(pen.at);
            canvases.pen_drag(panes, pen, pane_at(panes, area, pen.at))
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

#[cfg(test)]
mod tests;
