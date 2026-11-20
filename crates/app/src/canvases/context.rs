//! What a context click on a canvas pane is on ([`Canvases::context_at`]),
//! by Finder's rule: a click on an item outside the selection selects it
//! alone first; a click on a selected item, or inside the selection's box
//! where a press would grab it, keeps the selection. The item commands
//! then act on the selection as they always do. A click on the text being
//! typed is that text's; one anywhere else ends it first, as a press does.
//! The hit is the select tool's own ([`topmost`]), on what the pane shows:
//! inside the box it is stepped into, a folded box's contents hidden.

use std::borrow::Cow;

use super::Canvases;
use crate::commands::context::{Kind, Subject};
use crate::layout::{PaneId, Point, Rect};
use crate::panes::Panes;
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::select::{REACH, topmost};

/// What the pane shows under a context click, before the selection moves.
enum Found {
    /// The text being typed.
    Typing,
    /// The selection, kept: its items as shown.
    Kept(Vec<Kind>),
    /// An item outside the selection, of this kind.
    Hit(ItemId, Kind),
    /// No item, and no text being typed elsewhere to end first.
    Nothing,
}

impl Canvases {
    /// What a context click at `at`, a point in the window over `under`
    /// (the canvas pane of `panes` there and its frame), is on, the
    /// selection made as the module says: the text being typed, the item
    /// selected alone, the selection kept, or the empty canvas. `None` off
    /// every canvas pane, or before its page arrives.
    pub fn context_at(
        &mut self,
        panes: &Panes,
        under: Option<(PaneId, Rect)>,
        at: Point,
    ) -> Option<Subject> {
        let (found, here) = self.at_pane(panes, under, at, |c, here, at, zoom, items| {
            let found = if c.shapes.typing_at(here, at) {
                Found::Typing
            } else if c.select.grabs(here, at, zoom, items) {
                let ids = c.selection().map(|s| s.ids.clone()).unwrap_or_default();
                Found::Kept(ids.iter().filter_map(|id| kind_of(items, *id)).collect())
            } else {
                let id = topmost(here.0, items, at, REACH / zoom);
                id.and_then(|id| Some(Found::Hit(id, kind_of(items, id)?)))
                    .unwrap_or(Found::Nothing)
            };
            (found, (here.0.to_string(), here.1.to_string()))
        })?;
        if !matches!(found, Found::Typing) && self.shapes.end(&mut self.ink) {
            self.send();
        }
        Some(match found {
            Found::Typing => Subject::Typing,
            Found::Kept(kinds) => match kinds[..] {
                [] => Subject::Canvas,
                [kind] => Subject::Item { kind },
                _ => Subject::Selection { boxed: kinds.contains(&Kind::Box) },
            },
            Found::Hit(id, kind) => {
                self.select.only((&here.0, &here.1), id);
                Subject::Item { kind }
            }
            Found::Nothing => Subject::Canvas,
        })
    }
}

/// The kind of item `id` of `items`; `None` for one not there, or chrome.
fn kind_of(items: &[Cow<'_, Item>], id: ItemId) -> Option<Kind> {
    Some(match items.iter().find(|i| i.id == id)?.shape {
        Shape::Stroke { .. } => Kind::Stroke,
        Shape::Text { .. } => Kind::Text,
        Shape::Box { .. } => Kind::Box,
        Shape::Arrow { .. } => Kind::Arrow,
        Shape::Pdf { .. } => Kind::Pdf,
        Shape::Live { .. } => Kind::Live,
        Shape::Button { .. } | Shape::Panel { .. } | Shape::Caret { .. } => return None,
    })
}
