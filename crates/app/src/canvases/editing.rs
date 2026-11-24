//! What the pen and the keys change on the canvas panes, and its way to
//! the daemon. The pen's strokes show over the page at once ([`Ink`]) and
//! go to the daemon through an [`Outbox`], in the order made; the eraser,
//! the lasso and the select tool work on the page as shown ([`Select`]),
//! and so do the text, box and arrow tools ([`Shapes`]) and the crop tool
//! ([`Crop`], whose snippet goes to the clipboard, not the canvas); what
//! they change goes the same way, in turn with the strokes. The changes
//! not yet written are kept on disk ([`kept`]). [`Editing`] hides all of
//! these, with the gesture under way ([`pen`]), the selection's and the
//! text's keys ([`keys`]), a text started from the keyboard ([`start`]) and
//! what a context click is on ([`context`]). It starts the outbox's thread.
//!
//! Each change is made on the page a pane shows, which is
//! [`Follow`]'s: Editing is handed it where it needs one, and hands Follow
//! the ink where what the pen adds joins a box or a page let go needs it.
//! Runs on the main thread and never waits.

mod context;
pub(super) mod kept;
mod keys;
mod pen;
mod start;
mod vim;

use std::borrow::Cow;

use slinger_control::Client;

use super::follow::Follow;
use super::{Open, find};
use crate::backend::{Outbox, Problem, Wake, run};
use crate::commands::{Command, Keymap};
use crate::config::Theme;
use crate::layout::{PaneId, Point, Rect};
use crate::mirror::Mirror;
use crate::panes::{Pane, Panes};
use crate::scene::canvas::{Item, ItemId};
use crate::scene::crop::Crop;
use crate::scene::cursor::Cursor;
use crate::scene::ink::{Answer, Change, Ink};
use crate::scene::select::{Select, Selection};
use crate::scene::shapes::Shapes;
use crate::scene::tools::Tools;

/// The ink, the selection, the shapes and the crop, and their way to the
/// daemon.
pub(super) struct Editing {
    ink: Ink,
    select: Select,
    shapes: Shapes,
    crop: Crop,
    outbox: Outbox,
    /// Where the strokes not yet written are kept, if anywhere.
    kept: Option<kept::Kept>,
    /// The pane the pen draws in, and its frame in the window, while down.
    pen: Option<(PaneId, Rect)>,
    /// When the oldest pen event no scene has shown yet happened.
    unshown: Option<f64>,
    /// The pane the pointer is on a selection handle in, and its cursor.
    handle: Option<(PaneId, Cursor)>,
    /// The Nvim that edits the text being typed, if one does ([`vim`]).
    vim: vim::Vim,
}

impl Editing {
    /// Nothing drawn yet: the ink goes to the daemon through `daemon` on
    /// the outbox's thread, whose news wakes the main thread with `wake`.
    /// Strokes not yet written are kept nowhere until [`Editing::keeping`].
    pub(super) fn start(daemon: &Client, wake: &Wake) -> Self {
        Self {
            ink: Ink::named(run()),
            select: Select::default(),
            shapes: Shapes::default(),
            crop: Crop::default(),
            outbox: Outbox::start(daemon.clone(), wake),
            kept: None,
            pen: None,
            unshown: None,
            handle: None,
            vim: vim::Vim::new(wake),
        }
    }

    /// Sends the ink not yet sent through `daemon` instead, from the start.
    pub(super) fn reconnect(&mut self, daemon: Client) {
        self.outbox.reconnect(daemon);
    }

    /// The ink: the pages as shown are the server's copy with it laid over.
    pub(super) fn ink(&self) -> &Ink {
        &self.ink
    }

    /// The ink, for a test to draw with.
    #[cfg(test)]
    pub(super) fn ink_mut(&mut self) -> &mut Ink {
        &mut self.ink
    }

    /// The outbox, for a test to send elsewhere.
    #[cfg(test)]
    pub(super) fn outbox(&mut self) -> &mut Outbox {
        &mut self.outbox
    }

    /// The selection, if any: one page's items, by the server's ids.
    pub(super) fn selection(&self) -> Option<&Selection> {
        self.select.selection()
    }

    /// Whether a text is being typed.
    pub(super) fn text_open(&self) -> bool {
        self.shapes.editing()
    }

    /// Whether characters are selected in the text being typed (Nvim's
    /// visual mode, while Nvim edits it).
    pub(super) fn text_selects(&self) -> bool {
        self.vim.selects().unwrap_or_else(|| self.shapes.selects())
    }

    /// The canvas and page of the text being typed, if any.
    pub(super) fn typing_on(&self) -> Option<(&str, &str)> {
        self.shapes.typing_on()
    }

    /// The pen's line width and a new text's size, as the footers show them.
    pub(super) fn pen_sizes(&self) -> (f64, f64) {
        (self.ink.width(), self.shapes.size())
    }

    /// Takes the pen's line width and a new text's size from `tools`.
    pub(super) fn set_pen(&mut self, tools: &Tools) {
        self.ink.set_width(tools.width().points());
        self.shapes.set_size(f64::from(tools.size()));
    }

    /// Follows `keys`: the hint under a typed text names `new-text`'s key.
    pub(super) fn follow_keys(&mut self, keys: &Keymap) {
        self.shapes.set_next(keys.chord(Command::NewText));
    }

    /// Whether every change made to `canvas` is written.
    pub(super) fn settled(&self, canvas: &str) -> bool {
        self.ink.settled(canvas)
    }

    /// What sending the ink went wrong with, if anything.
    pub(super) fn problem(&self) -> Option<&Problem> {
        self.ink.problem()
    }

    /// What keeping the unsent ink in its file went wrong with, if anything.
    pub(super) fn kept_problem(&self) -> Option<&Problem> {
        self.kept.as_ref().and_then(kept::Kept::problem)
    }

    /// A paint showed ink `delay` seconds after its pen event.
    pub(super) fn painted(&mut self, delay: f64) {
        self.ink.painted(delay);
    }

    /// The last stroke's pen-to-paint delays as one line (SPEC O2), once.
    pub(super) fn delay_log(&mut self) -> Option<String> {
        self.ink.delay_log()
    }

    /// The panes now are another workspace's: what was under way in a pane
    /// of the one left ends. A stroke is kept; the rest is let go.
    pub(super) fn switched(&mut self) {
        if self.pen.take().is_some() {
            self.ink.up(None);
            self.unshown = None;
        }
        self.handle = None;
    }

    /// Ends the text being typed: it goes to the ink, with what its Nvim
    /// holds once it has taken the keys sent. False when none is.
    pub(super) fn end_text(&mut self) -> bool {
        self.vim.settle(&mut self.shapes);
        self.shapes.end(&mut self.ink)
    }

    /// Only `SLINGER_NVIM` turns Nvim on from now on ([`vim::Vim::driven`]).
    pub(super) fn driven(&mut self) {
        self.vim.driven();
    }

    /// Follows the config's `[editor] nvim`: which Nvim edits a text.
    pub(super) fn follow_editor(&mut self, nvim: &crate::config::Nvim) {
        self.vim.follow(nvim);
    }

    /// Whether an Nvim edits the texts typed from now on.
    pub(super) fn nvim_on(&self) -> bool {
        self.vim.on()
    }

    /// Why no Nvim edits the text, once.
    pub(super) fn take_flash(&mut self) -> Option<String> {
        self.vim.take_flash()
    }

    /// Takes the news of the text's Nvim: a text it ended goes to the
    /// daemon. Returns whether the text changed or ended.
    pub(super) fn poll_vim(&mut self, follow: &mut Follow) -> bool {
        let changed = self.vim.poll(&mut self.shapes, &mut self.ink);
        if changed && !self.shapes.editing() {
            self.send(follow);
        }
        changed
    }

    /// Undoes the text being typed's last edit, or redoes the last undone.
    /// False when none is typed, or it has none left.
    /// While Nvim edits the text, its undo and redo, always true.
    pub(super) fn undo_text(&mut self, redo: bool) -> bool {
        self.vim.undo(redo, &self.shapes) || self.shapes.undo(redo)
    }

    /// Copies the characters selected in the text being typed; with `cut`,
    /// takes them out of it. While Nvim edits the text, Nvim puts them on
    /// the clipboard itself, and this is `None`.
    pub(super) fn copy_text(&mut self, cut: bool) -> Option<String> {
        if self.vim.copy(cut, &self.shapes) {
            return None;
        }
        self.shapes.copy(cut)
    }

    /// Queues `change` in its turn.
    pub(super) fn push(&mut self, change: Change) {
        self.ink.push(change);
    }

    /// Queues `change` in its turn, under the key it returns.
    pub(super) fn push_keyed(&mut self, change: Change) -> u64 {
        self.ink.push_keyed(change)
    }

    /// Lets go of the selection. False when there was none.
    pub(super) fn deselect(&mut self) -> bool {
        self.select.clear()
    }

    /// Selects item `id` alone on `place`, a canvas and its page.
    pub(super) fn select_only(&mut self, place: (&str, &str), id: ItemId) {
        self.select.only(place, id);
    }

    /// The selection's box among `items`, the page as a pane shows it.
    pub(super) fn selection_bounds(&self, items: &[Cow<'_, Item>]) -> Option<Rect> {
        self.select.bounds(items)
    }

    /// Whether a press at `at` on `here`, at `zoom`, grabs the selection.
    pub(super) fn grabs(
        &self,
        here: (&str, &str),
        at: Point,
        zoom: f64,
        items: &[Cow<'_, Item>],
    ) -> bool {
        self.select.grabs(here, at, zoom, items)
    }

    /// Takes in what each pane of `placed` shows ([`Follow::follow`]): the
    /// strokes already written of a canvas no pane shows now are let go.
    pub(super) fn follow<'a>(
        &mut self,
        follow: &mut Follow,
        panes: &Panes,
        placed: &[(PaneId, &'a Pane, Rect)],
    ) -> Vec<(&'a str, Option<String>)> {
        follow.follow(panes, placed, &mut self.ink)
    }

    /// Clears the selection when no pane in `shown` (each a canvas and the
    /// page it asked for) shows its page. A pane whose first page has not
    /// arrived may show it.
    pub(super) fn clear_hidden(&mut self, open: &[Open], shown: &[(&str, Option<String>)]) {
        let Some(s) = self.select.selection() else { return };
        let maybe = shown.iter().any(|(canvas, asked)| {
            let open = find(open, canvas, asked.as_deref()).and_then(|o| o.mirror.as_ref());
            let page = asked.as_deref().or(open.map(Mirror::page));
            *canvas == s.canvas && page.is_none_or(|p| p == s.page)
        });
        if !maybe {
            self.select.clear();
        }
    }

    /// Keeps of the selection on `here` only what `items` holds.
    pub(super) fn keep_selection(&mut self, here: (&str, &str), items: &[Cow<'_, Item>]) {
        self.select.keep(here, items);
    }

    /// `items`, the page `here` as a pane shows it at `zoom`, with the
    /// selection, the shape being drawn or typed and the crop's box over
    /// it, in `theme`'s colours.
    pub(super) fn drawn<'a>(
        &self,
        here: (&str, &str),
        items: Vec<Cow<'a, Item>>,
        zoom: f64,
        theme: &Theme,
    ) -> Vec<Cow<'a, Item>> {
        // The selection's marks are not drawn over a text being typed.
        let typing = self.shapes.typing_on() == Some(here);
        let items = if typing { items } else { self.select.shown(here, items, zoom, theme) };
        let items = self.shapes.shown(here, items, zoom, theme);
        self.crop.shown(here, items, zoom, theme)
    }

    /// When the oldest pen event no scene has shown yet happened, once,
    /// for a scene of `pane` while the pen draws in it.
    pub(super) fn pen_event(&mut self, pane: PaneId) -> Option<f64> {
        if self.pen.is_some_and(|(p, _)| p == pane) { self.unshown.take() } else { None }
    }

    /// Folds each box of `boxes` on `here`, a page of a canvas, or unfolds
    /// it when it says so, each one undo step; an unfold whose room is
    /// taken peeks in `pane` ([`Follow::unfolding`]). False for none.
    pub(super) fn fold(
        &mut self,
        follow: &mut Follow,
        pane: PaneId,
        here: (&str, &str),
        boxes: &[(ItemId, bool)],
    ) -> bool {
        for &(id, open) in boxes {
            let (canvas, on) = (here.0.to_string(), here.1.to_string());
            let fold = Change::Fold { canvas: canvas.clone(), page: on.clone(), id, open };
            let key = self.ink.push_keyed(fold);
            if open {
                follow.unfolding(key, pane, (canvas, on), id);
            }
        }
        !boxes.is_empty() && {
            self.send(follow);
            true
        }
    }

    /// Takes the daemon's answers to the ink (a pane that asked for a new
    /// page turns to it), lets go of the changes every page `follow`
    /// follows of their canvas now holds, and hands the next change to the
    /// outbox. Returns whether there were answers.
    pub(super) fn send(&mut self, follow: &mut Follow) -> bool {
        self.vim.sync(&self.shapes);
        if !self.shapes.editing() {
            follow.text_joined(&mut self.ink);
        }
        let answers = self.outbox.answers();
        let answered = !answers.is_empty();
        for (key, answer) in answers {
            follow.turned(key, &answer);
            if let Answer::Made { ids, .. } = &answer {
                self.select.made(key, ids);
            }
            follow.fold_answered(key, &answer);
            let renamed = self.ink.answered(key, answer);
            self.select.renamed(&renamed);
        }
        // Every page of a canvas hears the same numbers: the slowest decides.
        for (canvas, upto) in follow.heard() {
            self.ink.heard(canvas, upto);
        }
        if let Some((key, change)) = self.ink.next() {
            self.outbox.send(key, self.ink.once(key), change, self.ink.joins(key));
        }
        self.keep(follow);
        answered
    }
}
