//! The canvases the panes show. [`Canvases`] coordinates three owners and
//! keeps nothing of theirs:
//!
//! - [`follow`] (`Follow`) hides what each pane shows: each page a pane
//!   shows is followed once, however many panes show it (a [`Follower`]
//!   on its own thread, and the [`Mirror`] its news keeps up to date), each
//!   canvas pane's view and last scene, and the threads that fetch the live
//!   items' pages and the PDFs and keep the views.
//! - [`editing`] (`Editing`) hides what the pen and the keys change: the
//!   [`Ink`] that shows over the page at once and goes to the daemon
//!   through an [`Outbox`] (whose thread it starts), the selection, the
//!   text, box and arrow tools, the crop, the gesture under way and the
//!   file the changes not yet written are kept in. It is handed the Follow
//!   where a change needs a page; it hands Follow the ink.
//! - [`flow`] (`SlingFlow`) hides the sling ([`sling`]), what became of it
//!   ([`slung`]), its spoken prompt, the feedback entry and the agents
//!   slings go to; it is handed the pages and the ink.
//!
//! Canvases holds the rest: the daemon, the waker, a page command's
//! question, the last find, the flash and the grid. It starts no thread
//! itself. Runs on the main thread and never waits: the followers, the
//! outbox, the slinger and Flick's thread do.
//!
//! Page commands, undo and redo are in [`paging`]; the keys in [`keys`];
//! the pen's drag and lift and the pointer in [`drag`]; cut, copy and paste
//! in [`clip`]; finding text on a canvas's pages in [`seek`]; folding boxes
//! in [`fold`] and stepping into them in [`step`]; a PDF's canvas in reader
//! mode, in [`reader`].
//!
//! [`Ink`]: crate::scene::ink::Ink
//! [`Outbox`]: crate::backend::Outbox

mod button;
mod clip;
mod drag;
mod editing;
mod figure;
mod find;
mod fold;
mod follow;
mod kept;
mod keys;
mod news;
mod paging;
mod problems;
mod reader;
mod seek;
mod start;
mod step;
mod view;

use std::borrow::Cow;
use std::path::PathBuf;
use std::rc::Rc;

use slinger_control::Client;

use crate::backend::{Follower, Problem, Wake};
use crate::config::{Rgb, Theme};
use crate::layout::{PaneId, Point, Rect, Size};
use crate::mirror::Mirror;
use crate::panes::{Kind, Panes};
use crate::scene::cursor::Cursor;
use crate::scene::grid::Paper;
use crate::scene::ink::Pen;
use crate::scene::pages::Pages;
use crate::scene::shapes::Press;
use crate::scene::tools::Tool;
use crate::scene::{CanvasPane, Notes, Scene, canvas};
use crate::trace::Paged;
use editing::Editing;
use find::{canvas_of, find, to_canvas, wanted};
use flow::SlingFlow;
use follow::Follow;
use view::View;

mod feedback;
mod flow;
mod profile;
mod sling;
mod slung;
mod status;

/// The canvases of a window's panes.
pub struct Canvases {
    daemon: Client,
    /// The pages the panes show, their views, and the threads that fetch
    /// them ([`follow`]).
    follow: Follow,
    /// The ink, the selection, the shapes and the crop, and the outbox
    /// that takes their changes to the daemon ([`editing`]).
    editing: Editing,
    /// The sling, what became of it, its dictation, the feedback entry and
    /// the agents slings go to ([`flow`]).
    flow: SlingFlow,
    /// The question a page command asks, if any.
    asks: Pages,
    /// The last find, for `find-next`.
    found: Option<seek::Last>,
    /// What to flash once: a pane's new zoom ("Zoom 150%"), the grid's
    /// toggle or where `save` said the canvas went.
    zoomed: Option<String>,
    /// Whether every canvas pane shows the dotted grid.
    paper: Paper,
    /// Wakes the main thread when a backend thread has news.
    wake: Wake,
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
        Self {
            editing: Editing::start(&daemon, &wake),
            flow: SlingFlow::start(&daemon, &wake),
            follow: Follow::start(&daemon, &wake),
            asks: Pages::default(),
            found: None,
            zoomed: None,
            paper: Paper::default(),
            wake,
            daemon,
        }
    }

    /// Follows every canvas, and sends ink not yet sent, through `daemon`
    /// instead, from the start.
    pub fn reconnect(&mut self, daemon: Client) {
        self.editing.reconnect(daemon.clone());
        self.flow.reconnect(&daemon);
        self.follow.reconnect(daemon.clone());
        self.daemon = daemon;
    }

    /// The pen went down in `pane` of `panes`, laid at `frame` in the
    /// window, with `tool` and `colour`: a press closes a page command's
    /// question, and any open entry (the sling's, the feedback's) and an
    /// ended sling's pill unless it taps a button, which sends the
    /// sling's; a press outside a text being typed
    /// ends it, and only the text tool's tap does nothing more; the pen
    /// starts a stroke on the page the pane shows, the eraser an erase,
    /// the lasso a loop or a drag of the selection, the select tool a pick,
    /// a box or a drag of the selection ([`Select::pick`](crate::scene::select::Select::pick)), the text tool a
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
        let open = self.follow.shown(canvas, pane);
        let Some(shown) = open.and_then(|o| o.mirror.as_ref()).map(|m| m.page().to_string()) else {
            self.close_entries();
            return false;
        };
        let viewport = self.view(pane).viewport;
        let at = to_canvas(viewport, frame, pen.at);
        if self.follow.peek_press(pane, at) {
            return true;
        }
        let items = self.follow.pane_shown(self.editing.ink(), panes, pane);
        let press = Press { at, when: pen.when, zoom: viewport.zoom(), shift: pen.shift };
        let here = (canvas, shown.as_str());
        let mark =
            (tool != Tool::Eraser).then(|| self.mark_under(here, (at, viewport.zoom()), &items));
        if let Some(fold) = mark.flatten() {
            drop(items);
            return self.editing.fold(&mut self.follow, pane, here, &[fold]);
        }
        let taps = !matches!(tool, Tool::Pen | Tool::Eraser);
        let mini = taps.then(|| self.follow.miniature_under(here, at, &items)).flatten();
        let grabs = self.editing.grabs(here, at, viewport.zoom(), &items);
        if let Some(button) = button::under(&items, at).filter(|_| !grabs && tool != Tool::Eraser) {
            drop(items);
            return self.tap(here, button, viewport, frame);
        }
        drop(items);
        if self.tapped_in(pane, here, pen.when, mini) {
            return true;
        }
        self.close_entries();
        self.flow.pressed();
        let items = self.follow.pane_shown(self.editing.ink(), panes, pane);
        if self.follow.live_under(&items, at, viewport.zoom()) && !grabs {
            return false;
        }
        drop(items);
        self.editing.press(&mut self.follow, panes, (pane, frame), here, press, (tool, colour))
    }

    /// Puts the page of each live item `scene`'s canvas panes draw over it,
    /// with its note from `notes` in `theme`'s colours ([`follow`]).
    pub fn show_live(&mut self, scene: &mut Scene, notes: &Notes, theme: &Theme) {
        self.follow.show_live(scene, notes, theme);
    }

    /// Opens the PDF `file` as a new canvas, its pages' sizes read by
    /// `sizes`, beside the focused pane of `panes` in `workspace`, on a
    /// thread of its own ([`follow`]).
    pub fn open_pdf(
        &mut self,
        panes: &Panes,
        workspace: Option<&str>,
        file: PathBuf,
        sizes: crate::backend::pdf::Pages,
    ) {
        self.follow.open_pdf(panes, workspace, file, sizes);
    }

    /// A paint showed ink `delay` seconds after its pen event.
    pub fn painted(&mut self, delay: f64) {
        self.editing.painted(delay);
    }

    /// The last stroke's pen-to-paint delays as one line (SPEC O2), once.
    pub fn delay_log(&mut self) -> Option<String> {
        self.editing.delay_log()
    }

    /// Drags `pane`'s canvas by `dx`, `dy` screen points.
    pub fn pan(&mut self, pane: PaneId, dx: f64, dy: f64) {
        self.view(pane).pan(dx, dy);
    }

    fn view(&mut self, pane: PaneId) -> &mut View {
        self.follow.view(pane)
    }

    /// The panes now are another workspace's: what was under way in a pane
    /// of the one left ends. A stroke is kept; the rest is let go.
    fn switched(&mut self) {
        self.editing.switched();
        self.asks.cancel();
    }

    /// The scene of each canvas pane of `panes` laid in `area` whose page
    /// has arrived; a pane turned to a page of its canvas still on its way
    /// keeps what it drew, and one whose canvas or workspace changed draws
    /// nothing until its page comes ([`view`]). Starts following each page
    /// a pane shows, stops following the others, forgets the views of
    /// panes that closed, and clears a selection no pane shows. Each pane's
    /// pointer is the arrow: the window decides it from the pen.
    pub fn show(&mut self, panes: &Panes, area: Rect, theme: &Theme) -> Vec<CanvasPane> {
        if self.follow.within(panes.workspace()) {
            self.switched();
        }
        // A canvas pane's canvas is what its footer leaves of it.
        let placed: Vec<_> = (panes.place(area).into_iter())
            .filter(|(_, p, _)| p.kind == Kind::Canvas)
            .map(|(id, p, frame)| (id, p, crate::scene::status::content(frame)))
            .collect();
        let shown = self.editing.follow(&mut self.follow, panes, &placed);
        self.editing.clear_hidden(self.follow.open(), &shown);
        // A selection may hold hidden items on a page a pane is inside a box of.
        let inside = self.follow.insides();
        let (mut scenes, grid) = (Vec::new(), self.paper.grid(theme));
        for (id, pane, frame) in placed {
            let follow::Showing { mirror, peek: marks, view, pdfs } =
                self.follow.showing(id, &pane.name);
            let screen = Size { width: frame.width, height: frame.height };
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
            let (items, within, holds) = step::shown_in(mirror, self.editing.ink(), here.0, &trail);
            view.keep_steps(here, holds);
            view.place(step::content(&items, within.as_ref()), screen);
            if inside.iter().any(|(c, p)| (c.as_str(), p.as_str()) == here) {
                let present = step::present(mirror, self.editing.ink(), here.0);
                self.editing.keep_selection(here, &present);
            } else {
                self.editing.keep_selection(here, &items);
            }
            if let Some(room) = within.as_ref().map(|w| w.room) {
                view.viewport = view.viewport.held(Point { x: room.x, y: room.y });
            }
            let last = view.last.as_deref();
            let zoom = view.viewport.zoom();
            let items = self.editing.drawn(here, items, zoom, theme);
            let status = self.flow.agent_of(here.0).and_then(|a| a.status);
            let items = button::shown(items, status);
            let mut items = fold::drawn(items, mirror, marks, theme);
            items.extend(within.map(|w| Cow::Owned(step::room(w.room, theme.muted))));
            let items = items.iter().map(AsRef::as_ref);
            let files = |hash: &str| pdfs.file(hash);
            let scene = canvas::build_with(items, (view.viewport, screen), theme, last, &files);
            let mut scene = scene.on(grid, last);
            if let Some(event) = self.editing.pen_event(id) {
                scene.pen_event = Some(event);
            }
            let scene = Rc::new(scene);
            view.last = Some(Rc::clone(&scene));
            scenes.push(CanvasPane { pane: id, frame, scene, cursor: Cursor::Arrow });
        }
        self.follow.want(&scenes);
        self.follow.save(panes);
        scenes
    }
}

#[cfg(test)]
mod tests;
