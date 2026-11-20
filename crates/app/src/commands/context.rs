//! The context menus (plan pl-9880f1): what a context click on a thing
//! offers ([`items`]). The thing is named in the type ([`Subject`]). Each
//! item is the bar's ([`menu::item`]): the command's title, its live key,
//! grey where [`able::can`](super::able::can) says it cannot act. A pick
//! runs the command as its menu bar item does, on what the click made
//! current (the pane it focused, the pointer it moved). `platform` only
//! pops the menu a [`Popup`] says. Pure.

use super::menu::{self, Action, Item, Native};
use super::{Command, Keymap, able::Facts};
use crate::layout::Point;
use Action::{Native as N, Run as R, Separator as S};

/// What a context click was on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    /// The empty part of a canvas pane.
    Canvas,
    /// A pane's footer; `ui` when the pane shows an agent UI.
    Pane { ui: bool },
}

/// A context menu to pop: where (in the window, from its top left) and
/// what it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Popup {
    pub at: Point,
    pub items: Vec<Item>,
}

/// The items `subject`'s menu holds, in order: titles and keys as `keys`
/// has them, a check on `checked`, each grey where `facts` say its command
/// cannot act. No menu starts or ends with a separator or has two in a row.
pub fn items(subject: Subject, keys: &Keymap, checked: Command, facts: &Facts) -> Vec<Item> {
    let actions: &[Action] = match subject {
        Subject::Canvas => &[
            N(Native::Paste),
            R(Command::NewText),
            R(Command::SelectAll),
            S,
            R(Command::SlingNow),
            S,
            R(Command::ZoomReset),
            R(Command::ToggleGrid),
            S,
            R(Command::NewPage),
            R(Command::StepOut),
        ],
        Subject::Pane { ui: true } => &[
            R(Command::ExportUi),
            S,
            R(Command::SplitRight),
            R(Command::SplitDown),
            R(Command::ClosePane),
        ],
        Subject::Pane { ui: false } => {
            &[R(Command::SplitRight), R(Command::SplitDown), R(Command::ClosePane)]
        }
    };
    tidy(actions.iter().map(|&action| menu::item(action, keys, checked, facts)).collect())
}

/// `items` with no separator first, last or after another.
fn tidy(items: Vec<Item>) -> Vec<Item> {
    let line = |i: &Item| i.action == Action::Separator;
    let mut out: Vec<Item> = Vec::with_capacity(items.len());
    for item in items {
        if !line(&item) || out.last().is_some_and(|last| !line(last)) {
            out.push(item);
        }
    }
    if out.last().is_some_and(line) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests;
