//! Opening and closing a workspace from the window: `open-workspace`,
//! `close-workspace` and a sidebar row's ✕. A close names its workspace by
//! folder, asks the daemon first, and takes the row out only once the
//! daemon closed it; only closing the shown one changes what the window
//! shows.

use std::path::Path;
use std::time::Instant;

use crate::backend;
use crate::canvases::Canvases;
use crate::scene::sidebar::Sidebar;
use crate::scene::tools::Tools;
use crate::shared::Shared;

/// Opens the folder `dir` as a workspace through the daemon on `socket` and
/// shows it in `sidebar`: its name, for the watch to follow. The flash in
/// `tools` says why it cannot.
pub fn open_workspace(
    dir: &Path,
    socket: &Path,
    sidebar: &mut Sidebar,
    tools: &mut Tools,
) -> Option<String> {
    match backend::open_workspace(&slinger_control::Client::new(socket), dir) {
        Ok(name) => {
            sidebar.show(Some(name.clone()));
            Some(name)
        }
        Err(problem) => {
            tools.flash().say(problem.message, Instant::now());
            None
        }
    }
}

/// Closes the workspace in the folder `dir` through the daemon on `socket`
/// (which ends its agent's process, unless another open workspace names
/// that agent), then takes it out of `sidebar` and flashes "Closed <name>"
/// in `tools`, with the agent it ended. Its views and the pane it left
/// focused in `canvases` and `shared` go (the shown one's once the window
/// has left it): open again, it comes back as the daemon kept it. On a
/// failure the row stays and the flash says why.
/// Whether the window showed it: the watch then follows the first usable.
pub fn close_workspace(
    dir: &Path,
    socket: &Path,
    sidebar: &mut Sidebar,
    tools: &mut Tools,
    (shared, canvases): (&mut Shared, &mut Canvases),
) -> bool {
    let now = Instant::now();
    if sidebar.shown_dir().as_deref() == Some(dir) {
        // Its last looks reach the daemon first: open again, it comes back there.
        canvases.quitting(&shared.panes);
    }
    if let Err(problem) = backend::close_workspace(&slinger_control::Client::new(socket), dir) {
        tools.flash().say(problem.message, now);
        return false;
    }
    let Some((gone, shown)) = sidebar.closed(dir) else { return false };
    let others = sidebar.agents().unwrap_or_default();
    let ended = gone.agent.as_ref().filter(|a| a.running).filter(|a| {
        !others.iter().any(|(_, other)| other.as_ref().is_some_and(|o| o.name == a.name))
    });
    let said = match ended {
        Some(agent) => format!("Closed {} · ended agent {}", gone.title(), agent.name),
        None => format!("Closed {}", gone.title()),
    };
    tools.flash().say(said, now);
    let name = match gone.name.as_str() {
        "" => gone.dir.to_string_lossy().into_owned(),
        name => name.to_string(),
    };
    canvases.forget(&name);
    shared.forget(&name, shown);
    shown
}
