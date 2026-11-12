//! What the window does with a command (`run`, and the selection's commands
//! in `selection`) and what it shows (`view`), and where its panes are: the
//! canvas pane under a point (`pane_at`) and the focused pane (`focus`).
//! Split from `main.rs`, which routes the events here.

use std::path::Path;
use std::time::Instant;

use crate::backend::{Problem, Settings};
use crate::canvases::Canvases;
use crate::commands::{self, Command};
use crate::layout::{PaneId, Point, Rect};
use crate::panes::{self, Panes};
use crate::scene::sidebar::Sidebar;
use crate::scene::tools::{Kind, Tools};
use crate::scene::{self, Scene, cursor, status};
use crate::shared::Shared;

/// Runs `command` (not `quit` or `reload-config`) on the focused pane of
/// `shared`: a picker opens its pop-up there; a pane command goes to the
/// daemon on `socket`. False for a selection command with nothing selected:
/// the key goes on (a delete in a UI's form).
pub fn run(
    command: Command,
    tools: &mut Tools,
    canvases: &mut Canvases,
    sidebar: &mut Sidebar,
    shared: &mut Shared,
    socket: &Path,
) -> bool {
    let area = sidebar.panes(shared.size.rect());
    match command {
        c if Kind::opened_by(c).is_some_and(|kind| {
            tools.open(kind, focused(&shared.panes, area));
            true
        }) => {}
        c if !selection(canvases, shared, tools, area, c) => return false,
        Command::ToggleSidebar => sidebar.set_open(!sidebar.is_open()),
        Command::DismissNotice => sidebar.dismiss(),
        c if canvases.command(&shared.panes, focus(&shared.panes, area), c) || tools.command(c) => {
        }
        c => shared.command(c, area, socket),
    }
    true
}

/// Runs a selection command (`sling`, `dictate`: the entry) when the focused pane of `shared`
/// laid in `area` shows its canvas; false when it does not. `leave-feedback` opens its entry
/// there, with the pen's tool in `tools`. Any other command goes on (true).
pub fn selection(
    canvases: &mut Canvases,
    shared: &Shared,
    tools: &Tools,
    area: Rect,
    command: Command,
) -> bool {
    let panes = &shared.panes;
    match command {
        Command::LeaveFeedback => {
            let (at, workspace) = (focus(panes, area), shared.workspace.as_deref());
            canvases.open_feedback(panes, at, workspace, tools.tool());
            true
        }
        Command::DeleteSelection => canvases.delete_selection(panes, panes.focus()),
        Command::ClearSelection => canvases.clear_selection(panes, panes.focus()),
        Command::Sling | Command::Dictate => canvases.sling_command(command, panes, area),
        _ => true,
    }
}

/// The canvas pane of `panes` laid in `area` under `at`, a point in the
/// window, and its canvas's frame (above its footer). Over any other pane,
/// or a footer, a gesture goes on to what it shows (an agent UI scrolls itself).
pub fn pane_at(panes: &Panes, area: Rect, at: Point) -> Option<(PaneId, Rect)> {
    let mut placed = panes.place(area).into_iter().map(|(id, p, f)| (id, p, status::content(f)));
    let found = placed.find(|(_, _, frame)| frame.contains(at));
    found.filter(|(_, pane, _)| pane.kind == panes::Kind::Canvas).map(|(id, _, frame)| (id, frame))
}

/// The frame of the focused pane of `panes` laid in `area`.
pub fn focused(panes: &Panes, area: Rect) -> Rect {
    focus(panes, area).1
}

/// The focused pane of `panes` laid in `area`, and its frame above its footer.
pub fn focus(panes: &Panes, area: Rect) -> (PaneId, Rect) {
    let placed = panes.place(area);
    let found = placed.into_iter().find(|(id, _, _)| *id == panes.focus());
    (panes.focus(), status::content(found.map_or(area, |(_, _, frame)| frame)))
}

/// The window: the sidebar, the panes and their footers, any problem with the
/// daemon, a canvas or the config (each said once), the notice, the pop-up
/// at the pen and the flash of a new tool, a page command or a saved note, the key overlay on top, and the
/// tool's cursor.
pub fn view(
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
    canvases.set_pen_width(tools.width().points());
    scene.footers = canvases.footers(&shared.panes, area, theme, pen, &settings.config.keys);
    scene.labels.extend(tools.labels(theme));
    let keys = &settings.config.keys;
    let overlay = shared.overlay.shown(keys, theme, size);
    scene.overlay = overlay.or_else(|| shared.palette.shown(keys, theme, size)).map(Box::new);
    scene.cursor = cursor::over_canvas(tools, theme);
    let now = Instant::now();
    tools.follow(keys, now);
    let said = canvases.said();
    if let Some(text) = canvases.take_flash().or(said) {
        tools.flash().say(text, now);
    }
    scene.flash = tools.flash().pill(focused(&shared.panes, area), theme, now).map(Box::new);
    scene.menus = commands::menu::bar(keys, status::command(tools.chosen()));
    Box::new(scene)
}
