//! The eraser, the lasso and the selection (SPEC D6). Pure: `canvases`
//! hands in pen points in canvas units, the zoom, and the page as the user
//! sees it (the server's copy with the ink laid over, [`Ink::shown`]); the
//! finished gesture goes to the [`Ink`] as one [`Change`], so one undo step.
//!
//! - The eraser: each stroke its path touches hides at once, and the lift
//!   removes them all in one `remove`. Its radius is [`ERASER`] screen
//!   points at any zoom.
//! - The lasso: the loop draws as the pen goes, and the lift selects the
//!   items it holds whole. A press inside the selection's box drags the
//!   selection instead, and the lift moves it in one `move`, with the
//!   arrows bound to it following ([`shapes::follow`]).
//! - The selection: one page's items, with a plain box around them. It is
//!   app state, read by what acts on it ([`Select::selection`]); delete
//!   removes it in one `remove`.
//!
//! Which items a path touches or a loop holds is [`hit`]'s, the core's rules
//! kept here so the answer comes at every pen event without the socket.
//! Only the server's items count: ink still on its way has no server id yet.
//! Any of them can be erased or moved, an agent's too: the user may change
//! every item.

mod carry;
mod hit;

pub use carry::Landing;

use std::borrow::Cow;
use std::collections::HashSet;

use crate::config::Rgb;
use crate::layout::{Point, Rect};
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::ink::{Change, Ink, on_server};
use crate::scene::shapes;
use crate::scene::tools::Tool;

/// How far the eraser reaches from the pen, in screen points.
pub const ERASER: f64 = 6.0;
/// How far the selection's box lies outside its items, in screen points: a
/// press that far out still drags the selection.
const MARGIN: f64 = 6.0;
/// The width of the lasso's loop and the selection's box, in screen points.
const THIN: f64 = 1.0;
/// The ids of the two marks drawn over a page: above every server id and
/// every id of local ink, so neither is ever hit.
const LOOP: ItemId = ItemId(u64::MAX);
const BOX: ItemId = ItemId(u64::MAX - 1);

/// Items selected on one page, by the server's ids, in drawing order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub canvas: String,
    pub page: String,
    pub ids: Vec<ItemId>,
}

/// The gesture under the pen.
#[derive(Debug, Clone, PartialEq)]
enum Gesture {
    /// The strokes hit so far, hidden until the lift removes them; the last
    /// point of the path, and the radius in canvas units.
    Erase { ids: Vec<ItemId>, last: Point, radius: f64 },
    /// The loop so far.
    Lasso { points: Vec<Point> },
    /// The selection dragged from `from`, `by` so far.
    Move { from: Point, by: Point },
}

/// The gesture under the pen, if any, and the selection, if any.
#[derive(Debug, Default)]
pub struct Select {
    /// The canvas and page the gesture is on, and the gesture.
    gesture: Option<(String, String, Gesture)>,
    selection: Option<Selection>,
}

impl Select {
    /// The pen went down at `at` on `page` of `canvas` with `tool`, at
    /// `zoom`, over `items`. Returns false, and nothing starts, for a tool
    /// that is neither the eraser nor the lasso. A lasso pressed outside
    /// the selection's box clears the selection and starts a loop.
    pub fn down(
        &mut self,
        tool: Tool,
        (canvas, page): (&str, &str),
        at: Point,
        zoom: f64,
        items: &[Cow<'_, Item>],
    ) -> bool {
        let gesture = match tool {
            Tool::Eraser => Gesture::Erase { ids: Vec::new(), last: at, radius: ERASER / zoom },
            Tool::Lasso if self.grabs((canvas, page), at, zoom, items) => {
                Gesture::Move { from: at, by: Point { x: 0.0, y: 0.0 } }
            }
            Tool::Lasso => {
                self.selection = None;
                Gesture::Lasso { points: vec![at] }
            }
            Tool::Pen | Tool::Text | Tool::Box | Tool::Arrow => return false,
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
            Gesture::Move { from, by } => *by = Point { x: at.x - from.x, y: at.y - from.y },
        }
        true
    }

    /// The pen lifted, at `at` if it said where, over `items`: an erase that
    /// hit anything, or a move that went anywhere, goes to `ink` as one
    /// change; a loop selects what it holds (nothing, when it holds
    /// nothing). Returns false when no gesture was under the pen.
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
                let held =
                    items.iter().filter(|i| on_server(i.id) && hit::holds(&points, &i.shape));
                let ids: Vec<ItemId> = held.map(|i| i.id).collect();
                self.selection = (!ids.is_empty()).then_some(Selection { canvas, page, ids });
            }
            Gesture::Move { by, .. } => {
                // Whole tenths, as the canvas file stores them, so the
                // server's echo lands where the move showed.
                let tenth = |v: f64| (v * 10.0).round() / 10.0;
                let by = Point { x: tenth(by.x), y: tenth(by.y) };
                let moved = by.x != 0.0 || by.y != 0.0;
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

    /// Lets go of the selected items no longer in `items`, the page shown of
    /// `canvas`: another client, or an agent, removed them. A selection
    /// left with no items is cleared.
    pub fn keep(&mut self, (canvas, page): (&str, &str), items: &[Cow<'_, Item>]) {
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
    /// `zoom`: the strokes being erased hidden, the selection where it is
    /// being dragged, then the selection's box and the lasso's loop, thin,
    /// in `colour`.
    pub fn shown<'a>(
        &self,
        (canvas, page): (&str, &str),
        mut items: Vec<Cow<'a, Item>>,
        zoom: f64,
        colour: Rgb,
    ) -> Vec<Cow<'a, Item>> {
        let here = |c: &str, p: &str| c == canvas && p == page;
        let gesture = self.gesture.as_ref().filter(|(c, p, _)| here(c, p)).map(|(_, _, g)| g);
        let selection = self.selection.as_ref().filter(|s| here(&s.canvas, &s.page));
        match (gesture, selection) {
            (Some(Gesture::Erase { ids, .. }), _) => items.retain(|i| !ids.contains(&i.id)),
            (Some(Gesture::Move { by, .. }), Some(selection)) => {
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
        if let Some(frame) = selection.and_then(|s| bounds(&items, &s.ids)) {
            let shape = Shape::Box { frame: frame.outset(MARGIN / zoom), width };
            items.push(Cow::Owned(Item { id: BOX, colour, shape }));
        }
        if let Some(Gesture::Lasso { points }) = gesture {
            let mut points = points.clone();
            points.extend(points.first().copied());
            items.push(Cow::Owned(Item {
                id: LOOP,
                colour,
                shape: Shape::Stroke { points, width },
            }));
        }
        items
    }

    /// Whether a press at `at` on `page` of `canvas` is inside the
    /// selection's box at `zoom`, over `items`.
    fn grabs(
        &self,
        (canvas, page): (&str, &str),
        at: Point,
        zoom: f64,
        items: &[Cow<'_, Item>],
    ) -> bool {
        let Some(selection) = &self.selection else { return false };
        let here = selection.canvas == canvas && selection.page == page;
        here && bounds(items, &selection.ids).is_some_and(|b| b.outset(MARGIN / zoom).contains(at))
    }
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
