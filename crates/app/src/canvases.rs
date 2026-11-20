//! The canvases the panes show. Each canvas a pane shows is followed once,
//! however many panes show it: a [`Follower`] on its own thread, and the
//! [`Mirror`] its news keeps up to date. Each canvas pane has its own
//! viewport, and keeps the last scene it drew, so the next one repaints only
//! what changed. The pen draws on a canvas pane: its [`Ink`] shows over the
//! page at once and goes to the daemon through an [`Outbox`]. The eraser and
//! the lasso work on the page as shown ([`Select`]), and so do the text, box
//! and arrow tools ([`Shapes`]) and the crop tool ([`Crop`], whose snippet goes
//! to the clipboard, not the canvas); what they change goes the same way, in turn
//! with the strokes. The sling sends the selection ([`sling`]) once the ink
//! before it is in, and what became of it shows on the canvas ([`slung`]);
//! its prompt can be spoken ([`dictation`]). Runs on the
//! main thread and never waits: the followers, the outbox, the slinger and
//! Flick's thread do.
//!
//! Each pane keeps its own page, followed once however many panes show it:
//! page commands, undo and redo are in [`paging`], live items' pages in [`live`];
//! typed text and the selection's keys in [`keys`]; cut, copy and paste in
//! [`clip`]; finding text on a canvas's pages in [`seek`]; the PDFs `pdf` items show, and opening
//! one, in [`pdf`]; a PDF's canvas in reader mode, in [`reader`].

mod button;
mod clip;
mod context;
mod drag;
mod find;
mod fold;
mod kept;
mod keys;
mod live;
mod looks;
mod news;
mod paging;
mod pdf;
mod problems;
mod reader;
mod seek;
mod start;
mod step;
mod view;

use std::borrow::Cow;
use std::rc::Rc;

use slinger_control::Client;

use crate::backend::{Flick, Follower, Outbox, Problem, Saver, Slinger, Wake, run};
use crate::config::{Rgb, Theme};
use crate::layout::{PaneId, Point, Rect, Size};
use crate::mirror::Mirror;
use crate::panes::{Kind, Panes};
use crate::scene::crop::Crop;
use crate::scene::cursor::Cursor;
use crate::scene::grid::Paper;
use crate::scene::ink::{Ink, Pen};
use crate::scene::pages::Pages;
use crate::scene::select::Select;
use crate::scene::shapes::{Press, Shapes};
use crate::scene::sidebar::Agent;
use crate::scene::sling::Sling;
use crate::scene::slung::Slung;
use crate::scene::tools::Tool;
use crate::scene::{CanvasPane, canvas};
use crate::trace::Paged;
use find::{canvas_of, find, matches, pane_shown, to_canvas, wanted};
use view::{View, Views};

mod dictation;
mod feedback;
mod profile;
mod sling;
mod slung;
mod status;

/// The canvases of a window's panes.
pub struct Canvases {
    daemon: Client,
    open: Vec<Open>,
    views: Views,
    ink: Ink,
    select: Select,
    shapes: Shapes,
    crop: Crop,
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
    /// The last find, for `find-next`.
    found: Option<seek::Last>,
    /// The panes waiting for a new page, by the key of its `page.add`.
    turning: Vec<(u64, PaneId, String)>,
    lives: live::Lives,
    pdfs: pdf::Pdfs,
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
    /// The unfolds on their way, and the peek shown.
    folds: fold::Folds,
    /// The last tap on a miniature, and what the pen and the text being
    /// typed join ([`step`]).
    steps: step::Steps,
    /// Wakes the main thread when a backend thread has news.
    wake: Wake,
    /// Sends where the panes look to the daemon to keep ([`looks`]).
    saver: Saver,
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
    /// When its page came, for the switch's trace.
    paged: Option<Paged>,
}

impl Canvases {
    /// None followed yet, and news waits for the timer: a test's.
    #[cfg(test)]
    pub fn new(daemon: Client) -> Self {
        Self::waking(daemon, Wake::none())
    }

    /// None followed yet; [`Canvases::show`] starts following. Every
    /// backend thread's news wakes the main thread with `wake`. Strokes not
    /// yet written are kept nowhere until [`Canvases::keeping`].
    pub fn waking(daemon: Client, wake: Wake) -> Self {
        let (outbox, slinger) =
            (Outbox::start(daemon.clone(), &wake), Slinger::start(daemon.clone(), None, &wake));
        let (ink, select, sling) = (Ink::named(run()), Select::default(), Sling::default());
        let (open, views) = (Vec::new(), Views::default());
        let (shapes, asks, turning) = (Shapes::default(), Pages::default(), Vec::new());
        let (pen, unshown, lives) = (None, None, live::Lives::start(daemon.clone(), &wake));
        let pdfs = pdf::Pdfs::start(daemon.clone(), crate::backend::pdf::cache(), &wake);
        let voice = dictation::Voice::new(Flick::socket(), &wake);
        let notes = feedback::Notes::new(daemon.clone(), &wake);
        Self {
            open,
            views,
            ink,
            select,
            shapes,
            crop: Crop::default(),
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
            found: None,
            turning,
            lives,
            pdfs,
            agents: None,
            zoomed: None,
            paper: Paper::default(),
            kept: None,
            handle: None,
            folds: fold::Folds::default(),
            steps: step::Steps::default(),
            saver: Saver::start(daemon.clone()),
            wake,
            daemon,
        }
    }

    /// Follows every canvas, and sends ink not yet sent, through `daemon`
    /// instead, from the start.
    pub fn reconnect(&mut self, daemon: Client) {
        self.outbox.reconnect(daemon.clone());
        self.slinger.reconnect(daemon.clone());
        self.notes.reconnect(daemon.clone());
        self.saver.reconnect(daemon.clone());
        self.lives = live::Lives::start(daemon.clone(), &self.wake);
        self.pdfs = pdf::Pdfs::start(daemon.clone(), crate::backend::pdf::cache(), &self.wake);
        self.daemon = daemon;
        self.open.clear();
    }

    /// The pen went down in `pane` of `panes`, laid at `frame` in the
    /// window, with `tool` and `colour`: a press closes a page command's
    /// question, and any open entry (the sling's, the feedback's) and an
    /// ended sling's pill unless it taps a button, which sends the
    /// sling's; a press outside a text being typed
    /// ends it, and only the text tool's tap does nothing more; the pen
    /// starts a stroke on the page the pane shows, the eraser an erase,
    /// the lasso a loop or a drag of the selection, the select tool a pick,
    /// a box or a drag of the selection ([`Select::pick`]), the text tool a
    /// text, the box, arrow and crop tools a drag. Returns false, and nothing starts, for touch (a palm), a pane whose canvas page has
    /// not arrived, or a press on a live item's page outside the selection.
    /// A press on a button outside the selection slings ([`button`]),
    /// except the eraser's, which erases around it.
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
        if self.peek_press(pane, at) {
            return true;
        }
        let items = pane_shown(&self.open, &self.views, &self.ink, panes, pane);
        let press = Press { at, when: pen.when, zoom: viewport.zoom(), shift: pen.shift };
        let here = (canvas, shown.as_str());
        let mark =
            (tool != Tool::Eraser).then(|| self.mark_under(here, (at, viewport.zoom()), &items));
        if let Some(fold) = mark.flatten() {
            drop(items);
            return self.fold_boxes(pane, here, &[fold]);
        }
        let taps = !matches!(tool, Tool::Pen | Tool::Eraser);
        let mini = taps.then(|| self.miniature_under(here, at, &items)).flatten();
        let grabs = self.select.grabs(here, at, viewport.zoom(), &items);
        if let Some(button) = button::under(&items, at).filter(|_| !grabs && tool != Tool::Eraser) {
            drop(items);
            return self.tap(here, button, viewport, frame);
        }
        drop(items);
        if self.tapped_in(pane, here, pen.when, mini) {
            return true;
        }
        self.close_entries();
        self.slung.pressed();
        let items = pane_shown(&self.open, &self.views, &self.ink, panes, pane);
        if self.lives.under(&items, at, viewport.zoom()) && !grabs {
            return false;
        }
        drop(items);
        self.pen_in(pane, here);
        let items = pane_shown(&self.open, &self.views, &self.ink, panes, pane);
        if self.shapes.down((tool, colour), here, press, &items, &mut self.ink) {
            drop(items);
            self.typed_in(step::Typed::In(pane));
            self.send();
        } else if self.crop.down(tool, here, press, &items) {
            // Its box shows over the page; the lift makes the snippet.
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
        self.view(pane).pan(dx, dy);
    }

    fn view(&mut self, pane: PaneId) -> &mut View {
        self.views.get(pane)
    }

    /// The panes now are another workspace's: what was under way in a pane
    /// of the one left ends. A stroke is kept; the rest is let go.
    fn switched(&mut self) {
        if self.pen.take().is_some() {
            self.ink.up(None);
            self.unshown = None;
        }
        self.handle = None;
        self.asks.cancel();
        self.steps.switched();
    }

    /// The scene of each canvas pane of `panes` laid in `area` whose page
    /// has arrived; a pane turned to a page of its canvas still on its way
    /// keeps what it drew, and one whose canvas or workspace changed draws
    /// nothing until its page comes ([`view`]). Starts following each page
    /// a pane shows, stops following the others, forgets the views of
    /// panes that closed, and clears a selection no pane shows. Each pane's
    /// pointer is the arrow: the window decides it from the pen.
    pub fn show(&mut self, panes: &Panes, area: Rect, theme: &Theme) -> Vec<CanvasPane> {
        if self.views.within(panes.workspace()) {
            self.switched();
        }
        // A canvas pane's canvas is what its footer leaves of it.
        let placed: Vec<_> = (panes.place(area).into_iter())
            .filter(|(_, p, _)| p.kind == Kind::Canvas)
            .map(|(id, p, frame)| (id, p, crate::scene::status::content(frame)))
            .collect();
        for (id, pane, _) in &placed {
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
            self.ink.heard(&o.canvas, u64::MAX);
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
        self.clear_hidden_selection(&shown);
        // A selection may hold hidden items on a page a pane is inside a box of.
        let inside: Vec<(String, String)> =
            self.views.iter().filter_map(|(_, v)| v.trail.page()).collect();
        let (mut scenes, grid) = (Vec::new(), self.paper.grid(theme));
        for (id, pane, frame) in placed {
            let open = find(&self.open, &pane.name, wanted(&self.views, id, &pane.name));
            let mirror = open.and_then(|o| o.mirror.as_ref());
            let marks = mirror.and_then(|m| self.peek_in(id, (&*pane.name, m.page())));
            let screen = Size { width: frame.width, height: frame.height };
            let view = self.views.get(id);
            view.read(&pane.name, mirror.map(Mirror::items), screen);
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
            let here = (pane.name.as_str(), mirror.page());
            let trail = view.trail.on(here);
            let (items, within, holds) = step::shown_in(mirror, &self.ink, here.0, &trail);
            view.keep_steps(here, holds);
            view.place(step::content(&items, within.as_ref()), screen);
            if inside.iter().any(|(c, p)| (c.as_str(), p.as_str()) == here) {
                self.select.keep(here, &step::present(mirror, &self.ink, here.0));
            } else {
                self.select.keep(here, &items);
            }
            if let Some(room) = within.as_ref().map(|w| w.room) {
                view.viewport = view.viewport.held(Point { x: room.x, y: room.y });
            }
            let last = view.last.as_deref();
            let zoom = view.viewport.zoom();
            let items = self.select.shown(here, items, zoom, theme);
            let items = self.shapes.shown(here, items, zoom, theme);
            let items = self.crop.shown(here, items, zoom, theme);
            let status = slung::agent_in(self.agents.as_deref(), here.0).and_then(|a| a.status);
            let items = button::shown(items, status);
            let mut items = fold::drawn(items, mirror, marks, theme);
            items.extend(within.map(|w| Cow::Owned(step::room(w.room, theme.muted))));
            let items = items.iter().map(AsRef::as_ref);
            let files = |hash: &str| self.pdfs.file(hash);
            let scene = canvas::build_with(items, (view.viewport, screen), theme, last, &files);
            let mut scene = scene.on(grid, last);
            if self.pen.is_some_and(|(p, _)| p == id) {
                scene.pen_event = self.unshown.take();
            }
            let scene = Rc::new(scene);
            view.last = Some(Rc::clone(&scene));
            scenes.push(CanvasPane { pane: id, frame, scene, cursor: Cursor::Arrow });
        }
        self.pdfs.want(&scenes);
        self.save(panes);
        scenes
    }
}

#[cfg(test)]
mod tests;
