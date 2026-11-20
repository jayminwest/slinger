//! The window's side of the launcher ([`Launcher`]): what it heard goes to
//! the sidebar (the launch line, the offer) and the flash, and
//! `start-slingerd` and `restart-daemon` go to it, or flash why not
//! ([`daemon::launching`]).

use std::time::Instant;

use crate::backend::Launcher;
use crate::backend::daemon::Said;
use crate::commands::Command;
use crate::scene::daemon::{self, Launching};
use crate::scene::sidebar::Sidebar;
use crate::scene::sidebar::launched::Offer;
use crate::scene::tools::Tools;

/// Takes what `launcher` told since the last call into `sidebar` and the
/// flash of `tools`: whether anything shown changed, and whether the
/// daemon it started came to answer (the watch then asks at once).
pub fn heard(
    launcher: Option<&Launcher>,
    sidebar: &mut Sidebar,
    tools: &mut Tools,
) -> (bool, bool) {
    let (mut changed, mut up) = (false, false);
    for said in launcher.map(Launcher::news).unwrap_or_default() {
        match said {
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
    (changed, up)
}

/// Runs `command` (`start-slingerd` or `restart-daemon`) through
/// `launcher`, the app's if it manages a daemon; else flashes why not.
pub fn run(command: Command, launcher: Option<&Launcher>, sidebar: &Sidebar, tools: &mut Tools) {
    let old = sidebar.offer().is_some_and(Offer::old);
    match (daemon::launching(command, launcher.is_some(), old), launcher) {
        (Ok(Launching::Start), Some(launcher)) => launcher.ask(),
        (Ok(Launching::Restart), Some(launcher)) => launcher.restart(),
        (Err(why), _) => tools.flash().say(why.into(), Instant::now()),
        (Ok(_), None) => {}
    }
}
