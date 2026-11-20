//! Find typed text: the text items whose words hold a phrase, on every page.
//!
//! A match ignores case and how the words are spaced: a run of spaces, tabs
//! or line breaks in the phrase or the text counts as one space, so a phrase
//! finds words a text has wrapped onto two lines. Matches come page by page,
//! in the canvas's page order, and on a page in reading order: top to
//! bottom, then left to right, then the older item first. Only text items
//! hold words (a PDF's words are not the canvas's). No errors: a phrase of
//! no words finds nothing.
//!
//! Find sees into folded boxes (docs/design/fold.md): a text a folded box
//! holds is found, in the reading order of the place the page shows for it,
//! the outermost folded box that holds it ([`Canvas::outermost`]); texts in
//! one box keep their own order.

use std::cmp::Ordering;

use super::{Canvas, Item, ItemId, Rect, Shape};

impl Canvas {
    /// The text items whose words hold `phrase`, in the order above.
    #[must_use]
    pub fn find(&self, phrase: &str) -> Vec<ItemId> {
        let phrase = folded(phrase);
        if phrase.is_empty() {
            return Vec::new();
        }
        let holds = |i: &&Item| matches!(i.shape(), Shape::Text { text, .. } if folded(text).contains(&phrase));
        let shown = |i: &Item| {
            let box_ = self.outermost(i.id()).and_then(|b| self.item(b));
            (box_.map_or_else(|| i.bounds(), Item::bounds), i.bounds())
        };
        let mut found = Vec::new();
        for page in self.pages() {
            let mut on: Vec<(Rect, Rect, ItemId)> = self
                .items(page.id())
                .filter(holds)
                .map(|i| (shown(i).0, shown(i).1, i.id()))
                .collect();
            on.sort_by(|(a_at, a_own, a), (b_at, b_own, b)| {
                by_place(*a_at, *b_at).then(by_place(*a_own, *b_own)).then(a.cmp(b))
            });
            found.extend(on.into_iter().map(|(.., id)| id));
        }
        found
    }
}

/// Top to bottom, then left to right.
fn by_place(a: Rect, b: Rect) -> Ordering {
    a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x))
}

/// `text` as a match compares it: lowercase, its words one space apart.
fn folded(text: &str) -> String {
    text.split_whitespace().map(str::to_lowercase).collect::<Vec<_>>().join(" ")
}
