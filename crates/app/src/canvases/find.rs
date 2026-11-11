//! Where things are in the canvas panes: the page each pane shows, the
//! page followed for it, and a window point on its canvas. Lookups only.

use std::borrow::Cow;

use super::{Open, View};
use crate::layout::{PaneId, Point, Rect, Viewport};
use crate::mirror::Mirror;
use crate::panes::{Kind, Panes};
use crate::scene::canvas::Item;
use crate::scene::ink::Ink;

/// The canvas point under `at`, a point in the window, in a pane laid at
/// `frame` that looks through `viewport`.
pub(super) fn to_canvas(viewport: Viewport, frame: Rect, at: Point) -> Point {
    viewport.to_canvas(Point { x: at.x - frame.x, y: at.y - frame.y })
}

/// The page `pane` of `panes` shows, as the user sees it: the server's copy
/// with the ink laid over it; empty while it has not arrived.
pub(super) fn pane_shown<'a>(
    open: &'a [Open],
    views: &[(PaneId, View)],
    ink: &Ink,
    panes: &Panes,
    pane: PaneId,
) -> Vec<Cow<'a, Item>> {
    let Some(canvas) = canvas_of(panes, pane) else { return Vec::new() };
    let mirror = find(open, canvas, wanted(views, pane, canvas)).and_then(|o| o.mirror.as_ref());
    mirror.map_or_else(Vec::new, |m| ink.shown(canvas, m.page(), m.items()))
}

/// Whether `open` follows the page `page` of `canvas` names (`None`: the
/// first): it was asked for that, or it shows that page.
pub(super) fn matches(open: &Open, canvas: &str, page: Option<&str>) -> bool {
    let showing = || open.mirror.as_ref().map(Mirror::page);
    open.canvas == canvas && (open.asked.as_deref() == page || page.is_some() && showing() == page)
}

/// The page followed that `page` of `canvas` names, if any.
pub(super) fn find<'a>(open: &'a [Open], canvas: &str, page: Option<&str>) -> Option<&'a Open> {
    open.iter().find(|o| matches(o, canvas, page))
}

/// The page `pane` shows of `canvas`: `None` for the first.
pub(super) fn wanted<'a>(
    views: &'a [(PaneId, View)],
    pane: PaneId,
    canvas: &str,
) -> Option<&'a str> {
    let view = views.iter().find(|(p, _)| *p == pane).map(|(_, v)| v)?;
    view.page.as_ref().filter(|(c, _)| c == canvas).map(|(_, p)| p.as_str())
}

/// The canvas `pane` shows, if it shows one.
pub(super) fn canvas_of(panes: &Panes, pane: PaneId) -> Option<&str> {
    let shown = panes.shows(pane);
    (shown.kind == Kind::Canvas).then_some(shown.name.as_str())
}
