//! The live items gone from the screen: each keeps its view, hidden, for
//! [`CLOSE_AFTER`] after it went, so a page panned away and back keeps its
//! state; then its view closes, and opens again if it comes back. The
//! clock is the caller's.

use std::time::{Duration, Instant};

use super::{Live, Shows};
use crate::scene::Web;

/// How long a live item's view is kept after its item left the screen
/// (docs/design/live-items.md, "How it scales").
pub const CLOSE_AFTER: Duration = Duration::from_secs(30);

/// The live items last on the screen: each as it last showed, and since
/// when it is gone (`None` while it shows).
#[derive(Debug, Default)]
pub struct Away {
    seen: Vec<(Web, Option<Instant>)>,
}

/// Which view a live item's web is: its pane, its item and its page.
fn same(a: &Web, b: &Web) -> bool {
    let key = |w: &Web| w.live.map(|l| (l.pane, l.item));
    key(a) == key(b) && a.page.url == b.page.url
}

impl Away {
    /// Adds to `webs`, the pages on the screen at `now`, each live item's
    /// page that left it less than [`CLOSE_AFTER`] ago, as it last showed
    /// but [`Shows::Away`]; forgets the ones gone longer.
    pub fn keep(&mut self, webs: &mut Vec<Web>, now: Instant) {
        for (web, gone) in &mut self.seen {
            if gone.is_none() && !webs.iter().any(|w| same(w, web)) {
                *gone = Some(now);
            }
        }
        self.seen.retain(|(web, gone)| {
            let shown = webs.iter().any(|w| same(w, web));
            !shown && gone.is_some_and(|since| now.duration_since(since) < CLOSE_AFTER)
        });
        let kept: Vec<Web> = self.seen.iter().map(|(web, _)| web.clone()).collect();
        let shown = webs.iter().filter(|w| w.live.is_some()).map(|w| (w.clone(), None));
        self.seen.extend(shown);
        webs.extend(kept.into_iter().map(|web| {
            let live = web.live.map(|l| Live { shows: Shows::Away, ..l });
            Web { live, ..web }
        }));
    }

    /// Whether a kept view is due to close at `now`: the window draws
    /// again, so it does.
    pub fn due(&self, now: Instant) -> bool {
        self.seen
            .iter()
            .any(|(_, gone)| gone.is_some_and(|since| now.duration_since(since) >= CLOSE_AFTER))
    }
}

#[cfg(test)]
mod tests;
