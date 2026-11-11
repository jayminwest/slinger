//! Pages (SPEC D6): which page a page command turns a pane to, the name a
//! new page gets, where a pane goes when its page is removed, the line
//! that names a pane's page in its footer, and the two questions a page command
//! asks: a rename's new name, typed, and whether to remove a page that has
//! items, asked once. Pure: `canvases` hands in the canvas's pages and the
//! typed keys, and sends the [`PageOp`] that comes out through the ink's
//! queue, in turn with the pen's changes.

use crate::layout::PaneId;

/// A canvas page as the app knows it: its id and its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub id: String,
    pub name: String,
}

/// A change to a canvas's pages: one operation of one `canvas_apply`, so
/// one undo step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageOp {
    /// A new page, last, named `name`.
    Add {
        name: String,
    },
    Rename {
        page: String,
        name: String,
    },
    /// Page `page` and its items go.
    Remove {
        page: String,
    },
}

/// The page after `at` among `pages` (before it, when not `forward`),
/// round from the last to the first and back. A page not found counts as
/// the first. `None` without pages.
pub fn step<'a>(pages: &'a [Page], at: &str, forward: bool) -> Option<&'a str> {
    let count = pages.len();
    if count == 0 {
        return None;
    }
    let here = pages.iter().position(|p| p.id == at).unwrap_or(0);
    let there = if forward { here + 1 } else { here + count - 1 };
    pages.get(there % count).map(|p| p.id.as_str())
}

/// The name a new page gets: `Page <n>`, with `n` one more than the number
/// of pages, or the next number no page is named after.
pub fn new_name(pages: &[Page]) -> String {
    let taken = |name: &String| pages.iter().any(|p| p.name == *name);
    // One of the next `len + 1` numbers is free: there are only `len` names.
    let free = pages.len() + 1..=2 * pages.len() + 1;
    free.map(|n| format!("Page {n}")).find(|name| !taken(name)).unwrap_or_default()
}

/// Where a pane on page `at` goes when it is removed: the page before it,
/// or the one after the first. `None` when `at` is the only page (the
/// canvas keeps its last page) or not among `pages`.
pub fn after_remove<'a>(pages: &'a [Page], at: &str) -> Option<&'a str> {
    let here = pages.iter().position(|p| p.id == at)?;
    let there = if here == 0 { 1 } else { here - 1 };
    pages.get(there).map(|p| p.id.as_str())
}

/// The line that names page `at` in its pane's footer: its name, and
/// where it is among the pages. Empty for a page not among them.
pub fn line(pages: &[Page], at: &str) -> String {
    let Some(here) = pages.iter().position(|p| p.id == at) else { return String::new() };
    format!("{} · {}/{}", pages[here].name, here + 1, pages.len())
}

/// A question a page command asks, in the pane it was asked in.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Ask {
    /// The new name of `page`, as typed so far.
    Rename { pane: PaneId, canvas: String, page: String, text: String },
    /// Whether to remove `page`, which has `items` items.
    Remove { pane: PaneId, canvas: String, page: Page, items: usize },
}

/// The question open, if any.
#[derive(Debug, Default)]
pub struct Pages {
    ask: Option<Ask>,
}

impl Pages {
    /// Asks for a new name for page `shown` of `canvas`, in `pane`: typed keys
    /// change its name, return keeps it, escape keeps the old one.
    pub fn rename(&mut self, pane: PaneId, canvas: &str, shown: &Page) {
        let (canvas, text) = (canvas.to_string(), shown.name.clone());
        self.ask = Some(Ask::Rename { pane, canvas, page: shown.id.clone(), text });
    }

    /// The remove-page command on page `shown` of `canvas`, in `pane`, with
    /// `items` items on it: an empty page goes at once; else the first press
    /// asks, and a second (or return) removes it. The removal to send, if any.
    pub fn remove(
        &mut self,
        pane: PaneId,
        canvas: &str,
        shown: &Page,
        items: usize,
    ) -> Option<(String, PageOp)> {
        let asked = matches!(&self.ask,
            Some(Ask::Remove { canvas: c, page: p, .. }) if c == canvas && p.id == shown.id);
        if items == 0 || asked {
            self.ask = None;
            return Some((canvas.to_string(), PageOp::Remove { page: shown.id.clone() }));
        }
        let canvas = canvas.to_string();
        self.ask = Some(Ask::Remove { pane, canvas, page: shown.clone(), items });
        None
    }

    /// Whether a question is open: the keys that type answer it.
    pub fn asking(&self) -> bool {
        self.ask.is_some()
    }

    /// Closes the question unanswered.
    pub fn cancel(&mut self) {
        self.ask = None;
    }

    /// Typed `text` answers the question: a rename takes each character
    /// (delete takes one back), a removal waits for return; return ends
    /// either, escape closes either with no change. The change to send, if
    /// the answer makes one (a rename to a new, not blank, name).
    pub fn typed(&mut self, text: &str) -> Option<(String, PageOp)> {
        for c in text.chars() {
            let Some(ask) = &mut self.ask else { return None };
            match (c, ask) {
                ('\u{1b}', _) => self.ask = None,
                ('\r' | '\n', _) => return self.answer(),
                ('\u{7f}' | '\u{8}', Ask::Rename { text, .. }) => {
                    text.pop();
                }
                // The arrow and function keys are in this private range.
                (c, _) if c.is_control() || ('\u{f700}'..='\u{f8ff}').contains(&c) => {}
                (c, Ask::Rename { text, .. }) => text.push(c),
                (_, Ask::Remove { .. }) => {}
            }
        }
        None
    }

    /// The question's answer as a change, and the question closed.
    fn answer(&mut self) -> Option<(String, PageOp)> {
        match self.ask.take()? {
            Ask::Rename { canvas, page, text, .. } => {
                let name = text.trim().to_string();
                (!name.is_empty()).then_some((canvas, PageOp::Rename { page, name }))
            }
            Ask::Remove { canvas, page, .. } => Some((canvas, PageOp::Remove { page: page.id })),
        }
    }

    /// What `pane` says in place of its page's line while it asks.
    pub fn prompt(&self, pane: PaneId) -> Option<String> {
        match self.ask.as_ref()? {
            Ask::Rename { pane: p, text, .. } if *p == pane => {
                Some(format!("page name: {text}▏ (return: keep, escape: cancel)"))
            }
            Ask::Remove { pane: p, page, items, .. } if *p == pane => {
                let s = if *items == 1 { "" } else { "s" };
                Some(format!(
                    "remove “{}” and its {items} item{s}? return or remove-page: yes, escape: no",
                    page.name
                ))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
