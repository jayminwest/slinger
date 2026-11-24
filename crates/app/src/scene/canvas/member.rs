//! Which figure an item is in, and whether the user's hands pass over it
//! (docs/design/figures.md, "Lock"): `locked`, `fig` and `part` as the
//! mirror reads them from the canvas file (`mirror::style`).
//!
//! Every gesture that picks items by place asks [`free`] (the design's
//! gesture table): a locked item is passed over as if it were not there,
//! so ink drawn over it is written as on empty paper. The one gesture that
//! picks a locked item is `unlock-figure`, which selects its whole figure
//! as one unit (`Select::grab`).

use super::Item;

/// An item's figure, its part and whether it is locked. The default is a
/// free item in no figure.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Member {
    /// The user's hands pass over it.
    pub locked: bool,
    /// The figure's name.
    pub fig: Option<String>,
    /// The part's name; only with `fig`.
    pub part: Option<String>,
}

/// Whether a gesture that picks by place may pick `item`: it is not locked.
pub fn free(item: &Item) -> bool {
    !item.member.locked
}

/// Whether `item` is in the figure `unlocked` names: a figure's name, or
/// the id of a locked item in none, a figure of one.
pub fn in_figure(item: &Item, unlocked: &Item) -> bool {
    match &unlocked.member.fig {
        Some(fig) => item.member.fig.as_ref() == Some(fig),
        None => item.id == unlocked.id,
    }
}
