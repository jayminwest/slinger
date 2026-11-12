//! What the window does with a command (`run`, and the selection's commands
//! in `selection`) or a tap on a button that does more (`tap`), and what it
//! shows (`view`), and where its panes are: the canvas pane under a point
//! (`pane_at`), the focused pane (`focus`) and whether a press grabs the
//! selection (`grabs`). Split from `main.rs`, which
//! routes the events here.

use std::path::Path;
use std::time::Instant;

use crate::backend::{Problem, Settings};
use crate::canvases::Canvases;
use crate::commands::{self, Command};
use crate::layout::{PaneId, Point, Rect};
use crate::panes::{self, Panes};
use crate::scene::ink::Pen;
use crate::scene::sidebar::Row;
use crate::scene::sidebar::Sidebar;
use crate::scene::taps::{Tap, Target};
use crate::scene::tools::{Kind, Tools};
use crate::scene::{self, Label, Scene, cursor, status};
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

/// A tap on `target` (a press already focused its pane in `shared`): the
/// page name opens the page menu over it, a page menu's page turns the
/// focused pane to it, and a workspace in `sidebar` shows its panes: that
/// workspace's name, for the watch to follow. A command's tap does nothing
/// here.
pub fn tap(
    (frame, tap): Target,
    canvases: &mut Canvases,
    shared: &mut Shared,
    sidebar: &mut Sidebar,
) -> Option<String> {
    let pane = shared.panes.focus();
    match tap {
        Tap::Pages => {
            if let Some(pages) = canvases.page_names(&shared.panes, pane) {
                shared.menu.open(pane, frame, pages);
            }
        }
        Tap::Page(index) => canvases.turn_to(&shared.panes, pane, index),
        Tap::Workspace(index) => return sidebar.pick(index),
        Tap::Run(_) => {}
    }
    None
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

/// Whether a press of `pen` on the panes of `panes` laid in `area` grabs
/// the selection with the tool of `tools` ([`Canvases::grabs`]): it drags
/// the selection, and focuses its pane, even within reach of a divider.
/// The pen's eraser end never grabs it.
pub fn grabs(canvases: &Canvases, panes: &Panes, tools: &Tools, area: Rect, pen: Pen) -> bool {
    !pen.eraser && canvases.grabs(panes, pane_at(panes, area, pen.at), pen, tools.tool())
}

/// A press of `pen` focuses the pane of `shared` laid in `area` under it
/// ([`Shared::press_focus`]), even on a divider when it [`grabs`] the
/// selection. Returns whether focus moved.
pub fn press_focus(
    shared: &mut Shared,
    canvases: &Canvases,
    tools: &Tools,
    area: Rect,
    pen: Pen,
) -> bool {
    let grabs = grabs(canvases, &shared.panes, tools, area, pen);
    shared.press_focus(area, pen.at, pen.touch, grabs)
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

/// The window: the sidebar, the panes and their footers (their buttons kept
/// in `shared` for the pen, the one under it lit), any problem with the
/// daemon, a canvas or the config (each said once), the notice, the pop-up
/// at the pen and the flash of a new tool, a page command or a saved note,
/// the key overlay, the palette or the page menu on top, and the tool's
/// cursor.
pub fn view(
    settings: &Settings,
    shared: &mut Shared,
    tools: &Tools,
    sidebar: &Sidebar,
    canvases: &mut Canvases,
) -> Box<Scene> {
    let theme = &settings.config.theme;
    let area = sidebar.panes(shared.size.rect());
    canvases.agents(sidebar.statuses());
    canvases.follow_paper(settings.config.canvas);
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
    let keys = &settings.config.keys;
    let collapsed = !sidebar.is_open();
    let mut footers = canvases.footers(&shared.panes, area, theme, pen, keys, collapsed);
    footers.extend(sidebar.foot(size.rect()).map(|foot| status::strip(foot, theme)));
    let rows = sidebar.targets(theme, size.rect()).into_iter();
    let mut targets: Vec<Target> = footers.iter().flat_map(|f| f.targets.clone()).collect();
    targets.extend(rows.map(|(frame, index)| (frame, Tap::Workspace(index))));
    shared.taps.set(targets);
    if let Some(lit) =
        shared.taps.lit(theme).filter(|&lit| !status::light(&mut footers, lit, theme))
    {
        // A workspace's lines: lit under their text, over the shown one's fill.
        let (frame, fill) = lit;
        let label = Label { text: String::new(), colour: fill, fill: Some(fill), frame };
        let fills = scene.sidebar.iter().take_while(|r| r.label.text.is_empty()).count();
        scene.sidebar.insert(fills, Row { label, font: scene::sidebar::Font::PLAIN });
    }
    scene.footers = footers;
    scene.labels.extend(tools.labels(theme));
    let pages = shared.menu.pane().and_then(|pane| canvases.page_names(&shared.panes, pane));
    shared.menu.refresh(pages);
    let overlay = shared.overlay.shown(keys, theme, size);
    let overlay = overlay.or_else(|| shared.palette.shown(keys, theme, size));
    scene.overlay = overlay.or_else(|| shared.menu.shown(theme, size)).map(Box::new);
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
