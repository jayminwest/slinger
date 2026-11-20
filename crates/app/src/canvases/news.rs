//! What the canvases take in on a tick: what the followers heard and what
//! the daemon said to the ink; and, for the switch's trace, when each
//! canvas pane's page came.

use std::time::Instant;

use super::Canvases;
use super::find::{find, wanted};
use crate::backend::News;
use crate::layout::Rect;
use crate::mirror::{Applied, Mirror};
use crate::panes::{Kind, Panes};
use crate::scene::ink::Answer;
use crate::scene::pages;
use crate::trace::Paged;

impl Canvases {
    /// Takes in what the followers heard and what the daemon said to the
    /// ink, and sends the next stroke. Returns whether anything a pane
    /// shows changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        let mut lost = Vec::new();
        for open in &mut self.open {
            for news in open.follower.news() {
                changed = true;
                match news {
                    News::Shown(page, shown) => {
                        open.paged = Some(Paged { shown, heard: Instant::now() });
                        (open.heard, open.file) = (page.next.saturating_sub(1), page.path.clone());
                        (open.mirror, open.problem) = (Some(Mirror::new(&page)), None);
                    }
                    News::Problem(problem) => open.problem = Some(problem),
                    News::Changed(feed) => {
                        open.heard = feed.next.saturating_sub(1);
                        let Some(mirror) = &mut open.mirror else { continue };
                        if mirror.apply(&feed) == Applied::Gone {
                            let next = pages::after_remove(mirror.pages(), mirror.page());
                            let next = next.map(str::to_string);
                            lost.push((open.canvas.clone(), mirror.page().to_string(), next));
                        }
                    }
                }
            }
        }
        for (canvas, page, next) in lost {
            self.gone(&canvas, &page, next.as_deref());
        }
        let (sent, dictated, came) = (self.send(), self.flow.poll_dictation(), self.lives.poll());
        self.poll_sling() || sent || changed || dictated || came | self.pdfs.poll()
    }

    /// Takes the daemon's answers to the ink (a pane that asked for a new
    /// page turns to it), lets go of the changes every page followed of
    /// their canvas now holds, and hands the next change to the outbox.
    /// Returns whether there were answers.
    pub(super) fn send(&mut self) -> bool {
        if !self.shapes.editing() {
            self.text_joined();
        }
        let answers = self.outbox.answers();
        let answered = !answers.is_empty();
        for (key, answer) in answers {
            if let Some(at) = self.turning.iter().position(|(k, ..)| *k == key) {
                if let Answer::NewPage { page, .. } = &answer {
                    let (_, pane, canvas) = self.turning.remove(at);
                    self.views.turn(pane, canvas, page.clone());
                } else if matches!(answer, Answer::Refused(_)) {
                    self.turning.remove(at);
                }
            }
            if let Answer::Made { ids, .. } = &answer {
                self.select.made(key, ids);
            }
            self.fold_answered(key, &answer);
            let renamed = self.ink.answered(key, answer);
            self.select.renamed(&renamed);
        }
        // Every page of a canvas hears the same numbers: the slowest decides.
        let mut heard: Vec<(&str, u64)> = Vec::new();
        for open in self.open.iter().filter(|o| o.mirror.is_some()) {
            match heard.iter_mut().find(|(c, _)| *c == open.canvas) {
                Some((_, upto)) => *upto = (*upto).min(open.heard),
                None => heard.push((&open.canvas, open.heard)),
            }
        }
        for (canvas, upto) in heard {
            self.ink.heard(canvas, upto);
        }
        if let Some((key, change)) = self.ink.next() {
            self.outbox.send(key, self.ink.once(key), change, self.ink.joins(key));
        }
        self.keep();
        answered
    }

    /// When the page of each canvas pane of `panes` laid in `area` came, or
    /// `None` while one has none yet ([`crate::trace`]).
    pub fn paged(&self, panes: &Panes, area: Rect) -> Option<Vec<Paged>> {
        let placed = panes.place(area).into_iter().filter(|(_, p, _)| p.kind == Kind::Canvas);
        let page = |(id, pane, _): (_, &crate::panes::Pane, _)| {
            let open = find(&self.open, &pane.name, wanted(&self.views, id, &pane.name))?;
            open.mirror.as_ref().and(open.paged)
        };
        placed.map(page).collect()
    }
}
