//! The last sling's own reckoning: moving to a stage, the pane its pill
//! shows in, where its new items are, what the pill says once the agent
//! answered; and where the pill stands.

use std::time::Instant;

use super::words::arrow;
use super::{GAP, HIGH, Out, Stage, View};
use crate::layout::{Rect, Size};

impl Out {
    /// Moves to `stage` at `now`; an answer opened closes.
    pub(super) fn to(&mut self, stage: Stage, now: Instant) {
        (self.stage, self.since, self.reading) = (stage, now, false);
    }

    /// The pane of `views` to show the pill in, with its index: the focused
    /// one when it shows the sling's page, else the first that does, else
    /// the focused one.
    pub(super) fn view<'v>(&self, views: &[View<'v>]) -> Option<(usize, View<'v>)> {
        let here = |v: &View| v.canvas == self.canvas && v.page == self.page;
        let found = views.iter().position(|v| here(v) && v.focused);
        let found = found.or_else(|| views.iter().position(here));
        let found = found.or_else(|| views.iter().position(|v| v.focused))?;
        Some((found, views[found]))
    }

    /// "scout", or "scout (dev)" under a profile: whom the sling went to,
    /// as its flash and its pill while the agent works name it.
    pub(super) fn who(&self) -> String {
        let under = self.under.as_ref().map(|p| format!(" ({p})")).unwrap_or_default();
        format!("{}{under}", self.agent)
    }

    /// Whether `view` shows the sling's page.
    pub(super) fn shows(&self, view: &View) -> bool {
        view.canvas == self.canvas && view.page == self.page
    }

    /// The box of the agent's new items when `view` shows none of them.
    pub(super) fn away(&self, view: &View) -> Option<Rect> {
        let seen =
            view.viewport.visible(Size { width: view.frame.width, height: view.frame.height });
        let all = self.items.iter().map(|(_, frame)| *frame).reduce(Rect::union)?;
        let shown =
            self.shows(view) && self.items.iter().any(|(_, f)| f.intersection(seen).is_some());
        (!shown).then_some(all)
    }

    /// What the pill says once the agent answered (`text`: with words).
    pub(super) fn answered(&self, text: bool, view: &View) -> String {
        let agent = &self.agent;
        match self.items.len() {
            0 if !self.told => format!("✓ {agent} answered"),
            0 if text => format!("✓ {agent} answered in text · tap to read"),
            0 => format!("✓ {agent} is done, with nothing on the canvas"),
            n => {
                let items =
                    if n == 1 { "1 new item".to_string() } else { format!("{n} new items") };
                let arrow = self.away(view).map(|away| arrow(away, view)).unwrap_or_default();
                format!("✓ {agent} answered · {items}{arrow}")
            }
        }
    }
}

/// The pill's frame, `width` wide, in `view`: at the pane's top right, the
/// one place it shows (slinger-b015f9).
pub(super) fn place(view: &View, width: f64) -> Rect {
    let frame = view.frame;
    let x = (frame.x + frame.width - width - GAP).max(frame.x + GAP);
    Rect { x, y: frame.y + GAP, width, height: HIGH }
}
