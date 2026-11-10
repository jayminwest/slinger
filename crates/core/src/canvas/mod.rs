//! The canvas: pages of items, changed only by operations.
//!
//! The interface is three things: build a canvas ([`Canvas::new`]), change it
//! with [`Canvas::apply`], and query it ([`Canvas::pages`], [`Canvas::items`],
//! [`Canvas::item`]). An operation is plain data ([`Op`]), so a log of them can
//! be stored, sent and replayed: the same operations in the same order build
//! the same canvas, ids included. How the canvas stores its items is hidden.
//!
//! The rules every operation keeps:
//! - Removing an item or a page that is already gone succeeds (SPEC B4).
//! - An agent edits, moves or removes only items it made. The user's ink is
//!   never changed by an agent (SPEC section 7).
//! - A canvas always has at least one page, and page names are unique.

mod delta;
pub mod file;
mod geometry;
mod history;
mod item;
mod restore;

use std::collections::BTreeMap;

pub use delta::{Changes, Marker, changes_since};
pub use history::History;
pub use item::{Author, Colour, End, Item, ItemId, Page, PageId, Point, Rect, Shape, Time};

use crate::{Error, Result};
use item::invalid;

/// The name of the page a new canvas starts with.
pub const FIRST_PAGE: &str = "Page 1";

/// One change to a canvas: who asks, when, and what.
#[derive(Debug, Clone, PartialEq)]
pub struct Op {
    pub by: Author,
    pub at: Time,
    pub action: Action,
}

/// What an operation does.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Put a new item on a page. The item is the author's, made at the op's time.
    Add { page: PageId, shape: Shape },
    /// Replace an item's shape. Its author and time stay.
    Edit { item: ItemId, shape: Shape },
    /// Shift an item by `by` canvas units.
    Move { item: ItemId, by: Point },
    /// Take an item off the canvas.
    Remove { item: ItemId },
    /// Add a page after the last one.
    PageAdd { name: String },
    /// Give a page a new name.
    PageRename { page: PageId, name: String },
    /// Take a page and every item on it off the canvas.
    PageRemove { page: PageId },
    /// Put a removed item back exactly as given, under its own id. Undo and
    /// redo write it; see [`History`].
    Restore { item: Item },
    /// Put a removed page back at `index` (clamped to the end), with its
    /// items. Undo writes it.
    PageRestore { page: Page, index: usize, items: Vec<Item> },
}

/// What applying an operation did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// A new item, with its id.
    Added(ItemId),
    /// A new page, with its id.
    PageAdded(PageId),
    /// The canvas changed.
    Changed,
    /// The canvas was already as asked (for example, the item was already gone).
    Unchanged,
}

/// A canvas: one or more named pages, each holding items in drawing order.
#[derive(Debug, Clone, PartialEq)]
pub struct Canvas {
    pages: Vec<Page>,
    /// Keyed by id. Ids rise, so this is also drawing order.
    items: BTreeMap<ItemId, Item>,
    next_item: u64,
    next_page: u64,
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

impl Canvas {
    /// An empty canvas with one page, named [`FIRST_PAGE`].
    #[must_use]
    pub fn new() -> Self {
        let first = Page { id: PageId::new(1), name: FIRST_PAGE.into() };
        Self { pages: vec![first], items: BTreeMap::new(), next_item: 1, next_page: 2 }
    }

    /// Applies one operation. This is the only way to change a canvas. A
    /// failed operation changes nothing.
    ///
    /// # Errors
    ///
    /// - [`Error::NotFound`] when the op names a page, or an item to edit or
    ///   move, that is not on the canvas, or an arrow points at a missing item.
    /// - [`Error::Invalid`] when an agent would change an item it did not make
    ///   or a page holding such items, an agent has a blank name, a page name
    ///   is blank, or a shape cannot be drawn (no points, a width not above
    ///   zero, a size below zero, a value that is not a number), or a restore
    ///   names an id the canvas never handed out.
    /// - [`Error::Conflict`] when a page name is taken, the op would remove
    ///   the last page, or a restore names a page or item still here.
    pub fn apply(&mut self, op: Op) -> Result<Applied> {
        let Op { by, at, action } = op;
        if matches!(&by, Author::Agent(name) if name.trim().is_empty()) {
            return Err(invalid("the agent has no name", "name the agent that asks"));
        }
        match action {
            Action::Add { page, shape } => {
                self.page_index(page)?;
                self.check_shape(&shape, None)?;
                let id = ItemId::new(self.next_item);
                self.next_item += 1;
                self.items.insert(id, Item { id, page, author: by, time: at, shape });
                Ok(Applied::Added(id))
            }
            Action::Edit { item, shape } => {
                self.check_shape(&shape, Some(item))?;
                let found = self.changeable(item, &by)?;
                found.shape = shape;
                Ok(Applied::Changed)
            }
            Action::Move { item, by: offset } => {
                if !(offset.x.is_finite() && offset.y.is_finite()) {
                    return Err(invalid("the move is not a number", "give a finite offset"));
                }
                let found = self.changeable(item, &by)?;
                found.shape = found.shape.shifted(offset);
                Ok(Applied::Changed)
            }
            Action::Remove { item } => {
                if !self.items.contains_key(&item) {
                    return Ok(Applied::Unchanged);
                }
                self.changeable(item, &by)?;
                self.items.remove(&item);
                Ok(Applied::Changed)
            }
            Action::PageAdd { name } => {
                let name = self.free_name(&name, None)?;
                let id = PageId::new(self.next_page);
                self.next_page += 1;
                self.pages.push(Page { id, name });
                Ok(Applied::PageAdded(id))
            }
            Action::PageRename { page, name } => {
                let i = self.page_index(page)?;
                let name = self.free_name(&name, Some(page))?;
                if self.pages[i].name == name {
                    return Ok(Applied::Unchanged);
                }
                self.pages[i].name = name;
                Ok(Applied::Changed)
            }
            Action::PageRemove { page } => self.remove_page(page, &by),
            Action::Restore { item } => self.restore(item, &by),
            Action::PageRestore { page, index, items } => {
                self.restore_page(&page, index, items, &by)
            }
        }
    }

    /// The pages, in order.
    pub fn pages(&self) -> impl Iterator<Item = &Page> {
        self.pages.iter()
    }

    /// The page with this id, if it is on the canvas.
    #[must_use]
    pub fn page(&self, id: PageId) -> Option<&Page> {
        self.pages.iter().find(|p| p.id == id)
    }

    /// The page with this name, if there is one.
    #[must_use]
    pub fn page_named(&self, name: &str) -> Option<&Page> {
        self.pages.iter().find(|p| p.name == name.trim())
    }

    /// The items on a page, in drawing order (first made, first drawn). A page
    /// that is not on the canvas has no items.
    pub fn items(&self, page: PageId) -> impl Iterator<Item = &Item> {
        self.items.values().filter(move |i| i.page == page)
    }

    /// The item with this id, if it is on the canvas.
    #[must_use]
    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.items.get(&id)
    }

    fn page_index(&self, id: PageId) -> Result<usize> {
        self.pages.iter().position(|p| p.id == id).ok_or_else(|| Error::NotFound {
            what: id.to_string(),
            hint: Some("list the canvas's pages for their ids".into()),
        })
    }

    /// The shape can be drawn and every item it points at is here. An edit
    /// may keep pointing at an item that is gone (an arrow outlives its
    /// targets, and undo puts such an arrow back), but not point anew at one.
    fn check_shape(&self, shape: &Shape, editing: Option<ItemId>) -> Result<()> {
        shape.check()?;
        let kept = editing.and_then(|id| self.items.get(&id)).map(|i| &i.shape);
        let gone = |t: &ItemId| {
            !self.items.contains_key(t) && !kept.is_some_and(|k| k.targets().any(|old| old == *t))
        };
        match shape.targets().find(gone) {
            Some(missing) => Err(Error::NotFound {
                what: missing.to_string(),
                hint: Some("point the arrow at an item on the canvas, or at none".into()),
            }),
            None => Ok(()),
        }
    }

    /// The item, if it exists and `by` may change it.
    fn changeable(&mut self, id: ItemId, by: &Author) -> Result<&mut Item> {
        let item = self.items.get_mut(&id).ok_or_else(|| Error::NotFound {
            what: id.to_string(),
            hint: Some("it may have been removed; read the canvas again".into()),
        })?;
        if !by.may_change(&item.author) {
            return Err(not_yours(by, &format!("{id}"), &item.author));
        }
        Ok(item)
    }

    /// `name` trimmed, if it is not blank and no other page has it.
    fn free_name(&self, name: &str, renaming: Option<PageId>) -> Result<String> {
        let name = name.trim();
        if name.is_empty() {
            return Err(invalid("the page name is blank", "give the page a name"));
        }
        match self.page_named(name) {
            Some(other) if Some(other.id) != renaming => Err(Error::Conflict {
                message: format!("a page named {name:?} exists"),
                hint: Some("pick another name".into()),
            }),
            _ => Ok(name.to_string()),
        }
    }

    fn remove_page(&mut self, page: PageId, by: &Author) -> Result<Applied> {
        let Some(i) = self.pages.iter().position(|p| p.id == page) else {
            return Ok(Applied::Unchanged);
        };
        if self.pages.len() == 1 {
            return Err(Error::Conflict {
                message: "a canvas keeps at least one page".into(),
                hint: Some("add another page before you remove this one".into()),
            });
        }
        if let Some(other) = self.items(page).find(|item| !by.may_change(&item.author)) {
            return Err(not_yours(
                by,
                &format!("{page}, which holds {},", other.id),
                &other.author,
            ));
        }
        self.pages.remove(i);
        self.items.retain(|_, item| item.page != page);
        Ok(Applied::Changed)
    }
}

fn not_yours(by: &Author, what: &str, maker: &Author) -> Error {
    invalid(
        format!("{by} may not change {what} made by {maker}"),
        "an agent changes only items it made; add a new item instead",
    )
}
