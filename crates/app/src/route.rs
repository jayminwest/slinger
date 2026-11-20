//! The window's routing (`main.rs`'s `window` answers through it, and the
//! tests hand their events in through it): an event goes to the open
//! palette or overlay, a command, a tap, the pen, or on to the focused view.
//! Space held, and the press, drags and lift of a pan with it, are the
//! hand's ([`hand`]) before anything else.

use std::path::Path;
use std::time::Instant;

use crate::canvases::Canvases;
use crate::commands::menu::Native;
use crate::commands::{Chord, Command};
use crate::layout::Rect;
use crate::scene::paste::Keyboard;
use crate::scene::sidebar::Sidebar;
use crate::scene::tools::{Picked, Resize, Tools};
use crate::shared::Shared;
use crate::window::{run, tap, view};
use crate::{Ask, Event, Settings, Step, asked, pen, window};

/// What the window does with an event once [`route`] has taken it.
pub enum Routed {
    /// Answer the window this.
    Answer(Step),
    /// A tap in the sidebar or `open-workspace` showed the workspace
    /// named, or `close-workspace` the first (`None`): watch it, then answer.
    Watch(Option<String>, Step),
    /// Look at the news (a tick); `true` draws whether or not there is any.
    News(bool),
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
        (Ask::Not, Event::Tick) => return Routed::News(false),
        (Ask::Not, Event::Reply(line)) => match shared.reply(&line, socket) {
            Ok(false) => Step::Stay,
            Ok(true) => Step::Draw(view(settings, shared, tools, sidebar, canvases)),
            Err(why) => Step::Refuse(why),
        },
        (asked @ (Ask::Tap(_) | Ask::Open(_) | Ask::Run(Command::CloseWorkspace)), _) => {
            let shown = match asked {
                Ask::Open(dir) => window::open_workspace(&dir, socket, sidebar, tools).map(Some),
                Ask::Tap(target) => tap(target, canvases, shared, sidebar).map(Some),
                _ => window::close_workspace(socket, sidebar, tools).then_some(None),
            };
            let step = Step::Draw(view(settings, shared, tools, sidebar, canvases));
            return match shown {
                Some(name) => Routed::Watch(name, step),
                None => Routed::Answer(step),
            };
        }
        (Ask::Run(Command::Quit), _) => Step::Quit,
        // It flashes, and the window is drawn whether or not the files changed.
        (Ask::Run(Command::ReloadConfig), _) => {
            tools.flash().command(Command::ReloadConfig, &settings.config.keys, Instant::now());
            return Routed::News(true);
        }
        (Ask::Run(command), _) => {
            if !run(command, tools, canvases, sidebar, shared, socket) {
                return Routed::Answer(Step::Pass);
            }
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
/// text being typed (its caret keys), then a pop-up ([`picked`]). False
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
