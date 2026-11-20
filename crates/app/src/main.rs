//! The `slinger` macOS app: a thin front end over `slingerd`. It does not
//! link the core.
//!
//! `backend` gets the data (and keeps a bundled app's slingerd running, as
//! `launch` decides), `config` reads the config file, `commands` turns
//! keys into commands, `panes` keeps the window's panes and their focus,
//! `shared` keeps them as the server does, `canvases` keeps what each canvas
//! pane shows (a `mirror` of a page, a viewport and the pen's ink), `scene`
//! decides what the window shows (the agents' statuses in `scene::sidebar`),
//! `ui` holds an agent UI's page and its bridge, `layout` does the geometry,
//! `window` runs a command and builds the window's scene, `route` routes
//! each event the window hears, and `platform` draws.
//! Only `platform` touches `AppKit`, and only this file starts it (gate
//! check:layers).
//!
//! A UI pane's reply goes to the daemon, which hands it to the agent that
//! opened the showing (`backend::ui::reply`); the pane notes one its agent
//! did not take. `slinger-app --preview UI [--data JSON]` shows one agent UI
//! (or a showing) alone in the window, `--as item` as a live item on a
//! canvas, and writes each of its replies to stdout, one `ui_reply` line each;
//! with `--background` last its window is never shown (the tests pass it).

mod backend;
mod canvases;
mod commands;
mod config;
mod drive;
mod launch;
mod layout;
mod mirror;
mod panes;
mod platform;
mod route;
mod scene;
mod shared;
mod trace;
mod ui;
mod window;

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use slinger_control::{self as control, PaneKind::Canvas};

use crate::backend::{Launcher, Settings, Wake, Watch};
use crate::canvases::Canvases;
use crate::commands::Command;
use crate::config::Shown;
use crate::platform::{Event, Pick, Step};
use crate::route::{Routed, route, taken};
use crate::scene::paste::Clip;
use crate::scene::sidebar::Sidebar;
use crate::scene::taps::{Tap, Target, Took};
use crate::scene::tools::{Kind, Tools};
use crate::shared::Shared;
use crate::trace::Trace;
use crate::window::{Edited, view};

fn main() -> ExitCode {
    let settings = backend::Settings::load(backend::ConfigFiles::locate());
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        [] => window(settings, None),
        // A driven window never falls back to the user's own daemon.
        ["--drive", _] if settings.config.socket.is_none() || settings.problem.is_some() => {
            fail("a driven window needs a config that names its daemon's socket", Some(USAGE))
        }
        ["--drive", socket] => window(settings, Some(Path::new(socket))),
        ["--preview", reference, ref more @ ..] => preview(&settings, reference, more),
        _ => fail(USAGE, None),
    }
}

/// What to type, said when the arguments are not these.
const USAGE: &str =
    "usage: slinger-app [--preview UI [--data JSON] [--as item] [--background] | --drive SOCKET]";

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
/// With `--background` last the window is never shown.
fn preview(settings: &Settings, reference: &str, more: &[&str]) -> ExitCode {
    let (more, kept) = match more {
        [more @ .., "--background"] => (more, platform::Kept::Background),
        _ => (more, platform::Kept::Preview),
    };
    let (data, item) = match more {
        [] | ["--as", "item"] => (None, !more.is_empty()),
        ["--data", data] | ["--data", data, "--as", "item"] => (Some(*data), more.len() > 2),
        _ => return fail(USAGE, None),
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
        Event::Tick
        | Event::Wake
        | Event::Moved(_)
        | Event::Painted(_)
        | Event::Typed(_)
        | Event::Edit(_) => Step::Stay,
        _ => Step::Pass,
    };
    match platform::run(&scene, kept, answer) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e.to_string(), None),
    }
}

/// The window onto the daemon: the panes of its workspace, the sidebar and
/// the pen.
fn window(mut settings: Settings, drive: Option<&Path>) -> ExitCode {
    let mut socket = settings.socket();
    // A driven window keeps nothing and takes nothing the app kept.
    let own = drive.is_none();
    let last = platform::remembered_workspace().filter(|_| own);
    let wake = Wake::new(platform::waker());
    let wanted = settings.config.workspace.clone();
    let watch = Watch::start(socket.clone(), wanted, last.clone(), &wake);
    let launcher = backend::daemon::managed(&settings, !own).map(|m| Launcher::start(m, &wake));
    let mut shown = settings.config.sidebar;
    let open = Shown::starts_open(shown, platform::remembered_sidebar().filter(|_| own));
    let mut sidebar = Sidebar::new(open).preferring(last);
    sidebar.show(settings.config.workspace.clone());
    let mut tools = Tools::configured(&settings.config);
    let mut shared = Shared::default();
    let mut canvases = canvases(&socket, own, wake.clone());
    let mut trace = Trace::new(std::env::var("SLINGER_TRACE").ok().as_deref());
    let first = view(&settings, &mut shared, &tools, &sidebar, &mut canvases);
    // The workspace the config names, to tell when an edit changes it; a
    // tap on another in the sidebar shows that one until it does.
    let mut named = settings.config.workspace.clone();
    // Runs on the main thread for each key press, menu item, pen, mouse or
    // trackpad event, paint of new ink, cut, copy or paste, wake and tick
    // of the window's timer. The open command palette, then the open key
    // overlay, takes every key and press (the palette's run and a menu
    // item's go as a key's); then the sling's entry, then a pop-up, takes
    // its keys first; a press on a divider between panes drags it; the pen
    // draws with the pen tool on a canvas pane. A wake (a backend thread's
    // news) or a tick stats the two config files and takes the watch's
    // news of the agents and the panes and what the canvases' followers
    // heard; the watch and the followers follow an edit that moves the
    // socket or the workspace. Only a split, a close or a resize asks the
    // daemon here. Any pen or mouse press first focuses the pane under it;
    // one an agent UI's page takes still draws the new focus.
    let mut step = move |event, sidebar: &mut Sidebar| {
        let routed =
            route(event, &mut shared, &mut canvases, &mut tools, &settings, &socket, sidebar);
        let force = match routed {
            Routed::Answer(step) => return step,
            Routed::Watch(name, step) => {
                trace.pick(name.clone(), Instant::now());
                watch.want(name);
                return step;
            }
            Routed::News(force) => force,
            Routed::Launch(command) => {
                window::launched::run(command, launcher.as_ref(), sidebar, &mut tools);
                return Step::Draw(view(&settings, &mut shared, &tools, sidebar, &mut canvases));
            }
        };
        // Before the channels are drained: news sent from here on wakes again.
        wake.taken();
        let (daemon_heard, _) = window::launched::heard(launcher.as_ref(), sidebar, &mut tools);
        let heard = watch.latest().is_some_and(|now| sidebar.update(now)) | daemon_heard;
        let moved = watch
            .layout()
            .map(|(news, stamps)| {
                say(trace.taken(stamps, canvas_panes(news.as_ref().ok()), Instant::now()));
                shared.follow(news);
            })
            .is_some();
        let fetched = shared.fetched(watch.uis());
        let drawn = canvases.poll();
        let reloaded = settings.refresh(force);
        let faded = tools.flash().fade(Instant::now()) | sidebar.fade(Instant::now());
        if !force && !reloaded && !heard && !moved && !drawn && !fetched && !faded {
            return Step::Stay;
        }
        if reloaded {
            tools.set_config(&settings.config);
            if settings.config.sidebar != shown {
                shown = settings.config.sidebar;
                sidebar.set_open(Shown::starts_open(shown, Some(sidebar.is_open())));
            }
            if settings.config.workspace != named {
                named.clone_from(&settings.config.workspace);
                trace.pick(named.clone(), Instant::now());
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
        let scene = view(&settings, &mut shared, &tools, sidebar, &mut canvases);
        let area = sidebar.panes(shared.size.rect());
        say(trace.drawn(canvases.paged(&shared.panes, area), Instant::now()));
        Step::Draw(scene)
    };
    let ran = match drive {
        None => platform::run(&first, platform::Kept::Window, remembering(step, sidebar)),
        Some(at) => {
            if let Err(e) = drive::listen(at, platform::drive::ask) {
                return fail(&format!("{}: {e}", at.display()), None);
            }
            let answer = platform::drive::watching(&first, move |event| step(event, &mut sidebar));
            platform::run(&first, platform::Kept::Driven, answer)
        }
    };
    match ran {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e.to_string(), None),
    }
}

/// Writes the switch's trace `line`, if any, to stderr.
fn say(line: Option<String>) {
    if let Some(line) = line {
        // Nothing can be done if stderr is closed.
        let _ = writeln!(io::stderr(), "{line}");
    }
}

/// The workspace `layout` shows and whether it has a canvas pane.
fn canvas_panes(layout: Option<&control::PaneLayout>) -> Option<(&str, bool)> {
    let canvas = |l: &control::PaneLayout| l.layout.panes().iter().any(|p| p.kind == Canvas);
    layout.map(|l| (l.workspace.as_str(), canvas(l)))
}

/// The canvases of the window, through the daemon on `socket`, keeping
/// the strokes not yet written on disk ([`backend::Unsent`]), their threads'
/// news woken by `wake`.
fn canvases(socket: &Path, keep: bool, wake: Wake) -> Canvases {
    let daemon = control::Client::new(socket);
    let canvases = Canvases::waking(daemon, wake).rendering(platform::pdf::backdrop);
    backend::Unsent::locate().filter(|_| keep).into_iter().fold(canvases, Canvases::keeping)
}

/// `step` with `sidebar`, which it may open or collapse or show another
/// workspace in: the next launch starts with the sidebar as it is left,
/// showing the workspace it shows.
fn remembering(
    mut step: impl FnMut(Event, &mut Sidebar) -> Step,
    mut sidebar: Sidebar,
) -> impl FnMut(Event) -> Step {
    let mut kept = platform::remembered_workspace();
    move |event| {
        let was = sidebar.is_open();
        let step = step(event, &mut sidebar);
        if sidebar.is_open() != was {
            platform::remember_sidebar(sidebar.is_open());
        }
        if let Some(name) = sidebar.shown_name().filter(|n| kept.as_deref() != Some(*n)) {
            platform::remember_workspace(name);
            kept = Some(name.to_string());
        }
        step
    }
}

/// What `event` asks ([`ask`]); `show-canvas-file`, `show-workspace-folder`,
/// `show-feedback-file`, `export-ui`, `open-pdf` and `open-workspace` are
/// done here, as only `platform` reaches the Finder and the open panel: the
/// focused pane's canvas file, the folder of the workspace `sidebar` shows,
/// the daemon's feedback file ([`window::finder`])
/// or the agent UI saved in a folder picked ([`window::export_ui`]) shows
/// there, or the flash says why not; the PDF chosen, or dropped on the
/// window, opens as a canvas, its pages read by `platform`. A cut, copy or
/// paste no text field took is [`window::edit`]'s in `area` (a pass greys its menu item).
fn asked(
    event: &Event,
    shared: &mut Shared,
    canvases: &mut Canvases,
    tools: &mut Tools,
    settings: &Settings,
    sidebar: &Sidebar,
) -> Ask {
    let area = sidebar.panes(shared.size.rect());
    let asked = match event {
        Event::Can(edit) if window::can_edit(edit, shared, canvases) => {
            return Ask::Step(Step::Stay);
        }
        Event::Can(_) => return Ask::Step(Step::Pass),
        Event::Dropped(at, files) => {
            let pdf = platform::pdf::pages;
            let opened = window::open_pdf(files, Some(*at), shared, canvases, area, pdf);
            return if opened { Ask::Draw(None) } else { Ask::Step(Step::Pass) };
        }
        Event::Edit(edit) => {
            return match window::edit(edit, shared, canvases, tools, area) {
                Edited::No => Ask::Step(Step::Stay),
                Edited::Drawn(clip) => Ask::Draw(clip),
            };
        }
        _ => ask(event, shared, canvases, tools, settings),
    };
    let found = match asked {
        Ask::Run(
            c
            @ (Command::ShowCanvasFile | Command::ShowWorkspaceFolder | Command::ShowFeedbackFile),
        ) => window::finder(c, shared, canvases, sidebar),
        Ask::Run(Command::OpenWorkspace) => {
            return platform::choose(Pick::Workspace, tools.flash())
                .map_or(Ask::Draw(None), Ask::Open);
        }
        Ask::Run(Command::ExportUi) => {
            let Some(dir) = platform::choose(Pick::ExportTo, tools.flash()) else {
                return Ask::Draw(None);
            };
            window::export_ui(shared, &dir)
        }
        Ask::Run(Command::OpenPdf) => {
            let file: Vec<_> =
                platform::choose(Pick::Document, tools.flash()).into_iter().collect();
            window::open_pdf(&file, None, shared, canvases, area, platform::pdf::pages);
            return Ask::Draw(None);
        }
        _ => return asked,
    };
    match found {
        Ok(file) if platform::reveal(&file) => {}
        Ok(file) => tools.flash().say(format!("The Finder cannot show {file}"), Instant::now()),
        Err(said) => tools.flash().say(said, Instant::now()),
    }
    Ask::Draw(None)
}

/// What a key or a press asks of the window.
enum Ask {
    /// Run a command, as its key does.
    Run(Command),
    /// A tap on a button that does more than run a command.
    Tap(Target),
    /// Open this folder as a workspace and show it.
    Open(PathBuf),
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
/// and closes what is open over `shared` first (undo and redo pass on to
/// an open entry's field); the palette takes
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
        // Undo or redo chosen with an entry open is its field's, as the key is.
        (Took::No, &Event::Command(Command::Undo | Command::Redo)) if canvases.typing() => {
            Ask::Pass
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

#[cfg(test)]
mod tests;
