//! A figure placed whole (docs/design/figures.md, "The verbs" and "Replace
//! by match"): a set of shapes, each a `canvas apply` `add` item object in
//! the figure's own coordinates (tenths, no `page`, no `id`), placed as
//! one undo step and named. The engine that makes the shapes
//! (`crates/script`) is the daemon's; this is where they meet the canvas.
//!
//! [`Placement`] says where the figure goes: its frame (the joint frame of
//! its shapes) beside an item where [`Canvas::free_beside`] finds room, or
//! in a box's room as [`Canvas::free_in`] finds it; or its origin (0,0) on
//! an item's top left (`Over`, which moves nothing) or at a point.
//!
//! A name the canvas already has is drawn again, by match on (part, kind,
//! ordinal) (`canvas/figure/matching.rs`): an item that is the same stays
//! as it is, under its id, and one that changed is set in place, so arrows
//! bound to it hold. The same shapes in the same place write nothing at
//! all. Only the items of that figure the author may change are matched,
//! so the user's ink (in no figure) is never touched. An agent may not draw
//! over a figure another made (`conflict`); the user may draw over any.
//!
//! An agent's figure text keeps the agent's length rule ([`AGENT_ROWS`]):
//! a figure is not a way around it.
//!
//! [`Canvas::free_beside`]: crate::canvas::Canvas::free_beside
//! [`Canvas::free_in`]: crate::canvas::Canvas::free_in
//! [`AGENT_ROWS`]: super::AGENT_ROWS

use std::borrow::Cow;
use std::mem::{Discriminant, discriminant};

use serde_json::{Map, Value, json};

use super::{Log, Written, json as file_json, snap};
use crate::canvas::figure::FIGURE_RULE;
use crate::canvas::figure::matching::{Pair, pair};
use crate::canvas::{
    Action, Author, Canvas, Item, ItemId, Member, Op, PageId, Point, Rect, Shape, Style, Time,
    figure_name,
};
use crate::{Error, Result};

/// The most shapes a figure may have, as `slinger_script::ITEMS`.
pub const FIGURE_ITEMS: usize = 5_000;

/// Where a figure goes. An id is what an item id is anywhere (`i12`, `b4`,
/// a figure address).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placement {
    /// Its frame beside the item, where it covers nothing.
    Beside(String),
    /// Its origin on the item's top left; nothing moves.
    Over(String),
    /// Its frame in the box's room, where it covers nothing the box holds.
    /// A folded box takes it in.
    In(String),
    /// Its origin at `x`,`y`, in tenths of a unit, on the page with this id
    /// or name, or the first.
    At { page: Option<String>, x: i64, y: i64 },
}

/// What a draw would do: the figure as it would be placed. Nothing is
/// written until [`Log::place_figure`].
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    /// The figure's name.
    pub name: String,
    /// Its page's id (`p1`).
    pub page: String,
    /// Where its (0,0) goes, `[x, y]` in tenths.
    pub origin: [i64; 2],
    /// The joint frame of its items as placed, `[x, y, w, h]` in tenths.
    pub frame: [i64; 4],
    /// Its part names, in the order the shapes first name them.
    pub parts: Vec<String>,
    /// Its items as placed, in the order of the shapes: a kept item under
    /// its own id, maker and time; a new one under the id its add gives it.
    pub items: Vec<Item>,
    /// The step that places it: sets, then adds, then removes.
    actions: Vec<Action>,
}

impl Draft {
    /// Whether the draw changes nothing on the canvas.
    #[must_use]
    pub fn unchanged(&self) -> bool {
        self.actions.is_empty()
    }
}

/// What a draw did: the figure as placed and the lines written (none when
/// it changed nothing).
#[derive(Debug, Clone, PartialEq)]
pub struct Drawn {
    pub draft: Draft,
    pub written: Written,
}

/// A shape read from a request: its style, shape and figure.
type Read = (Style, Shape, Member);

/// An item's key for the match: its part and its kind.
type Key = (Option<String>, Discriminant<Shape>);

impl Log {
    /// The frame a script draws over or in, `[x, y, w, h]` in tenths: the
    /// item's, for [`Placement::Over`]; the box's room (its open frame,
    /// folded or not), for [`Placement::In`]. `None` for the others.
    ///
    /// # Errors
    ///
    /// Those of an id that names nothing or is not one, and `invalid` for
    /// `In` an item that is not a box.
    pub fn target(&self, at: &Placement) -> Result<Option<[i64; 4]>> {
        let frame = match at {
            Placement::Over(id) => self.named(id)?.1,
            Placement::In(id) => self.room(id)?.1,
            Placement::Beside(_) | Placement::At { .. } => return Ok(None),
        };
        Ok(Some(tenths(frame)))
    }

    /// The figure `name` that `items` (see the module doc) make, placed as
    /// `at` says, as `by` would draw it at `time`, without writing it.
    ///
    /// # Errors
    ///
    /// - `invalid`: a name that is not a figure name, no shapes or more than
    ///   [`FIGURE_ITEMS`], a shape that is not an `add` item object (the
    ///   message names its place, from 0) or that has a `page`, an `id` or
    ///   another `fig`, and `In` an item that is not a box.
    /// - `conflict`: the name is a figure `by` may not change, or the box it
    ///   goes in has no room.
    /// - `not_found`: an id or page that names nothing.
    /// - `too_long`: an agent's text over [`super::AGENT_ROWS`] rows.
    pub fn draft_figure(
        &self,
        by: &Author,
        time: Time,
        name: &str,
        items: &[Value],
        at: &Placement,
    ) -> Result<Draft> {
        if !figure_name(name) {
            return Err(invalid(format!("{name:?} is not a figure name"), FIGURE_RULE));
        }
        if items.is_empty() || items.len() > FIGURE_ITEMS {
            let hint = format!("draw from 1 to {FIGURE_ITEMS} shapes");
            return Err(invalid(format!("the figure has {} shapes", items.len()), &hint));
        }
        let shapes = items
            .iter()
            .enumerate()
            .map(|(n, item)| self.figure_shape(name, item).map_err(|e| at_shape(n, e)))
            .collect::<Result<Vec<Read>>>()?;
        let canvas = self.canvas();
        if let Some(figure) = canvas.figure(name)
            && !by.may_change(&figure.maker)
        {
            return Err(Error::Conflict {
                message: format!("figure {name} is {}'s", figure.maker),
                hint: Some("pick another --name".into()),
            });
        }
        let old: Vec<&Item> = canvas
            .items
            .values()
            .filter(|i| i.member.fig.as_deref() == Some(name) && by.may_change(&i.author))
            .collect();
        let local = joint(shapes.iter().map(|(_, shape, _)| shape.bounds())).unwrap_or_default();
        let (page, origin) = self.origin(at, local, &old)?;
        let shift = Point::new(file_json::units(origin[0]), file_json::units(origin[1]));
        let new: Vec<Read> = shapes
            .into_iter()
            .map(|(style, shape, member)| {
                let action = Action::Add { page, shape: shape.shifted(shift), style, member };
                match snap(Op { by: by.clone(), at: time, action }).action {
                    Action::Add { shape, style, member, .. } => (style, shape.fitted(), member),
                    _ => unreachable!("snap keeps an add an add"),
                }
            })
            .collect();
        let (actions, items) = self.replace(by, time, (page, at), &old, &new)?;
        let actions = if actions.is_empty() { actions } else { self.followed(by, actions) };
        self.short(by, &actions).map_err(|e| match e {
            Error::TooLong { message, .. } => Error::TooLong {
                message: format!("in figure {name}, {message}"),
                hint: Some("split it into several texts, or draw it wider (w)".into()),
            },
            other => other,
        })?;
        let mut parts: Vec<String> = Vec::new();
        for part in items.iter().filter_map(|i| i.member.part.as_ref()) {
            if !parts.contains(part) {
                parts.push(part.clone());
            }
        }
        let frame = joint(items.iter().map(Item::bounds)).unwrap_or_default();
        Ok(Draft {
            name: name.to_owned(),
            page: file_json::page_id(page),
            origin,
            frame: tenths(frame),
            parts,
            items,
            actions,
        })
    }

    /// Draws the figure `name` as [`Log::draft_figure`] drafts it, as one
    /// undo step of `by` at `time`, under the key `once` as
    /// [`Log::apply_once`] says. A draw that changes nothing writes nothing.
    ///
    /// # Errors
    ///
    /// Those of [`Log::draft_figure`], and those of applying the step: the
    /// log is then as it was.
    pub fn place_figure(
        &mut self,
        by: &Author,
        time: Time,
        name: &str,
        items: &[Value],
        at: &Placement,
        once: Option<&str>,
    ) -> Result<Drawn> {
        let draft = self.draft_figure(by, time, name, items, at)?;
        let actions = draft.actions.clone();
        let written = self.once(once, |log| {
            if actions.is_empty() { Ok(Written::default()) } else { log.step(by, time, actions) }
        })?;
        Ok(Drawn { draft, written })
    }

    /// The step that draws `new` (placed on `page`, as `at` says) over
    /// `old`, the figure's items `by` may change, by match: sets, then adds,
    /// then removes. Also the items as placed, in the order of `new`.
    fn replace(
        &self,
        by: &Author,
        time: Time,
        (page, at): (PageId, &Placement),
        old: &[&Item],
        new: &[Read],
    ) -> Result<(Vec<Action>, Vec<Item>)> {
        let pairs = if old.iter().all(|i| i.page == page) {
            let old_keys: Vec<Key> = old.iter().map(|i| key(&i.shape, &i.member)).collect();
            let new_keys: Vec<Key> = new.iter().map(|(_, s, m)| key(s, m)).collect();
            pair(&old_keys, &new_keys)
        } else {
            // A figure drawn on another page is drawn anew there.
            (0..new.len()).map(Pair::New).chain((0..old.len()).map(Pair::Gone)).collect()
        };
        let (mut actions, mut adds, mut removes) = (Vec::new(), Vec::new(), Vec::new());
        let mut placed: Vec<Option<Item>> = vec![None; new.len()];
        let mut next = self.canvas().next_item().get();
        for p in pairs {
            match p {
                Pair::Both(i, j) => {
                    let (style, shape, member) = new[j].clone();
                    let was = old[i];
                    let item = Item { shape, style, member, ..was.clone() };
                    if item != *was {
                        let (shape, member) = (item.shape.clone(), Some(item.member.clone()));
                        actions.push(Action::Edit {
                            item: was.id,
                            shape,
                            style: Some(style),
                            member,
                        });
                    }
                    placed[j] = Some(item);
                }
                Pair::New(j) => {
                    let (style, shape, member) = new[j].clone();
                    let (id, author) = (ItemId::new(next), by.clone());
                    next += 1;
                    placed[j] = Some(Item { id, page, author, time, shape, style, member });
                    adds.push(j);
                }
                Pair::Gone(i) => removes.push(Action::Remove { item: old[i].id }),
            }
        }
        for j in adds {
            let (style, shape, member) = new[j].clone();
            actions.push(Action::Add { page, shape, style, member });
            if let Placement::In(boxed) = at {
                self.join_box(boxed, &mut actions)?;
            }
        }
        actions.extend(removes);
        Ok((actions, placed.into_iter().flatten().collect()))
    }

    /// One shape of the figure `name`, read and checked as a request's
    /// `add` item, in the figure's coordinates.
    fn figure_shape(&self, name: &str, item: &Value) -> Result<Read> {
        let Some(fields) = item.as_object() else {
            return Err(invalid("it is not an object".into(), SHAPE_HINT));
        };
        if let Some(key) = ["page", "id"].into_iter().find(|k| fields.contains_key(*k)) {
            return Err(invalid(format!("it has `{key}`"), SHAPE_HINT));
        }
        if let Some(fig) = fields.get("fig")
            && fig.as_str() != Some(name)
        {
            return Err(invalid(format!("its `fig` {fig} is not {name:?}"), SHAPE_HINT));
        }
        let mut fields: Map<String, Value> = fields.clone();
        fields.insert("page".into(), "p1".into());
        fields.insert("fig".into(), name.into());
        let mut out = Vec::new();
        if let Value::Object(op) = json!({"op": "add", "item": fields}) {
            self.read_op(&op, &mut out)?;
        }
        match out.pop() {
            Some(Action::Add { shape, style, member, .. }) => Ok((style, shape.fitted(), member)),
            _ => Err(invalid("it is not an item".into(), SHAPE_HINT)),
        }
    }

    /// The page a figure goes on and where its (0,0) goes, in tenths, for
    /// a figure whose shapes' joint frame is `local` and whose own items
    /// (drawn anew) are `old`: those are not in its way.
    fn origin(&self, at: &Placement, local: Rect, old: &[&Item]) -> Result<(PageId, [i64; 2])> {
        let (page, free) = match at {
            Placement::At { page, x, y } => {
                return Ok((self.page(page.as_deref())?.id(), [*x, *y]));
            }
            Placement::Over(id) => {
                let (_, frame, page) = self.named(id)?;
                return Ok((page, [frame.x, frame.y].map(file_json::tenths)));
            }
            Placement::Beside(id) => {
                let (_, near, page) = self.named(id)?;
                (page, without(self.canvas(), old).free_beside(page, near, local.w, local.h))
            }
            Placement::In(id) => {
                let (boxed, _, page) = self.room(id)?;
                let free = without(self.canvas(), old).free_in(boxed, None, local.w, local.h);
                let free = free.ok_or_else(|| Error::Conflict {
                    message: format!("the box {id} has no room for the figure"),
                    hint: Some("draw it beside the box, or over it".into()),
                })?;
                (page, free)
            }
        };
        Ok((page, [free.x - local.x, free.y - local.y].map(file_json::tenths)))
    }

    /// The box `id` names, its room (its open frame) and its page.
    fn room(&self, id: &str) -> Result<(ItemId, Rect, PageId)> {
        let (item, _, page) = self.named(id)?;
        match self.canvas().item(item).map(Item::shape) {
            Some(Shape::Box { frame, fold, .. }) => {
                let (w, h) = fold.as_ref().map_or((frame.w, frame.h), |f| f.open);
                Ok((item, Rect::new(frame.x, frame.y, w, h), page))
            }
            _ => {
                Err(invalid(format!("{id} is not a box"), "draw in a box (b4), or beside an item"))
            }
        }
    }
}

fn key(shape: &Shape, member: &Member) -> Key {
    (member.part.clone(), discriminant(shape))
}

/// `canvas`, or a copy of it without `items` when there are any.
fn without<'a>(canvas: &'a Canvas, items: &[&Item]) -> Cow<'a, Canvas> {
    if items.is_empty() {
        return Cow::Borrowed(canvas);
    }
    let mut copy = canvas.clone();
    for item in items {
        copy.items.remove(&item.id);
    }
    Cow::Owned(copy)
}

/// The smallest rectangle that holds every one of `frames`.
fn joint(frames: impl Iterator<Item = Rect>) -> Option<Rect> {
    frames.reduce(crate::render::union)
}

fn tenths(r: Rect) -> [i64; 4] {
    [r.x, r.y, r.w, r.h].map(file_json::tenths)
}

/// `e`, about the shape at `n`.
fn at_shape(n: usize, e: Error) -> Error {
    match e {
        Error::Invalid { message, hint } => {
            Error::Invalid { message: format!("shape {n}: {message}"), hint }
        }
        other => other,
    }
}

const SHAPE_HINT: &str = "a figure's shape is a 'canvas apply' add item object in tenths, \
    relative to the figure's origin, with no page or id";

fn invalid(message: String, hint: &str) -> Error {
    Error::Invalid { message, hint: Some(hint.into()) }
}
