//! The unfolds on their way, and the peek ([`super::super::fold`]): an
//! unfold whose old room is taken writes nothing and shows the box at full
//! size in its old room, in the pane the command ran in, until escape or a
//! press outside it.

use super::Follow;
use crate::commands::Chord;
use crate::layout::{PaneId, Point, Rect};
use crate::scene::canvas::ItemId;
use crate::scene::ink::Answer;

/// The escape key.
const ESCAPE: Chord = Chord::new(0, '\u{1b}');

/// The unfolds on their way, and the peek shown, if any.
#[derive(Debug, Default)]
pub(super) struct Folds {
    /// Each unfold sent, by its key in the ink: the peek it shows if its
    /// room is taken.
    unfolding: Vec<(u64, Peek)>,
    peek: Option<Peek>,
}

/// A folded box shown as a peek in a pane.
#[derive(Debug, Clone, PartialEq)]
struct Peek {
    pane: PaneId,
    canvas: String,
    page: String,
    id: ItemId,
}

impl Follow {
    /// The unfold of box `id` on `page` of `canvas`, sent under `key`,
    /// peeks in `pane` if its room is taken.
    pub(in crate::canvases) fn unfolding(
        &mut self,
        key: u64,
        pane: PaneId,
        (canvas, on): (String, String),
        id: ItemId,
    ) {
        self.folds.unfolding.push((key, Peek { pane, canvas, page: on, id }));
    }

    /// The daemon's `answer` to the change sent under `key`: an unfold
    /// whose room is taken shows as a peek.
    pub(in crate::canvases) fn fold_answered(&mut self, key: u64, answer: &Answer) {
        let Some(at) = self.folds.unfolding.iter().position(|(k, _)| *k == key) else { return };
        let (_, peek) = self.folds.unfolding.remove(at);
        if matches!(answer, Answer::Peek) {
            self.folds.peek = Some(peek);
        }
    }

    /// Escape closes the peek. False when there is none, or `chord` is
    /// not escape.
    pub(in crate::canvases) fn peek_key(&mut self, chord: Chord) -> bool {
        chord == ESCAPE && self.peeked().is_some() && self.folds.peek.take().is_some()
    }

    /// A press at `at`, a canvas point in `pane`, while a peek shows: one
    /// outside it closes it, one inside does nothing. False, and the press
    /// goes on, when no peek shows.
    pub(in crate::canvases) fn peek_press(&mut self, pane: PaneId, at: Point) -> bool {
        let Some((peek, room)) = self.peeked() else { return false };
        if peek.pane != pane || !super::super::fold::contains(room, at) {
            self.folds.peek = None;
        }
        true
    }

    /// The box `pane` shows as a peek on `page` of `canvas`, if any.
    pub(super) fn peek_in(&self, pane: PaneId, here: (&str, &str)) -> Option<ItemId> {
        let (peek, _) = self.peeked()?;
        (peek.pane == pane && (&*peek.canvas, &*peek.page) == here).then_some(peek.id)
    }

    /// The peek, and its room, while its box is still folded on a page followed.
    fn peeked(&self) -> Option<(&Peek, Rect)> {
        let peek = self.folds.peek.as_ref()?;
        let mirror = self.mirror(&peek.canvas, &peek.page)?;
        Some((peek, mirror.room(peek.id)?))
    }
}
