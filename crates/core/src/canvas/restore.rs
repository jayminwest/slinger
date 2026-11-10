//! Putting things back: the two operations undo and redo write. They take
//! whole items and pages, ids included, so the log replays to the same canvas.

use super::item::invalid;
use super::{Applied, Author, Canvas, Item, Page, not_yours};
use crate::{Error, Result};

impl Canvas {
    /// [`super::Action::Restore`]: put a removed item back under its id.
    pub(super) fn restore(&mut self, item: Item, by: &Author) -> Result<Applied> {
        self.page_index(item.page)?;
        if self.items.contains_key(&item.id) {
            return Err(still_here(&format!("{}", item.id), "items"));
        }
        self.check_item(&item, by)?;
        self.items.insert(item.id, item);
        Ok(Applied::Changed)
    }

    /// [`super::Action::PageRestore`]: put a removed page back with its items.
    pub(super) fn restore_page(
        &mut self,
        page: &Page,
        index: usize,
        items: Vec<Item>,
        by: &Author,
    ) -> Result<Applied> {
        if page.id.get() == 0 || page.id.get() >= self.next_page {
            return Err(never_made(&format!("{}", page.id)));
        }
        if self.page(page.id).is_some() {
            return Err(still_here(&format!("{}", page.id), "pages"));
        }
        let name = self.free_name(&page.name, None)?;
        for item in &items {
            if item.page != page.id {
                return Err(invalid(
                    format!("{} is not on {}", item.id, page.id),
                    "restore a page with only its own items",
                ));
            }
            if self.items.contains_key(&item.id) {
                return Err(still_here(&format!("{}", item.id), "items"));
            }
            self.check_item(item, by)?;
        }
        let index = index.min(self.pages.len());
        self.pages.insert(index, Page { id: page.id, name });
        self.items.extend(items.into_iter().map(|item| (item.id, item)));
        Ok(Applied::Changed)
    }

    /// The id was handed out, the shape can be drawn and `by` may change
    /// what its maker made. An arrow's targets are not checked: an arrow may
    /// outlive the items it points at.
    fn check_item(&self, item: &Item, by: &Author) -> Result<()> {
        if item.id.get() == 0 || item.id.get() >= self.next_item {
            return Err(never_made(&format!("{}", item.id)));
        }
        item.shape.check()?;
        if !by.may_change(&item.author) {
            return Err(not_yours(by, &format!("{}", item.id), &item.author));
        }
        Ok(())
    }
}

fn still_here(what: &str, kind: &str) -> Error {
    Error::Conflict {
        message: format!("{what} is on the canvas"),
        hint: Some(format!("restore only {kind} that were removed")),
    }
}

fn never_made(what: &str) -> Error {
    invalid(
        format!("{what} was never made on this canvas"),
        "restore only what the canvas once held; add new things with add",
    )
}
