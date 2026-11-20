//! What each canvas pane shows, and the threads that fetch it. Each page a
//! pane shows is followed once, however many panes show it: a [`Follower`]
//! on its own thread, and the [`Mirror`] its news keeps up to date
//! ([`Open`]). Each pane has its own view ([`Views`]: page, pan, zoom, the
//! boxes it is stepped into), kept per workspace and handed to the daemon
//! to keep ([`looks`]); the live items' pages ([`live`]) and the PDFs
//! ([`pdf`]) its items draw are fetched on threads of their own. [`Follow`]
//! hides all of these, with the panes waiting for a page they added, the
//! unfolds on their way and the peek ([`fold`]), and what stepping keeps
//! besides each pane's trail ([`step`]). It starts the threads it needs:
//! a follower per page, the live items', the PDFs' and the view saver.
//! Runs on the main thread and never waits.

mod fold;
mod live;
mod looks;
mod pdf;
mod step;

use std::borrow::Cow;
use std::time::Instant;

use slinger_control::Client;

use super::Open;
use super::find::{find, matches, wanted};
use super::view::{View, Views};
use crate::backend::{Follower, News, Problem, Saver, Wake};
use crate::layout::{PaneId, Rect};
use crate::mirror::{Applied, Mirror};
use crate::panes::{Kind, Pane, Panes};
use crate::scene::canvas::{Item, ItemId};
use crate::scene::ink::{Answer, Ink};
use crate::scene::pages;
use crate::trace::Paged;

/// The pages the panes show, their views, and the threads that fetch them.
pub(super) struct Follow {
    daemon: Client,
    /// Wakes the main thread when a follower has news.
    wake: Wake,
    open: Vec<Open>,
    views: Views,
    /// The panes waiting for a new page, by the key of its `page.add`.
    turning: Vec<(u64, PaneId, String)>,
    lives: live::Lives,
    pdfs: pdf::Pdfs,
    /// Sends where the panes look to the daemon to keep ([`looks`]).
    saver: Saver,
    /// The unfolds on their way, and the peek shown.
    folds: fold::Folds,
    /// The last tap on a miniature, and what the pen and the text being
    /// typed join ([`super::step`]).
    steps: step::Steps,
}

/// A canvas pane being drawn: the page it shows, once it has come; the box
/// it shows as a peek there; its view; and the PDFs fetched.
pub(super) struct Showing<'a> {
    pub(super) mirror: Option<&'a Mirror>,
    pub(super) peek: Option<ItemId>,
    pub(super) view: &'a mut View,
    pub(super) pdfs: &'a pdf::Pdfs,
}

impl Follow {
    /// None followed yet: everything is fetched through `daemon`, and each
    /// thread's news wakes the main thread with `wake`.
    pub(super) fn start(daemon: &Client, wake: &Wake) -> Self {
        let lives = live::Lives::start(daemon.clone(), wake);
        let pdfs = pdf::Pdfs::start(daemon.clone(), crate::backend::pdf::cache(), wake);
        Self {
            daemon: daemon.clone(),
            wake: wake.clone(),
            open: Vec::new(),
            views: Views::default(),
            turning: Vec::new(),
            lives,
            pdfs,
            saver: Saver::start(daemon.clone()),
            folds: fold::Folds::default(),
            steps: step::Steps::default(),
        }
    }

    /// Follows every page, fetches and keeps views through `daemon`
    /// instead, from the start.
    pub(super) fn reconnect(&mut self, daemon: Client) {
        self.saver.reconnect(daemon.clone());
        self.lives = live::Lives::start(daemon.clone(), &self.wake);
        self.pdfs = pdf::Pdfs::start(daemon.clone(), crate::backend::pdf::cache(), &self.wake);
        self.daemon = daemon;
        self.open.clear();
    }

    /// The pages followed.
    pub(super) fn open(&self) -> &[Open] {
        &self.open
    }

    /// The views of the shown workspace's panes.
    pub(super) fn views(&self) -> &Views {
        &self.views
    }

    /// Pane `pane`'s view, a new one if it has none.
    pub(super) fn view(&mut self, pane: PaneId) -> &mut View {
        self.views.get(pane)
    }

    /// The page followed that `pane` shows of `canvas`, if any.
    pub(super) fn shown(&self, canvas: &str, pane: PaneId) -> Option<&Open> {
        find(&self.open, canvas, wanted(&self.views, pane, canvas))
    }

    /// Page `page` of `canvas`, once it has come.
    pub(super) fn mirror(&self, canvas: &str, page: &str) -> Option<&Mirror> {
        find(&self.open, canvas, Some(page))?.mirror.as_ref()
    }

    /// The page `pane` of `panes` shows ([`super::find::pane_shown`]).
    pub(super) fn pane_shown<'a>(
        &'a self,
        ink: &Ink,
        panes: &Panes,
        pane: PaneId,
    ) -> Vec<Cow<'a, Item>> {
        super::find::pane_shown(&self.open, &self.views, ink, panes, pane)
    }

    /// Shows the views of `workspace`'s panes ([`Views::within`]); a first
    /// tap on a miniature waits no more. Whether the workspace changed.
    pub(super) fn within(&mut self, workspace: Option<&str>) -> bool {
        let changed = self.views.within(workspace);
        if changed {
            self.steps.switched();
        }
        changed
    }

    /// Takes in what each pane of `placed` (each laid in `panes`) shows:
    /// learns the view the daemon kept for it, starts following each page
    /// a pane shows, stops following the others (their strokes already
    /// written let go from `ink`), and forgets the views of panes that
    /// closed. Returns each pane's canvas and the page it asked for.
    pub(super) fn follow<'a>(
        &mut self,
        panes: &Panes,
        placed: &[(PaneId, &'a Pane, Rect)],
        ink: &mut Ink,
    ) -> Vec<(&'a str, Option<String>)> {
        for (id, pane, _) in placed {
            self.views.get(*id).learn(&pane.name, panes.kept(*id));
        }
        let shown: Vec<(&str, Option<String>)> = placed
            .iter()
            .map(|(id, p, _)| {
                (p.name.as_str(), wanted(&self.views, *id, &p.name).map(str::to_string))
            })
            .collect();
        for o in self.open.iter().filter(|o| !shown.iter().any(|(c, _)| *c == o.canvas)) {
            // No feed of it comes now: its strokes already written are let go.
            ink.heard(&o.canvas, u64::MAX);
        }
        self.open.retain(|o| shown.iter().any(|(c, p)| matches(o, c, p.as_deref())));
        for (canvas, page) in &shown {
            if find(&self.open, canvas, page.as_deref()).is_none() {
                let (daemon, wake) = (self.daemon.clone(), &self.wake);
                let follower = Follower::start(daemon, (*canvas).to_string(), page.clone(), wake);
                let (canvas, asked) = ((*canvas).to_string(), page.clone());
                let (mirror, problem, file, paged) = (None, None, None, None);
                let heard = 0;
                self.open.push(Open {
                    canvas,
                    asked,
                    follower,
                    mirror,
                    heard,
                    problem,
                    file,
                    paged,
                });
            }
        }
        self.views.keep(&placed.iter().map(|(id, ..)| *id).collect::<Vec<_>>());
        shown
    }

    /// Each page a pane is inside a folded box on: a selection there may
    /// hold hidden items.
    pub(super) fn insides(&self) -> Vec<(String, String)> {
        self.views.iter().filter_map(|(_, v)| v.trail.page()).collect()
    }

    /// Pane `pane`, which shows `canvas`, as it is drawn.
    pub(super) fn showing(&mut self, pane: PaneId, canvas: &str) -> Showing<'_> {
        let open = find(&self.open, canvas, wanted(&self.views, pane, canvas));
        let mirror = open.and_then(|o| o.mirror.as_ref());
        let peek = mirror.and_then(|m| self.peek_in(pane, (canvas, m.page())));
        Showing { mirror, peek, view: self.views.get(pane), pdfs: &self.pdfs }
    }

    /// Asks for the PDF of each `pdf` item `scenes` draw ([`pdf::Pdfs::want`]).
    pub(super) fn want(&mut self, scenes: &[crate::scene::CanvasPane]) {
        self.pdfs.want(scenes);
    }

    /// Takes in what the followers heard: a page removed sends the panes on
    /// it to the page beside it. Returns whether any heard anything.
    pub(super) fn hear(&mut self) -> bool {
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
        changed
    }

    /// Takes the live items' pages fetched ([`live::Lives::poll`]).
    pub(super) fn poll_lives(&mut self) -> bool {
        self.lives.poll()
    }

    /// Takes the PDFs fetched and the end of an open ([`pdf::Pdfs::poll`]).
    pub(super) fn poll_pdfs(&mut self) -> bool {
        self.pdfs.poll()
    }

    /// Page `gone` of `canvas` was removed (the feed says so): the panes
    /// on it go to `next`, and its follower follows that page instead (the
    /// first page, for one that followed the first).
    fn gone(&mut self, canvas: &str, gone: &str, next: Option<&str>) {
        self.leave(canvas, gone, next);
        let on = |o: &&mut Open| {
            o.canvas == canvas && o.mirror.as_ref().is_some_and(|m| m.page() == gone)
        };
        for open in self.open.iter_mut().filter(on) {
            open.asked = open.asked.as_ref().and(next.map(str::to_string));
            let (daemon, asked) = (self.daemon.clone(), open.asked.clone());
            open.follower = Follower::start(daemon, canvas.to_string(), asked, &self.wake);
        }
    }

    /// The panes that asked for page `gone` of `canvas` show `next` (the
    /// first page without one) instead.
    pub(super) fn leave(&mut self, canvas: &str, gone: &str, next: Option<&str>) {
        for (_, view) in self.views.iter_mut() {
            if view.page.as_ref().is_some_and(|(c, p)| c == canvas && p == gone) {
                view.page = next.map(|n| (canvas.to_string(), n.to_string()));
            }
        }
    }

    /// Pane `pane` turns to the page of `canvas` the change sent under
    /// `key` adds, once it is written.
    pub(super) fn turn_when(&mut self, key: u64, pane: PaneId, canvas: String) {
        self.turning.push((key, pane, canvas));
    }

    /// The panes waiting for a new page: each one's key and pane number.
    pub(super) fn turnings(&self) -> Vec<(u64, u32)> {
        self.turning.iter().map(|(key, pane, _)| (*key, pane.number())).collect()
    }

    /// The daemon's `answer` to the change sent under `key`: a pane that
    /// asked for a new page turns to it.
    pub(super) fn turned(&mut self, key: u64, answer: &Answer) {
        let Some(at) = self.turning.iter().position(|(k, ..)| *k == key) else { return };
        if let Answer::NewPage { page, .. } = answer {
            let (_, pane, canvas) = self.turning.remove(at);
            self.views.turn(pane, canvas, page.clone());
        } else if matches!(answer, Answer::Refused(_)) {
            self.turning.remove(at);
        }
    }

    /// The number of the last change heard of each canvas followed: every
    /// page of a canvas hears the same numbers, so the slowest decides.
    pub(super) fn heard(&self) -> Vec<(&str, u64)> {
        let mut heard: Vec<(&str, u64)> = Vec::new();
        for open in self.open.iter().filter(|o| o.mirror.is_some()) {
            match heard.iter_mut().find(|(c, _)| *c == open.canvas) {
                Some((_, upto)) => *upto = (*upto).min(open.heard),
                None => heard.push((&open.canvas, open.heard)),
            }
        }
        heard
    }

    /// When the page of each canvas pane of `panes` laid in `area` came, or
    /// `None` while one has none yet ([`crate::trace`]).
    pub(super) fn paged(&self, panes: &Panes, area: Rect) -> Option<Vec<Paged>> {
        let placed = panes.place(area).into_iter().filter(|(_, p, _)| p.kind == Kind::Canvas);
        let page = |(id, pane, _): (_, &Pane, _)| {
            let open = self.shown(&pane.name, id)?;
            open.mirror.as_ref().and(open.paged)
        };
        placed.map(page).collect()
    }

    /// What following the pages went wrong with.
    pub(super) fn problems(&self) -> impl Iterator<Item = &Problem> {
        self.open.iter().filter_map(|o| o.problem.as_ref())
    }

    /// What went wrong with the last PDF opened, if anything.
    pub(super) fn pdf_problem(&self) -> Option<&Problem> {
        self.pdfs.problem()
    }

    /// Pane `to` takes a copy of pane `from`'s view ([`Views::copy`]).
    pub(super) fn split_view(&mut self, from: PaneId, to: PaneId) {
        self.views.copy(from, to);
    }

    /// Forgets the panes' views of `workspace`, closed ([`Views::forget`]).
    pub(super) fn forget(&mut self, workspace: &str) {
        self.views.forget(workspace);
    }
}
