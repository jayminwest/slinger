//! Find typed text on a canvas's pages (`find`, `find-next`): which text a
//! find goes to among those the daemon found (`canvas_find`: page by page,
//! each in reading order), and what the flash says of it. Pure: `canvases`
//! asks the daemon, turns the pane to the text's page and centres it.

use super::Page;
use crate::layout::{PaneId, Rect};
use crate::scene::canvas::ItemId;

/// The words a find's entry was answered with, and where it was asked: the
/// pane and its frame, and the canvas it shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Sought {
    pub pane: (PaneId, Rect),
    pub canvas: String,
    pub text: String,
}

/// One text that holds the words: its page, its id and its frame, in
/// canvas points, and the outermost folded box that hides it, if one does
/// (docs/design/fold.md): a find goes to that box.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub page: String,
    pub item: ItemId,
    pub frame: Rect,
    pub folded: Option<ItemId>,
}

/// Which of `hits` a find goes to. After `last`, the text the last find of
/// the same words went to, when it is still among them: the next, after the
/// last the first (`find-next` goes round). Else the first on the page
/// `shown` or a page after it among `pages`, after the last page the first:
/// a find starts where the user is. `None` when there are no hits.
pub fn next(hits: &[Hit], pages: &[Page], shown: &str, last: Option<ItemId>) -> Option<usize> {
    if hits.is_empty() {
        return None;
    }
    if let Some(at) = last.and_then(|l| hits.iter().position(|h| h.item == l)) {
        return Some((at + 1) % hits.len());
    }
    let place = |page: &str| pages.iter().position(|p| p.id == page);
    let here = place(shown).unwrap_or(0);
    let from = hits.iter().position(|h| place(&h.page).is_some_and(|p| p >= here));
    Some(from.unwrap_or(0))
}

/// What the flash says of a find of `text` that went to hit `at` of `hits`
/// on a canvas with `pages`: “plan” 2/5 · its page's name, and that it is
/// in a folded box when one hides it; or that no text holds it.
pub fn said(text: &str, hits: &[Hit], at: Option<usize>, pages: &[Page]) -> String {
    let Some(hit) = at.and_then(|i| hits.get(i)) else {
        return format!("No text holds “{text}”");
    };
    let page = pages.iter().find(|p| p.id == hit.page).map_or(hit.page.as_str(), |p| &p.name);
    let inside = if hit.folded.is_some() { " · in a folded box" } else { "" };
    format!("“{text}” {}/{} · {page}{inside}", at.unwrap_or(0) + 1, hits.len())
}

#[cfg(test)]
mod tests;
