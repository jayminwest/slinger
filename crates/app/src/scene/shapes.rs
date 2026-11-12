//! The text, box and arrow tools (SPEC D6). Pure: `canvases` hands in pen
//! presses, drags and lifts in canvas units, typed text, and the page as the
//! user sees it ([`Ink::shown`]); each finished text or shape goes to the
//! [`Ink`] as one [`Change`], so one `canvas_apply` and one undo step.
//!
//! - Text: a tap places it and the keyboard types it ([`Shapes::typed`]);
//!   a paste goes in at its end ([`Shapes::pasted`]); escape or a press
//!   outside ends it. While it is typed it shows a caret
//!   after its last character, a dashed frame around it, and under that how
//!   to end it. A press outside with any other tool
//!   also starts that tool's gesture: the first gesture after a text is
//!   never lost. A tap with the text tool on a text,
//!   or a double tap on one with any tool but the pen, edits it. With the
//!   select tool or the lasso the second press may be a drag of what the
//!   first tap selected, so the edit waits for the lift, and starts only
//!   when the pen lifts where it went down ([`Shapes::tapped`]). The edit
//!   is one change when it ends: an `add`, a `set`, or a `remove` when it
//!   was emptied.
//! - Box: a drag from corner to corner.
//! - Arrow: a drag from start to end. An end that lands on an item binds to
//!   it ([`target`]), and when that item moves the end follows ([`follow`]),
//!   in the same change as the move.
//!
//! No snapping and no rotation (D6). A drag shorter than [`MIN_DRAG`] is a
//! tap and makes nothing.

use std::borrow::Cow;
use std::collections::HashSet;

use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect};
use crate::mirror::{LINE_WIDTH, TEXT_SIZE};
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::glyphs;
use crate::scene::ink::{Change, Ink, on_server};
use crate::scene::paste;
use crate::scene::tools::Tool;

/// Two presses this close in time, in seconds, are a double tap...
pub const DOUBLE_TAP: f64 = 0.4;
/// ...when this close in place, in screen points. An arrow's end binds to
/// an item it lands this close to, and a press this close to the text
/// being edited is inside it.
pub const REACH: f64 = 8.0;
/// The least drag, in screen points, that makes a box or an arrow.
pub const MIN_DRAG: f64 = 4.0;
/// The width of the caret after the text being edited, of the dashed frame
/// around it, and how far out the frame lies, in screen points.
const CARET: f64 = 2.0;
const FRAME_LINE: f64 = 1.0;
const FRAME_PAD: f64 = 4.0;
/// Under the text being edited, this far below its frame, in screen points.
const HINT: &str = "esc or a tap outside ends the text";
const HINT_GAP: f64 = 4.0;
/// How high chrome's labels are set (the selection's count, the hint), and
/// how tall their line is, in screen points.
const LABEL_SIZE: f64 = 11.0;
const LABEL_LINE: f64 = 1.3;
/// The ids of the marks drawn over a page: above every server id and every
/// id of local ink, and apart from the selection's marks.
const PREVIEW: ItemId = ItemId(u64::MAX - 2);
const EDITING: ItemId = ItemId(u64::MAX - 3);
const CARET_ID: ItemId = ItemId(u64::MAX - 9);
const FRAME_ID: ItemId = ItemId(u64::MAX - 10);
const HINT_ID: ItemId = ItemId(u64::MAX - 11);

/// One press of the pen: where on the canvas, when (seconds, the pen's
/// clock) and at what zoom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Press {
    pub at: Point,
    pub when: f64,
    pub zoom: f64,
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

/// A text being typed.
#[derive(Debug, Clone, PartialEq)]
struct Editing {
    canvas: String,
    page: String,
    /// The text's top left, in canvas units.
    at: Point,
    text: String,
    colour: Rgb,
    /// The text item edited and its text before; `None` for a new text.
    item: Option<(ItemId, String)>,
}

impl Editing {
    /// The box its glyphs fill, unwrapped: the caret, the dashed frame and
    /// a press inside follow the glyphs. The file stores its own frame.
    fn frame(&self) -> Rect {
        glyphs::frame(self.at, &self.text, TEXT_SIZE, glyphs::UNWRAPPED)
    }
}

/// The box or arrow under the pen, the text being typed, the last press,
/// and the edit a double tap with the select tool or the lasso asks for
/// once the pen lifts where it went down, within the reach there.
#[derive(Debug, Default)]
pub struct Shapes {
    drag: Option<(String, String, Drag)>,
    editing: Option<Editing>,
    last: Option<(String, String, Press)>,
    waiting: Option<(Editing, Point, f64)>,
}

impl Shapes {
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
        let double = last.is_some_and(|(c, p, last)| {
            c == canvas
                && p == page
                && press.when - last.when <= DOUBLE_TAP
                && distance(press.at, last.at) <= reach
        });
        if let Some(editing) = &self.editing {
            let here = editing.canvas == canvas && editing.page == page;
            if here && editing.frame().outset(reach).contains(press.at) {
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
            let Shape::Text { frame, text, .. } = &item.shape else { return true };
            let editing = Editing {
                canvas: canvas.to_string(),
                page: page.to_string(),
                at: Point { x: frame.x, y: frame.y },
                text: text.clone(),
                colour: item.colour,
                item: Some((item.id, text.clone())),
            };
            if matches!(tool, Tool::Select | Tool::Lasso) {
                self.waiting = Some((editing, press.at, reach));
                return false;
            }
            self.editing = Some(editing);
            return true;
        }
        let arrow = match tool {
            Tool::Text => {
                // The tap is at the middle of the first line's left end.
                let half = glyphs::frame(press.at, "", TEXT_SIZE, glyphs::UNWRAPPED).height / 2.0;
                self.editing = Some(Editing {
                    canvas: canvas.to_string(),
                    page: page.to_string(),
                    at: snap(Point { x: press.at.x, y: press.at.y - half }),
                    text: String::new(),
                    colour,
                    item: None,
                });
                return true;
            }
            Tool::Box => false,
            Tool::Arrow => true,
            Tool::Pen | Tool::Eraser | Tool::Lasso | Tool::Select => return false,
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

    /// The pen moved to `at` while down. Returns false when no box or arrow
    /// is under it.
    pub fn drag(&mut self, at: Point) -> bool {
        let Some((_, _, drag)) = &mut self.drag else { return false };
        drag.to = at;
        true
    }

    /// The pen lifted, at `at` if it said where, over `items`: a box or an
    /// arrow that went far enough goes to `ink` as one change, an arrow's
    /// end bound to the item it landed on. Returns false when no box or
    /// arrow was under the pen.
    pub fn up(&mut self, at: Option<Point>, items: &[Cow<'_, Item>], ink: &mut Ink) -> bool {
        if let Some(at) = at {
            self.drag(at);
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

    /// The canvas and page of the text being typed, if any.
    pub fn typing_on(&self) -> Option<(&str, &str)> {
        self.editing.as_ref().map(|e| (e.canvas.as_str(), e.page.as_str()))
    }

    /// Typed `text` goes into the text being edited: delete takes the last
    /// character back, return starts a line, escape ends the text (to
    /// `ink`). Other control and function keys do nothing. Returns false,
    /// and nothing changes, when no text is being edited.
    pub fn typed(&mut self, text: &str, ink: &mut Ink) -> bool {
        let Some(editing) = &mut self.editing else { return false };
        for c in text.chars() {
            match c {
                '\u{1b}' => {
                    self.end(ink);
                    return true;
                }
                '\u{7f}' | '\u{8}' => {
                    editing.text.pop();
                }
                '\r' | '\n' => editing.text.push('\n'),
                // The arrow and function keys are in this private range.
                c if c.is_control() || ('\u{f700}'..='\u{f8ff}').contains(&c) => {}
                c => editing.text.push(c),
            }
        }
        true
    }

    /// Pasted `text` goes into the text being edited at its end, each line
    /// end starting a line ([`paste::plain`]). Returns false, and nothing
    /// changes, when no text is being edited.
    pub fn pasted(&mut self, text: &str) -> bool {
        let Some(editing) = &mut self.editing else { return false };
        editing.text.push_str(&paste::plain(text));
        true
    }

    /// Ends the text being edited, as one change to `ink`: a new text's
    /// `add`, an edited text's `set`, or its `remove` when emptied. Blank
    /// new text and an unchanged edit make none. Returns false when no text
    /// was being edited.
    pub fn end(&mut self, ink: &mut Ink) -> bool {
        let Some(editing) = self.editing.take() else { return false };
        let blank = editing.text.trim().is_empty();
        let shape = Shape::Text { frame: editing.frame(), text: editing.text, size: TEXT_SIZE };
        let (canvas, page) = (editing.canvas, editing.page);
        match editing.item {
            None if blank => {}
            None => ink.push(Change::Put { canvas, page, colour: editing.colour, shape }),
            Some((id, _)) if blank => ink.push(Change::Remove { canvas, page, ids: vec![id] }),
            Some((_, before)) if matches!(&shape, Shape::Text { text, .. } if *text == before) => {}
            Some((id, _)) => ink.push(Change::Set { canvas, page, id, shape }),
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
            items.push(Cow::Owned(Item { id: PREVIEW, colour: drag.colour, shape }));
        }
        if let Some(editing) = self.editing.as_ref().filter(|e| here(&e.canvas, &e.page)) {
            if let Some((id, _)) = &editing.item {
                items.retain(|i| i.id != *id);
            }
            let (frame, text) = (editing.frame(), editing.text.clone());
            let shape = Shape::Text { frame, text: text.clone(), size: TEXT_SIZE };
            items.push(Cow::Owned(Item { id: EDITING, colour: editing.colour, shape }));
            let width = CARET / zoom;
            let shape = Shape::Caret { frame, text, size: TEXT_SIZE, width };
            items.push(Cow::Owned(Item { id: CARET_ID, colour: theme.focus, shape }));
            let around = frame.outset(FRAME_PAD / zoom);
            let shape =
                Shape::Panel { frame: around, fill: None, width: FRAME_LINE / zoom, dashed: true };
            items.push(Cow::Owned(Item { id: FRAME_ID, colour: theme.muted, shape }));
            let under = Point { x: around.x, y: around.y + around.height + HINT_GAP / zoom };
            let hint = label(HINT_ID, HINT.to_string(), theme.muted, under, zoom);
            items.push(Cow::Owned(hint));
        }
        items
    }
}

/// A line of chrome's text, `id`, from `at` (its top left), set
/// [`LABEL_SIZE`] screen points high at `zoom`, in `colour`.
pub fn label(id: ItemId, text: String, colour: Rgb, at: Point, zoom: f64) -> Item {
    let size = LABEL_SIZE / zoom;
    let count = u32::try_from(text.chars().count()).map_or(f64::from(u32::MAX), f64::from);
    // A cell of the full size a character: the line never wraps.
    let frame = Rect { x: at.x, y: at.y, width: count.max(1.0) * size, height: label_height(zoom) };
    Item { id, colour, shape: Shape::Text { frame, text, size } }
}

/// How tall a [`label`] is at `zoom`, in canvas units.
pub fn label_height(zoom: f64) -> f64 {
    LABEL_SIZE * LABEL_LINE / zoom
}

/// The topmost server item of `items` an arrow's end at `at` binds to: one
/// whose box, `reach` canvas units bigger, holds the end. Arrows are not
/// bound to.
pub fn target(items: &[Cow<'_, Item>], at: Point, reach: f64) -> Option<ItemId> {
    let bindable = items.iter().filter(|i| on_server(i.id));
    let bindable = bindable.filter(|i| !matches!(i.shape, Shape::Arrow { .. }));
    let mut found =
        bindable.filter(|i| i.shape.bounds().is_some_and(|b| b.outset(reach).contains(at)));
    found.next_back().map(|i| i.id)
}

/// The arrows of `items` that follow a move of the items `ids` by `by`, as
/// they are after it: each arrow not moved itself with an end bound to a
/// moved item, that end moved by `by`.
pub fn follow(items: &[Cow<'_, Item>], ids: &[ItemId], by: Point) -> Vec<Item> {
    follow_by(items, ids, |p| Point { x: p.x + by.x, y: p.y + by.y })
}

/// As [`follow`], with each bound end put where `map` says: a move, or a
/// resize of the selection.
pub fn follow_by(
    items: &[Cow<'_, Item>],
    ids: &[ItemId],
    map: impl Fn(Point) -> Point,
) -> Vec<Item> {
    let moved: HashSet<&ItemId> = ids.iter().collect();
    let on = |end: Option<ItemId>| end.is_some_and(|id| moved.contains(&id));
    let mut follows = Vec::new();
    for item in items.iter().filter(|i| !moved.contains(&i.id)) {
        let Shape::Arrow { from, to, width, bound } = &item.shape else { continue };
        if !on(bound[0]) && !on(bound[1]) {
            continue;
        }
        let shift = |p: Point, end| if on(end) { map(p) } else { p };
        let (from, to) = (shift(*from, bound[0]), shift(*to, bound[1]));
        let shape = Shape::Arrow { from, to, width: *width, bound: *bound };
        follows.push(Item { id: item.id, colour: item.colour, shape });
    }
    follows
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

#[cfg(test)]
mod tests;
