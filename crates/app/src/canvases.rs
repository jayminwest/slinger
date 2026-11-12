//! The canvases the panes show. Each canvas a pane shows is followed once,
//! however many panes show it: a [`Follower`] on its own thread, and the
//! [`Mirror`] its news keeps up to date. Each canvas pane has its own
//! viewport, and keeps the last scene it drew, so the next one repaints only
//! what changed. The pen draws on a canvas pane: its [`Ink`] shows over the
//! page at once and goes to the daemon through an [`Outbox`]. The eraser and
//! the lasso work on the page as shown ([`Select`]), and so do the text, box
//! and arrow tools ([`Shapes`]); what they change goes the same way, in turn
//! with the strokes. The sling sends the selection ([`sling`]) once the ink
//! before it is in, and what became of it shows on the canvas ([`slung`]);
//! its prompt can be spoken ([`dictation`]). Runs on the
//! main thread and never waits: the followers, the outbox, the slinger and
//! Flick's thread do.
//!
//! Each pane keeps its own page, followed once however many panes show it:
//! page commands, undo and redo are in [`paging`], live items' pages in [`live`];
//! cut, copy and paste in [`clip`].

mod button;
mod clip;
mod drag;
mod find;
mod kept;
mod live;
mod paging;

use std::rc::Rc;

use slinger_control::Client;

use crate::backend::{Flick, Follower, News, Outbox, Problem, Slinger, run};
use crate::config::{Rgb, Theme};
use crate::layout::{PaneId, Rect, Size, Viewport};
use crate::mirror::{Applied, Mirror};
use crate::panes::{Kind, Panes};
use crate::scene::cursor::Cursor;
use crate::scene::grid::Paper;
use crate::scene::ink::{Answer, Ink, Pen};
use crate::scene::pages::{self, Pages};
use crate::scene::select::{Select, Selection};
use crate::scene::shapes::{Press, Shapes};
use crate::scene::sidebar::Agent;
use crate::scene::sling::Sling;
use crate::scene::slung::Slung;
use crate::scene::tools::Tool;
use crate::scene::{CanvasPane, canvas};
use find::{canvas_of, find, matches, pane_shown, to_canvas, wanted};

mod dictation;
mod feedback;
mod sling;
mod slung;
mod status;

/// The canvases of a window's panes.
pub struct Canvases {
    daemon: Client,
    open: Vec<Open>,
    views: Vec<(PaneId, View)>,
    ink: Ink,
    select: Select,
    shapes: Shapes,
    outbox: Outbox,
    sling: Sling,
    slinger: Slinger,
    /// What became of the last sling.
    slung: Slung,
    /// The words a sling the user wrote none for sends.
    prompt: String,
    /// Speaking the sling's prompt through Flick.
    voice: dictation::Voice,
    /// The feedback being left.
    notes: feedback::Notes,
    /// The pane the pen draws in, and its frame in the window, while down.
    pen: Option<(PaneId, Rect)>,
    /// When the oldest pen event no scene has shown yet happened.
    unshown: Option<f64>,
    /// The question a page command asks, if any.
    asks: Pages,
    /// The panes waiting for a new page, by the key of its `page.add`.
    turning: Vec<(u64, PaneId, String)>,
    lives: live::Lives,
    /// Each open workspace's canvas and its agent, as the sidebar last
    /// heard: for the buttons, the footers and `sling-now`.
    agents: Option<Vec<(String, Option<Agent>)>>,
    /// What to flash once: a pane's new zoom ("Zoom 150%"), the grid's
    /// toggle or where `save` said the canvas went.
    zoomed: Option<String>,
    /// Whether every canvas pane shows the dotted grid.
    paper: Paper,
    /// Where the strokes not yet written are kept, if anywhere.
    kept: Option<kept::Kept>,
    /// The pane the pointer is on a selection handle in, and its cursor.
    handle: Option<(PaneId, crate::scene::cursor::Cursor)>,
}

/// One canvas page followed: the page asked for (`None`: the first), as
/// last heard, the number of the last change heard, and what is wrong, if
/// anything.
struct Open {
    canvas: String,
    asked: Option<String>,
    follower: Follower,
    mirror: Option<Mirror>,
    heard: u64,
    problem: Option<Problem>,
    /// The canvas's file, as the daemon last said.
    file: Option<String>,
}

/// One canvas pane: where it looks, the page it shows (by canvas and page
/// id; the first page without one), and what it last drew.
struct View {
    viewport: Viewport,
    page: Option<(String, String)>,
    last: Option<Rc<canvas::Scene>>,
}

impl Default for View {
    /// A new pane's view: at the start, on the first page, nothing drawn.
    fn default() -> Self {
        Self { viewport: Viewport::start(), page: None, last: None }
    }
}

impl Canvases {
    /// None followed yet; [`Canvases::show`] starts following. Strokes not
    /// yet written are kept nowhere until [`Canvases::keeping`].
    pub fn new(daemon: Client) -> Self {
        let (outbox, slinger) = (Outbox::start(daemon.clone()), Slinger::start(daemon.clone()));
        let (ink, select, sling) = (Ink::named(run()), Select::default(), Sling::default());
        let (open, views) = (Vec::new(), Vec::new());
        let (shapes, asks, turning) = (Shapes::default(), Pages::default(), Vec::new());
        let (pen, unshown, lives) = (None, None, live::Lives::start(daemon.clone()));
        let voice = dictation::Voice::new(Flick::socket());
        let notes = feedback::Notes::new(daemon.clone());
        Self {
            daemon,
            open,
            views,
            ink,
            select,
            shapes,
            outbox,
            sling,
            slinger,
            slung: Slung::default(),
            prompt: crate::config::Prompt::default().text().to_string(),
            voice,
            notes,
            pen,
            unshown,
            asks,
            turning,
            lives,
            agents: None,
            zoomed: None,
            paper: Paper::default(),
            kept: None,
            handle: None,
        }
    }

    /// Follows every canvas, and sends ink not yet sent, through `daemon`
    /// instead, from the start.
    pub fn reconnect(&mut self, daemon: Client) {
        self.outbox.reconnect(daemon.clone());
        self.slinger.reconnect(daemon.clone());
        self.notes.reconnect(daemon.clone());
        self.lives = live::Lives::start(daemon.clone());
        self.daemon = daemon;
        self.open.clear();
    }

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
                    News::Shown(page) => {
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
        let (sent, dictated, came) = (self.send(), self.poll_dictation(), self.lives.poll());
        self.poll_sling() || sent || changed || dictated || came
    }

    /// Takes the daemon's answers to the ink (a pane that asked for a new
    /// page turns to it), lets go of the changes every page followed of
    /// their canvas now holds, and hands the next change to the outbox.
    /// Returns whether there were answers.
    fn send(&mut self) -> bool {
        let answers = self.outbox.answers();
        let answered = !answers.is_empty();
        for (key, answer) in answers {
            if let Some(at) = self.turning.iter().position(|(k, ..)| *k == key) {
                if let Answer::NewPage { page, .. } = &answer {
                    let (_, pane, canvas) = self.turning.remove(at);
                    self.view(pane).page = Some((canvas, page.clone()));
                } else if matches!(answer, Answer::Refused(_)) {
                    self.turning.remove(at);
                }
            }
            if let Answer::Made { ids, .. } = &answer {
                self.select.made(key, ids);
            }
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
            self.outbox.send(key, self.ink.once(key), change);
        }
        self.keep();
        answered
    }

    /// What is wrong with any canvas followed, or with sending the ink.
    pub fn problems(&self) -> impl Iterator<Item = &Problem> {
        let kept = self.kept.as_ref().and_then(kept::Kept::problem);
        self.open.iter().filter_map(|o| o.problem.as_ref()).chain(self.ink.problem()).chain(kept)
    }

    /// The pen went down in `pane` of `panes`, laid at `frame` in the
    /// window, with `tool` and `colour`: a press closes a page command's
    /// question, and any open entry (the sling's, the feedback's) unless it
    /// taps a button, which sends the sling's; a press outside a text being typed
    /// ends it, and only the text tool's tap does nothing more; the pen
    /// starts a stroke on the page the pane shows, the eraser an erase,
    /// the lasso a loop or a drag of the selection, the select tool a pick,
    /// a box or a drag of the selection ([`Select::pick`]), the text tool a
    /// text, the box and arrow tools a drag. Returns false, and nothing starts, for touch (a palm), a pane whose canvas page has
    /// not arrived, or a press on a live item's page outside the selection.
    /// A press on a button outside the selection slings ([`button`]).
    pub fn pen_down(
        &mut self,
        panes: &Panes,
        (pane, frame): (PaneId, Rect),
        pen: Pen,
        (tool, colour): (Tool, Rgb),
    ) -> bool {
        let Some(canvas) = canvas_of(panes, pane).filter(|_| !pen.touch) else { return false };
        self.asks.cancel();
        let open = find(&self.open, canvas, wanted(&self.views, pane, canvas));
        let Some(shown) = open.and_then(|o| o.mirror.as_ref()).map(|m| m.page().to_string()) else {
            self.close_entries();
            return false;
        };
        let viewport = self.view(pane).viewport;
        let at = to_canvas(viewport, frame, pen.at);
        let items = pane_shown(&self.open, &self.views, &self.ink, panes, pane);
        let press = Press { at, when: pen.when, zoom: viewport.zoom() };
        let here = (canvas, shown.as_str());
        let grabs = self.select.grabs(here, at, viewport.zoom(), &items);
        if let Some(button) = button::under(&items, at).filter(|_| !grabs) {
            drop(items);
            return self.tap(here, button, viewport, frame);
        }
        drop(items);
        self.close_entries();
        let items = pane_shown(&self.open, &self.views, &self.ink, panes, pane);
        if self.lives.under(&items, at, viewport.zoom()) && !grabs {
            return false;
        } else if self.shapes.down((tool, colour), here, press, &items, &mut self.ink) {
            drop(items);
            self.send();
        } else if tool == Tool::Pen {
            self.ink.down(canvas, &shown, colour, at);
        } else if tool == Tool::Select {
            self.select.pick(here, at, viewport.zoom(), pen.shift, &items);
        } else if !self.select.down(tool, here, at, viewport.zoom(), &items) {
            return false;
        }
        self.pen = Some((pane, frame));
        self.unshown = Some(pen.when);
        true
    }

    /// Whether a text is being typed, or a page command asks: the keys
    /// that type go to it.
    pub fn editing(&self) -> bool {
        self.shapes.editing() || self.asks.asking()
    }

    /// Typed `text` answers a page command's question, or goes into the
    /// text being typed; escape ends it, and the text goes to the daemon.
    /// Returns false when neither is open.
    pub fn typed(&mut self, text: &str) -> bool {
        // The sling's open entry has the keys: its text is not the canvas's.
        if self.typing() {
            return false;
        }
        if self.asks.asking() {
            if let Some((canvas, op)) = self.asks.typed(text) {
                self.change_pages(canvas, op);
            }
            return true;
        }
        let used = self.shapes.typed(text, &mut self.ink);
        if used {
            self.send();
        }
        used
    }

    /// Pasted `text` goes where typed text goes, as text, not keys. False
    /// when nothing takes it (the sling's entry pastes for itself).
    pub fn pasted(&mut self, text: &str) -> bool {
        !self.typing() && (self.asks.pasted(text) || self.shapes.pasted(text))
    }

    /// The selection, if any: one page's items, by the server's ids.
    pub fn selection(&self) -> Option<&Selection> {
        self.select.selection()
    }

    /// Whether `pane` of `panes` shows the selection's page.
    fn shows_selection(&self, panes: &Panes, pane: PaneId) -> bool {
        let Some(s) = self.selection() else { return false };
        let asked = wanted(&self.views, pane, &s.canvas);
        let open = find(&self.open, &s.canvas, asked).and_then(|o| o.mirror.as_ref());
        let showing = asked.or(open.map(Mirror::page));
        canvas_of(panes, pane) == Some(&s.canvas) && showing == Some(&s.page)
    }

    /// Removes the selection, in one step, when `pane` of `panes` shows its
    /// page. Returns false when it does not, or there is none.
    pub fn delete_selection(&mut self, panes: &Panes, pane: PaneId) -> bool {
        let shown = self.shows_selection(panes, pane);
        shown && self.select.delete(&mut self.ink) && {
            self.send();
            true
        }
    }

    /// Clears the selection when `pane` of `panes` shows its page.
    /// Returns false when it does not, or there is none.
    pub fn clear_selection(&mut self, panes: &Panes, pane: PaneId) -> bool {
        let shown = self.shows_selection(panes, pane);
        shown && self.select.clear()
    }

    /// A paint showed ink `delay` seconds after its pen event.
    pub fn painted(&mut self, delay: f64) {
        self.ink.painted(delay);
    }

    /// The last stroke's pen-to-paint delays as one line (SPEC O2), once.
    pub fn delay_log(&mut self) -> Option<String> {
        self.ink.delay_log()
    }

    /// Drags `pane`'s canvas by `dx`, `dy` screen points.
    pub fn pan(&mut self, pane: PaneId, dx: f64, dy: f64) {
        let view = self.view(pane);
        view.viewport = view.viewport.pan(dx, dy);
    }

    fn view(&mut self, pane: PaneId) -> &mut View {
        let at = self.views.iter().position(|(p, _)| *p == pane).unwrap_or_else(|| {
            self.views.push((pane, View::default()));
            self.views.len() - 1
        });
        &mut self.views[at].1
    }

    /// The scene of each canvas pane of `panes` laid in `area` whose page
    /// has arrived; a pane turned to a page still on its way keeps what it
    /// drew. Starts following each page a pane shows, stops following the
    /// others, forgets the views of panes that closed, and clears a
    /// selection no pane shows. Each pane's pointer is the arrow: the
    /// window decides it from the pen.
    pub fn show(&mut self, panes: &Panes, area: Rect, theme: &Theme) -> Vec<CanvasPane> {
        // A canvas pane's canvas is what its footer leaves of it.
        let placed: Vec<_> = (panes.place(area).into_iter())
            .filter(|(_, p, _)| p.kind == Kind::Canvas)
            .map(|(id, p, frame)| (id, p, crate::scene::status::content(frame)))
            .collect();
        let shown: Vec<(&str, Option<String>)> = placed
            .iter()
            .map(|(id, p, _)| {
                (p.name.as_str(), wanted(&self.views, *id, &p.name).map(str::to_string))
            })
            .collect();
        for o in self.open.iter().filter(|o| !shown.iter().any(|(c, _)| *c == o.canvas)) {
            // No feed of it comes now: its strokes already written are let go.
            self.ink.heard(&o.canvas, u64::MAX);
        }
        self.open.retain(|o| shown.iter().any(|(c, p)| matches(o, c, p.as_deref())));
        for (canvas, page) in &shown {
            if find(&self.open, canvas, page.as_deref()).is_none() {
                let follower =
                    Follower::start(self.daemon.clone(), (*canvas).to_string(), page.clone());
                let (canvas, asked) = ((*canvas).to_string(), page.clone());
                let (mirror, problem, file) = (None, None, None);
                self.open.push(Open { canvas, asked, follower, mirror, heard: 0, problem, file });
            }
        }
        self.views.retain(|(id, _)| placed.iter().any(|(p, _, _)| p == id));
        self.clear_hidden_selection(&shown);
        let (mut scenes, grid) = (Vec::new(), self.paper.grid(theme));
        for (id, pane, frame) in placed {
            let open = find(&self.open, &pane.name, wanted(&self.views, id, &pane.name));
            let mirror = open.and_then(|o| o.mirror.as_ref());
            let screen = Size { width: frame.width, height: frame.height };
            let at = self.views.iter().position(|(p, _)| *p == id);
            let at = at.unwrap_or_else(|| {
                self.views.push((id, View::default()));
                self.views.len() - 1
            });
            let view = &mut self.views[at].1;
            let Some(mirror) = mirror else {
                if let Some(last) = &view.last {
                    scenes.push(CanvasPane {
                        pane: id,
                        frame,
                        scene: Rc::clone(last),
                        cursor: Cursor::Arrow,
                    });
                }
                continue;
            };
            let last = view.last.as_deref();
            let here = (pane.name.as_str(), mirror.page());
            let items = self.ink.shown(here.0, here.1, mirror.items());
            self.select.keep(here, &items);
            let zoom = view.viewport.zoom();
            let items = self.select.shown(here, items, zoom, theme);
            let items = self.shapes.shown(here, items, zoom, theme);
            let status = slung::agent_in(self.agents.as_deref(), here.0).and_then(|a| a.status);
            let items = button::shown(items, status);
            let items = items.iter().map(AsRef::as_ref);
            let mut scene = canvas::build(items, view.viewport, screen, theme, last).on(grid, last);
            if self.pen.is_some_and(|(p, _)| p == id) {
                scene.pen_event = self.unshown.take();
            }
            let scene = Rc::new(scene);
            view.last = Some(Rc::clone(&scene));
            scenes.push(CanvasPane { pane: id, frame, scene, cursor: Cursor::Arrow });
        }
        scenes
    }

    /// Clears the selection when no pane in `shown` (each a canvas and the
    /// page it asked for) shows its page. A pane whose first page has not
    /// arrived may show it.
    fn clear_hidden_selection(&mut self, shown: &[(&str, Option<String>)]) {
        let Some(s) = self.select.selection() else { return };
        let maybe = shown.iter().any(|(canvas, asked)| {
            let open = find(&self.open, canvas, asked.as_deref()).and_then(|o| o.mirror.as_ref());
            let page = asked.as_deref().or(open.map(Mirror::page));
            *canvas == s.canvas && page.is_none_or(|p| p == s.page)
        });
        if !maybe {
            self.select.clear();
        }
    }
}

#[cfg(test)]
mod tests;
