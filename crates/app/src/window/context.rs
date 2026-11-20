//! A context click in the window ([`clicked`]): what it was on
//! ([`subject_at`]) and the menu that offers ([`commands::context`]).

use super::{facts, view};
use crate::Step;
use crate::backend::Settings;
use crate::canvases::Canvases;
use crate::commands::context::{self, Popup, Subject};
use crate::layout::{Point, Rect};
use crate::panes::{Kind, Panes};
use crate::scene::sidebar::Sidebar;
use crate::scene::status;
use crate::scene::tools::Tools;
use crate::shared::Shared;

/// What `at`, a point in the window, is on in the panes of `panes` laid in
/// `area`: a pane's footer, or the empty part of a canvas; `None` off every
/// pane (the sidebar) and on an agent UI's page, which keeps its own menu.
pub fn subject_at(at: Point, panes: &Panes, area: Rect) -> Option<Subject> {
    let placed = panes.place(area);
    let (_, pane, frame) = placed.into_iter().find(|(_, _, frame)| frame.contains(at))?;
    let ui = pane.kind == Kind::Ui;
    match status::content(frame).contains(at) {
        false => Some(Subject::Pane { ui }),
        true if ui => None,
        true => Some(Subject::Canvas),
    }
}

/// A context click at `at`, a point in the window of `shared`: the pointer
/// is there for `tools` (a new text lands at it) and the pane under it
/// takes focus, as a press there does; then the menu for what it is on,
/// over the scene drawn with `settings`, its items grey by the window's
/// facts now. With nothing to offer, the scene alone. Under the key
/// overlay, the palette, the page menu or an open pop-up, nothing.
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
    let scene = view(settings, shared, tools, sidebar, canvases);
    let Some(subject) = subject_at(at, &shared.panes, area) else { return Step::Draw(scene) };
    let facts = facts(shared, canvases, tools, sidebar, area);
    let checked = status::command(tools.chosen());
    let items = context::items(subject, &settings.config.keys, checked, &facts);
    Step::Menu(Popup { at, items }, scene)
}
