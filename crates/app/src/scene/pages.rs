//! Pages (SPEC D6): which page a page command turns a pane to, the name a
//! new page gets, where a pane goes when its page is removed, the line
//! that names a pane's page in its footer, and the two questions a page command
//! asks: a rename's new name, in an entry like the sling's (with the pen's
//! buttons beside it: [`crate::scene::sling::pen`]), and whether to remove a page
//! that has items, asked once; and what a page command did, for the flash
//! (SPEC P6: the footer's line alone is easy to miss). Pure: `canvases` hands in the canvas's pages and the
//! typed keys, and sends the [`PageOp`] that comes out through the ink's
//! queue, in turn with the pen's changes.

pub mod menu;

use crate::config::{Rgb, Theme};
use crate::layout::{PaneId, Point, Rect};
use crate::scene::paste;
use crate::scene::sling::pen::beside;
use crate::scene::sling::{Entry, SEND, under};

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

/// The line that names page `at` in its pane's footer: its name (a long
/// one cut short, so the footer keeps room for its buttons), and where it
/// is among the pages. Empty for a page not among them.
pub fn line(pages: &[Page], at: &str) -> String {
    let Some(here) = pages.iter().position(|p| p.id == at) else { return String::new() };
    let name = &pages[here].name;
    let name = match name.char_indices().nth(NAME) {
        Some((cut, _)) => format!("{}…", &name[..cut]),
        None => name.clone(),
    };
    format!("{name} · {}/{}", here + 1, pages.len())
}

/// The most characters of a page's name a footer shows.
const NAME: usize = 18;

/// A question a page command asks, in the pane it was asked in.
#[derive(Debug, Clone, PartialEq)]
enum Ask {
    /// The new name of `page`, as typed so far, in its entry's box `frame`.
    Rename { pane: PaneId, canvas: String, page: String, text: String, frame: Rect },
    /// Whether to remove `page`, which has `items` items.
    Remove { pane: PaneId, canvas: String, page: Page, items: usize },
}

/// The question open, if any, and what the last page command did, until
/// the flash takes it.
#[derive(Debug, Default)]
pub struct Pages {
    ask: Option<Ask>,
    said: Option<String>,
}

impl Pages {
    /// Asks for a new name for page `shown` of `canvas`, in `pane`, whose
    /// frame is `within`: an entry ([`Pages::entry`]) starts from the old
    /// name; return keeps the new one, escape keeps the old one.
    pub fn rename(&mut self, (pane, within): (PaneId, Rect), canvas: &str, shown: &Page) {
        let (canvas, text, frame) = (canvas.to_string(), shown.name.clone(), beside(within));
        self.ask = Some(Ask::Rename { pane, canvas, page: shown.id.clone(), text, frame });
        self.said = Some(format!("Rename “{}”", shown.name));
    }

    /// Closes the new name's entry, the old name kept. False when none is
    /// asked.
    pub fn cancel_rename(&mut self) -> bool {
        let renaming = self.renaming();
        if renaming {
            self.ask = None;
        }
        renaming
    }

    /// Whether a new name is being typed: its entry has the keys.
    pub fn renaming(&self) -> bool {
        matches!(self.ask, Some(Ask::Rename { .. }))
    }

    /// The new name's entry is now `text`. Ignored when none is asked.
    pub fn named(&mut self, text: &str) {
        if let Some(Ask::Rename { text: name, .. }) = &mut self.ask {
            *name = text.to_string();
        }
    }

    /// The new name's entry, while it is asked, in `theme`'s colours: the
    /// look of every entry ([`Entry::framed`]).
    pub fn entry(&self, theme: &Theme) -> Option<Entry> {
        let Some(Ask::Rename { text, frame, .. }) = &self.ask else { return None };
        let keys = format!("{} renames · esc cancels", SEND.glyphs());
        let mut entry = Entry::framed(*frame, theme, "Rename page".into(), keys);
        entry.text.clone_from(text);
        entry.placeholder = "a name for the page".into();
        entry.done = Some("Rename");
        Some(entry)
    }

    /// `lines` (text and colour) under the new name's entry: what
    /// dictation says.
    pub fn under(&self, theme: &Theme, lines: Vec<(String, Rgb)>) -> Vec<crate::scene::Label> {
        let Some(Ask::Rename { frame, .. }) = &self.ask else { return Vec::new() };
        under(*frame, None, theme, lines)
    }

    /// Whether `at`, a point in the window, is on the new name's entry.
    pub fn covers(&self, at: Point) -> bool {
        matches!(&self.ask, Some(Ask::Rename { frame, .. }) if frame.contains(at))
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
        self.said = Some(format!("Remove “{}”?", shown.name));
        None
    }

    /// Pasted `text` answers the question: a rename takes it, on one line
    /// ([`paste::one_line`]); a removal waits for return. Returns false when
    /// no question is open.
    pub fn pasted(&mut self, text: &str) -> bool {
        match &mut self.ask {
            Some(Ask::Rename { text: name, .. }) => name.push_str(&paste::one_line(text)),
            Some(Ask::Remove { .. }) => {}
            None => return false,
        }
        true
    }

    /// A pane turned to page `to` among `pages`: its line flashes.
    pub fn turned(&mut self, pages: &[Page], to: &str) {
        self.said = Some(line(pages, to)).filter(|l| !l.is_empty());
    }

    /// `op` goes to a canvas whose pages were `pages`: a new page flashes
    /// its name and place (it comes last), a rename the new name, a removal
    /// the old one.
    pub fn changed(&mut self, pages: &[Page], op: &PageOp) {
        let name = |id: &str| pages.iter().find(|p| p.id == id).map_or(id, |p| &p.name).to_string();
        self.said = Some(match op {
            PageOp::Add { name } => format!("New page: {name} · {n}/{n}", n = pages.len() + 1),
            PageOp::Rename { name, .. } => format!("Renamed: {name}"),
            PageOp::Remove { page } => format!("Removed: {}", name(page)),
        });
    }

    /// The canvas's last page cannot go: says so.
    pub fn kept_last(&mut self) {
        self.said = Some("The last page stays".to_string());
    }

    /// What the last page command did, once: the flash shows it.
    pub fn said(&mut self) -> Option<String> {
        self.said.take()
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

    /// What `pane` says in place of its page's line while it asks whether to
    /// remove a page (a new name has its entry).
    pub fn prompt(&self, pane: PaneId) -> Option<String> {
        match self.ask.as_ref()? {
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
