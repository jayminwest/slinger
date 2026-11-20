//! Find typed text: the text items whose words hold a phrase, on every page.
//!
//! A match ignores case and how the words are spaced: a run of spaces, tabs
//! or line breaks in the phrase or the text counts as one space, so a phrase
//! finds words a text has wrapped onto two lines. Matches come page by page,
//! in the canvas's page order, and on a page in reading order: top to
//! bottom, then left to right, then the older item first. Only text items
//! hold words (a PDF's words are not the canvas's). No errors: a phrase of
//! no words finds nothing.

use super::{Canvas, Item, ItemId, Shape};

impl Canvas {
    /// The text items whose words hold `phrase`, in the order above.
    #[must_use]
    pub fn find(&self, phrase: &str) -> Vec<ItemId> {
        let phrase = folded(phrase);
        if phrase.is_empty() {
            return Vec::new();
        }
        let holds = |i: &&Item| matches!(i.shape(), Shape::Text { text, .. } if folded(text).contains(&phrase));
        let mut found = Vec::new();
        for page in self.pages() {
            let mut on: Vec<&Item> = self.items(page.id()).filter(holds).collect();
            on.sort_by(|a, b| {
                let (a_at, b_at) = (a.bounds(), b.bounds());
                a_at.y.total_cmp(&b_at.y).then(a_at.x.total_cmp(&b_at.x)).then(a.id().cmp(&b.id()))
            });
            found.extend(on.into_iter().map(Item::id));
        }
        found
    }
}

/// `text` as a match compares it: lowercase, its words one space apart.
fn folded(text: &str) -> String {
    text.split_whitespace().map(str::to_lowercase).collect::<Vec<_>>().join(" ")
}
