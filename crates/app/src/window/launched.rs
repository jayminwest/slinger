//! The window's side of the launcher ([`Launcher`]): what it heard goes to
//! the sidebar (the launch line, the offer) and the flash, and
//! `start-slingerd`, `restart-daemon` and `restart-slinger` go to it, or flash why not
//! ([`daemon::launching`]). A daemon it started coming to answer pokes
//! the [`crate::backend::Watch`] (`Watch::poke`), so the sidebar's list
//! comes at once.

use std::time::Instant;

use crate::backend::Launcher;
use crate::backend::daemon::Said;
use crate::commands::Command;
use crate::scene::daemon::{self, Launching};
use crate::scene::sidebar::Sidebar;
use crate::scene::sidebar::launched::Offer;
use crate::scene::tools::Tools;

/// What the window does with what the launcher told.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Heard {
    /// Something shown changed.
    pub changed: bool,
    /// The daemon it started came to answer: the watch asks at once.
    pub up: bool,
    /// `restart-slinger` stopped the daemon and the app opens again once
    /// this one is gone: quit.
    pub quit: bool,
}

/// Takes what `launcher` told since the last call into `sidebar` and the
/// flash of `tools`.
pub fn heard(launcher: Option<&Launcher>, sidebar: &mut Sidebar, tools: &mut Tools) -> Heard {
    let Heard { mut changed, mut up, mut quit } = Heard::default();
    for said in launcher.map(Launcher::news).unwrap_or_default() {
        match said {
            Said::Relaunch => match launcher.map(Launcher::relaunch) {
                Some(Err(e)) => {
                    tools.flash().say(format!("slinger could not open again: {e}"), Instant::now());
                    changed = true;
                }
                _ => quit = true,
            },
            Said::Daemon(now) => {
                up |= now.is_none();
                changed |= sidebar.set_daemon(now);
            }
            Said::Offer(offer) => changed |= sidebar.set_offer(offer),
            Said::Refused(why) => {
                tools.flash().say(why, Instant::now());
                changed = true;
            }
        }
    }
    Heard { changed, up, quit }
}

/// Runs `command` (`start-slingerd`, `restart-daemon` or `restart-slinger`) through
/// `launcher`, the app's if it manages a daemon; else flashes why not.
pub fn run(command: Command, launcher: Option<&Launcher>, sidebar: &Sidebar, tools: &mut Tools) {
    let old = sidebar.offer().is_some_and(Offer::old);
    match (daemon::launching(command, launcher.is_some(), old), launcher) {
        (Ok(Launching::Start), Some(launcher)) => launcher.ask(),
        (Ok(Launching::Restart), Some(launcher)) => launcher.restart(),
        (Ok(Launching::Reboot), Some(launcher)) => {
            launcher.reboot();
            tools.flash().say(daemon::RESTARTING.into(), Instant::now());
        }
        (Err(why), _) => tools.flash().say(why.into(), Instant::now()),
        (Ok(_), None) => {}
    }
}
