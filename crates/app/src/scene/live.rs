//! Live items (docs/design/live-items.md): an agent UI's page shown over its
//! item on a canvas. Pure: from what each canvas pane draws, the web views
//! to place. A live item's [`Mark::Live`] is in a pane's draws only while
//! the pane shows its page and some of it is on the screen, so the page
//! follows the item under a pan, a zoom, a drag of the selection and a pane
//! resized, and goes when the pane turns to another page. The item's box
//! and name show under it until its page has come.

use std::collections::HashMap;
use std::sync::Arc;

use crate::layout::{PaneId, Point, Rect, Size};
use crate::scene::canvas::{ItemId, Mark};
use crate::scene::{CanvasPane, Web};
use crate::ui::Page;

/// Where a live item's page is: the canvas pane it is in (`None` alone in
/// the window, `--preview --as item`), the item, the pane's frame in the
/// window, which cuts the page off at its edges, and the zoom it is drawn
/// at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Live {
    pub pane: Option<PaneId>,
    pub item: ItemId,
    pub clip: Rect,
    pub zoom: f64,
}

/// The pages of the showings `canvases` draw as live items, from `pages`
/// (by showing id), each over its item's box in the window.
pub fn webs(canvases: &[CanvasPane], pages: &HashMap<String, Arc<Page>>) -> Vec<Web> {
    let mut webs = Vec::new();
    for pane in canvases {
        for draw in &pane.scene.draws {
            let Mark::Live { frame, open, zoom, .. } = &draw.mark else { continue };
            let Some(page) = pages.get(open) else { continue };
            let frame = Rect { x: pane.frame.x + frame.x, y: pane.frame.y + frame.y, ..*frame };
            let live = Live { pane: Some(pane.pane), item: draw.id, clip: pane.frame, zoom: *zoom };
            webs.push(Web { frame, page: Arc::clone(page), live: Some(live) });
        }
    }
    webs
}

/// The showings `canvases` draw as live items, each once.
pub fn showings(canvases: &[CanvasPane]) -> Vec<&str> {
    let mut opens: Vec<&str> = Vec::new();
    let draws = canvases.iter().flat_map(|c| &c.scene.draws);
    for draw in draws {
        if let Mark::Live { open, .. } = &draw.mark
            && !opens.contains(&open.as_str())
        {
            opens.push(open);
        }
    }
    opens
}

/// `page` alone in a window of `size` as a live item would show it: the
/// default item's size ([`slinger_control::LIVE_SIZE`], in tenths) at
/// `zoom`, in the middle.
pub fn alone(page: Arc<Page>, size: Size, zoom: f64) -> Web {
    let [w, h] = slinger_control::LIVE_SIZE.map(|v| f64::from(u32::try_from(v).unwrap_or(0)));
    let (width, height) = (w / 10.0 * zoom, h / 10.0 * zoom);
    let centre = Point { x: size.width / 2.0, y: size.height / 2.0 };
    let frame = Rect { x: centre.x - width / 2.0, y: centre.y - height / 2.0, width, height };
    let live = Live { pane: None, item: ItemId(0), clip: size.rect(), zoom };
    Web { frame, page, live: Some(live) }
}

#[cfg(test)]
mod tests;
