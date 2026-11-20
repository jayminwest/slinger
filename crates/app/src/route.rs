//! The window's routing (`main.rs`'s `window` answers through it, and the
//! tests hand their events in through it): an event goes to the open
//! palette or overlay, a command, a tap, the pen, or on to the focused view.
//! Space held, and the press, drags and lift of a pan with it, are the
//! hand's ([`hand`]) before anything else; then the right and middle
//! buttons' events ([`button`]): a pan goes on as the scroll it makes.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::canvases::Canvases;
use crate::commands::menu::Native;
use crate::commands::{Chord, Command};
use crate::layout::{Point, Rect};
use crate::panes::Panes;
use crate::scene::paste::Keyboard;
use crate::scene::sidebar::Sidebar;
use crate::scene::taps::Tap;
use crate::scene::tools::{Picked, Resize, Tools};
use crate::shared::Shared;
use crate::window::button::Pressed;
use crate::window::{grabs, pane_at, run, tap, view};
use crate::{Ask, Event, Settings, Step, asked, window};

/// What the window does with an event once [`route`] has taken it.
pub enum Routed {
    /// Answer the window this.
    Answer(Step),
    /// A tap in the sidebar or `open-workspace` showed the workspace
    /// named, or a close of the shown one (`close-workspace`, its row's ✕)
    /// the first (`None`): watch it, then answer.
    Watch(Option<String>, Step),
    /// Look at the news (a wake or a tick); `true` draws whether or not there is any.
    News(bool),
    /// `start-slingerd` or `restart-daemon`: for the launcher, then draw.
    Launch(Command),
}

/// The workspace a tap, an open or a close leaves shown, for the watch to
/// follow: one named, the first (`None`), or none to follow. A close names
/// its workspace by folder: a row's ✕ its row's, `close-workspace` the
/// shown one's.
#[expect(clippy::option_option, reason = "what Routed::Watch takes, or no watch")]
fn shows(
    asked: Ask,
    socket: &Path,
    sidebar: &mut Sidebar,
    tools: &mut Tools,
    (shared, canvases): (&mut Shared, &mut Canvases),
) -> Option<Option<String>> {
    let dir = match asked {
        Ask::Open(dir) => return window::open_workspace(&dir, socket, sidebar, tools).map(Some),
        Ask::Tap((_, Tap::Close(i))) => sidebar.dir(i),
        Ask::Tap(target) => return tap(target, canvases, shared, sidebar).map(Some),
        _ => sidebar.shown_dir(),
    };
    let closed =
        |d: &PathBuf| window::close_workspace(d, socket, sidebar, tools, (shared, canvases));
    dir.filter(closed).map(|_| None)
}

/// Routes `event` as the window's answer does, all but the news a tick
/// brings: the tests hand events in through it too.
pub fn route(
    event: Event,
    shared: &mut Shared,
    canvases: &mut Canvases,
    tools: &mut Tools,
    settings: &Settings,
    socket: &Path,
    sidebar: &mut Sidebar,
) -> Routed {
    let area = sidebar.panes(shared.size.rect());
    let event = heard(event, tools);
    if let Event::Press(pen) = event
        && !pen.touch
    {
        tools.set_device(pen.tablet);
    }
    match hand(&event, shared, canvases, tools, area) {
        Some(true) => {
            return Routed::Answer(Step::Draw(view(settings, shared, tools, sidebar, canvases)));
        }
        Some(false) => return Routed::Answer(Step::Stay),
        None => {}
    }
    let event = match button(event, shared, tools, area, canvases) {
        Ok(event) => event,
        Err(Some(true)) => {
            return Routed::Answer(Step::Draw(view(settings, shared, tools, sidebar, canvases)));
        }
        Err(Some(false)) => return Routed::Answer(Step::Stay),
        Err(None) => return Routed::Answer(Step::Pass),
    };
    // A press focuses the pane under it first, so the press works there.
    // The app takes it, and the window the keyboard, unless it goes on;
    // then the view under it has the keyboard, but for an open entry's.
    let pressed = matches!(event, Event::Press(_));
    let on = if canvases.typing() { Keyboard::Window } else { Keyboard::Pressed };
    let refocused = match event {
        Event::Press(pen) => {
            shared.keyboard = Keyboard::Window;
            window::press(shared, sidebar, canvases, tools, pen)
        }
        _ => false,
    };
    let asked = asked(&event, shared, canvases, tools, settings, sidebar);
    let step = match (asked, event) {
        (Ask::Draw(clip), _) => {
            let scene = view(settings, shared, tools, sidebar, canvases);
            match clip {
                Some(clip) => Step::Copy(clip, scene),
                None => Step::Draw(scene),
            }
        }
        (Ask::Step(step), _) => step,
        (Ask::Not, Event::Tick | Event::Wake) => return Routed::News(false),
        (Ask::Not, Event::Quitting) => {
            canvases.quitting(&shared.panes);
            Step::Stay
        }
        (Ask::Not, Event::Reply(line)) => match shared.reply(&line, socket) {
            Ok(false) => Step::Stay,
            Ok(true) => Step::Draw(view(settings, shared, tools, sidebar, canvases)),
            Err(why) => Step::Refuse(why),
        },
        (asked @ (Ask::Tap(_) | Ask::Open(_) | Ask::Run(Command::CloseWorkspace)), _) => {
            let shown = shows(asked, socket, sidebar, tools, (shared, canvases));
            let step = Step::Draw(view(settings, shared, tools, sidebar, canvases));
            return match shown {
                Some(name) => Routed::Watch(name, step),
                None => Routed::Answer(step),
            };
        }
        (Ask::Run(Command::Quit), _) => Step::Quit,
        (Ask::Run(c @ (Command::StartSlingerd | Command::RestartDaemon)), _) => {
            return Routed::Launch(c);
        }
        // It flashes, and the window is drawn whether or not the files changed.
        (Ask::Run(Command::ReloadConfig), _) => {
            tools.flash().command(Command::ReloadConfig, &settings.config.keys, Instant::now());
            return Routed::News(true);
        }
        (Ask::Run(command), _) => {
            if !run(command, tools, canvases, sidebar, shared, socket) {
                return Routed::Answer(Step::Pass);
            }
            canvases.unsaved_while(command, sidebar.daemon());
            Step::Draw(view(settings, shared, tools, sidebar, canvases))
        }
        // The pen, the mouse, the trackpad, typed text and a resize.
        // A crop's lift puts its snippet on the clipboard.
        (Ask::Not, event) if pen(&event, tools, canvases, shared, area, socket) => {
            let scene = view(settings, shared, tools, sidebar, canvases);
            match canvases.cropped() {
                Some(clip) => Step::Copy(clip, scene),
                None => Step::Draw(scene),
            }
        }
        // Paste's key with the window holding the keyboard comes only when
        // its greyed Edit menu item let it by: say why, not just beep.
        (Ask::Pass, Event::Key(chord)) if unpasted(chord, shared, canvases, tools) => {
            Step::Draw(view(settings, shared, tools, sidebar, canvases))
        }
        (Ask::Pass | Ask::Not, _) if refocused => {
            shared.keyboard = on;
            Step::Through(view(settings, shared, tools, sidebar, canvases))
        }
        (Ask::Pass | Ask::Not, _) => Step::Pass,
    };
    if pressed && matches!(step, Step::Pass) {
        shared.keyboard = on;
    }
    Routed::Answer(step)
}

/// `event` with its pen as `tools` hears it ([`Tools::hears`]): a
/// trackpad's touch is a mouse's while no tablet pen is near.
fn heard(event: Event, tools: &Tools) -> Event {
    match event {
        Event::Press(pen) => Event::Press(tools.hears(pen)),
        Event::Drag(pen) => Event::Drag(tools.hears(pen)),
        Event::Lift(pen) => Event::Lift(tools.hears(pen)),
        event => event,
    }
}

/// Hands `event` to the hand of `shared` (docs/design/pen-less.md): space
/// down while nothing in `canvases`, `shared` or `tools` takes typed text
/// holds it; a press with it held starts a pan, its drags pan the canvas
/// pane of `shared` laid in `area` under the press, as a two-finger scroll
/// there would, and its lift ends it. `Some(draw)` when the hand took it;
/// `None` when it goes on.
fn hand(
    event: &Event,
    shared: &mut Shared,
    canvases: &mut Canvases,
    tools: &Tools,
    area: Rect,
) -> Option<bool> {
    let hand = &mut shared.hand;
    match *event {
        Event::Space(down) => {
            let typing = canvases.typing() || canvases.editing();
            let free = !typing && !shared.modal() && tools.open_kind().is_none();
            let was = shared.hand.cursor();
            let took = shared.hand.space(down, free);
            let changed = shared.hand.cursor() != was;
            (took || changed).then_some(changed)
        }
        Event::Press(pen) if hand.press(pen.at) => Some(true),
        Event::Drag(pen) => hand.drag(pen.at).map(|(at, dx, dy)| {
            window::gesture(canvases, &shared.panes, area, &Event::Scroll { at, dx, dy })
        }),
        Event::Lift(_) if hand.lift() => Some(true),
        _ => None,
    }
}

/// A right or middle button's `event` ([`window::button::pressed`]):
/// `Ok` with the event to route on (any other event, or a pan's scroll),
/// else whether to draw, or `None` when it goes on to the view under it.
fn button(
    event: Event,
    shared: &mut Shared,
    tools: &mut Tools,
    area: Rect,
    canvases: &Canvases,
) -> Result<Event, Option<bool>> {
    let Event::Button { button, phase, at } = event else { return Ok(event) };
    match window::button::pressed((button, phase, at), shared, canvases, area) {
        Pressed::On(event) => Ok(event),
        Pressed::Took => Err(Some(false)),
        Pressed::Pass => Err(None),
        Pressed::Click(at) => Err(Some(clicked(at, shared, tools, area))),
    }
}

/// A context click at `at`, a point in the window of `shared` with its
/// panes laid in `area`: the pointer is there for `tools` (a new text
/// lands at it) and the pane under it takes focus, as a press there does.
/// Under the key overlay, the palette, the page menu or an open pop-up it
/// does nothing (false).
fn clicked(at: Point, shared: &mut Shared, tools: &mut Tools, area: Rect) -> bool {
    if shared.modal() || tools.open_kind().is_some() {
        return false;
    }
    tools.moved(at);
    shared.press_focus(area, at, false, false);
    true
}

/// Flashes why a paste went nowhere when `chord` is paste's key and the
/// window of `shared` has the keyboard (slinger-acc9ee). False otherwise.
fn unpasted(chord: Chord, shared: &Shared, canvases: &Canvases, tools: &Tools) -> bool {
    let window = window::keyboard(shared, canvases) == Keyboard::Window;
    let paste = window && Some(chord) == Native::Paste.key();
    if paste {
        window::unpasted(None, shared, canvases, tools);
    }
    paste
}

/// Hands `chord` to what takes keys before the keymap: the open key overlay
/// in `shared` (every key), the page menu (escape), the sling's entry, the
/// text being typed (its caret keys), a peek (escape), then a pop-up ([`picked`]). False
/// when none took it.
pub fn taken(
    chord: Chord,
    shared: &mut Shared,
    canvases: &mut Canvases,
    tools: &mut Tools,
    settings: &Settings,
) -> bool {
    shared.overlay.key(chord, &settings.config.keys)
        || shared.menu.key(chord)
        || canvases.sling_key(chord)
        || canvases.edit_key(chord)
        || canvases.peek_key(chord)
        || tools.key(chord) && picked(tools, canvases)
}

/// A colour picked from the pop-up recolours the selection; a size
/// resizes the text being typed or the selected texts. Always true.
pub fn picked(tools: &mut Tools, canvases: &mut Canvases) -> bool {
    match tools.picked() {
        Some(Picked::Colour(colour)) => _ = canvases.recolour(colour),
        Some(Picked::Size(size)) => _ = canvases.resize(Resize::To(size)),
        None => {}
    }
    true
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
            shared.hover(at) | window::divider(shared, canvases, tools, area, at)
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
        Event::Proximity { near, eraser } => tools.set_eraser_end(eraser) | tools.set_device(near),
        Event::Press(pen) if tools.press(pen.at) => picked(tools, canvases),
        Event::Press(pen) => {
            tools.set_eraser_end(pen.eraser);
            let tool = (tools.tool(), tools.colour());
            canvases.set_pen(tools);
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
        _ => window::gesture(canvases, panes, area, event),
    }
}
