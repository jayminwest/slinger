//! `unlock-figure` from the keys, the menus or the palette: on the canvas
//! pane under the pointer (docs/design/figures.md, "The unlock").

use super::pane_at;
use crate::canvases::Canvases;
use crate::layout::{PaneId, Point, Rect};
use crate::panes::Panes;
use crate::scene::tools::Tools;

/// Grabs the figure under the pointer of `tools`, over the panes of
/// `panes` laid in `area`; with none there, the flash says so.
pub fn unlock(canvases: &mut Canvases, panes: &Panes, tools: &Tools, area: Rect) -> bool {
    canvases.unlock_figure(panes, under(panes, tools, area));
    true
}

/// The canvas pane under the pointer of `tools`, its frame, and the pointer.
pub fn under(panes: &Panes, tools: &Tools, area: Rect) -> Option<((PaneId, Rect), Point)> {
    let at = tools.pointer()?;
    Some((pane_at(panes, area, at)?, at))
}
