//! The eraser, the lasso and the selection (SPEC D6). Pure: `canvases`
//! hands in pen points in canvas units, the zoom, and the page as the user
//! sees it (the server's copy with the ink laid over, [`Ink::shown`]); the
//! finished gesture goes to the [`Ink`] as one [`Change`], so one undo step.
//!
//! - The eraser: each stroke, text, box (by its outline) or arrow its path
//!   touches hides at once, and the lift removes them all in one `remove`.
//!   Its radius is [`ERASER`] screen points at any zoom.
//! - The lasso: the loop draws as the pen goes, and the lift selects the
//!   items it holds whole. A press inside the selection's box drags the
//!   selection instead, and the lift moves it in one `move`, with the
//!   arrows bound to it following ([`shapes::follow`]).
//! - The select tool ([`pick`]): a press on an item selects it, a drag from
//!   empty canvas draws a box that selects what it touches, and a drag
//!   inside the selection's box moves it, as the lasso's does.
//! - The selection: one page's items, in a box with a faint fill of the
//!   focus colour, a square handle at each corner (a drag on one resizes
//!   the selection, [`scale`]) and the number of items above it. It
//!   is app state, read by what acts on it ([`Select::selection`]); delete
//!   removes it in one `remove`, and a colour picked recolours its strokes,
//!   texts, boxes and arrows in one `set` each ([`Select::recolour`]). It
//!   is of a page, not of a pane: every pane on its page shows it, and its
//!   move, as the items themselves do.
//! - On a PDF's canvas the select tool and the lasso do not take `pdf`
//!   items ([`takes`]): the pages are fixed paper.
//! - A drag of the selection follows the pane under the pen, and can carry
//!   it to another page ([`carry`]).
//!
//! Which items a path touches, a loop holds or a press picks is [`hit`]'s, the core's rules
//! kept here so the answer comes at every pen event without the socket.
//! Every item on the page counts but the stroke under the pen and the
//! marks: ink not yet written too, which the ink names by the server's id
//! once the daemon has made it ([`Ink::answered`], [`Select::renamed`]).
//! Any of them can be erased or moved, an agent's too: the user may change
//! every item.

mod carry;
mod hit;
mod pick;
mod scale;

pub use carry::Landing;
use carry::{Arriving, Target};

use std::borrow::Cow;
use std::collections::HashSet;

use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect};
use crate::scene::canvas::{Fill, Item, ItemId, Shape};
use crate::scene::ink::{Change, Ink, on_server};
use crate::scene::tools::Tool;
use crate::scene::{pdf, shapes};

/// How far the eraser reaches from the pen, in screen points.
pub const ERASER: f64 = 6.0;
/// How far the selection's box lies outside its items, in screen points: a
/// press that far out still drags the selection.
const MARGIN: f64 = 6.0;
/// The width of the lasso's loop and the selection's box, in screen points.
const THIN: f64 = 1.0;
/// How much of the focus colour fills the selection's box.
const TINT: f64 = 0.07;
/// The side of a corner handle and the width of its outline, in screen points.
const HANDLE: f64 = 9.0;
const HANDLE_LINE: f64 = 1.5;
/// How far above the handles the item count sits, in screen points.
const COUNT_GAP: f64 = 2.0;
/// The ids of the marks drawn over a page: above every server id and every
/// id of local ink, so none is ever hit, and apart from `shapes`' marks.
const LOOP: ItemId = ItemId(u64::MAX);
const BOX: ItemId = ItemId(u64::MAX - 1);
const COUNT: ItemId = ItemId(u64::MAX - 4);
/// The four handles: this id and the three below it.
const HANDLES: u64 = u64::MAX - 5;

/// Items selected on one page, by the ids they show under, in drawing order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub canvas: String,
    pub page: String,
    pub ids: Vec<ItemId>,
}

/// The gesture under the pen.
#[derive(Debug, Clone, PartialEq)]
enum Gesture {
    /// The items hit so far, hidden until the lift removes them; the last
    /// point of the path, and the radius in canvas units.
    Erase { ids: Vec<ItemId>, last: Point, radius: f64 },
    /// The loop so far.
    Lasso { points: Vec<Point> },
    /// The selection dragged from `from`, `by` so far, towards `to`.
    /// `click` is the item the select tool pressed on, selected alone if
    /// the pen lifts unmoved.
    Move { from: Point, by: Point, click: Option<ItemId>, to: Target },
    /// The select tool's box, from `from` to `to`; `adding` (shift) adds
    /// what it touches to the selection.
    Marquee { from: Point, to: Point, adding: bool },
    /// A corner handle dragged: the selection resizing.
    Scale(scale::Grip),
}

impl Gesture {
    /// A drag of the selection pressed at `from`, not moved yet.
    fn moving(from: Point, click: Option<ItemId>) -> Self {
        Gesture::Move { from, by: Point { x: 0.0, y: 0.0 }, click, to: Target::Here }
    }
}

/// The gesture under the pen, if any, the selection, if any, and the
/// items a carry is bringing to another page ([`carry`]).
#[derive(Debug, Default)]
pub struct Select {
    /// The canvas and page the gesture is on, and the gesture.
    gesture: Option<(String, String, Gesture)>,
    selection: Option<Selection>,
    arriving: Option<Arriving>,
}

impl Select {
    /// The pen went down at `at` on `page` of `canvas` with `tool`, at
    /// `zoom`, over `items`. Returns false, and nothing starts, for a tool
    /// that is neither the eraser nor the lasso (the select tool's press is
    /// [`Select::pick`]). A lasso pressed on a handle resizes the
    /// selection; pressed outside the selection's box it clears the
    /// selection and starts a loop.
    pub fn down(
        &mut self,
        tool: Tool,
        (canvas, page): (&str, &str),
        at: Point,
        zoom: f64,
        items: &[Cow<'_, Item>],
    ) -> bool {
        if tool == Tool::Lasso && self.grip((canvas, page), at, zoom, items) {
            return true;
        }
        self.arriving = None;
        let gesture = match tool {
            Tool::Eraser => Gesture::Erase { ids: Vec::new(), last: at, radius: ERASER / zoom },
            Tool::Lasso if self.grabs((canvas, page), at, zoom, items) => Gesture::moving(at, None),
            Tool::Lasso => {
                self.selection = None;
                Gesture::Lasso { points: vec![at] }
            }
            Tool::Pen | Tool::Text | Tool::Box | Tool::Arrow | Tool::Select | Tool::CropPdf => {
                return false;
            }
        };
        self.gesture = Some((canvas.to_string(), page.to_string(), gesture));
        self.drag(at, items)
    }

    /// The pen moved to `at` while down, over `items`. Returns false when no
    /// gesture is under it.
    pub fn drag(&mut self, at: Point, items: &[Cow<'_, Item>]) -> bool {
        let Some((_, _, gesture)) = &mut self.gesture else { return false };
        match gesture {
            Gesture::Erase { ids, last, radius } => {
                let hit = items.iter().filter(|i| on_server(i.id) && !ids.contains(&i.id));
                let hit = hit.filter(|i| hit::erases(&i.shape, *last, at, *radius));
                let hit: Vec<ItemId> = hit.map(|i| i.id).collect();
                ids.extend(hit);
                *last = at;
            }
            Gesture::Lasso { points } => {
                if points.last() != Some(&at) {
                    points.push(at);
                }
            }
            Gesture::Move { from, by, to, .. } => {
                *by = Point { x: at.x - from.x, y: at.y - from.y };
                *to = Target::Here;
            }
            Gesture::Marquee { to, .. } => *to = at,
            Gesture::Scale(grip) => grip.drag(at),
        }
        true
    }

    /// The pen lifted, at `at` if it said where, over `items`: an erase that
    /// hit anything, or a move that went anywhere, goes to `ink` as one
    /// change; a loop selects what it holds (nothing, when it holds
    /// nothing), and the select tool's box what it touches; a resize that
    /// went anywhere goes to `ink` as one change; a press on an
    /// item that did not move selects that item alone. Returns false when no gesture was under the pen.
    pub fn up(&mut self, at: Option<Point>, items: &[Cow<'_, Item>], ink: &mut Ink) -> bool {
        if let Some(at) = at {
            self.drag(at, items);
        }
        let Some((canvas, page, gesture)) = self.gesture.take() else { return false };
        match gesture {
            Gesture::Erase { ids, .. } if !ids.is_empty() => {
                ink.push(Change::Remove { canvas, page, ids });
            }
            Gesture::Erase { .. } => {}
            Gesture::Lasso { points } => {
                let held = items.iter().filter(|i| on_server(i.id) && takes(&canvas, i));
                let held = held.filter(|i| hit::holds(&points, &i.shape));
                let ids: Vec<ItemId> = held.map(|i| i.id).collect();
                self.selection = (!ids.is_empty()).then_some(Selection { canvas, page, ids });
            }
            Gesture::Scale(grip) => {
                if let Some(selection) = &self.selection {
                    scale::lift(&grip, (canvas, page), items, &selection.ids, ink);
                }
            }
            Gesture::Marquee { from, to, adding } => {
                self.boxed((canvas, page), pick::spanned(from, to), adding, items);
            }
            Gesture::Move { by, click, .. } => {
                // Whole tenths, as the canvas file stores them, so the
                // server's echo lands where the move showed.
                let tenth = |v: f64| (v * 10.0).round() / 10.0;
                let by = Point { x: tenth(by.x), y: tenth(by.y) };
                let moved = by.x != 0.0 || by.y != 0.0;
                if let Some(id) = click.filter(|_| !moved) {
                    self.selection = Some(Selection { canvas, page, ids: vec![id] });
                    return true;
                }
                if let Some(selection) = self.selection.as_ref().filter(|_| moved) {
                    let ids = selection.ids.clone();
                    let follow = shapes::follow(items, &ids, by);
                    ink.push(Change::Move { canvas, page, ids, by, follow });
                }
            }
        }
        true
    }

    /// The selection, if any.
    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    /// The selection's items' box in canvas units, over `items` (its page
    /// as shown); `None` without a selection or when none of it is there.
    pub fn bounds(&self, items: &[Cow<'_, Item>]) -> Option<Rect> {
        bounds(items, &self.selection.as_ref()?.ids)
    }

    /// Clears the selection. Returns false when there was none.
    pub fn clear(&mut self) -> bool {
        self.selection.take().is_some()
    }

    /// Removes the selected items, as one change to `ink`, and clears the
    /// selection. Returns false when there was none.
    pub fn delete(&mut self, ink: &mut Ink) -> bool {
        let Some(Selection { canvas, page, ids }) = self.selection.take() else { return false };
        ink.push(Change::Remove { canvas, page, ids });
        true
    }

    /// Makes the selected strokes, texts, boxes and arrows in `items` (its
    /// page as shown) `colour`, as one change to `ink`; any other item keeps
    /// its own. Returns false when there is no selection or none of it changes.
    pub fn recolour(&self, colour: Rgb, items: &[Cow<'_, Item>], ink: &mut Ink) -> bool {
        let Some(Selection { canvas, page, ids }) = &self.selection else { return false };
        let takes = |i: &&Cow<'_, Item>| {
            let kind = matches!(
                i.shape,
                Shape::Stroke { .. } | Shape::Text { .. } | Shape::Box { .. } | Shape::Arrow { .. }
            );
            kind && i.colour != colour && ids.contains(&i.id)
        };
        let items: Vec<Item> =
            items.iter().filter(takes).map(|i| Item { colour, ..i.as_ref().clone() }).collect();
        let changed = !items.is_empty();
        if changed {
            ink.push(Change::Reshape { canvas: canvas.clone(), page: page.clone(), items });
        }
        changed
    }

    /// Items that showed under the first id of each of `ids` show under the
    /// second from now on (the daemon made them, [`Ink::answered`]): the
    /// selection, and an erase under way, keep them.
    pub fn renamed(&mut self, ids: &[(ItemId, ItemId)]) {
        let rename = |id: &mut ItemId| {
            if let Some((_, now)) = ids.iter().find(|(was, _)| was == id) {
                *id = *now;
            }
        };
        if let Some(selection) = &mut self.selection {
            selection.ids.iter_mut().for_each(&rename);
        }
        if let Some((_, _, Gesture::Erase { ids: hit, .. })) = &mut self.gesture {
            hit.iter_mut().for_each(&rename);
        }
    }

    /// Lets go of the selected items no longer in `items`, the page shown of
    /// `canvas`: another client, or an agent, removed them. A selection
    /// left with no items is cleared.
    /// Items a carry brought here, once all are in `items`, are selected.
    pub fn keep(&mut self, (canvas, page): (&str, &str), items: &[Cow<'_, Item>]) {
        self.arrive((canvas, page), items);
        let Some(selection) = &mut self.selection else { return };
        if selection.canvas != canvas || selection.page != page {
            return;
        }
        let present: HashSet<ItemId> = items.iter().map(|i| i.id).collect();
        selection.ids.retain(|id| present.contains(id));
        if selection.ids.is_empty() {
            self.selection = None;
        }
    }

    /// `items`, the page shown of `canvas`, with the gesture laid over it at
    /// `zoom` in `theme`'s colours: the items being erased hidden, the
    /// selection where it is being dragged (gone from here while it is
    /// over a pane on another page, and shown there instead), then the
    /// selection's box, its handles and its count, and the lasso's loop or
    /// the select tool's box, thin and muted.
    pub fn shown<'a>(
        &self,
        (canvas, page): (&str, &str),
        mut items: Vec<Cow<'a, Item>>,
        zoom: f64,
        theme: &Theme,
    ) -> Vec<Cow<'a, Item>> {
        let here = |c: &str, p: &str| c == canvas && p == page;
        let gesture = self.gesture.as_ref().filter(|(c, p, _)| here(c, p)).map(|(_, _, g)| g);
        let selection = self.selection.as_ref().filter(|s| here(&s.canvas, &s.page));
        match (gesture, selection) {
            (Some(Gesture::Erase { ids, .. }), _) => items.retain(|i| !ids.contains(&i.id)),
            (Some(Gesture::Scale(grip)), Some(selection)) => {
                scale::show(grip, &mut items, &selection.ids);
            }
            (Some(Gesture::Move { to: Target::Away { .. }, .. }), Some(selection)) => {
                items.retain(|i| !selection.ids.contains(&i.id));
            }
            (Some(Gesture::Move { by, to: Target::Here, .. }), Some(selection)) => {
                let follow = shapes::follow(&items, &selection.ids, *by);
                let ids: HashSet<&ItemId> = selection.ids.iter().collect();
                for item in &mut items {
                    if let Some(after) = follow.iter().find(|f| f.id == item.id) {
                        *item = Cow::Owned(after.clone());
                    } else if ids.contains(&item.id) {
                        let (id, colour) = (item.id, item.colour);
                        *item = Cow::Owned(Item { id, colour, shape: item.shape.moved(*by) });
                    }
                }
            }
            _ => {}
        }
        let width = THIN / zoom;
        let mut boxed = selection.and_then(|s| Some((bounds(&items, &s.ids)?, s.ids.len())));
        if let Some(coming) = self.coming((canvas, page)) {
            let ids: Vec<ItemId> = coming.iter().map(|i| i.id).collect();
            boxed = bounds(&coming, &ids).map(|b| (b, ids.len()));
            items.extend(coming);
        }
        if let Some((frame, count)) = boxed {
            items.extend(chrome(frame.outset(MARGIN / zoom), count, zoom, theme));
        }
        if let Some(Gesture::Lasso { points }) = gesture {
            let mut points = points.clone();
            points.extend(points.first().copied());
            items.push(Cow::Owned(Item {
                id: LOOP,
                colour: theme.muted,
                shape: Shape::Stroke { points, width },
            }));
        }
        if let Some(&Gesture::Marquee { from, to, .. }) = gesture {
            let frame = pick::spanned(from, to);
            let shape = Shape::Panel { frame, fill: None, width, dashed: false };
            items.push(Cow::Owned(Item { id: LOOP, colour: theme.muted, shape }));
        }
        items
    }

    /// Whether a press at `at` on `page` of `canvas` is inside the
    /// selection's box or on one of its handles at `zoom`, over `items`.
    pub fn grabs(
        &self,
        (canvas, page): (&str, &str),
        at: Point,
        zoom: f64,
        items: &[Cow<'_, Item>],
    ) -> bool {
        let Some(selection) = &self.selection else { return false };
        let here = selection.canvas == canvas && selection.page == page;
        let inside =
            bounds(items, &selection.ids).is_some_and(|b| b.outset(MARGIN / zoom).contains(at));
        here && (inside || self.handle((canvas, page), at, zoom, items).is_some())
    }
}

/// The marks of a selection of `count` items whose box is `frame`, at
/// `zoom`: the box, tinted, its four corner handles, and the count above
/// its top left.
fn chrome(frame: Rect, count: usize, zoom: f64, theme: &Theme) -> Vec<Cow<'static, Item>> {
    let focus = theme.focus;
    let tint = Fill { colour: focus, alpha: TINT };
    let shape = Shape::Panel { frame, fill: Some(tint), width: THIN / zoom, dashed: false };
    let mut marks = vec![Item { id: BOX, colour: focus, shape }];
    let side = HANDLE / zoom;
    let (right, bottom) = (frame.x + frame.width, frame.y + frame.height);
    let corners = [(frame.x, frame.y), (right, frame.y), (right, bottom), (frame.x, bottom)];
    for ((x, y), id) in corners.into_iter().zip((0..4).map(|n| HANDLES - n)) {
        let square = Rect { x: x - side / 2.0, y: y - side / 2.0, width: side, height: side };
        let paper = Fill { colour: theme.background, alpha: 1.0 };
        let width = HANDLE_LINE / zoom;
        let shape = Shape::Panel { frame: square, fill: Some(paper), width, dashed: false };
        marks.push(Item { id: ItemId(id), colour: focus, shape });
    }
    let text = if count == 1 { "1 item".to_string() } else { format!("{count} items") };
    let top = frame.y - side / 2.0 - COUNT_GAP / zoom - shapes::label_height(zoom);
    marks.push(shapes::label(COUNT, text, focus, Point { x: frame.x, y: top }, zoom));
    marks.into_iter().map(Cow::Owned).collect()
}

/// Whether the select tool and the lasso may take `item` on `canvas`:
/// not a `pdf` item on a PDF's canvas, where the pages are the paper and
/// a tap or a drag on one never moves it (docs/design/pdf.md).
fn takes(canvas: &str, item: &Item) -> bool {
    !(pdf::reader(canvas) && matches!(item.shape, Shape::Pdf { .. }))
}

/// The box around the items of `items` whose ids are `ids`; `None` when
/// none of them is there.
fn bounds(items: &[Cow<'_, Item>], ids: &[ItemId]) -> Option<Rect> {
    let ids: HashSet<&ItemId> = ids.iter().collect();
    let chosen = items.iter().filter(|i| ids.contains(&i.id));
    chosen.filter_map(|i| i.shape.bounds()).reduce(Rect::union)
}

#[cfg(test)]
mod tests;
