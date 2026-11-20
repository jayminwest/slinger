//! A context click in the window ([`clicked`]): what it was on
//! ([`subject_at`]) and the menu that offers ([`commands::context`]); and
//! a pick that carries its subject ([`on`]): a workspace row's agent
//! commands and a divider's grow and shrink.

use std::path::Path;

use super::{agent, facts, view};
use crate::Step;
use crate::backend::Settings;
use crate::canvases::Canvases;
use crate::commands::Command;
use crate::commands::context::{self, Popup, Subject};
use crate::config::Theme;
use crate::layout::{Point, Rect, tree::MIN_RATIO};
use crate::panes::{self, Kind};
use crate::scene::paste::Keyboard;
use crate::scene::sidebar::Sidebar;
use crate::scene::status;
use crate::scene::tools::Tools;
use crate::shared::Shared;

/// What `at`, a point in the window of `shared`, is on, in this order: a
/// workspace's rows in the open `sidebar` laid in `theme` (its ✕ too), else the rest of
/// the sidebar; a divider between panes; then in the pane under it (the
/// panes laid in `area`) its footer, the text being typed, an item or the
/// selection of `canvases` (selected first by Finder's rule:
/// [`Canvases::context_at`]), or the empty part of a canvas. `None` on an
/// agent UI's page, which keeps its own menu.
pub fn subject_at(
    at: Point,
    theme: &Theme,
    shared: &Shared,
    canvases: &mut Canvases,
    sidebar: &Sidebar,
    area: Rect,
) -> Option<Subject> {
    if sidebar.is_open() && at.x < area.x {
        let row = sidebar.row_at(theme, shared.size.rect(), at).and_then(|i| sidebar.dir(i));
        return Some(row.map_or(Subject::Sidebar, |dir| Subject::Workspace { dir }));
    }
    let panes = &shared.panes;
    if let Some(divider) = panes.divider_at(area, at) {
        let edge = Point { x: divider.at, y: divider.at };
        return Some(Subject::Divider { before: divider.before, ratio: divider.ratio(edge) });
    }
    let placed = panes.place(area);
    let (id, pane, frame) = placed.into_iter().find(|(_, _, frame)| frame.contains(at))?;
    let ui = pane.kind == Kind::Ui;
    match status::content(frame).contains(at) {
        false => Some(Subject::Pane { ui }),
        true if ui => None,
        true => canvases.context_at(panes, Some((id, frame)), at).or(Some(Subject::Canvas)),
    }
}

/// A context click at `at`, a point in the window of `shared`: the pointer
/// is there for `tools` (a new text lands at it) and the pane under it
/// takes focus, as a press there does; an item under it is selected as
/// [`subject_at`] says; then the menu for what it is on, over the scene
/// drawn with `settings`, its items grey by the window's facts after
/// that, a workspace row's by that row's workspace and agent. With nothing
/// to offer, the scene alone. Under the key overlay, the palette, the page
/// menu or an open pop-up, nothing.
pub fn clicked(
    at: Point,
    settings: &Settings,
    shared: &mut Shared,
    canvases: &mut Canvases,
    tools: &mut Tools,
    sidebar: &Sidebar,
) -> Step {
    if shared.modal() || tools.open_kind().is_some() {
        return Step::Stay;
    }
    let area = sidebar.panes(shared.size.rect());
    tools.moved(at);
    shared.press_focus(area, at, false, false);
    let subject = subject_at(at, &settings.config.theme, shared, canvases, sidebar, area);
    let scene = view(settings, shared, tools, sidebar, canvases);
    let Some(subject) = subject else { return Step::Draw(scene) };
    // The app took the click: the keys are the window's, as after a press.
    shared.keyboard = Keyboard::Window;
    let mut facts = facts(shared, canvases, tools, sidebar, area);
    if let Subject::Workspace { dir } = &subject {
        facts.workspace = true;
        facts.agent = sidebar.workspace(dir).and_then(|w| w.agent.as_ref()).map(|a| a.running);
    }
    let checked = status::command(tools.chosen());
    let items = context::items(&subject, &settings.config.keys, checked, &facts);
    Step::Menu(Popup { at, items, subject }, scene)
}

/// Runs `command` picked from `subject`'s menu on that subject (see
/// [`context::carried`]), all but a workspace's close, which the route
/// does as any close: a workspace row's start, stop or restart goes to its
/// agent (through the daemon on `socket`; the flash in `tools` names it);
/// a divider's grow or shrink moves it by one [`panes::STEP`] in `shared`
/// and at the daemon, as a drag of it does. Anything else does nothing.
pub fn on(
    command: Command,
    subject: &Subject,
    socket: &Path,
    shared: &mut Shared,
    tools: &mut Tools,
) {
    match (command, subject) {
        (
            Command::StartAgent | Command::StopAgent | Command::RestartAgent,
            Subject::Workspace { dir },
        ) => {
            agent(command, socket, Some(dir), tools);
        }
        (Command::GrowPane | Command::ShrinkPane, &Subject::Divider { before, ratio }) => {
            let ratio = moved(ratio, command == Command::GrowPane);
            let moved = shared.panes.set_ratio(before, ratio);
            let number = shared.panes.number_of(before);
            shared.resized(moved.then_some((number, ratio)), socket);
        }
        _ => {}
    }
}

/// A divider's share `ratio` one [`panes::STEP`] on (`grow`) or back, in
/// thousandths, held in the ratio's limits.
fn moved(ratio: f64, grow: bool) -> f64 {
    let step = if grow { panes::STEP } else { -panes::STEP };
    (((ratio + step) * 1000.0).round() / 1000.0).clamp(MIN_RATIO, 1.0 - MIN_RATIO)
}

#[cfg(test)]
mod tests {
    use super::moved;

    #[test]
    fn a_divider_moves_one_step_held_in_the_ratio_s_limits() {
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(near(moved(0.5, true), 0.55));
        assert!(near(moved(0.5, false), 0.45));
        assert!(near(moved(0.88, true), 0.9), "held at the limit");
        assert!(near(moved(0.12, false), 0.1));
    }
}
