//! The window's routing (`main.rs`'s `window` answers through it, and the
//! tests hand their events in through it): an event goes to the open
//! palette or overlay, a command, a tap, the pen, or on to the focused view.

use std::path::Path;
use std::time::Instant;

use crate::canvases::Canvases;
use crate::commands::Command;
use crate::scene::sidebar::Sidebar;
use crate::scene::tools::Tools;
use crate::shared::Shared;
use crate::window::{run, tap, view};
use crate::{Ask, Event, Settings, Step, asked, pen, window};

/// What the window does with an event once [`route`] has taken it.
pub enum Routed {
    /// Answer the window this.
    Answer(Step),
    /// A tap in the sidebar, or `open-workspace`, showed the workspace
    /// named: watch it, then answer.
    Watch(String, Step),
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
    // A press focuses the pane under it first, so the press works there.
    let refocused = match event {
        Event::Press(pen) => window::press(shared, sidebar, canvases, tools, pen),
        _ => false,
    };
    let asked = asked(&event, shared, canvases, tools, settings, area);
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
        (asked @ (Ask::Tap(_) | Ask::Open(_)), _) => {
            let shown = match asked {
                Ask::Open(dir) => window::open_workspace(&dir, socket, sidebar, tools),
                Ask::Tap(target) => tap(target, canvases, shared, sidebar),
                _ => None,
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
        (Ask::Pass | Ask::Not, _) if refocused => {
            Step::Through(view(settings, shared, tools, sidebar, canvases))
        }
        (Ask::Pass | Ask::Not, _) => Step::Pass,
    };
    Routed::Answer(step)
}
