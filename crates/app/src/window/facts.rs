//! What the window can act on now ([`Facts`]), read for the one enabled
//! predicate ([`crate::commands::able::can`]) from the queries the
//! commands' own gates ask, so the menu greys exactly what would not act.

use super::{focus, items, pane_at, shown_ui};
use crate::canvases::{Boxed, Canvases};
use crate::commands::able::Facts;
use crate::layout::Rect;
use crate::panes::Kind;
use crate::scene::sidebar::Sidebar;
use crate::scene::sidebar::launched::Offer;
use crate::scene::tools::Tools;
use crate::shared::Shared;

/// The facts of `shared`, `canvases`, `tools` (the pointer) and `sidebar`,
/// the panes laid in `area`.
pub fn facts(
    shared: &Shared,
    canvases: &Canvases,
    tools: &Tools,
    sidebar: &Sidebar,
    area: Rect,
) -> Facts {
    let panes = &shared.panes;
    let pane = panes.focus();
    let canvas = panes.focused().kind == Kind::Canvas;
    let under = tools.pointer().and_then(|at| pane_at(panes, area, at));
    let new_text = canvases.page_names(panes, under.unwrap_or_else(|| focus(panes, area)).0);
    let workspace = sidebar.shown_dir().is_some();
    let target = canvases.step_target(panes, pane);
    Facts {
        panes: panes.place(area).len(),
        canvas,
        pages: canvases.page_names(panes, pane).map_or(0, |(names, _)| names.len()),
        ui: shown_ui(shared).is_some(),
        ui_pane: panes.focused().kind == Kind::Ui,
        items: items::holds(shared, canvases),
        selected: canvases.shows_selection(panes, pane),
        boxed: target.as_ref().is_some_and(Boxed::is_box),
        steps: target.is_some(),
        inside: canvases.inside(panes, pane),
        sling: canvases.can_sling(panes, area),
        entry: canvases.typing(),
        sling_now: canvases.can_sling_now(panes, area),
        new_text: new_text.is_some() && !canvases.typing(),
        text: canvases.focus_target(panes, pane).is_some() || canvases.write_focused(pane),
        workspace,
        agent: sidebar.shown_agent().map(|a| a.running).filter(|_| workspace),
        notice: sidebar.noticed(),
        managed: sidebar.managed(),
        old: sidebar.offer().is_some_and(Offer::old),
    }
}
