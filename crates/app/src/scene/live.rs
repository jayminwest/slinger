//! Live items (docs/design/live-items.md): an agent UI's page shown over its
//! item on a canvas. Pure: from what each canvas pane draws, the web views
//! to place. A live item's [`Mark::Live`] is in a pane's draws only while
//! the pane shows its page and some of it is on the screen, so the page
//! follows the item under a pan, a zoom, a drag of the selection and a pane
//! resized. The item's box and name show under it until its page has come.
//!
//! How each page shows ([`Shows`]): an item narrower than [`SMALLEST`] on
//! the screen takes no presses; one that went off the screen, or whose
//! pane turned to another page, keeps its view, hidden, for [`away::CLOSE_AFTER`]
//! ([`Away`]), so a page panned away and back keeps its state; then the
//! view closes. A reply its agent did not take shows over its item
//! ([`notes`]).

use std::collections::HashMap;
use std::sync::Arc;

use crate::config::Theme;
use crate::layout::{PaneId, Point, Rect, Size};
use crate::scene::canvas::{ItemId, Mark};
use crate::scene::{CanvasPane, Label, Notes, Web};
use crate::ui::Page;

mod away;

pub use self::away::Away;

/// The narrowest a live item is on the screen, in points, and still takes
/// presses (docs/design/live-items.md, "How it scales").
pub const SMALLEST: f64 = 48.0;

/// The height of the line a note takes over its item, in points.
const NOTE_HEIGHT: f64 = 22.0;

/// Where a live item's page is: the canvas pane it is in (`None` alone in
/// the window, `--preview --as item`), the item, the pane's frame in the
/// window, which cuts the page off at its edges, and the zoom it is drawn
/// at, and how it shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Live {
    pub pane: Option<PaneId>,
    pub item: ItemId,
    pub clip: Rect,
    pub zoom: f64,
    pub shows: Shows,
}

/// How a live item's page shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shows {
    /// Its page, which takes presses.
    Page,
    /// Narrower than [`SMALLEST`] on the screen: its page takes no presses.
    Still,
    /// Off the screen: its view is kept, hidden, until [`away::CLOSE_AFTER`].
    Away,
}

impl Shows {
    /// How an item `width` points wide on the screen shows.
    pub fn at(width: f64) -> Self {
        if width < SMALLEST { Self::Still } else { Self::Page }
    }
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
            let shows = Shows::at(frame.width);
            let (item, clip) = (draw.id, pane.frame);
            let live = Live { pane: Some(pane.pane), item, clip, zoom: *zoom, shows };
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
    let shows = Shows::at(frame.width);
    let live = Live { pane: None, item: ItemId(0), clip: size.rect(), zoom, shows };
    Web { frame, page, live: Some(live) }
}

/// Over each live item of `webs` on the screen whose showing has a note in
/// `notes` (its agent did not take its last reply), the note: a line in
/// `theme`'s error colour on its background, along the item's top, cut off
/// at its pane.
pub fn notes(webs: &[Web], notes: &Notes, theme: &Theme) -> Vec<Label> {
    let mut labels = Vec::new();
    for web in webs {
        let Some(live) = web.live.filter(|l| l.shows != Shows::Away) else { continue };
        let Some(note) = web.page.open.as_ref().and_then(|open| notes.get(open)) else { continue };
        let top = Rect { height: NOTE_HEIGHT.min(web.frame.height), ..web.frame };
        let Some(frame) = top.intersection(live.clip) else { continue };
        let text = format!("✗ {}", note.message);
        labels.push(Label { text, colour: theme.error, fill: Some(theme.background), frame });
    }
    labels
}

#[cfg(test)]
mod tests;
