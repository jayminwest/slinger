//! The text, box and arrow tools (SPEC D6). Pure: `canvases` hands in pen
//! presses, drags and lifts in canvas units, typed text, and the page as the
//! user sees it ([`Ink::shown`]); each finished text or shape goes to the
//! [`Ink`] as one [`Change`], so one `canvas_apply` and one undo step.
//!
//! - Text: a tap places it and the keyboard types it ([`Shapes::typed`])
//!   at its caret ([`crate::scene::editor`]), which a tap that opens it
//!   leaves at its end; a paste goes in at the caret ([`Shapes::pasted`]);
//!   the caret keys move it ([`Shapes::key`]), and a press inside puts it
//!   there, a drag or a shift-press selecting (a double tap selects a
//!   word, a triple tap a row); escape or a press
//!   outside ends it. While it is typed it shows its caret and selection,
//!   a dashed frame around it out to the column it wraps in, whose right
//!   edge a drag moves ([`Editing::widen`]), and under that how
//!   to end it. A press outside with any other tool
//!   also starts that tool's gesture: the first gesture after a text is
//!   never lost. A tap with the text tool on a text,
//!   or a double tap on one with any tool but the pen, edits it. With the
//!   select tool or the lasso the second press may be a drag of what the
//!   first tap selected, so the edit waits for the lift, and starts only
//!   when the pen lifts where it went down ([`Shapes::tapped`]). The edit
//!   is one change when it ends: an `add`, a `set` (with the arrows bound
//!   to it when its frame moved), or a `remove` when it was emptied.
//!   A text also starts from the keyboard ([`Shapes::start`], [`start`]),
//!   or opens there as a tap opens it ([`Shapes::start_at`]).
//! - Box: a drag from corner to corner.
//! - Arrow: a drag from start to end. An end that lands on an item binds to
//!   it ([`target`]), and when that item moves the end follows ([`follow`]),
//!   in the same change as the move; after a resize the end keeps its
//!   place on the item's box ([`follow_by`], [`follow_frames`]).
//!
//! No snapping and no rotation (D6). A drag shorter than [`MIN_DRAG`] is a
//! tap and makes nothing.

use std::borrow::Cow;
use std::sync::Arc;

use crate::commands::Chord;
use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect};
use crate::mirror::{LINE_WIDTH, TEXT_SIZE};
use crate::nvim::grid::Grid;
use crate::scene::canvas::{Fill, Item, ItemId, Shape};
use crate::scene::editor::Editing;
use crate::scene::ink::{Change, Ink, on_server};
use crate::scene::paste;
use crate::scene::tools::{Resize, Tool};

mod bind;
mod cells;
mod label;
pub use bind::{follow, follow_by, follow_frames, target};
pub use label::{label, label_height};

/// Two presses this close in time, in seconds, are a double tap...
pub const DOUBLE_TAP: f64 = 0.4;
/// ...when this close in place, in screen points. An arrow's end binds to
/// an item it lands this close to, and a press this close to the text
/// being edited is inside it.
pub const REACH: f64 = 8.0;
/// The least drag, in screen points, that makes a box or an arrow.
pub const MIN_DRAG: f64 = 4.0;
/// The width of the caret in the text being edited, of the dashed frame
/// around it, and how far out the frame lies, in screen points.
const CARET: f64 = 2.0;
const FRAME_LINE: f64 = 1.0;
const FRAME_PAD: f64 = 4.0;
/// Under the text being edited, this far below its frame, in screen points.
const HINT: &str = "esc or a tap outside ends the text";
const HINT_GAP: f64 = 4.0;
/// The ids of the marks drawn over a page: above every server id and every
/// id of local ink, and apart from the selection's marks.
const PREVIEW: ItemId = ItemId(u64::MAX - 2);
const EDITING: ItemId = ItemId(u64::MAX - 3);
const CARET_ID: ItemId = ItemId(u64::MAX - 9);
const FRAME_ID: ItemId = ItemId(u64::MAX - 10);
const HINT_ID: ItemId = ItemId(u64::MAX - 11);
/// The first of the ids of the selection's boxes in the text being typed,
/// one a row, counting down; and how strongly they tint what is under them.
const SELECTED: u64 = u64::MAX - (1 << 16);
const SELECTED_ALPHA: f64 = 0.3;

/// One press of the pen: where on the canvas, when (seconds, the pen's
/// clock) and at what zoom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Press {
    pub at: Point,
    pub when: f64,
    pub zoom: f64,
    /// Whether shift was held: inside the text being typed, the selection
    /// runs to the press.
    pub shift: bool,
}

/// A box or an arrow under the pen, from where it went down to where it is.
#[derive(Debug, Clone, PartialEq)]
struct Drag {
    arrow: bool,
    from: Point,
    to: Point,
    colour: Rgb,
    /// The item the arrow's start landed on.
    start: Option<ItemId>,
    zoom: f64,
}

/// The box or arrow under the pen, the text being typed (and whether the
/// pen, down inside it, is selecting its characters, or, down on its
/// frame's right edge, how far right of the column's edge it went down),
/// the last press,
/// the edit a double tap with the select tool or the lasso asks for
/// once the pen lifts where it went down, within the reach there, and the
/// size a new text starts at ([`TEXT_SIZE`] until one is set), where
/// the last text ended, and the `new-text` key the hint names ([`start`]).
#[derive(Debug, Default)]
pub struct Shapes {
    drag: Option<(String, String, Drag)>,
    editing: Option<Editing>,
    ended: Option<(String, String, Rect)>,
    selecting: bool,
    widening: Option<(f64, f64)>,
    /// How many quick presses in a row landed inside the text being typed.
    taps: u8,
    last: Option<(String, String, Press)>,
    waiting: Option<(Editing, Point, f64)>,
    size: Option<f64>,
    next: Option<Chord>,
    /// How many texts have ended: another editor tells one text from the next by it.
    ends: u64,
    /// The grid an Nvim editing the text flushed last, shown in its place ([`cells`]).
    grid: Option<Arc<Grid>>,
}

impl Shapes {
    /// The size, in canvas units, the next text the text tool places starts
    /// at. A text being typed keeps its own.
    pub fn set_size(&mut self, size: f64) {
        self.size = Some(size);
    }

    /// The size, in canvas units, the next text the text tool places starts at.
    pub fn size(&self) -> f64 {
        self.size.unwrap_or(TEXT_SIZE)
    }

    /// The text being typed takes the size `resize` makes of its own, as
    /// a step of its undo ([`Editing::resize`]). False when none is.
    pub fn resize(&mut self, resize: Resize) -> bool {
        let Some(editing) = &mut self.editing else { return false };
        editing.resize(resize.of(editing.size));
        true
    }

    /// The pen went down on `page` of `canvas`, over `items`, with `tool`
    /// in `colour`. A press outside a text being typed ends it, to `ink`, then
    /// goes on as below; only the text tool's single tap is used up by
    /// ending it. Returns whether the press was this module's: it ends a
    /// text with the text tool, keeps typing (a press inside the text),
    /// edits a text, places one, or starts a box or an arrow. False for a
    /// press the pen, the eraser or the lasso goes on with.
    pub fn down(
        &mut self,
        (tool, colour): (Tool, Rgb),
        (canvas, page): (&str, &str),
        press: Press,
        items: &[Cow<'_, Item>],
        ink: &mut Ink,
    ) -> bool {
        let reach = REACH / press.zoom;
        self.waiting = None;
        let last = self.last.replace((canvas.to_string(), page.to_string(), press));
        let taps = std::mem::take(&mut self.taps);
        let double = last.is_some_and(|(c, p, last)| {
            c == canvas
                && p == page
                && press.when - last.when <= DOUBLE_TAP
                && distance(press.at, last.at) <= reach
        });
        let cells = self.grid_frame();
        if let Some(editing) = &mut self.editing {
            let here = editing.canvas == canvas && editing.page == page;
            // Inside Nvim's grid a press is Nvim's mouse, which `canvases` hands it first: the text stays.
            if let Some(cells) = cells.filter(|_| here) {
                if cells.outset(reach).contains(press.at) {
                    return true;
                }
            } else if here && on_edge(editing, press.at, press.zoom) {
                let bounds = editing.bounds();
                self.widening = Some((press.at.x - (bounds.x + bounds.width), editing.column));
                return true;
            }
            if cells.is_none() && here && editing.bounds().outset(reach).contains(press.at) {
                self.taps = if double { taps.saturating_add(1) } else { 1 };
                editing.tap(press.at, press.shift, self.taps);
                self.selecting = true;
                return true;
            }
            self.end(ink);
            // The text tool's tap only ends the text. Any other tool's press,
            // and a double tap, goes on: the gesture that ends a text is never lost.
            if tool == Tool::Text && !double {
                return true;
            }
        }
        let edits = tool == Tool::Text || (double && tool != Tool::Pen);
        if let Some(item) = text_at(items, press.at, reach).filter(|_| edits) {
            let Some(mut editing) = start::edit((canvas, page), item, items) else { return true };
            if tool != Tool::Text {
                start::on_cell(&mut editing, press.at);
            }
            if matches!(tool, Tool::Select | Tool::Lasso) {
                self.waiting = Some((editing, press.at, reach));
                return false;
            }
            self.editing = Some(editing);
            return true;
        }
        let arrow = match tool {
            Tool::Text => {
                self.start((canvas, page), press.at, colour, ink);
                return true;
            }
            Tool::Box => false,
            Tool::Arrow => true,
            Tool::Pen | Tool::Eraser | Tool::Lasso | Tool::Select | Tool::CropPdf => return false,
        };
        let start = if arrow { target(items, press.at, reach) } else { None };
        let (from, zoom) = (press.at, press.zoom);
        let drag = Drag { arrow, from, to: from, colour, start, zoom };
        self.drag = Some((canvas.to_string(), page.to_string(), drag));
        true
    }

    /// The pen lifted at `at` after a double tap on a text with the select
    /// tool or the lasso: the text is edited when the pen lifts where it
    /// went down. Returns whether an edit started.
    pub fn tapped(&mut self, at: Point) -> bool {
        let Some((editing, down, reach)) = self.waiting.take() else { return false };
        let still = distance(at, down) <= reach;
        if still {
            self.editing = Some(editing);
        }
        still
    }

    /// The pen moved to `at` while down: a box or an arrow goes with it;
    /// after a press inside the text being typed, the selection runs to it
    /// (after a double or triple tap it keeps the word or row); after one
    /// on its frame's right edge, the edge goes with it and sets the column
    /// the text wraps in ([`Editing::widen`]). Returns false when none of
    /// them is under it.
    pub fn drag(&mut self, at: Point) -> bool {
        if let Some((editing, (off, _))) = self.editing.as_mut().zip(self.widening) {
            editing.widen(at.x - off);
            return true;
        }
        if let Some(editing) = self.editing.as_mut().filter(|_| self.selecting) {
            if self.taps < 2 {
                editing.place(at, true);
            }
            return true;
        }
        let Some((_, _, drag)) = &mut self.drag else { return false };
        drag.to = at;
        true
    }

    /// The pen lifted, at `at` if it said where, over `items`: a box or an
    /// arrow that went far enough goes to `ink` as one change, an arrow's
    /// end bound to the item it landed on. Returns false when no box or
    /// arrow, and no selecting inside the text being typed or drag of its
    /// frame's edge, was under the pen.
    pub fn up(&mut self, at: Option<Point>, items: &[Cow<'_, Item>], ink: &mut Ink) -> bool {
        if let Some(at) = at {
            self.drag(at);
        }
        let widened = self.widening.take();
        if let Some((editing, (_, column))) = self.editing.as_mut().zip(widened) {
            editing.widened(column);
        }
        if std::mem::take(&mut self.selecting) | widened.is_some() {
            return true;
        }
        let Some((canvas, page, drag)) = self.drag.take() else { return false };
        if distance(drag.from, drag.to) * drag.zoom < MIN_DRAG {
            return true;
        }
        let shape = if drag.arrow {
            let end = target(items, drag.to, REACH / drag.zoom);
            arrow(snap(drag.from), snap(drag.to), [drag.start, end])
        } else {
            boxed(snap(drag.from), snap(drag.to))
        };
        ink.push(Change::Put { canvas, page, colour: drag.colour, shape });
        true
    }

    /// Whether a text is being typed: keys go to it first.
    pub fn editing(&self) -> bool {
        self.editing.is_some()
    }

    /// Whether `at`, on `page` of `canvas` at `zoom`, is on the right edge
    /// of the frame of the text being typed: a drag there sets its column.
    pub fn on_edge(&self, (canvas, page): (&str, &str), at: Point, zoom: f64) -> bool {
        let here = |e: &&Editing| e.canvas == canvas && e.page == page;
        self.editing.as_ref().filter(here).is_some_and(|e| on_edge(e, at, zoom))
    }

    /// The canvas and page of the text being typed, if any.
    pub fn typing_on(&self) -> Option<(&str, &str)> {
        self.editing.as_ref().map(|e| (e.canvas.as_str(), e.page.as_str()))
    }

    /// Typed `text` goes into the text being edited at its caret, in place
    /// of its selection: delete deletes back, return starts a line, escape
    /// ends the text (to `ink`). Other control and function keys do
    /// nothing ([`Editor::typed`](super::editor::Editor::typed)). Returns
    /// false, and nothing changes, when no text is being edited.
    pub fn typed(&mut self, text: &str, ink: &mut Ink) -> bool {
        let Some(editing) = &mut self.editing else { return false };
        match text.split_once('\u{1b}') {
            Some((before, _)) => {
                editing.editor.typed(before);
                self.end(ink);
            }
            None => editing.editor.typed(text),
        }
        true
    }

    /// A key of the text being typed: its caret, selection and delete keys
    /// ([`Editor::key`](super::editor::Editor::key)). Returns false when no
    /// text is being typed, or the key is not one of them.
    pub fn key(&mut self, chord: Chord) -> bool {
        self.editing.as_mut().is_some_and(|editing| editing.key(chord))
    }

    /// Undoes the last edit of the text being typed (a new size or column
    /// too), or with `redo` redoes one. False when no text is being typed
    /// or it has no such step.
    pub fn undo(&mut self, redo: bool) -> bool {
        self.editing.as_mut().is_some_and(|editing| editing.undo(redo))
    }

    /// The characters selected in the text being typed, if any; with `cut`,
    /// they are taken out of it.
    pub fn copy(&mut self, cut: bool) -> Option<String> {
        let editor = &mut self.editing.as_mut()?.editor;
        if cut { editor.cut() } else { editor.selected().map(str::to_string) }
    }

    /// Whether characters are selected in the text being typed.
    pub fn selects(&self) -> bool {
        self.editing.as_ref().is_some_and(|e| e.editor.selection().is_some())
    }

    /// Pasted `text` goes into the text being edited at its caret, in place
    /// of its selection, each line end starting a line ([`paste::plain`]).
    /// Returns false, and nothing changes, when no text is being edited.
    pub fn pasted(&mut self, text: &str) -> bool {
        let Some(editing) = &mut self.editing else { return false };
        editing.editor.insert(&paste::plain(text));
        true
    }

    /// Ends the text being edited, as one change to `ink` ([`start::ended`]):
    /// a new text's `add`, an edited text's `set` (with the arrows bound to
    /// it following a new frame), or its `remove` when emptied. Blank new
    /// text and an unchanged edit make none. Returns false when no text was
    /// being edited.
    pub fn end(&mut self, ink: &mut Ink) -> bool {
        (self.selecting, self.widening, self.grid) = (false, None, None);
        let Some(editing) = self.editing.take() else { return false };
        self.ends += 1;
        self.ended = Some((editing.canvas.clone(), editing.page.clone(), editing.frame()));
        if let Some(change) = start::ended(editing) {
            ink.push(change);
        }
        true
    }

    /// `items`, the page shown of `canvas`, with the box or arrow under the
    /// pen laid over it, and the text being edited in place of its item,
    /// with its caret in the focus colour, its dashed muted frame and the
    /// hint under it, sized for `zoom` in `theme`'s colours.
    pub fn shown<'a>(
        &self,
        (canvas, page): (&str, &str),
        mut items: Vec<Cow<'a, Item>>,
        zoom: f64,
        theme: &Theme,
    ) -> Vec<Cow<'a, Item>> {
        let here = |c: &str, p: &str| c == canvas && p == page;
        if let Some((_, _, drag)) = self.drag.as_ref().filter(|(c, p, _)| here(c, p)) {
            let shape = if drag.arrow {
                arrow(drag.from, drag.to, [None; 2])
            } else {
                boxed(drag.from, drag.to)
            };
            items.push(Cow::Owned(Item::plain(PREVIEW, drag.colour, shape)));
        }
        if let Some(editing) = self.editing.as_ref().filter(|e| here(&e.canvas, &e.page)) {
            if let Some(was) = &editing.item {
                items.retain(|i| i.id != was.id);
            }
            if self.cells(&mut items, zoom, theme) {
                return items;
            }
            let (frame, text, size) = (editing.whole(), editing.editor.text(), editing.size);
            let (caret, selected) = editing.marks(CARET / zoom);
            let fill = Some(Fill { colour: theme.focus, alpha: SELECTED_ALPHA });
            for (n, frame) in (0..).zip(selected) {
                let shape = Shape::Panel { frame, fill, width: 0.0, dashed: false };
                let id = ItemId(SELECTED - n);
                items.push(Cow::Owned(Item::plain(id, theme.focus, shape)));
            }
            let column = Some(editing.column);
            let shape = Shape::Text { frame, text: text.to_string(), size, column };
            items.push(Cow::Owned(Item::plain(EDITING, editing.colour, shape)));
            let shape = Shape::Caret { frame: caret };
            items.push(Cow::Owned(Item::plain(CARET_ID, theme.focus, shape)));
            let around = editing.bounds().outset(FRAME_PAD / zoom);
            let shape =
                Shape::Panel { frame: around, fill: None, width: FRAME_LINE / zoom, dashed: true };
            items.push(Cow::Owned(Item::plain(FRAME_ID, theme.muted, shape)));
            let under = Point { x: around.x, y: around.y + around.height + HINT_GAP / zoom };
            let hint = label(HINT_ID, self.hint(), theme.muted, under, zoom);
            items.push(Cow::Owned(hint));
        }
        items
    }
}

/// Whether `at` is on the right edge of the dashed frame `editing` shows
/// at `zoom`: from its glyphs' frame out past the dash, level with it.
fn on_edge(editing: &Editing, at: Point, zoom: f64) -> bool {
    let (around, near) = (editing.bounds().outset(FRAME_PAD / zoom), FRAME_PAD / zoom);
    let edge = Rect { x: around.x + around.width - near, width: 2.0 * near, ..around };
    edge.contains(at)
}

/// The topmost server text of `items` within `reach` of `at`.
fn text_at<'b>(items: &'b [Cow<'_, Item>], at: Point, reach: f64) -> Option<&'b Item> {
    let texts = items.iter().filter(|i| on_server(i.id));
    let mut texts = texts.filter(|i| match &i.shape {
        Shape::Text { frame, .. } => frame.outset(reach).contains(at),
        _ => false,
    });
    texts.next_back().map(AsRef::as_ref)
}

fn arrow(from: Point, to: Point, bound: [Option<ItemId>; 2]) -> Shape {
    Shape::Arrow { from, to, width: LINE_WIDTH, bound }
}

/// The box with corners `a` and `b`, whichever way the drag went.
fn boxed(a: Point, b: Point) -> Shape {
    let frame = Rect {
        x: a.x.min(b.x),
        y: a.y.min(b.y),
        width: (a.x - b.x).abs(),
        height: (a.y - b.y).abs(),
    };
    Shape::Box { frame, width: LINE_WIDTH }
}

fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// A point on the whole tenths the canvas file stores, so the server's echo
/// lands where the shape showed.
fn snap(p: Point) -> Point {
    let tenth = |v: f64| (v * 10.0).round() / 10.0;
    Point { x: tenth(p.x), y: tenth(p.y) }
}

mod start;
#[cfg(test)]
mod tests;
mod typing;
